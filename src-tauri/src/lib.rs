mod commands;

use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

#[tauri::command]
fn set_game_mode(enabled: bool, app: tauri::AppHandle) -> Result<(), String> {
    let ctrl_space = Shortcut::new(Some(Modifiers::CONTROL), Code::Space);
    let alt_space = Shortcut::new(Some(Modifiers::ALT), Code::Space);
    if enabled {
        let _ = app.global_shortcut().unregister(ctrl_space);
        app.global_shortcut().register(alt_space).map_err(|e| e.to_string())?;
    } else {
        let _ = app.global_shortcut().unregister(alt_space);
        app.global_shortcut().register(ctrl_space).map_err(|e| e.to_string())?;
    }
    Ok(())
}

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
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|win, event| {
            if win.label() == "palette" {
                if let WindowEvent::Focused(false) = event {
                    let _ = win.hide();
                }
            }
        })
        .setup(|app| {
            // Default: Ctrl+Space only. Game mode (Alt+Space) enabled dynamically via set_game_mode.
            app.global_shortcut().register(Shortcut::new(Some(Modifiers::CONTROL), Code::Space))?;

            #[cfg(target_os = "windows")]
            {
                use windows::Win32::Foundation::HWND;
                use windows::Win32::Graphics::Dwm::{
                    DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
                };
                if let Some(win) = app.get_webview_window("palette") {
                    if let Ok(hwnd) = win.hwnd() {
                        let pref = DWMWCP_ROUND;
                        unsafe {
                            let _ = DwmSetWindowAttribute(
                                HWND(hwnd.0 as *mut _),
                                DWMWA_WINDOW_CORNER_PREFERENCE,
                                &pref as *const _ as *const _,
                                std::mem::size_of_val(&pref) as u32,
                            );
                        }
                    }
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::config::read_config,
            commands::config::write_config,
            commands::fs::list_scripts,
            commands::fs::run_script,
            set_game_mode,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
