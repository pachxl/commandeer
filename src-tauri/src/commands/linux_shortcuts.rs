//! Desktop-specific global-shortcut registration on Linux.
//!
//! The X11 grab made by tauri-plugin-global-shortcut is unreliable-to-dead on
//! Wayland, so the working trigger is a compositor-managed keybinding that
//! re-launches the binary: the single-instance plugin turns that into a
//! palette toggle (and `commandeer://screenshot` into a capture). This module
//! writes those bindings for the desktops whose config we can manage safely:
//!
//! - COSMIC: the custom-shortcuts RON file (replaces only our own lines)
//! - GNOME: two gsettings custom keybindings (`.../commandeer/` and
//!   `.../commandeer-screenshot/`), which show up in Settings → Keyboard
//! - KDE and others: left alone — kglobalshortcutsrc surgery is fragile
//!   across Plasma versions, so the user binds the exe to a shortcut
//!   manually (the plugin registration still covers X11 sessions).

/// Sync the toggle (Ctrl+Space / Alt+Space in game mode) and PrtScn-screenshot
/// bindings with whichever desktop is running. Never fatal.
pub fn update_toggle_shortcut(game_mode: bool) {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_lowercase();
    if desktop.contains("cosmic") {
        update_cosmic_shortcut(game_mode);
    } else if desktop.contains("gnome") {
        update_gnome_shortcuts(game_mode);
    }
}

/// COSMIC custom keybindings live in one RON file. Mirrors the Windows
/// shortcut: Ctrl+Space normally, Alt+Space in game mode. Only our own
/// entries are touched; any other custom shortcuts are preserved.
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
    // Second managed binding: PrtScn relaunches us with the screenshot deep
    // link (cosmic-comp spawns via shell, so the appended arg survives).
    // NOTE: both managed lines embed the exe path bare; a path containing
    // spaces would break Spawn's word-splitting.
    let screenshot_line = format!(
        "    (modifiers: [], key: \"Print\", description: Some(\"Commandeer Screenshot\")): Spawn(\"{exe} commandeer://screenshot\"),"
    );

    // Preserve unrelated custom shortcuts; replace only our bindings (the
    // exe-path filter below drops every line we manage).
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
    out.push_str(&screenshot_line);
    out.push('\n');
    for line in kept {
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str("}\n");

    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&file, out);
}

/// GNOME custom keybindings: register our two entries in the media-keys
/// custom-keybindings list (preserving everything else) and write their
/// name/command/binding keys. Same relaunch-to-toggle model as COSMIC.
fn update_gnome_shortcuts(game_mode: bool) {
    const LIST_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys";
    const ENTRY_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
    const TOGGLE_PATH: &str =
        "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/commandeer/";
    const SCREENSHOT_PATH: &str =
        "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/commandeer-screenshot/";

    let exe = match std::env::current_exe() {
        Ok(p) => p.to_string_lossy().into_owned(),
        Err(_) => return,
    };

    let gsettings_get = |args: &[&str]| -> Option<String> {
        let out = std::process::Command::new("gsettings")
            .args(args)
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    let gsettings_set = |schema_path: &str, key: &str, value: &str| {
        let _ = std::process::Command::new("gsettings")
            .args(["set", schema_path, key, value])
            .output();
    };

    // Ensure both paths are in the custom-keybindings list. The value prints
    // as `@as []` when empty or `['/a/', '/b/']` otherwise.
    let Some(current) = gsettings_get(&["get", LIST_SCHEMA, "custom-keybindings"]) else {
        return; // no gsettings — nothing to manage
    };
    let mut entries: Vec<String> = current
        .trim_start_matches("@as")
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let mut changed = false;
    for path in [TOGGLE_PATH, SCREENSHOT_PATH] {
        if !entries.iter().any(|e| e == path) {
            entries.push(path.to_string());
            changed = true;
        }
    }
    if changed {
        let list = format!(
            "[{}]",
            entries
                .iter()
                .map(|e| format!("'{e}'"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        gsettings_set(LIST_SCHEMA, "custom-keybindings", &list);
    }

    let toggle_schema = format!("{ENTRY_SCHEMA}:{TOGGLE_PATH}");
    let binding = if game_mode { "<Alt>space" } else { "<Ctrl>space" };
    gsettings_set(&toggle_schema, "name", "Toggle Commandeer");
    gsettings_set(&toggle_schema, "command", &exe);
    gsettings_set(&toggle_schema, "binding", binding);

    let shot_schema = format!("{ENTRY_SCHEMA}:{SCREENSHOT_PATH}");
    gsettings_set(&shot_schema, "name", "Commandeer Screenshot");
    gsettings_set(&shot_schema, "command", &format!("{exe} commandeer://screenshot"));
    gsettings_set(&shot_schema, "binding", "Print");
}
