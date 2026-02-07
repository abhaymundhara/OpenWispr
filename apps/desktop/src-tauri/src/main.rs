#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{GlobalShortcutManager, Manager};

fn main() {
  tauri::Builder::default()
    .setup(|app| {
      let handle = app.handle();
      
      // Get the main window and show it when shortcut is pressed
      let window = app.get_window("main").unwrap();
      
      // Register a global shortcut (Ctrl+Space)
      #[allow(unused_must_use)]
      {
        let mut shortcut_manager = handle.global_shortcut_manager();
        let h = handle.clone();
        let w = window.clone();
        
        shortcut_manager.register("Ctrl+Space", move || {
          let _ = h.emit_all("global-shortcut-pressed", "");
          // Toggle window visibility
          if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
          } else {
            let _ = w.show();
            let _ = w.set_focus();
          }
        });
      }
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
