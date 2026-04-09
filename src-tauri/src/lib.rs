mod commands;

use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        if let Some(win) = app.get_webview_window("palette") {
                            let visible = win.is_visible().unwrap_or(false);
                            if visible {
                                let _ = win.hide();
                            } else {
                                let _ = win.show();
                                let _ = win.set_focus();
                            }
                        }
                    }
                })
                .build(),
        )
        .on_window_event(|win, event| {
            if win.label() == "palette" {
                if let WindowEvent::Focused(false) = event {
                    let _ = win.hide();
                }
            }
        })
        .setup(|app| {
            app.global_shortcut().register(Shortcut::new(Some(Modifiers::ALT), Code::Space))?;
            app.global_shortcut().register(Shortcut::new(Some(Modifiers::CONTROL), Code::Space))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::config::read_config,
            commands::config::write_config,
            commands::fs::list_scripts,
            commands::fs::run_script,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
