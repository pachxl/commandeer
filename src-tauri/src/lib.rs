mod commands;

use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Distance (logical px) from the top of the screen to the top of the palette on
/// Wayland. The surface is anchored to the top edge, so this stays fixed while
/// the height grows downward.
#[cfg(not(target_os = "windows"))]
const PALETTE_TOP_MARGIN: i32 = 150;

/// Resize the palette to `height` logical px. On the Wayland layer-shell surface
/// the size is taken from the GTK window's size request (it has no anchors), and
/// changing it reconfigures the surface in place — no unmap, no flicker. On
/// Windows the frontend resizes via setSize instead, so this is a no-op there.
#[tauri::command]
fn resize_palette(app: tauri::AppHandle, height: i32) {
    #[cfg(not(target_os = "windows"))]
    {
        use gtk::prelude::*;
        if let Some(win) = app.get_webview_window("palette") {
            if let Ok(gtk_win) = win.gtk_window() {
                gtk_win.set_size_request(669, height.max(1));
            }
        }
    }
    #[cfg(target_os = "windows")]
    let _ = (&app, height);
}

/// Center the palette horizontally on the monitor under the mouse cursor,
/// with its top at ~20% of that monitor's height (Raycast opens where you
/// are working, not on the primary display). Positioning happens once per
/// show — resizes afterwards only move the bottom edge, which keeps typing
/// smooth.
#[cfg(target_os = "windows")]
fn position_on_cursor_monitor(win: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    unsafe {
        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_err() {
            return;
        }
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return;
        }
        let work = info.rcWork;
        let width = win.outer_size().map(|s| s.width as i32).unwrap_or(669);
        let x = work.left + (work.right - work.left - width) / 2;
        let y = work.top + (work.bottom - work.top) / 5;
        let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    }
}

/// Show the palette if hidden, hide it if visible.
fn toggle_palette(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("palette") {
        let visible = win.is_visible().unwrap_or(false);
        if visible {
            let _ = win.hide();
        } else {
            // Remember where the user was so paste-style actions can return there.
            commands::paste::capture_foreground();
            // Snapshot the focused Explorer folder now (resolves on a worker
            // thread) so the frontend's Search Folder check is instant.
            commands::explorer::capture_location();
            #[cfg(target_os = "windows")]
            position_on_cursor_monitor(&win);
            let _ = win.show();
            let _ = win.set_focus();
        }
    }
}

/// On Linux the X11 global shortcut is unreliable under Wayland, so the working
/// trigger is a COSMIC custom keybinding that re-launches the binary (single
/// instance, which toggles the palette). We manage that binding here so it
/// mirrors the Windows shortcut: Ctrl+Space normally, Alt+Space in game mode.
/// Only our own entry is touched; any other custom shortcuts are preserved.
#[cfg(not(target_os = "windows"))]
fn update_cosmic_shortcut(game_mode: bool) {
    let home = match std::env::var_os("HOME") {
        Some(h) => h,
        None => return,
    };
    let dir = std::path::Path::new(&home)
        .join(".config/cosmic/com.system76.CosmicSettings.Shortcuts/v1");
    let file = dir.join("custom");

    let exe = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(_) => return,
    };

    let modifier = if game_mode { "Alt" } else { "Ctrl" };
    let our_line = format!(
        "    (modifiers: [{modifier}], key: \"space\", description: Some(\"Toggle Commandeer\")): Spawn(\"{exe}\"),"
    );

    // Preserve unrelated custom shortcuts; replace only our binding (any modifier).
    let mut kept: Vec<String> = Vec::new();
    if let Ok(existing) = std::fs::read_to_string(&file) {
        for line in existing.lines() {
            let trimmed = line.trim();
            if trimmed == "{" || trimmed == "}" || trimmed.is_empty() {
                continue;
            }
            if line.contains(&exe) {
                continue;
            }
            kept.push(line.to_string());
        }
    }

    let mut out = String::from("{\n");
    out.push_str(&our_line);
    out.push('\n');
    for line in kept {
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str("}\n");

    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&file, out);
}

#[tauri::command]
fn set_game_mode(enabled: bool, app: tauri::AppHandle) -> Result<(), String> {
    let ctrl_space = Shortcut::new(Some(Modifiers::CONTROL), Code::Space);
    let alt_space = Shortcut::new(Some(Modifiers::ALT), Code::Space);

    #[cfg(target_os = "windows")]
    {
        if enabled {
            let _ = app.global_shortcut().unregister(ctrl_space);
            app.global_shortcut().register(alt_space).map_err(|e| e.to_string())?;
        } else {
            let _ = app.global_shortcut().unregister(alt_space);
            app.global_shortcut().register(ctrl_space).map_err(|e| e.to_string())?;
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Best-effort X11 registration (works on X11 sessions, no-op under Wayland);
        // the COSMIC binding below is the reliable Wayland path.
        if enabled {
            let _ = app.global_shortcut().unregister(ctrl_space);
            let _ = app.global_shortcut().register(alt_space);
        } else {
            let _ = app.global_shortcut().unregister(alt_space);
            let _ = app.global_shortcut().register(ctrl_space);
        }
        update_cosmic_shortcut(enabled);
    }

    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        // Must be registered first: a second launch toggles the running palette
        // (the reliable trigger on Wayland, where the X11 global shortcut may not fire).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            toggle_palette(app);
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        toggle_palette(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|win, event| {
            if win.label() == "palette" {
                if let WindowEvent::Focused(false) = event {
                    // Auto-hide when focus is lost (click-away to dismiss).
                    // Set COMMANDEER_NO_AUTOHIDE=1 to disable (useful for debugging
                    // or on compositors with unusual focus behaviour).
                    if std::env::var_os("COMMANDEER_NO_AUTOHIDE").is_none() {
                        let _ = win.hide();
                    }
                }
            }
        })
        .setup(|app| {
            // Self-hosted file index (SQLite + FTS5) backing the find: search.
            let file_index = commands::file_index::FileIndex::new(app.app_handle())?;
            let file_index_clone = file_index.clone();
            app.manage(file_index);
            commands::file_index::start_index_manager(app.app_handle().clone(), file_index_clone);

            // Encrypted SQLite clipboard history. The monitor needs the db, so
            // create it first and keep a managed clone for commands.
            let clipboard_db = commands::clipboard::ClipboardDb::new(app.app_handle())?;
            app.manage(clipboard_db.clone());
            commands::clipboard::start_monitor(app.app_handle().clone(), clipboard_db);

            // Default trigger: Ctrl+Space. Game mode (Alt+Space) is applied via set_game_mode.
            #[cfg(target_os = "windows")]
            app.global_shortcut().register(Shortcut::new(Some(Modifiers::CONTROL), Code::Space))?;

            #[cfg(not(target_os = "windows"))]
            {
                // Best-effort on X11; harmless no-op under Wayland.
                let _ = app
                    .global_shortcut()
                    .register(Shortcut::new(Some(Modifiers::CONTROL), Code::Space));
                // Ensure a working default COSMIC binding even before the frontend
                // calls set_game_mode; the frontend then refines it for game mode.
                update_cosmic_shortcut(false);

                // Turn the palette into a wlr-layer-shell surface (must happen
                // before it is first shown/mapped). As an overlay it renders
                // transparent areas invisibly (no toplevel border/tint) and can
                // be resized in place without the unmap "flash" a normal toplevel
                // needs on Wayland.
                if std::env::var_os("WAYLAND_DISPLAY").is_some() {
                    use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
                    if let Some(win) = app.get_webview_window("palette") {
                        if let Ok(gtk_win) = win.gtk_window() {
                            use gtk::prelude::*;
                            // init_layer_shell must run before the window is
                            // realized (it isn't yet — the window is created
                            // hidden); unrealize defensively in case that changes.
                            if gtk_win.is_realized() {
                                gtk_win.unrealize();
                            }
                            gtk_win.init_layer_shell();
                            gtk_win.set_layer(Layer::Overlay);
                            gtk_win.set_namespace("commandeer");
                            // OnDemand: focusable when clicked, but click-away
                            // still moves focus so our auto-hide fires.
                            gtk_win.set_keyboard_mode(KeyboardMode::OnDemand);
                            // Anchor the TOP edge only: the top stays a fixed
                            // distance below the screen top and the surface grows
                            // downward as its height changes (anchoring a single
                            // edge leaves the perpendicular axis centered, so it
                            // stays horizontally centered). Anchoring to one edge
                            // does NOT stretch the surface — its size still comes
                            // from the size request below.
                            gtk_win.set_anchor(Edge::Top, true);
                            // set_layer_shell_margin (not set_margin) — the trait
                            // renames it to avoid clashing with GTK's
                            // Widget::set_margin from gtk::prelude.
                            gtk_win.set_layer_shell_margin(Edge::Top, PALETTE_TOP_MARGIN);
                            // Force a concrete initial size so the surface isn't
                            // mapped at 0; the frontend then sets the real height
                            // via resize_palette.
                            gtk_win.set_size_request(669, 300);
                        }
                    }
                }
            }

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
            commands::claude::claude_usage,
            commands::store::data_dir,
            commands::store::read_snippets,
            commands::store::write_snippets,
            commands::store::read_quicklinks,
            commands::store::write_quicklinks,
            commands::store::read_overrides,
            commands::store::write_overrides,
            commands::store::read_themes,
            commands::paste::paste_to_previous,
            commands::clipboard::read_clipboard_history,
            commands::clipboard::clear_clipboard_history,
            commands::clipboard::write_clipboard_text,
            commands::rates::get_rates,
            commands::explorer::explorer_location,
            commands::explorer::list_files_recursive,
            commands::search::search_files,
            commands::search::file_info,
            commands::search::path_icon,
            commands::file_index::search_indexed_files,
            commands::process::list_processes,
            commands::process::kill_process,
            commands::stats::system_stats,
            commands::window::set_window_transparency,
            set_game_mode,
            resize_palette,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
