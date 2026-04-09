use std::fs;
use std::path::Path;

#[tauri::command]
pub async fn update_workspace(
    workspace_path: String,
    resource_path: String,
) -> Result<(), String> {
    if workspace_path.is_empty() {
        return Err("Workspace file is not configured".into());
    }

    let path = Path::new(&workspace_path);

    // Read existing or start fresh
    let mut value: serde_json::Value = if path.exists() {
        let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&raw).map_err(|e| format!("Failed to parse workspace file: {e}"))?
    } else {
        serde_json::json!({ "folders": [], "settings": {} })
    };

    let folders = value
        .get_mut("folders")
        .and_then(|f| f.as_array_mut())
        .ok_or("workspace file missing 'folders' array")?;

    // Avoid duplicates
    let already_exists = folders.iter().any(|f| {
        f.get("path")
            .and_then(|p| p.as_str())
            .map(|p| p == resource_path)
            .unwrap_or(false)
    });

    if !already_exists {
        folders.push(serde_json::json!({ "path": resource_path }));
    }

    let json = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}
