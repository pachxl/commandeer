mod commands;

use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

/// Distance (logical px) from the top of the screen to the top of the palette on
/// Wayland. The surface is anchored to the top edge, so this stays fixed while
/// the height grows downward.
#[cfg(target_os = "linux")]
const PALETTE_TOP_MARGIN: i32 = 150;

/// Resize the palette to `height` logical px. On the Wayland layer-shell surface
/// the size is taken from the GTK window's size request (it has no anchors), and
/// changing it reconfigures the surface in place — no unmap, no flicker. On
/// Windows and macOS the frontend resizes via setSize instead, so this is a
/// no-op there.
#[tauri::command]
fn resize_palette(app: tauri::AppHandle, height: i32) {
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::*;
        if let Some(win) = app.get_webview_window("palette") {
            if let Ok(gtk_win) = win.gtk_window() {
                gtk_win.set_size_request(669, height.max(1));
            }
        }
    }
    // Windows and macOS resize via the frontend's setSize, so this is a no-op.
    #[cfg(not(target_os = "linux"))]
    let _ = (&app, height);
}

/// Runtime environment facts the frontend can't reliably sniff from the user
/// agent — chiefly Wayland vs X11, which decides how the palette resizes.
#[derive(serde::Serialize)]
struct EnvInfo {
    os: &'static str,
    wayland: bool,
    desktop: String,
    home: String,
}

#[tauri::command]
fn env_info() -> EnvInfo {
    EnvInfo {
        os: std::env::consts::OS,
        wayland: std::env::var_os("WAYLAND_DISPLAY").is_some(),
        desktop: std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default(),
        home: std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_default(),
    }
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

/// macOS equivalent of `position_on_cursor_monitor`, built on Tauri's
/// cross-platform monitor APIs instead of Win32. Center horizontally on the
/// display under the cursor with the top at ~20% of that display's work area
/// (below the menu bar). All physical pixels, matching `outer_size` /
/// `set_position`.
#[cfg(target_os = "macos")]
fn position_on_cursor_monitor(win: &tauri::WebviewWindow) {
    let Ok(cursor) = win.cursor_position() else {
        return;
    };
    let monitor = match win.monitor_from_point(cursor.x, cursor.y) {
        Ok(Some(m)) => m,
        _ => match win.primary_monitor() {
            Ok(Some(m)) => m,
            _ => return,
        },
    };
    let area = monitor.work_area();
    let width = win.outer_size().map(|s| s.width as i32).unwrap_or(669);
    let x = area.position.x + (area.size.width as i32 - width) / 2;
    let y = area.position.y + (area.size.height as i32) / 5;
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
}

/// Linux/X11 sessions get no layer-shell surface, so nothing positions the
/// palette at map time; mirror the Windows behavior with portable Tauri APIs
/// (centered on the cursor's monitor, top at ~20% height). No-op on Wayland.
#[cfg(target_os = "linux")]
fn position_palette_x11(app: &tauri::AppHandle, win: &tauri::WebviewWindow) {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return;
    }
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|pos| app.monitor_from_point(pos.x, pos.y).ok().flatten())
        .or_else(|| win.current_monitor().ok().flatten())
        .or_else(|| win.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let mpos = monitor.position();
    let msize = monitor.size();
    let width = win.outer_size().map(|s| s.width as i32).unwrap_or(669);
    let x = mpos.x + (msize.width as i32 - width) / 2;
    let y = mpos.y + msize.height as i32 / 5;
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
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
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            position_on_cursor_monitor(&win);
            #[cfg(target_os = "linux")]
            position_palette_x11(app, &win);
            let _ = win.show();
            let _ = win.set_focus();
        }
    }
}

/// Show and focus the palette (idempotent; unlike `toggle_palette` it never
/// hides). Used by deep links, where "open" should always surface the palette.
pub(crate) fn show_palette(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("palette") {
        commands::paste::capture_foreground();
        commands::explorer::capture_location();
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        position_on_cursor_monitor(&win);
        #[cfg(target_os = "linux")]
        position_palette_x11(app, &win);
        let _ = win.show();
        let _ = win.set_focus();
    }
}

#[tauri::command]
fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())
    } else {
        manager.disable().map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn get_autostart(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// Tray icon with Show / Start at Login / Quit — the only way to quit or
/// rediscover the app once the palette is hidden (especially on macOS, where
/// the app is an Accessory with no Dock icon). On Linux this goes through
/// StatusNotifier/libappindicator (runtime dep: libappindicator-gtk3; COSMIC
/// shows it in the Status Area applet), and failure is non-fatal — the
/// palette still works via the global shortcut / relaunch toggle.
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{CheckMenuItem, MenuBuilder, MenuItem};
    use tauri::tray::TrayIconBuilder;
    use tauri_plugin_autostart::ManagerExt;

    let autostart_on = app.autolaunch().is_enabled().unwrap_or(false);
    let show = MenuItem::with_id(app, "show", "Show Palette", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start at Login",
        true,
        autostart_on,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit Commandeer", true, None::<&str>)?;
    let menu = MenuBuilder::new(app)
        .item(&show)
        .item(&autostart)
        .separator()
        .item(&quit)
        .build()?;

    let autostart_item = autostart.clone();
    let mut tray = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("Commandeer")
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "show" => toggle_palette(app),
            "autostart" => {
                use tauri_plugin_autostart::ManagerExt;
                let manager = app.autolaunch();
                if manager.is_enabled().unwrap_or(false) {
                    let _ = manager.disable();
                } else {
                    let _ = manager.enable();
                }
                let _ = autostart_item.set_checked(manager.is_enabled().unwrap_or(false));
            }
            "quit" => app.exit(0),
            _ => {}
        });
    #[cfg(target_os = "macos")]
    {
        // Menu-bar icons are template images on macOS: alpha-only glyphs the
        // system tints for light/dark menu bars and selection. The colored app
        // icon would read as an opaque square, so use a dedicated monochrome
        // glyph (18pt @2x) and fall back to the app icon only if it fails to
        // decode.
        const TRAY_TEMPLATE: &[u8] = include_bytes!("../icons/tray-template.png");
        match image::load_from_memory(TRAY_TEMPLATE) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                tray = tray
                    .icon(tauri::image::Image::new_owned(rgba.into_raw(), w, h))
                    .icon_as_template(true);
            }
            Err(_) => {
                if let Some(icon) = app.default_window_icon() {
                    tray = tray.icon(icon.clone());
                }
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[tauri::command]
fn set_game_mode(enabled: bool, app: tauri::AppHandle) -> Result<(), String> {
    // Re-register the configured hotkeys (game hotkey when enabled).
    commands::shortcuts::reload_shortcuts(&app, enabled)?;

    #[cfg(target_os = "linux")]
    {
        commands::linux_shortcuts::update_toggle_shortcut(enabled);
    }

    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        // Must be registered first: a second launch toggles the running palette
        // (the reliable trigger on Wayland, where the X11 global shortcut may not fire).
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // A second launch carrying a commandeer:// URL is a deep link: route
            // it instead of toggling. Otherwise it's the "toggle" hotkey path.
            if commands::deeplink::handle_args(app, args.into_iter()) {
                return;
            }
            toggle_palette(app);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    // PrtScn starts the screenshot flow (Windows; on Linux the
                    // COSMIC binding relaunches us with a deep link instead).
                    if commands::shortcuts::is_screenshot_hotkey(*shortcut) {
                        commands::screenshot::start_screenshot_bg(app);
                        return;
                    }
                    // Per-command shortcuts take precedence over the toggle.
                    if let Some(command_id) = commands::shortcuts::is_command_hotkey(*shortcut) {
                        let _ = app.emit("command-hotkey", command_id);
                        return;
                    }
                    toggle_palette(app);
                })
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
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
            // Windows/macOS: dismiss the screenshot overlay on click-away. On
            // Linux the overlay layer's focus semantics are quirky (a spurious
            // unfocus would make the tool unusable), so Esc is the only out.
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            if win.label() == "screenshot" {
                if let WindowEvent::Focused(false) = event {
                    if std::env::var_os("COMMANDEER_NO_AUTOHIDE").is_none()
                        && win.is_visible().unwrap_or(false)
                    {
                        commands::screenshot::cancel_screenshot(win.app_handle().clone());
                    }
                }
            }
        })
        .setup(|app| {
            // Pending screenshot capture (frame path + dimensions).
            app.manage(commands::screenshot::ScreenshotState::default());

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

            // commandeer:// deep links. Register the URI scheme at runtime so
            // dev/portable runs work without an installer, and route any URL the
            // OS hands us. Second-launch URLs are handled in the single-instance
            // callback above; this covers same-process (on_open_url) delivery.
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let _ = app.deep_link().register("commandeer");
                let handle = app.app_handle().clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        commands::deeplink::handle_url(&handle, url.as_str());
                    }
                });
            }

            #[cfg(target_os = "windows")]
            setup_tray(app)?;
            #[cfg(not(target_os = "windows"))]
            if let Err(e) = setup_tray(app) {
                eprintln!("tray unavailable: {e}");
            }

            // Configurable base hotkey (default Ctrl+Space; game mode applied
            // later via set_game_mode) plus any per-command shortcuts.
            commands::shortcuts::setup_shortcuts(app.app_handle())?;

            #[cfg(target_os = "linux")]
            {
                // Ensure a working default desktop keybinding (COSMIC/GNOME)
                // even before the frontend calls set_game_mode; the frontend
                // then refines it for game mode.
                #[cfg(target_os = "linux")]
                commands::linux_shortcuts::update_toggle_shortcut(false);

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

                    // The screenshot overlay is a second layer surface: anchored
                    // to ALL four edges so the compositor stretches it across the
                    // whole output (no size request — unlike the palette above,
                    // its size must come from the anchors). Exclusive keyboard so
                    // Esc always reaches it; exclusive zone -1 so it covers
                    // panels/docks instead of being pushed inside them.
                    if let Some(win) = app.get_webview_window("screenshot") {
                        if let Ok(gtk_win) = win.gtk_window() {
                            use gtk::prelude::*;
                            if gtk_win.is_realized() {
                                gtk_win.unrealize();
                            }
                            gtk_win.init_layer_shell();
                            gtk_win.set_layer(Layer::Overlay);
                            gtk_win.set_namespace("commandeer-screenshot");
                            gtk_win.set_keyboard_mode(KeyboardMode::Exclusive);
                            gtk_win.set_anchor(Edge::Top, true);
                            gtk_win.set_anchor(Edge::Bottom, true);
                            gtk_win.set_anchor(Edge::Left, true);
                            gtk_win.set_anchor(Edge::Right, true);
                            gtk_win.set_exclusive_zone(-1);
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

            #[cfg(target_os = "macos")]
            {
                // A command palette is a background agent: no Dock icon and no
                // Cmd-Tab entry. The tray icon and global hotkey are the entry
                // points (mirrors Raycast/Spotlight).
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);

                // Translucent NSVisualEffect background + rounded corners for the
                // palette — the macOS analogue of the Windows acrylic effect.
                // Best-effort: any failure leaves the plain transparent window.
                use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
                if let Some(win) = app.get_webview_window("palette") {
                    let _ = apply_vibrancy(
                        &win,
                        NSVisualEffectMaterial::HudWindow,
                        Some(NSVisualEffectState::Active),
                        Some(12.0),
                    );
                }

                // The screenshot overlay must cover the *whole* display,
                // including the menu-bar strip. At a normal window level AppKit
                // both draws the menu bar over it and may clamp the frame below
                // the bar (constrainFrameRect), which would misalign the
                // region-to-pixel mapping. Raise it to the screen-saver level
                // (1000) — only normal-level windows are constrained — and let
                // it join every Space, fullscreen apps included. Safe to do
                // once here: hide/show doesn't reset either property.
                if let Some(win) = app.get_webview_window("screenshot") {
                    if let Ok(ns_window) = win.ns_window() {
                        use objc2::runtime::AnyObject;
                        let ns_window = ns_window as *mut AnyObject;
                        unsafe {
                            let _: () = objc2::msg_send![ns_window, setLevel: 1000isize];
                            // canJoinAllSpaces (1<<0) | fullScreenAuxiliary (1<<8)
                            let _: () =
                                objc2::msg_send![ns_window, setCollectionBehavior: (1u64 | (1u64 << 8))];
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
            commands::launcher::list_apps,
            commands::launcher::run_app,
            commands::search::search_files,
            commands::search::file_info,
            commands::search::path_icon,
            commands::fs::read_text_preview,
            commands::file_index::search_indexed_files,
            commands::process::list_processes,
            commands::process::kill_process,
            commands::stats::system_stats,
            commands::system::system_action,
            commands::audio::list_audio_devices,
            commands::audio::get_volume,
            commands::audio::set_volume,
            commands::audio::toggle_mute,
            commands::window::set_window_transparency,
            commands::screenshot::start_screenshot,
            commands::screenshot::show_screenshot_overlay,
            commands::screenshot::reveal_screenshot_overlay,
            commands::screenshot::hide_screenshot_overlay,
            commands::screenshot::finish_screenshot,
            commands::screenshot::cancel_screenshot,
            commands::shortcuts::set_global_hotkey,
            commands::shortcuts::set_screenshot_hotkey,
            commands::shortcuts::set_command_hotkey,
            commands::shortcuts::get_command_hotkey,
            set_autostart,
            get_autostart,
            set_game_mode,
            resize_palette,
            env_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
