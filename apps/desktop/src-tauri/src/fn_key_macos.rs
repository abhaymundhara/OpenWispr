#![cfg(target_os = "macos")]

use objc2_core_foundation::{kCFRunLoopDefaultMode, CFMachPort, CFRunLoop};
use objc2_core_graphics::{
    CGEvent, CGEventFlags, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventTapProxy, CGEventType,
};
use std::ffi::c_void;
use std::ptr::NonNull;
use tauri::{AppHandle, Manager, Wry};
use crate::audio::{self, AudioCapture};

struct FnHoldState {
    app: AppHandle<Wry>,
    is_down: bool,
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

    if event_type != CGEventType::FlagsChanged {
        return event.as_ptr();
    }

    let state = &mut *(user_info as *mut FnHoldState);
    let event_ref = event.as_ref();
    let flags = CGEvent::flags(Some(event_ref));
    let is_down = flags.contains(CGEventFlags::MaskSecondaryFn);

    if is_down != state.is_down {
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
            // CRITICAL: Must keep the JoinHandle alive to prevent task from being dropped mid-paste
            let _join_handle = tauri::async_runtime::spawn(async move {
                if let Err(err) = audio::stop_recording_for_capture(capture, app_handle).await {
                    eprintln!("Failed to stop recording on Fn release: {}", err);
                }
            });
            // Note: We intentionally don't await the join_handle here, as this callback
            // must return immediately. The task will complete independently.
        }
    }

    event.as_ptr()
}

pub fn start_fn_hold_listener(app: AppHandle<Wry>) {
    std::thread::spawn(move || {
        let state = Box::new(FnHoldState {
            app,
            is_down: false,
        });
        let user_info = Box::into_raw(state) as *mut c_void;

        let mask = 1u64 << (CGEventType::FlagsChanged.0 as u64);

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
