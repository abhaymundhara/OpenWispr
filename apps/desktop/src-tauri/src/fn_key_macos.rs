#![cfg(target_os = "macos")]

use objc2_core_foundation::{kCFRunLoopDefaultMode, CFMachPort, CFRunLoop};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventTapLocation, CGEventTapOptions,
    CGEventTapPlacement, CGEventTapProxy, CGEventType,
};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Wry};
use crate::audio::{self, AudioCapture};

static TASK_HANDLE: Mutex<Option<tauri::async_runtime::JoinHandle<()>>> = Mutex::new(None);

struct FnHoldState {
    app: AppHandle<Wry>,
    is_fn_down: bool,
    is_push_key_down: bool,
    is_push_active: bool,
    is_hands_free: bool,
    is_recording_active: bool,
    hold_emitted: bool,
}

fn hands_free_keycode(shortcut: &str) -> i64 {
    match crate::store::normalize_shortcut(shortcut).as_str() {
        "fn+enter" => 36,
        "fn+tab" => 48,
        _ => 49, // fn+space
    }
}

fn push_to_talk_keycode(shortcut: &str) -> Option<i64> {
    match crate::store::normalize_shortcut(shortcut).as_str() {
        "fn+enter" => Some(36),
        "fn+tab" => Some(48),
        _ => None,
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

fn recompute_push_active(state: &mut FnHoldState, push_keycode: Option<i64>) {
    let should_push_active = if state.is_hands_free {
        false
    } else {
        match push_keycode {
            Some(_) => state.is_fn_down && state.is_push_key_down,
            None => state.is_fn_down,
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

unsafe extern "C-unwind" fn fn_event_tap_callback(
    _proxy: CGEventTapProxy,
    event_type: CGEventType,
    event: NonNull<CGEvent>,
    user_info: *mut c_void,
) -> *mut CGEvent {
    if user_info.is_null() {
        return event.as_ptr();
    }

    let state = &mut *(user_info as *mut FnHoldState);
    let event_ref = event.as_ref();

    let push_keycode = push_to_talk_keycode(&crate::store::push_to_talk_shortcut());
    let hands_free_toggle_keycode = hands_free_keycode(&crate::store::hands_free_toggle_shortcut());

    // 1. Handle Fn Key (FlagsChanged)
    if event_type == CGEventType::FlagsChanged {
        let flags = CGEvent::flags(Some(event_ref));
        let is_fn_down = flags.contains(CGEventFlags::MaskSecondaryFn);

        if is_fn_down != state.is_fn_down {
            state.is_fn_down = is_fn_down;
            if !state.is_fn_down {
                // Prevent stale key-down state if Fn is released first.
                state.is_push_key_down = false;
            }
        }
    }
    // 2. Handle configurable hands-free key (KeyDown) for toggle
    else if event_type == CGEventType::KeyDown || event_type == CGEventType::KeyUp {
        let keycode =
            CGEvent::integer_value_field(Some(event_ref), CGEventField::KeyboardEventKeycode);

        if let Some(push_keycode) = push_keycode {
            if keycode == push_keycode {
                state.is_push_key_down = event_type == CGEventType::KeyDown;
            }
        }

        if event_type == CGEventType::KeyDown && keycode == hands_free_toggle_keycode {
            let flags = CGEvent::flags(Some(event_ref));
            if flags.contains(CGEventFlags::MaskSecondaryFn) {
                state.is_hands_free = !state.is_hands_free;
                if state.is_hands_free {
                    println!("[Shortcuts] Hands-free mode ACTIVATED");
                } else {
                    println!("[Shortcuts] Hands-free mode DEACTIVATED");
                }
            }
        }
    }

    recompute_push_active(state, push_keycode);
    sync_recording(state);
    sync_hold_signal(state);

    event.as_ptr()
}

pub fn start_fn_hold_listener(app: AppHandle<Wry>) {
    std::thread::spawn(move || {
        let state = Box::new(FnHoldState {
            app,
            is_fn_down: false,
            is_push_key_down: false,
            is_push_active: false,
            is_hands_free: false,
            is_recording_active: false,
            hold_emitted: false,
        });
        let user_info = Box::into_raw(state) as *mut c_void;

        let mask = (1u64 << (CGEventType::FlagsChanged.0 as u64)) 
                 | (1u64 << (CGEventType::KeyDown.0 as u64))
                 | (1u64 << (CGEventType::KeyUp.0 as u64));

        let tap = unsafe {
            CGEvent::tap_create(
                CGEventTapLocation::HIDEventTap,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::ListenOnly,
                mask,
                Some(fn_event_tap_callback),
                user_info,
            )
        };

        let Some(tap) = tap else {
            eprintln!("Failed to create Fn key event tap. Check Accessibility permissions.");
            return;
        };

        CGEvent::tap_enable(&tap, true);

        let Some(source) = CFMachPort::new_run_loop_source(None, Some(&tap), 0) else {
            eprintln!("Failed to create run loop source for Fn key event tap.");
            return;
        };

        let Some(run_loop) = CFRunLoop::current() else {
            eprintln!("Failed to get current run loop for Fn key listener.");
            return;
        };

        let default_mode = unsafe { kCFRunLoopDefaultMode };
        let Some(default_mode) = default_mode else {
            eprintln!("Failed to get kCFRunLoopDefaultMode.");
            return;
        };
        run_loop.add_source(Some(&source), Some(default_mode));
        CFRunLoop::run();
    });
}
