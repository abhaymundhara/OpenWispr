#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager};

fn main() {
  tauri::Builder::default()
    .setup(|app| {
      let handle = app.handle();
      // Register a global shortcut (CmdOrCtrl+Shift+Space) — cross-platform via Tauri
      #[allow(unused_must_use)]
      {
        let mut shortcut_manager = handle.global_shortcut_manager();
        // Register and send event to frontend when triggered
        let h = handle.clone();
        shortcut_manager.register("Ctrl+Space", move || {
          let _ = h.emit_all("global-shortcut-pressed", "");
        });
      }
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
