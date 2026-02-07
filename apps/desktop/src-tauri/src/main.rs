#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use device_query::{DeviceQuery, DeviceState, Keycode};
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use std::sync::{Arc, Mutex};
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use std::time::Duration;
#[cfg(target_os = "macos")]
use tauri::ActivationPolicy;
use tauri::{Manager, RunEvent, SystemTray, SystemTrayEvent, Wry};

mod audio;
#[cfg(target_os = "macos")]
mod fn_key_macos;
#[cfg(target_os = "windows")]
mod fn_key_windows;
mod models;
use audio::AudioCapture;

fn show_models_window(app_handle: &tauri::AppHandle<Wry>) {
    if let Some(window) = app_handle.get_window("models") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub(crate) fn show_main_overlay_window(app_handle: &tauri::AppHandle<Wry>) {
    if let Some(window) = app_handle.get_window("main") {
        if let Ok(Some(monitor)) = window.current_monitor() {
            let monitor_pos = monitor.position();
            let monitor_size = monitor.size();
            let window_size = window
                .outer_size()
                .unwrap_or(tauri::PhysicalSize::new(400, 200));

            #[cfg(target_os = "macos")]
            let bottom_margin: i32 = 96;
            #[cfg(target_os = "windows")]
            let bottom_margin: i32 = 56;
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let bottom_margin: i32 = 48;

            let x = monitor_pos.x + (monitor_size.width as i32 - window_size.width as i32) / 2;
            let y = monitor_pos.y + monitor_size.height as i32 - window_size.height as i32 - bottom_margin;
            let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
        }
        let _ = window.show();
    }
}

fn main() {
    let mut tray = SystemTray::new();
    #[cfg(target_os = "macos")]
    {
        tray = tray.with_icon_as_template(false);
    }

    let app = tauri::Builder::default()
        .system_tray(tray)
        .on_system_tray_event(|app_handle, event| match event {
            SystemTrayEvent::LeftClick { .. }
            | SystemTrayEvent::RightClick { .. }
            | SystemTrayEvent::DoubleClick { .. } => {
                show_models_window(app_handle);
            }
            _ => {}
        })
        .setup(|app| {
            let handle = app.handle();
            if let Some(main_window) = app.get_window("main") {
                // Keep overlay non-interactive so it does not block the active app
                // while still allowing us to keep the process alive.
                let _ = main_window.set_ignore_cursor_events(true);
            }

            #[cfg(target_os = "macos")]
            {
                // Menu bar only (no Dock icon).
                app.set_activation_policy(ActivationPolicy::Accessory);
            }

            println!("\n==============================================");
            println!("🎙️  OpenWispr Starting...");
            println!("==============================================");
            println!("Press and hold Fn to dictate");
            println!("==============================================\n");

            #[cfg(target_os = "macos")]
            {
                fn_key_macos::start_fn_hold_listener(handle.clone());
            }

            #[cfg(target_os = "windows")]
            {
                fn_key_windows::start_fn_hold_listener(handle.clone());
            }

            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                let window = app.get_window("main").unwrap();

                // Track key state
                let key_pressed = Arc::new(Mutex::new(false));

                let w = window.clone();
                let h = handle.clone();
                let key_pressed_clone = key_pressed.clone();

                // Spawn keyboard polling thread
                std::thread::spawn(move || {
                    println!("⌨️  Keyboard polling started - Press Control to toggle");
                    let device_state = DeviceState::new();

                    loop {
                        let keys: Vec<Keycode> = device_state.get_keys();

                        // Check if Control key is pressed (works on both sides)
                        let control_pressed =
                            keys.contains(&Keycode::LControl) || keys.contains(&Keycode::RControl);

                        if let Ok(mut pressed) = key_pressed_clone.lock() {
                            if control_pressed && !*pressed {
                                // Key was just pressed - toggle window
                                *pressed = true;
                                println!("✅ Control key pressed - Toggling window...");

                                let _ = h.emit_all("global-shortcut-pressed", "");
                                if w.is_visible().unwrap_or(false) {
                                    let _ = w.hide();
                                    println!("👻 Window hidden");
                                } else {
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                    println!("👁️  Window shown");
                                }
                            } else if !control_pressed && *pressed {
                                // Key was released
                                *pressed = false;
                            }
                        }

                        // Poll every 50ms (responsive but not too CPU intensive)
                        std::thread::sleep(Duration::from_millis(50));
                    }
                });
            }

            Ok(())
        })
        .manage(AudioCapture::new())
        .invoke_handler(tauri::generate_handler![
            audio::start_recording,
            audio::stop_recording,
            models::list_models,
            models::download_model,
            models::get_active_model,
            models::set_active_model
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| match event {
        RunEvent::ExitRequested { api, .. } => {
            #[cfg(not(debug_assertions))]
            {
                // Keep the app alive even when no windows are visible.
                println!("[lifecycle] exit requested - preventing exit");
                api.prevent_exit();
            }
            #[cfg(debug_assertions)]
            {
                let _ = api;
                // In dev, allow process shutdown to avoid orphaned background instances.
                println!("[lifecycle] exit requested - allowing exit in debug");
            }
        }
        RunEvent::WindowEvent {
            label,
            event: tauri::WindowEvent::CloseRequested { api, .. },
            ..
        } if label == "models" || label == "main" => {
            println!("[lifecycle] close requested for window '{}' - hiding", label);
            api.prevent_close();
            if let Some(window) = app_handle.get_window(&label) {
                let _ = window.hide();
            }
        }
        RunEvent::Exit => {
            println!("[lifecycle] run loop exiting");
        }
        _ => {}
    });
}
