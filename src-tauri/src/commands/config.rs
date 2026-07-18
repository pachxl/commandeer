use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

const LEGACY_IDENTIFIER: &str = "dev.commandeer.app";
const LEGACY_SEED_MARKER: &str = ".commandeer-seeded";

// Exact byte lengths and FNV-1a checksums of the retired starter templates.
// They let upgrades identify pristine generated files without carrying those
// scripts in the binary again or touching a user's edited copy.
const LEGACY_STARTERS: &[(&str, usize, u64)] = &[
    ("current-time.ps1", 274, 0xcba3_1776_26b4_811c),
    ("open-scripts-folder.ps1", 267, 0x9938_f8cb_0773_e679),
    ("tutorial.ps1", 1984, 0xe0cf_3c10_a74c_7147),
    ("current-time.sh", 271, 0x25ba_18d8_68df_67a6),
    ("open-scripts-folder.sh", 450, 0xc665_c98e_f3af_860e),
    ("tutorial.sh", 1993, 0xf212_32ce_dd5c_d208),
];

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    })
}

fn copy_dir_missing(source: &std::path::Path, destination: &std::path::Path) {
    let Ok(entries) = fs::read_dir(source) else {
        return;
    };
    let _ = fs::create_dir_all(destination);
    for entry in entries.flatten() {
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_missing(&source_path, &destination_path);
        } else if !destination_path.exists() {
            let _ = fs::copy(source_path, destination_path);
        }
    }
}

/// Preserve settings, databases, encryption keys, and icon caches after the
/// bundle identifier changed from `dev.commandeer.app` to `dev.commandeer`.
/// Copy-only and non-overwriting: the old directories remain as a rollback,
/// while anything already written under the new identifier wins.
pub fn migrate_legacy_identifier(app: &tauri::AppHandle) {
    let Ok(data_dir) = app.path().app_data_dir() else {
        return;
    };
    let marker = data_dir.join(".identifier-migrated-v1");
    if marker.exists() {
        return;
    }

    if let Some(parent) = data_dir.parent() {
        copy_dir_missing(&parent.join(LEGACY_IDENTIFIER), &data_dir);
    }
    if let Ok(cache_dir) = app.path().app_cache_dir() {
        if let Some(parent) = cache_dir.parent() {
            copy_dir_missing(&parent.join(LEGACY_IDENTIFIER), &cache_dir);
        }
    }

    let _ = fs::create_dir_all(&data_dir);
    let _ = fs::write(marker, b"migrated from dev.commandeer.app\n");
}

fn remove_legacy_starters(dir: &Path) {
    let marker = dir.join(LEGACY_SEED_MARKER);
    if !marker.exists() {
        return;
    }

    for (name, expected_len, expected_hash) in LEGACY_STARTERS {
        let path = dir.join(name);
        if fs::read(&path).is_ok_and(|contents| {
            contents.len() == *expected_len && fnv1a(&contents) == *expected_hash
        }) {
            let _ = fs::remove_file(path);
        }
    }

    // This marker belonged solely to the retired seeding mechanism. Removing
    // it records that cleanup has run; edited starter files are left alone.
    let _ = fs::remove_file(marker);
}

/// Remove untouched examples written by Commandeer versions that seeded the
/// scripts directory at startup. Files the user edited, and all unrelated
/// commands, are preserved.
pub fn cleanup_legacy_seeded_scripts(app: &tauri::AppHandle) {
    let dir = PathBuf::from(load_config(app).scripts_dir);
    if !dir.as_os_str().is_empty() {
        remove_legacy_starters(&dir);
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// Replace Windows Alt+Tab with a monitor-local switcher. Windows only;
    /// None/false = off. Applied at startup and toggled from Settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_monitor_alt_tab: Option<bool>,
    /// Palette scale factor (CSS zoom applied to the whole palette). 1.0 =
    /// default size; the Settings slider maps 0–100% onto 0.5×–1.5× (50% = 1.0×).
    /// None = 1.0. Persisted here so it survives across builds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_scale: Option<f64>,
    /// UI style preset ("Default" or "Onix"). Controls layout, spacing, fonts,
    /// and component treatment; the separate theme setting owns all colors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_style: Option<String>,
}

fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
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

#[cfg(test)]
mod tests {
    use super::{fnv1a, remove_legacy_starters, AppConfig, LEGACY_SEED_MARKER};
    use std::fs;

    #[test]
    fn per_monitor_alt_tab_defaults_to_none_when_missing() {
        let config: AppConfig = serde_json::from_str(r#"{"scripts_dir": "x"}"#).unwrap();
        assert_eq!(config.per_monitor_alt_tab, None);
    }

    #[test]
    fn per_monitor_alt_tab_round_trips_true_and_false() {
        for value in [false, true] {
            let json = format!(r#"{{"scripts_dir": "x", "per_monitor_alt_tab": {value}}}"#);
            let config: AppConfig = serde_json::from_str(&json).unwrap();
            assert_eq!(config.per_monitor_alt_tab, Some(value));
            let serialized = serde_json::to_string(&config).unwrap();
            let reparsed: AppConfig = serde_json::from_str(&serialized).unwrap();
            assert_eq!(reparsed.per_monitor_alt_tab, Some(value));
        }
    }

    #[test]
    fn per_monitor_alt_tab_none_is_omitted_from_the_file() {
        let config = AppConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("per_monitor_alt_tab"));
    }

    #[test]
    fn legacy_cleanup_only_removes_untouched_seeded_files() {
        const CURRENT_TIME_PS1: &str = "# @raycast.schemaVersion 1\n# @raycast.title Current Time\n# @raycast.description Show the current local date and time\n# @raycast.icon clock\n# @raycast.mode inline\n# @vicinae.refreshTime 1m\n# @vicinae.keywords [\"date\", \"time\", \"clock\"]\n\nGet-Date -Format 'ddd dd MMM · HH:mm'\n";
        let dir = std::env::temp_dir().join(format!(
            "commandeer-legacy-seed-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(LEGACY_SEED_MARKER), "seeded by commandeer\n").unwrap();
        assert_eq!(CURRENT_TIME_PS1.len(), 274);
        assert_eq!(fnv1a(CURRENT_TIME_PS1.as_bytes()), 0xcba3_1776_26b4_811c);
        fs::write(dir.join("current-time.ps1"), CURRENT_TIME_PS1).unwrap();
        fs::write(dir.join("tutorial.ps1"), "my edited command\n").unwrap();
        fs::write(dir.join("mine.ps1"), "my command\n").unwrap();

        remove_legacy_starters(&dir);

        assert!(!dir.join(LEGACY_SEED_MARKER).exists());
        assert!(!dir.join("current-time.ps1").exists());
        assert_eq!(
            fs::read_to_string(dir.join("tutorial.ps1")).unwrap(),
            "my edited command\n"
        );
        assert!(dir.join("mine.ps1").exists());

        fs::remove_dir_all(dir).unwrap();
    }
}
