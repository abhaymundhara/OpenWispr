#![cfg(target_os = "windows")]

use crate::audio::{self, AudioCapture};
use std::ffi::c_void;
use std::mem::{size_of, MaybeUninit};
use std::ptr::null_mut;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Wry};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::{
    GetRawInputData, RegisterRawInputDevices, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER,
    RAWKEYBOARD, RID_INPUT, RIDEV_INPUTSINK, RIM_TYPEKEYBOARD,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    VK_F22, VK_F23, VK_F24, VK_RETURN, VK_SPACE, VK_TAB,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA,
    GetMessageW, MSG, PostQuitMessage, RegisterClassW, SetWindowLongPtrW, TranslateMessage,
    WM_CREATE, WM_DESTROY, WM_INPUT, WNDCLASSW,
};

/// Keyboard flag indicating key release. Not exported by windows-sys.
const RI_KEY_BREAK: u16 = 1;

static TASK_HANDLE: Mutex<Option<tauri::async_runtime::JoinHandle<()>>> = Mutex::new(None);

struct FnHoldState {
    app: AppHandle<Wry>,
    fn_is_down: bool,
    is_push_key_down: bool,
    is_push_active: bool,
    is_hands_free: bool,
    is_recording_active: bool,
    hold_emitted: bool,
    debug: bool,
    vkey_override: Option<u16>,
    makecode_override: Option<u16>,
}

fn wide(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}

fn hands_free_toggle_vkey(shortcut: &str) -> u16 {
    match crate::store::normalize_shortcut(shortcut).as_str() {
        "fn+enter" => VK_RETURN as u16,
        "fn+tab" => VK_TAB as u16,
        _ => VK_SPACE as u16, // fn+space
    }
}

fn push_to_talk_vkey(shortcut: &str) -> Option<u16> {
    match crate::store::normalize_shortcut(shortcut).as_str() {
        "fn+enter" => Some(VK_RETURN as u16),
        "fn+tab" => Some(VK_TAB as u16),
        _ => None, // fn (no secondary key)
    }
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

fn sync_hold_signal(state: &mut FnHoldState) {
    let should_emit_hold = state.is_hands_free || state.is_push_active;
    if should_emit_hold != state.hold_emitted {
        state.hold_emitted = should_emit_hold;
        let _ = state.app.emit_all("fn-hold", should_emit_hold);
    }
}

fn recompute_push_active(state: &mut FnHoldState, push_key_vkey: Option<u16>) {
    let should_push_active = if state.is_hands_free {
        false
    } else {
        match push_key_vkey {
            Some(_) => state.fn_is_down && state.is_push_key_down,
            None => state.fn_is_down,
        }
    };
    state.is_push_active = should_push_active;
}

fn sync_recording(state: &mut FnHoldState) {
    let should_record = state.is_hands_free || state.is_push_active;
    if should_record == state.is_recording_active {
        return;
    }

    if should_record {
        match start_capture(state) {
            Ok(_) => {
                state.is_recording_active = true;
            }
            Err(err) if err == "Already recording" => {
                state.is_recording_active = true;
            }
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
            let state_ptr = windows_sys::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW(
                hwnd,
                GWLP_USERDATA,
            );
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

    let push_key_vkey = push_to_talk_vkey(&crate::store::push_to_talk_shortcut());
    let hands_free_vkey = hands_free_toggle_vkey(&crate::store::hands_free_toggle_shortcut());

    if is_fn_key(vkey, makecode, state) {
        state.fn_is_down = is_down;
        if !state.fn_is_down {
            state.is_push_key_down = false;
        }
    }

    if let Some(push_key_vkey) = push_key_vkey {
        if vkey == push_key_vkey {
            state.is_push_key_down = is_down;
        }
    }

    if is_down && vkey == hands_free_vkey && state.fn_is_down {
        state.is_hands_free = !state.is_hands_free;
        if state.is_hands_free {
            println!("[Shortcuts] Hands-free mode ACTIVATED");
        } else {
            println!("[Shortcuts] Hands-free mode DEACTIVATED");
        }
    }

    recompute_push_active(state, push_key_vkey);
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
            fn_is_down: false,
            is_push_key_down: false,
            is_push_active: false,
            is_hands_free: false,
            is_recording_active: false,
            hold_emitted: false,
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
