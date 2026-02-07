#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod audio;
use audio::AudioCapture;

fn main() {
  tauri::Builder::default()
    .setup(|app| {
      let handle = app.handle();
      let window = app.get_window("main").unwrap();
      
      println!("\n==============================================");
      println!("🎙️  OpenWispr Starting...");
      println!("==============================================");
      println!("Press the Fn key to toggle dictation");
      println!("");
      println!("If Fn key doesn't work:");
      println!("1. macOS will prompt for Accessibility access");
      println!("2. Or manually: System Settings → Privacy & Security → Accessibility");
      println!("3. Enable this app and restart");
      println!("==============================================\n");
      
      // Track Fn key state
      let fn_pressed = Arc::new(Mutex::new(false));
      let last_fn_release = Arc::new(Mutex::new(Instant::now()));
      
      let w = window.clone();
      let h = handle.clone();
      let fn_pressed_clone = fn_pressed.clone();
      let last_fn_release_clone = last_fn_release.clone();
      
      // Spawn keyboard event listener in background thread
      std::thread::spawn(move || {
        if let Err(error) = rdev::listen(move |event| {
          match event.event_type {
            rdev::EventType::KeyPress(key) => {
              // Fn key on macOS is typically Key::Function
              if matches!(key, rdev::Key::Function) {
                let mut pressed = fn_pressed_clone.lock().unwrap();
                *pressed = true;
              }
            }
            rdev::EventType::KeyRelease(key) => {
              if matches!(key, rdev::Key::Function) {
                let mut pressed = fn_pressed_clone.lock().unwrap();
                if *pressed {
                  *pressed = false;
                  
                  // Check if it was a quick tap (not held down)
                  let mut last_release = last_fn_release_clone.lock().unwrap();
                  let now = Instant::now();
                  if now.duration_since(*last_release) > Duration::from_millis(300) {
                    // Toggle window
                    let _ = h.emit_all("global-shortcut-pressed", "");
                    if w.is_visible().unwrap_or(false) {
                      let _ = w.hide();
                    } else {
                      let _ = w.show();
                      let _ = w.set_focus();
                    }
                  }
                  *last_release = now;
                }
              }
            }
            _ => {}
          }
        }) {
          eprintln!("Error listening to keyboard events: {:?}", error);
        }
      });
      
      Ok(())
    })
    .manage(AudioCapture::new())
    .invoke_handler(tauri::generate_handler![
      audio::start_recording,
      audio::stop_recording
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
