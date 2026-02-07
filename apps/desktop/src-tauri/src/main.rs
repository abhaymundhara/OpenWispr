#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use device_query::{DeviceQuery, DeviceState, Keycode};

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
      println!("Press Control key to toggle dictation");
      println!("==============================================\n");
      
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
          let control_pressed = keys.contains(&Keycode::LControl) || 
                               keys.contains(&Keycode::RControl);
          
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
