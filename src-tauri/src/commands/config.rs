use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

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
/// Prefer a `commands` folder found by walking up from the executable's
/// directory (handles both dev builds and the packaged `bin/` layout), then
/// fall back to `<home>/commandeer/commands`, created on demand so there's
/// always a place to drop scripts. Stored with forward slashes for
/// cross-platform consistency.
fn default_scripts_dir(app: &tauri::AppHandle) -> String {
    fn find_commands_dir() -> Option<std::path::PathBuf> {
        if let Ok(exe) = std::env::current_exe() {
            let mut dir = exe.parent();
            while let Some(d) = dir {
                let candidate = d.join("commands");
                if candidate.is_dir() {
                    return Some(candidate);
                }
                dir = d.parent();
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            let candidate = cwd.join("commands");
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
        None
    }

    if let Some(dir) = find_commands_dir() {
        return dir.to_string_lossy().replace('\\', "/");
    }

    let dir = match app.path().home_dir() {
        Ok(home) => home.join("commandeer").join("commands"),
        Err(_) => return String::new(),
    };
    let _ = fs::create_dir_all(&dir);
    dir.to_string_lossy().replace('\\', "/")
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
