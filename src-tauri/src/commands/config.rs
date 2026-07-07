use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

/// Tutorial script seeded into an empty scripts folder on first run (Unix). It
/// is a working `inline` command whose printed line becomes its live subtitle,
/// and its header documents every supported directive.
const TUTORIAL_SH: &str = r#"#!/bin/bash
# @raycast.schemaVersion 1
# @raycast.title Script Tutorial
# @raycast.description Open this file to learn how to add your own commands
# @raycast.icon note
# @raycast.mode inline
# @vicinae.refreshTime 1h
# @vicinae.keywords ["tutorial", "help", "scripts", "example", "docs"]
#
# ---------------------------------------------------------------------------
#  Commandeer script commands
# ---------------------------------------------------------------------------
#  Drop any executable script in this folder and it appears in the palette.
#  The header comments above configure how it shows up. Everything is
#  optional -- with no directives, the file name becomes the title.
#
#  Supported directives (@raycast.* or @vicinae.*):
#
#    @raycast.title            Name shown in the palette
#    @raycast.description      Subtitle / detail text
#    @raycast.icon             A named icon: terminal, folder, note, clock, ...

#    @raycast.mode             inline | silent | fullOutput  (badge in the row)
#    @vicinae.refreshTime      For inline mode: re-run every 5s / 2m / 1h and
#                              show the latest stdout live in the row
#    @vicinae.needsConfirmation true   Ask before running (destructive actions)
#    @vicinae.keywords         JSON array of extra search terms
#    @raycast.argument1        JSON like {"type":"text","placeholder":"name"} --
#                              up to argument3, prompts for input before running
#
#  This script runs in "inline" mode, so the line it echoes below is shown as
#  its subtitle. Edit it, copy it, or delete it once you are comfortable --
#  Commandeer re-scans this folder every time the palette opens.
# ---------------------------------------------------------------------------

echo "Edit tutorial.sh in your scripts folder to build your own commands"
"#;

/// Windows (PowerShell) counterpart of [`TUTORIAL_SH`].
const TUTORIAL_PS1: &str = r#"# @raycast.schemaVersion 1
# @raycast.title Script Tutorial
# @raycast.description Open this file to learn how to add your own commands
# @raycast.icon note
# @raycast.mode inline
# @vicinae.refreshTime 1h
# @vicinae.keywords ["tutorial", "help", "scripts", "example", "docs"]
#
# ---------------------------------------------------------------------------
#  Commandeer script commands
# ---------------------------------------------------------------------------
#  Drop a .ps1 script in this folder and it appears in the palette. The header
#  comments above configure how it shows up. Everything is optional -- with no
#  directives, the file name becomes the title.
#
#  Supported directives (@raycast.* or @vicinae.*):
#
#    @raycast.title            Name shown in the palette
#    @raycast.description      Subtitle / detail text
#    @raycast.icon             A named icon: terminal, folder, note, clock, ...

#    @raycast.mode             inline | silent | fullOutput  (badge in the row)
#    @vicinae.refreshTime      For inline mode: re-run every 5s / 2m / 1h and
#                              show the latest stdout live in the row
#    @vicinae.needsConfirmation true   Ask before running (destructive actions)
#    @vicinae.keywords         JSON array of extra search terms
#    @raycast.argument1        JSON like {"type":"text","placeholder":"name"} --
#                              up to argument3, prompts for input before running
#
#  This script runs in "inline" mode, so the line it prints below is shown as
#  its subtitle. Edit it, copy it, or delete it once you are comfortable --
#  Commandeer re-scans this folder every time the palette opens.
# ---------------------------------------------------------------------------

Write-Output "Edit tutorial.ps1 in your scripts folder to build your own commands"
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub scripts_dir: String,
    /// Used by the testing-branch build (file search); round-tripped here so
    /// switching between builds doesn't drop it from config.json.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_paths: Option<Vec<String>>,
    /// Theme name ('Tokyo Night', 'Light', …); legacy values 'dark'/'light' still resolve
    #[serde(default)]
    pub theme: Option<String>,
    /// Window transparency: 0.0 (fully opaque) to 1.0 (fully transparent)
    #[serde(default)]
    pub transparency: Option<f64>,
    /// Global hotkey that toggles the palette (e.g. "Ctrl+Space")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_hotkey: Option<String>,
    /// Alternate global hotkey used in game mode (e.g. "Alt+Space")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_hotkey_game: Option<String>,
    /// Global hotkey that starts the region screenshot (default "Insert" on
    /// Windows; macOS has no default because PrintScreen keys don't exist.
    /// Linux uses a managed COSMIC binding instead.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot_hotkey: Option<String>,
    /// Alt-drag window management: hold Alt and drag to move any window, Alt +
    /// right-drag to resize it (Hyprland-style). Windows/macOS only; None/false
    /// = off. Applied at startup and toggled from Settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_drag: Option<bool>,
    /// Palette scale factor (CSS zoom applied to the whole palette). 1.0 =
    /// default size; the Settings slider maps 0–100% onto 0.5×–1.5× (50% = 1.0×).
    /// None = 1.0. Persisted here so it survives across builds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_scale: Option<f64>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            scripts_dir: String::new(),
            search_paths: None,
            theme: None,
            transparency: None,
            global_hotkey: None,
            global_hotkey_game: None,
            screenshot_hotkey: None,
            window_drag: None,
            palette_scale: None,
        }
    }
}

fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    Ok(data_dir.join("config.json"))
}

/// Where to look for scripts when the user hasn't configured a directory.
///
/// Prefer a `scripts` folder found by walking up from the executable's
/// directory (handles both dev builds and the packaged `bin/` layout), then
/// fall back to `<home>/commandeer/scripts`, created on demand so there's
/// always a place to drop scripts. Stored with forward slashes for
/// cross-platform consistency.
fn default_scripts_dir(app: &tauri::AppHandle) -> String {
    fn find_scripts_dir() -> Option<std::path::PathBuf> {
        if let Ok(exe) = std::env::current_exe() {
            let mut dir = exe.parent();
            while let Some(d) = dir {
                let candidate = d.join("scripts");
                if candidate.is_dir() {
                    return Some(candidate);
                }
                dir = d.parent();
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            let candidate = cwd.join("scripts");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
        None
    }

    if let Some(dir) = find_scripts_dir() {
        return dir.to_string_lossy().replace('\\', "/");
    }

    let dir = match app.path().home_dir() {
        Ok(home) => home.join("commandeer").join("scripts"),
        Err(_) => return String::new(),
    };
    let _ = fs::create_dir_all(&dir);
    dir.to_string_lossy().replace('\\', "/")
}

/// Ensure the configured scripts directory exists and, exactly once, seed a
/// tutorial script so a fresh install has a working example that documents the
/// `@raycast.*` / `@vicinae.*` directive format. A hidden marker file records
/// that we've seeded, so deleting the tutorial afterwards never brings it back.
/// Best-effort: any FS error is ignored (the app still runs without scripts).
pub fn ensure_scripts_seeded(app: &tauri::AppHandle) {
    let dir = PathBuf::from(load_config(app).scripts_dir);
    if dir.as_os_str().is_empty() {
        return;
    }
    let _ = fs::create_dir_all(&dir);

    // Seed at most once, ever — the marker survives the user deleting the file.
    let marker = dir.join(".commandeer-seeded");
    if marker.exists() {
        return;
    }

    // Windows scripts are PowerShell; Unix uses a shell script.
    let (name, body) = if cfg!(target_os = "windows") {
        ("tutorial.ps1", TUTORIAL_PS1)
    } else {
        ("tutorial.sh", TUTORIAL_SH)
    };
    let script = dir.join(name);
    if !script.exists() {
        if fs::write(&script, body).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&script, fs::Permissions::from_mode(0o755));
            }
        }
    }
    let _ = fs::write(&marker, b"seeded by commandeer\n");
}

/// Synchronous, lenient config read for startup code paths (e.g. applying the
/// window-drag setting before the webview is up). Returns defaults on any
/// missing-file / read / parse error rather than failing.
pub fn load_config(app: &tauri::AppHandle) -> AppConfig {
    let mut config = config_path(app)
        .ok()
        .filter(|p| p.exists())
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|raw| serde_json::from_str::<AppConfig>(&raw).ok())
        .unwrap_or_default();
    if config.scripts_dir.is_empty() {
        config.scripts_dir = default_scripts_dir(app);
    }
    config
}

#[tauri::command]
pub async fn read_config(app: tauri::AppHandle) -> Result<AppConfig, String> {
    let path = config_path(&app)?;
    let mut config = if path.exists() {
        let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&raw).map_err(|e| e.to_string())?
    } else {
        AppConfig::default()
    };
    if config.scripts_dir.is_empty() {
        config.scripts_dir = default_scripts_dir(&app);
    }
    Ok(config)
}

#[tauri::command]
pub async fn write_config(app: tauri::AppHandle, config: AppConfig) -> Result<(), String> {
    let path = config_path(&app)?;
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}
