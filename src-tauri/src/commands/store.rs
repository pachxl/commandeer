use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    pub id: String,
    pub keyword: String,
    pub text: String,
}

fn app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn snippets_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("snippets.json"))
}

fn read_json_file<T>(path: &PathBuf) -> Result<Vec<T>, String>
where
    T: for<'de> Deserialize<'de>,
{
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

fn write_json_file<T>(path: &PathBuf, value: &Vec<T>) -> Result<(), String>
where
    T: Serialize,
{
    let json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn data_dir(app: tauri::AppHandle) -> Result<String, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().replace('\\', "/"))
}

#[tauri::command]
pub async fn read_snippets(app: tauri::AppHandle) -> Result<Vec<Snippet>, String> {
    read_json_file(&snippets_path(&app)?)
}

#[tauri::command]
pub async fn write_snippets(app: tauri::AppHandle, snippets: Vec<Snippet>) -> Result<(), String> {
    write_json_file(&snippets_path(&app)?, &snippets)
}

/// User theme: a name plus the CSS variable map applied to :root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub variables: HashMap<String, String>,
}

/// Themes from `<app-data>/themes/*.json` (Vicinae-style). The directory is
/// created on demand so users have a place to drop files.
#[tauri::command]
pub async fn read_themes(app: tauri::AppHandle) -> Result<Vec<Theme>, String> {
    let dir = app_data_dir(&app)?.join("themes");
    let _ = fs::create_dir_all(&dir);
    let mut out: Vec<Theme> = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            if let Ok(theme) = serde_json::from_str::<Theme>(&raw) {
                out.push(theme);
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}
