#![cfg(target_os = "macos")]

use crate::audio::{self, AudioCapture};
use crate::store::ShortcutSpec;
use objc2_core_foundation::{kCFRunLoopDefaultMode, CFMachPort, CFRunLoop};
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
    CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy, CGEventType,
};
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, Wry};

static TASK_HANDLE: Mutex<Option<tauri::async_runtime::JoinHandle<()>>> = Mutex::new(None);
const FN_KEYCODE: u16 = 63;

struct FnHoldState {
    app: AppHandle<Wry>,
    fn_down: bool,
    ctrl_down: bool,
    shift_down: bool,
    alt_down: bool,
    meta_down: bool,
    pressed_keys: HashSet<String>,
    keycode_tokens: HashMap<i64, String>,
    fn_hardware_supported: bool,
    is_push_active: bool,
    is_hands_free: bool,
    is_recording_active: bool,
    hold_emitted: bool,
    hands_free_combo_prev_active: bool,
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

fn update_non_fn_modifier_state_from_flags(state: &mut FnHoldState, flags: CGEventFlags) {
    state.ctrl_down = flags.contains(CGEventFlags::MaskControl);
    state.shift_down = flags.contains(CGEventFlags::MaskShift);
    state.alt_down = flags.contains(CGEventFlags::MaskAlternate);
    state.meta_down = flags.contains(CGEventFlags::MaskCommand);
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

fn key_token_from_keycode(keycode: i64) -> Option<String> {
    match keycode {
        36 => Some("enter".to_string()),
        48 => Some("tab".to_string()),
        49 => Some("space".to_string()),
        51 => Some("backspace".to_string()),
        53 => Some("escape".to_string()),
        123 => Some("left".to_string()),
        124 => Some("right".to_string()),
        125 => Some("down".to_string()),
        126 => Some("up".to_string()),
        122 => Some("f1".to_string()),
        120 => Some("f2".to_string()),
        99 => Some("f3".to_string()),
        118 => Some("f4".to_string()),
        96 => Some("f5".to_string()),
        97 => Some("f6".to_string()),
        98 => Some("f7".to_string()),
        100 => Some("f8".to_string()),
        101 => Some("f9".to_string()),
        109 => Some("f10".to_string()),
        103 => Some("f11".to_string()),
        111 => Some("f12".to_string()),
        _ => None,
    }
}

fn key_token_from_event(event: &CGEvent) -> Option<String> {
    let keycode = CGEvent::integer_value_field(Some(event), CGEventField::KeyboardEventKeycode);
    if let Some(token) = key_token_from_keycode(keycode) {
        return Some(token);
    }

    let mut actual_len = 0;
    let mut unicode = [0u16; 8];
    unsafe {
        CGEvent::keyboard_get_unicode_string(
            Some(event),
            unicode.len() as _,
            &mut actual_len,
            unicode.as_mut_ptr(),
        );
    }
    if actual_len <= 0 {
        return None;
    }

    let s = String::from_utf16_lossy(&unicode[..actual_len as usize]);
    let ch = s.chars().next()?;
    if ch.is_control() {
        return None;
    }
    if ch == ' ' {
        return Some("space".to_string());
    }
    Some(ch.to_ascii_lowercase().to_string())
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

    let flags = CGEvent::flags(Some(event_ref));
    update_non_fn_modifier_state_from_flags(state, flags);

    let hardware_fn_down =
        CGEventSource::key_state(CGEventSourceStateID::HIDSystemState, FN_KEYCODE);
    if hardware_fn_down {
        state.fn_hardware_supported = true;
    }
    if state.fn_hardware_supported {
        state.fn_down = hardware_fn_down;
    } else if event_type == CGEventType::FlagsChanged {
        let keycode =
            CGEvent::integer_value_field(Some(event_ref), CGEventField::KeyboardEventKeycode);
        // Only trust Fn state when the actual Fn key emits a FlagsChanged event.
        // This avoids false positives from special keys (e.g. dictation/mic).
        if keycode == i64::from(FN_KEYCODE) {
            state.fn_down = flags.contains(CGEventFlags::MaskSecondaryFn);
        }
    }

    if event_type == CGEventType::KeyDown || event_type == CGEventType::KeyUp {
        let keycode =
            CGEvent::integer_value_field(Some(event_ref), CGEventField::KeyboardEventKeycode);
        if event_type == CGEventType::KeyDown {
            if let Some(token) = key_token_from_event(event_ref) {
                state.keycode_tokens.insert(keycode, token.clone());
                state.pressed_keys.insert(token);
            }
        } else if let Some(existing) = state.keycode_tokens.remove(&keycode) {
            state.pressed_keys.remove(&existing);
        } else if let Some(token) = key_token_from_event(event_ref) {
            state.pressed_keys.remove(&token);
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

    event.as_ptr()
}

pub fn start_fn_hold_listener(app: AppHandle<Wry>) {
    std::thread::spawn(move || {
        let state = Box::new(FnHoldState {
            app,
            fn_down: false,
            ctrl_down: false,
            shift_down: false,
            alt_down: false,
            meta_down: false,
            pressed_keys: HashSet::new(),
            keycode_tokens: HashMap::new(),
            fn_hardware_supported: false,
            is_push_active: false,
            is_hands_free: false,
            is_recording_active: false,
            hold_emitted: false,
            hands_free_combo_prev_active: false,
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
