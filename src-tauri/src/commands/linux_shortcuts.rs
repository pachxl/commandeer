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

/// Split a "Ctrl+Shift+Space"-style binding into (modifiers, key).
/// Modifier names are normalised to the COSMIC/GNOME capitalisation
/// (Ctrl, Alt, Shift, Super). Returns None if the string is empty.
fn parse_binding(s: &str) -> Option<(Vec<&'static str>, String)> {
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return None;
    }
    let key = parts.last().unwrap().to_lowercase();
    let mut mods: Vec<&'static str> = Vec::new();
    for part in &parts[..parts.len() - 1] {
        let m = match part.to_lowercase().as_str() {
            "ctrl" | "control" => "Ctrl",
            "alt" => "Alt",
            "shift" => "Shift",
            "win" | "meta" | "cmd" | "super" => "Super",
            _ => return None, // unknown modifier → bail, let the caller skip
        };
        mods.push(m);
    }
    Some((mods, key))
}

/// COSMIC RON modifier list: `[Ctrl, Shift]` or `[]` for none.
fn cosmic_modifiers(mods: &[&str]) -> String {
    if mods.is_empty() {
        return "[]".to_string();
    }
    format!("[{}]", mods.join(", "))
}

/// GNOME gsettings binding: `<Ctrl><Shift>space` or `space` for no modifiers.
fn gnome_binding(mods: &[&str], key: &str) -> String {
    let mut s = String::new();
    for m in mods {
        s.push_str(&format!("<{m}>"));
    }
    s.push_str(key);
    s
}

/// Sync the toggle and PrtScn-screenshot bindings with whichever desktop is
/// running, using the base + game hotkey strings from config.json so the
/// user-edited hotkeys are reflected in the COSMIC/GNOME bindings. Never fatal.
pub fn update_toggle_shortcut_with(base: &str, game: &str, game_mode: bool) {
    let hotkey = if game_mode { game } else { base };
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_lowercase();
    if desktop.contains("cosmic") {
        update_cosmic_shortcut(hotkey);
    } else if desktop.contains("gnome") {
        update_gnome_shortcuts(hotkey);
    }
}

/// COSMIC custom keybindings live in one RON file. Mirrors the configured
/// toggle hotkey. Only our own entries are touched; any other custom shortcuts
/// are preserved.
fn update_cosmic_shortcut(hotkey: &str) {
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

    let Some((mods, key)) = parse_binding(hotkey) else {
        return; // unparseable binding — leave the existing one intact
    };
    let our_line = format!(
        "    (modifiers: {}, key: \"{}\", description: Some(\"Toggle Commandeer\")): Spawn(\"{exe}\"),",
        cosmic_modifiers(&mods),
        key,
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
fn update_gnome_shortcuts(hotkey: &str) {
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
    let (mods, key) = match parse_binding(hotkey) {
        Some(mk) => mk,
        None => return, // unparseable binding — leave the existing one intact
    };
    let binding = gnome_binding(&mods, &key);
    gsettings_set(&toggle_schema, "name", "Toggle Commandeer");
    gsettings_set(&toggle_schema, "command", &exe);
    gsettings_set(&toggle_schema, "binding", &binding);

    let shot_schema = format!("{ENTRY_SCHEMA}:{SCREENSHOT_PATH}");
    gsettings_set(&shot_schema, "name", "Commandeer Screenshot");
    gsettings_set(&shot_schema, "command", &format!("{exe} commandeer://screenshot"));
    gsettings_set(&shot_schema, "binding", "Print");
}
