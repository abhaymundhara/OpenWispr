#![cfg(target_os = "windows")]

use std::ffi::c_void;
use std::mem::{size_of, MaybeUninit};
use std::ptr::null_mut;
use tauri::{AppHandle, Manager, Wry};
use crate::audio::{self, AudioCapture};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetRawInputData, RegisterRawInputDevices, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER,
    RAWKEYBOARD, RID_INPUT, RIDEV_INPUTSINK, RIM_TYPEKEYBOARD, RI_KEY_BREAK, VK_F22, VK_F23,
    VK_F24,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostQuitMessage,
    RegisterClassW, SetWindowLongPtrW, TranslateMessage, CREATESTRUCTW, CW_USEDEFAULT, GWLP_USERDATA,
    MSG, WM_CREATE, WM_DESTROY, WM_INPUT, WNDCLASSW,
};

struct FnHoldState {
    app: AppHandle<Wry>,
    is_down: bool,
    debug: bool,
    vkey_override: Option<u16>,
    makecode_override: Option<u16>,
}

fn wide(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
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
            ) as *mut FnHoldState;
            if !state_ptr.is_null() {
                handle_raw_input(lparam, &mut *state_ptr);
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
    let res = GetRawInputData(
        lparam as isize,
        RID_INPUT,
        null_mut(),
        &mut size,
        header_size,
    );
    if res == u32::MAX || size == 0 {
        return;
    }

    let mut buffer = vec![0u8; size as usize];
    let res = GetRawInputData(
        lparam as isize,
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

    if state.debug {
        println!(
            "RawInput keyboard: vkey=0x{:02X} make=0x{:02X} flags=0x{:02X}",
            vkey, makecode, flags
        );
    }

    if !is_fn_key(vkey, makecode, state) {
        return;
    }

    let is_down = !is_break;
    if is_down == state.is_down {
        return;
    }

    state.is_down = is_down;
    let _ = state.app.emit_all("fn-hold", is_down);

    let capture = state.app.state::<AudioCapture>().inner().clone();
    let app_handle = state.app.clone();

    if is_down {
        audio::remember_active_paste_target();
        crate::show_main_overlay_window(&state.app);
    }

    if is_down {
        if let Err(err) = audio::start_recording_for_capture(&capture, app_handle) {
            eprintln!("Failed to start recording on Fn press: {}", err);
        }
    } else {
        tauri::async_runtime::spawn(async move {
            if let Err(err) = audio::stop_recording_for_capture(capture, app_handle).await {
                eprintln!("Failed to stop recording on Fn release: {}", err);
            }
        });
    }
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
            is_down: false,
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
            hIcon: 0,
            hCursor: 0,
            hbrBackground: 0,
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
            0,
            0,
            hinstance,
            state_ptr,
        );

        if hwnd == 0 {
            eprintln!("Failed to create Raw Input window.");
            return;
        }

        let rid = RAWINPUTDEVICE {
            usUsagePage: 0x01,
            usUsage: 0x06,
            dwFlags: RIDEV_INPUTSINK,
            hwndTarget: hwnd,
        };
        if RegisterRawInputDevices(&[rid], 1, size_of::<RAWINPUTDEVICE>() as u32) == 0 {
            eprintln!("RegisterRawInputDevices failed.");
            return;
        }

        let mut msg: MSG = MaybeUninit::zeroed().assume_init();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}
