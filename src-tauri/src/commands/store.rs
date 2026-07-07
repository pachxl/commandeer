use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quicklink {
    pub id: String,
    pub name: String,
    pub url: String,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
}

fn app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn quicklinks_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("quicklinks.json"))
}

fn notes_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("notes.json"))
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

/// Seeded on first run so quicklinks (and their `{query}` argument flow)
/// aren't an empty feature. Fixed ids keep frecency/overrides stable.
fn default_quicklinks() -> Vec<Quicklink> {
    let seed = |id: &str, name: &str, url: &str| Quicklink {
        id: id.to_string(),
        name: name.to_string(),
        url: url.to_string(),
        icon: None,
    };
    vec![
        seed("seed-google", "Search Google", "https://www.google.com/search?q={query}"),
        seed("seed-youtube", "Search YouTube", "https://www.youtube.com/results?search_query={query}"),
        seed("seed-github", "Search GitHub", "https://github.com/search?q={query}"),
        seed("seed-wikipedia", "Search Wikipedia", "https://en.wikipedia.org/wiki/Special:Search?search={query}"),
    ]
}

#[tauri::command]
pub async fn read_quicklinks(app: tauri::AppHandle) -> Result<Vec<Quicklink>, String> {
    let path = quicklinks_path(&app)?;
    if !path.exists() {
        let seeds = default_quicklinks();
        let _ = write_json_file(&path, &seeds);
        return Ok(seeds);
    }
    read_json_file(&path)
}

#[tauri::command]
pub async fn write_quicklinks(
    app: tauri::AppHandle,
    quicklinks: Vec<Quicklink>,
) -> Result<(), String> {
    write_json_file(&quicklinks_path(&app)?, &quicklinks)
}

#[tauri::command]
pub async fn read_notes(app: tauri::AppHandle) -> Result<Vec<Note>, String> {
    read_json_file(&notes_path(&app)?)
}

#[tauri::command]
pub async fn write_notes(app: tauri::AppHandle, notes: Vec<Note>) -> Result<(), String> {
    write_json_file(&notes_path(&app)?, &notes)
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
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

/// Per-command user overrides (alias, pinned, hotkey), keyed by command id.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommandOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show_at_root: Option<bool>,
}

fn overrides_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("overrides.json"))
}

#[tauri::command]
pub async fn read_overrides(
    app: tauri::AppHandle,
) -> Result<HashMap<String, CommandOverride>, String> {
    let path = overrides_path(&app)?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn write_overrides(
    app: tauri::AppHandle,
    overrides: HashMap<String, CommandOverride>,
) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&overrides).map_err(|e| e.to_string())?;
    fs::write(overrides_path(&app)?, json).map_err(|e| e.to_string())
}
