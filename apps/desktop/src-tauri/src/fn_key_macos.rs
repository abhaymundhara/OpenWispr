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
    is_hands_free: bool,
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

    // 1. Handle Fn Key (FlagsChanged)
    if event_type == CGEventType::FlagsChanged {
        let flags = CGEvent::flags(Some(event_ref));
        let is_fn_down = flags.contains(CGEventFlags::MaskSecondaryFn);

        if is_fn_down != state.is_fn_down {
            state.is_fn_down = is_fn_down;
            let _ = state.app.emit_all("fn-hold", is_fn_down);

            let capture = state.app.state::<AudioCapture>().inner().clone();
            let app_handle = state.app.clone();

            if is_fn_down {
                // Fn Pressed: Start Recording (if not already)
                // If hands-free is active, we are already recording, so this just updates visual state if needed.
                if !state.is_hands_free {
                    audio::remember_active_paste_target();
                    crate::show_main_overlay_window(&state.app);
                    if let Err(err) = audio::start_recording_for_capture(&capture, app_handle) {
                         // Ignore "already recording" if we are just pressing Fn while hands-free
                         if err != "Already recording" {
                            eprintln!("Failed to start recording on Fn press: {}", err);
                         }
                    }
                }
            } else {
                // Fn Released
                if !state.is_hands_free {
                     // Stop recording ONLY if NOT in hands-free mode
                    let handle = tauri::async_runtime::spawn(async move {
                        if let Err(err) = audio::stop_recording_for_capture(capture, app_handle).await {
                            eprintln!("Failed to stop recording on Fn release: {}", err);
                        }
                    });
                    if let Ok(mut task) = TASK_HANDLE.lock() {
                        *task = Some(handle);
                    }
                }
            }
        }
    }
    // 2. Handle Space Key (KeyDown) for Hands-Free Toggle
    else if event_type == CGEventType::KeyDown {
        let keycode = event_ref.get_integer_value_field(CGEventField::KeyboardEventKeycode);
        if keycode == 49 { // Space
            // Check if Fn is held
            let flags = CGEvent::flags(Some(event_ref));
            if flags.contains(CGEventFlags::MaskSecondaryFn) {
                 // Fn + Space Detected
                 state.is_hands_free = !state.is_hands_free;
                 
                 let capture = state.app.state::<AudioCapture>().inner().clone();
                 let app_handle = state.app.clone();

                 if state.is_hands_free {
                     println!("[Shortcuts] Hands-free mode ACTIVATED");
                     // Ensure recording is started (it should be because Fn is down, but just in case)
                     audio::remember_active_paste_target();
                     crate::show_main_overlay_window(&state.app);
                     let _ = audio::start_recording_for_capture(&capture, app_handle);
                 } else {
                     println!("[Shortcuts] Hands-free mode DEACTIVATED");
                     // Stop recording immediately
                     let handle = tauri::async_runtime::spawn(async move {
                        if let Err(err) = audio::stop_recording_for_capture(capture, app_handle).await {
                            eprintln!("Failed to stop recording on hands-free toggle off: {}", err);
                        }
                    });
                     if let Ok(mut task) = TASK_HANDLE.lock() {
                        *task = Some(handle);
                    }
                 }
                 
                 // Consume the Space event so it doesn't type a space?
                 // Maybe safer to NOT consume it if user just wants to type Space while holding Fn (rare but possible).
                 // Converting it to null pointer would consume it.
                 // return std::ptr::null_mut(); 
            }
        }
    }

    event.as_ptr()
}

pub fn start_fn_hold_listener(app: AppHandle<Wry>) {
    std::thread::spawn(move || {
        let state = Box::new(FnHoldState {
            app,
            is_fn_down: false,
            is_hands_free: false,
        });
        let user_info = Box::into_raw(state) as *mut c_void;

        let mask = (1u64 << (CGEventType::FlagsChanged.0 as u64)) 
                 | (1u64 << (CGEventType::KeyDown.0 as u64));

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
