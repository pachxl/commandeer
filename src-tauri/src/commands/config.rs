use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub scripts_dir: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            scripts_dir: String::new(),
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
/// `<home>/commandeer/commands`, created on demand so there's always a place
/// to drop scripts. Stored with forward slashes for cross-platform consistency.
fn default_scripts_dir(app: &tauri::AppHandle) -> String {
    let dir = match app.path().home_dir() {
        Ok(home) => home.join("commandeer").join("commands"),
        Err(_) => return String::new(),
    };
    let _ = fs::create_dir_all(&dir);
    dir.to_string_lossy().replace('\\', "/")
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
