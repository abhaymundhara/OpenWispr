#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use device_query::{DeviceQuery, DeviceState, Keycode};
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use std::sync::{Arc, Mutex};
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use std::time::Duration;
#[cfg(target_os = "macos")]
use tauri::ActivationPolicy;
use tauri::{Manager, RunEvent, SystemTray, SystemTrayEvent, SystemTrayMenu, CustomMenuItem, Wry};

mod audio;
#[cfg(target_os = "macos")]
mod fn_key_macos;
#[cfg(target_os = "windows")]
mod fn_key_windows;
mod models;
mod logger;
use audio::AudioCapture;
use logger::log_session_start;

fn show_models_window(app_handle: &tauri::AppHandle<Wry>) {
    if let Some(window) = app_handle.get_window("models") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub(crate) fn show_main_overlay_window(app_handle: &tauri::AppHandle<Wry>) {
    if let Some(window) = app_handle.get_window("main") {
        eprintln!("📺 show_main_overlay_window called");
        if let Ok(Some(monitor)) = window.current_monitor() {
            let monitor_pos = monitor.position();
            let monitor_size = monitor.size();
            let raw_window_size = window
                .outer_size()
                .unwrap_or(tauri::PhysicalSize::new(400, 200));
            let width = if raw_window_size.width == 0 {
                400
            } else {
                raw_window_size.width as i32
            };
            let height = if raw_window_size.height == 0 {
                200
            } else {
                raw_window_size.height as i32
            };

            #[cfg(target_os = "macos")]
            let bottom_margin: i32 = 180;
            #[cfg(target_os = "windows")]
            let bottom_margin: i32 = 180;
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let bottom_margin: i32 = 48;

            let min_x = monitor_pos.x;
            let max_x = monitor_pos.x + monitor_size.width as i32 - width;
            let min_y = monitor_pos.y;
            let max_y = monitor_pos.y + monitor_size.height as i32 - height;

            let center_x = monitor_pos.x + (monitor_size.width as i32 - width) / 2;
            let bottom_y = monitor_pos.y + monitor_size.height as i32 - height - bottom_margin;

            let x = center_x.clamp(min_x, max_x.max(min_x));
            let y = bottom_y.clamp(min_y, max_y.max(min_y));

            eprintln!("📍 Positioning window at ({}, {})", x, y);
            let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
        }
        match window.show() {
            Ok(_) => eprintln!("✅ Main window shown successfully"),
            Err(e) => eprintln!("❌ Failed to show window: {}", e),
        }
    } else {
        eprintln!("❌ Main window not found!");
    }
}

#[tauri::command]
fn show_main_window(app: tauri::AppHandle<Wry>) {
    show_main_overlay_window(&app);
}

fn main() {
    let start_dictation = CustomMenuItem::new("start_dictation".to_string(), "Start Dictation (Hold Ctrl+Fn)");
    let open_models = CustomMenuItem::new("open_models".to_string(), "Model Manager");
    let quit = CustomMenuItem::new("quit".to_string(), "Quit");
    
    let tray_menu = SystemTrayMenu::new()
        .add_item(start_dictation)
        .add_item(open_models)
        .add_native_item(tauri::SystemTrayMenuItem::Separator)
        .add_item(quit);

    #[cfg(target_os = "macos")]
    let tray = SystemTray::new()
        .with_icon_as_template(false)
        .with_menu(tray_menu);
    
    #[cfg(not(target_os = "macos"))]
    let tray = SystemTray::new().with_menu(tray_menu);

    let app = tauri::Builder::default()
        .system_tray(tray)
        .on_system_tray_event(|app_handle, event| match event {
            SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
                "start_dictation" => {
                    // Just show instructions - actual recording happens via Ctrl+Fn
                    show_models_window(app_handle);
                }
                "open_models" => {
                    show_models_window(app_handle);
                }
                "quit" => {
                    std::process::exit(0);
                }
                _ => {}
            }
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
            #[cfg(target_os = "macos")]
            println!("Press and hold Fn to dictate");
            #[cfg(target_os = "windows")]
            {
                println!("🎮 Hold Ctrl + Shift together to start dictation");
                println!("   Release either key to stop and transcribe");
                println!("   All keystrokes are logged to console for debugging");
            }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            println!("Press Control to toggle dictation");
            println!("==============================================\n");

            // Initialize logging
            log_session_start();

            // Show models window on startup
            show_models_window(&handle);

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
            models::set_active_model,
            show_main_window
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
