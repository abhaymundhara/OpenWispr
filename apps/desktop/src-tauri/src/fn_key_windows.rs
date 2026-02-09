#![cfg(target_os = "windows")]

use crate::audio::{self, AudioCapture};
use crate::store::ShortcutSpec;
use std::collections::HashSet;
use std::ffi::c_void;
use std::mem::{size_of, MaybeUninit};
use std::ptr::null_mut;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Wry};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    VK_0, VK_9, VK_A, VK_BACK, VK_CAPITAL, VK_CONTROL, VK_ESCAPE, VK_F1, VK_F12, VK_F22, VK_F23,
    VK_F24, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_OEM_1, VK_OEM_2, VK_OEM_3,
    VK_OEM_4, VK_OEM_5, VK_OEM_6, VK_OEM_7, VK_OEM_COMMA, VK_OEM_MINUS, VK_OEM_PERIOD, VK_OEM_PLUS,
    VK_RCONTROL, VK_RETURN, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_SPACE, VK_TAB,
};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, RegisterRawInputDevices, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER,
    RAWKEYBOARD, RIDEV_INPUTSINK, RID_INPUT, RIM_TYPEKEYBOARD,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostQuitMessage,
    RegisterClassW, SetWindowLongPtrW, TranslateMessage, CREATESTRUCTW, CW_USEDEFAULT,
    GWLP_USERDATA, MSG, WM_CREATE, WM_DESTROY, WM_INPUT, WNDCLASSW,
};

/// Keyboard flag indicating key release. Not exported by windows-sys.
const RI_KEY_BREAK: u16 = 1;

static TASK_HANDLE: Mutex<Option<tauri::async_runtime::JoinHandle<()>>> = Mutex::new(None);

struct FnHoldState {
    app: AppHandle<Wry>,
    fn_down: bool,
    ctrl_down: bool,
    shift_down: bool,
    alt_down: bool,
    meta_down: bool,
    pressed_keys: HashSet<String>,
    is_push_active: bool,
    is_hands_free: bool,
    is_recording_active: bool,
    hold_emitted: bool,
    hands_free_combo_prev_active: bool,
    debug: bool,
    vkey_override: Option<u16>,
    makecode_override: Option<u16>,
}

fn wide(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}

fn fallback_push_spec() -> ShortcutSpec {
    crate::store::parse_shortcut("fn").unwrap_or_default()
}

fn fallback_hands_free_spec() -> ShortcutSpec {
    crate::store::parse_shortcut("fn+space").unwrap_or_default()
}

fn load_shortcuts() -> (ShortcutSpec, ShortcutSpec) {
    let push = crate::store::parse_shortcut(&crate::store::push_to_talk_shortcut())
        .unwrap_or_else(|_| fallback_push_spec());
    let hands_free = crate::store::parse_shortcut(&crate::store::hands_free_toggle_shortcut())
        .unwrap_or_else(|_| fallback_hands_free_spec());
    (push, hands_free)
}

fn start_capture(state: &FnHoldState) -> Result<(), String> {
    let capture = state.app.state::<AudioCapture>().inner().clone();
    let app_handle = state.app.clone();
    audio::remember_active_paste_target();
    crate::show_main_overlay_window(&state.app);
    audio::start_recording_for_capture(&capture, app_handle)
}

fn stop_capture(state: &FnHoldState) {
    let capture = state.app.state::<AudioCapture>().inner().clone();
    let app_handle = state.app.clone();
    let handle = tauri::async_runtime::spawn(async move {
        if let Err(err) = audio::stop_recording_for_capture(capture, app_handle).await {
            eprintln!("Failed to stop recording: {}", err);
        }
    });
    if let Ok(mut task) = TASK_HANDLE.lock() {
        *task = Some(handle);
    }
}

fn sync_recording(state: &mut FnHoldState) {
    let should_record = state.is_hands_free || state.is_push_active;
    if should_record == state.is_recording_active {
        return;
    }

    if should_record {
        match start_capture(state) {
            Ok(_) => state.is_recording_active = true,
            Err(err) if err == "Already recording" => state.is_recording_active = true,
            Err(err) => {
                eprintln!("Failed to start recording: {}", err);
                state.is_recording_active = false;
            }
        }
    } else {
        stop_capture(state);
        state.is_recording_active = false;
    }
}

fn sync_hold_signal(state: &mut FnHoldState) {
    let should_emit_hold = state.is_hands_free || state.is_push_active;
    if should_emit_hold != state.hold_emitted {
        state.hold_emitted = should_emit_hold;
        let _ = state.app.emit_all("fn-hold", should_emit_hold);
    }
}

fn is_shortcut_active(spec: &ShortcutSpec, state: &FnHoldState) -> bool {
    if spec.r#fn && !state.fn_down {
        return false;
    }
    if spec.ctrl && !state.ctrl_down {
        return false;
    }
    if spec.shift && !state.shift_down {
        return false;
    }
    if spec.alt && !state.alt_down {
        return false;
    }
    if spec.meta && !state.meta_down {
        return false;
    }
    if let Some(key) = &spec.key {
        return state.pressed_keys.len() == 1 && state.pressed_keys.contains(key);
    }
    true
}

fn vkey_to_key_token(vkey: u16) -> Option<String> {
    if (VK_A..=VK_A + 25).contains(&vkey) {
        let ch = (b'a' + (vkey - VK_A as u16) as u8) as char;
        return Some(ch.to_string());
    }
    if (VK_0..=VK_9).contains(&vkey) {
        let ch = (b'0' + (vkey - VK_0 as u16) as u8) as char;
        return Some(ch.to_string());
    }
    if (VK_F1..=VK_F12).contains(&vkey) {
        return Some(format!("f{}", vkey - VK_F1 as u16 + 1));
    }

    match vkey {
        x if x == VK_SPACE as u16 => Some("space".to_string()),
        x if x == VK_RETURN as u16 => Some("enter".to_string()),
        x if x == VK_TAB as u16 => Some("tab".to_string()),
        x if x == VK_ESCAPE as u16 => Some("escape".to_string()),
        x if x == VK_BACK as u16 => Some("backspace".to_string()),
        x if x == VK_OEM_MINUS as u16 => Some("-".to_string()),
        x if x == VK_OEM_PLUS as u16 => Some("=".to_string()),
        x if x == VK_OEM_COMMA as u16 => Some(",".to_string()),
        x if x == VK_OEM_PERIOD as u16 => Some(".".to_string()),
        x if x == VK_OEM_1 as u16 => Some(";".to_string()),
        x if x == VK_OEM_2 as u16 => Some("/".to_string()),
        x if x == VK_OEM_3 as u16 => Some("`".to_string()),
        x if x == VK_OEM_4 as u16 => Some("[".to_string()),
        x if x == VK_OEM_5 as u16 => Some("\\".to_string()),
        x if x == VK_OEM_6 as u16 => Some("]".to_string()),
        x if x == VK_OEM_7 as u16 => Some("'".to_string()),
        _ => None,
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = lparam as *const CREATESTRUCTW;
            if !cs.is_null() {
                let state_ptr = (*cs).lpCreateParams as isize;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr);
            }
            0
        }
        WM_INPUT => {
            let state_ptr =
                windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            if state_ptr != 0 {
                handle_raw_input(lparam, &mut *(state_ptr as *mut FnHoldState));
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn handle_raw_input(lparam: LPARAM, state: &mut FnHoldState) {
    let mut size: u32 = 0;
    let header_size = size_of::<RAWINPUTHEADER>() as u32;
    let hrawinput = lparam as *mut c_void;
    let res = GetRawInputData(hrawinput, RID_INPUT, null_mut(), &mut size, header_size);
    if res == u32::MAX || size == 0 {
        return;
    }

    let mut buffer = vec![0u8; size as usize];
    let res = GetRawInputData(
        hrawinput,
        RID_INPUT,
        buffer.as_mut_ptr() as *mut c_void,
        &mut size,
        header_size,
    );
    if res == u32::MAX {
        return;
    }

    let raw = &*(buffer.as_ptr() as *const RAWINPUT);
    if raw.header.dwType != RIM_TYPEKEYBOARD {
        return;
    }

    let kb: RAWKEYBOARD = raw.data.keyboard;
    let vkey = kb.VKey as u16;
    let makecode = kb.MakeCode as u16;
    let flags = kb.Flags;
    let is_break = (flags & RI_KEY_BREAK) != 0;
    let is_down = !is_break;

    if state.debug {
        println!(
            "key vkey=0x{:02X} make=0x{:02X} {}",
            vkey,
            makecode,
            if is_down { "DOWN" } else { "UP" }
        );
    }

    if is_fn_key(vkey, makecode, state) {
        state.fn_down = is_down;
    }

    match vkey {
        x if x == VK_CONTROL as u16 || x == VK_LCONTROL as u16 || x == VK_RCONTROL as u16 => {
            state.ctrl_down = is_down;
        }
        x if x == VK_SHIFT as u16 || x == VK_LSHIFT as u16 || x == VK_RSHIFT as u16 => {
            state.shift_down = is_down;
        }
        x if x == VK_MENU as u16 || x == VK_LMENU as u16 || x == VK_RMENU as u16 => {
            state.alt_down = is_down;
        }
        x if x == VK_LWIN as u16 || x == VK_RWIN as u16 => {
            state.meta_down = is_down;
        }
        _ => {}
    }

    if let Some(key_token) = vkey_to_key_token(vkey) {
        if is_down {
            state.pressed_keys.insert(key_token);
        } else {
            state.pressed_keys.remove(&key_token);
        }
    }

    let (push_shortcut, hands_free_shortcut) = load_shortcuts();
    let hands_combo_active = is_shortcut_active(&hands_free_shortcut, state);
    if hands_combo_active && !state.hands_free_combo_prev_active {
        state.is_hands_free = !state.is_hands_free;
        if state.is_hands_free {
            println!("[Shortcuts] Hands-free mode ACTIVATED");
        } else {
            println!("[Shortcuts] Hands-free mode DEACTIVATED");
        }
    }
    state.hands_free_combo_prev_active = hands_combo_active;

    state.is_push_active = if state.is_hands_free {
        false
    } else {
        is_shortcut_active(&push_shortcut, state)
    };

    sync_recording(state);
    sync_hold_signal(state);
}

fn is_fn_key(vkey: u16, makecode: u16, state: &FnHoldState) -> bool {
    if let Some(override_vkey) = state.vkey_override {
        return vkey == override_vkey;
    }
    if let Some(override_make) = state.makecode_override {
        return makecode == override_make;
    }

    // Best-effort defaults. Many keyboards do not expose Fn at all.
    vkey == VK_F24 as u16 || vkey == VK_F23 as u16 || vkey == VK_F22 as u16
}

fn parse_env_hex_u16(name: &str) -> Option<u16> {
    let value = std::env::var(name).ok()?;
    let trimmed = value.trim().trim_start_matches("0x");
    u16::from_str_radix(trimmed, 16).ok()
}

pub fn start_fn_hold_listener(app: AppHandle<Wry>) {
    std::thread::spawn(move || unsafe {
        let class_name = wide("OpenWisprRawInput");
        let hinstance = GetModuleHandleW(null_mut());

        let state = Box::new(FnHoldState {
            app,
            fn_down: false,
            ctrl_down: false,
            shift_down: false,
            alt_down: false,
            meta_down: false,
            pressed_keys: HashSet::new(),
            is_push_active: false,
            is_hands_free: false,
            is_recording_active: false,
            hold_emitted: false,
            hands_free_combo_prev_active: false,
            debug: std::env::var("OPENWISPR_RAWINPUT_DEBUG").ok().as_deref() == Some("1"),
            vkey_override: parse_env_hex_u16("OPENWISPR_FN_VKEY"),
            makecode_override: parse_env_hex_u16("OPENWISPR_FN_MAKECODE"),
        });
        let state_ptr = Box::into_raw(state) as *mut c_void;

        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            lpszClassName: class_name.as_ptr(),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null_mut(),
        };
        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            0,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            null_mut(),
            null_mut(),
            hinstance,
            state_ptr,
        );

        if hwnd.is_null() {
            eprintln!("Failed to create Raw Input window.");
            return;
        }

        let rid = RAWINPUTDEVICE {
            usUsagePage: 0x01,
            usUsage: 0x06,
            dwFlags: RIDEV_INPUTSINK,
            hwndTarget: hwnd,
        };
        if RegisterRawInputDevices(
            &rid as *const RAWINPUTDEVICE,
            1,
            size_of::<RAWINPUTDEVICE>() as u32,
        ) == 0
        {
            eprintln!("RegisterRawInputDevices failed.");
            return;
        }

        let mut msg: MSG = MaybeUninit::zeroed().assume_init();
        while GetMessageW(&mut msg, null_mut(), 0, 0) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}
