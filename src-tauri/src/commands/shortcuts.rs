//! Configurable global shortcuts: the base palette hotkey plus per-command
//! shortcuts stored in overrides.json.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

use super::config::AppConfig;
use super::store::CommandOverride;

/// Active command hotkeys so we can unregister them on reload.
#[derive(Default)]
struct ActiveShortcuts {
    base: Option<Shortcut>,
    base_game: Option<Shortcut>,
    screenshot: Option<Shortcut>,
    commands: HashMap<String, Shortcut>,
}

fn active() -> &'static Mutex<ActiveShortcuts> {
    static ACTIVE: OnceLock<Mutex<ActiveShortcuts>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(ActiveShortcuts::default()))
}

#[cfg(not(target_os = "macos"))]
const DEFAULT_HOTKEY: &str = "Ctrl+Space";
// macOS: Ctrl+Space is the system input-source switcher and Cmd+Space is
// Spotlight, so neither is usable out of the box. Default to Cmd+Shift+Space,
// which is normally free. (User-configurable via Settings → Global Hotkey.)
#[cfg(target_os = "macos")]
const DEFAULT_HOTKEY: &str = "Cmd+Shift+Space";
const DEFAULT_GAME_HOTKEY: &str = "Alt+Space";
// Insert, not PrintScreen: RegisterHotKey(VK_SNAPSHOT) "succeeds" but never
// fires because PrintScreen emits no WM_KEYDOWN, so WM_HOTKEY is never sent.
// Insert is an ordinary key that RegisterHotKey handles normally.
#[cfg(target_os = "windows")]
const DEFAULT_SCREENSHOT_HOTKEY: &str = "Insert";

/// Parse a human-readable shortcut like "Ctrl+Space" or "Alt+Shift+T" into a
/// Tauri Shortcut. Key names are case-insensitive.
pub fn parse_shortcut(s: &str) -> Result<Shortcut, String> {
    let parts: Vec<&str> = s
        .split('+')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        return Err(format!("empty shortcut: {}", s));
    }
    let key = parts.last().unwrap().to_lowercase();
    let code = parse_code(&key)?;
    let mut modifiers = Modifiers::empty();
    for part in &parts[..parts.len() - 1] {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "alt" => modifiers |= Modifiers::ALT,
            "shift" => modifiers |= Modifiers::SHIFT,
            "win" | "meta" | "cmd" | "super" => modifiers |= Modifiers::SUPER,
            _ => return Err(format!("unknown modifier '{}' in {}", part, s)),
        }
    }
    Ok(Shortcut::new(Some(modifiers), code))
}

fn parse_code(key: &str) -> Result<Code, String> {
    let code = match key {
        "space" => Code::Space,
        "printscreen" | "prtsc" | "print" => Code::PrintScreen,
        "enter" | "return" => Code::Enter,
        "escape" | "esc" => Code::Escape,
        "tab" => Code::Tab,
        "backspace" => Code::Backspace,
        "insert" | "ins" => Code::Insert,
        "delete" | "del" => Code::Delete,
        "home" => Code::Home,
        "end" => Code::End,
        "pageup" => Code::PageUp,
        "pagedown" => Code::PageDown,
        "up" => Code::ArrowUp,
        "down" => Code::ArrowDown,
        "left" => Code::ArrowLeft,
        "right" => Code::ArrowRight,
        "comma" => Code::Comma,
        "period" | "." => Code::Period,
        "slash" => Code::Slash,
        "semicolon" => Code::Semicolon,
        "quote" => Code::Quote,
        "backslash" => Code::Backslash,
        "bracketleft" | "[" => Code::BracketLeft,
        "bracketright" | "]" => Code::BracketRight,
        "minus" | "-" => Code::Minus,
        "equal" | "=" => Code::Equal,
        "grave" | "`" => Code::Backquote,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "f1" => Code::F1,
        "f2" => Code::F2,
        "f3" => Code::F3,
        "f4" => Code::F4,
        "f5" => Code::F5,
        "f6" => Code::F6,
        "f7" => Code::F7,
        "f8" => Code::F8,
        "f9" => Code::F9,
        "f10" => Code::F10,
        "f11" => Code::F11,
        "f12" => Code::F12,
        _ => {
            if key.len() == 1 {
                let c = key.chars().next().unwrap();
                if c.is_ascii_alphabetic() {
                    // Code enum variants are KeyA..KeyZ (uppercase).
                    let variant = format!("Key{}", c.to_ascii_uppercase());
                    return parse_code_variant(&variant);
                }
            }
            return Err(format!("unknown key '{}'", key));
        }
    };
    Ok(code)
}

fn parse_code_variant(variant: &str) -> Result<Code, String> {
    // The Code enum derives strum-style names; serialize via serde to match.
    serde_json::from_str(&format!("\"{}\"", variant))
        .map_err(|_| format!("unknown key variant '{}'", variant))
}

fn config_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    Ok(data_dir.join("config.json"))
}

fn overrides_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    Ok(data_dir.join("overrides.json"))
}

fn read_config_sync(app: &AppHandle) -> Result<AppConfig, String> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

fn read_overrides_sync(app: &AppHandle) -> Result<HashMap<String, CommandOverride>, String> {
    let path = overrides_path(app)?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

/// (Re-)register the base palette hotkey from config. If game mode is enabled,
/// the game hotkey is registered instead.
pub fn register_base_hotkey(
    app: &AppHandle,
    config: &AppConfig,
    game_mode: bool,
) -> Result<(), String> {
    let hotkey_str = if game_mode {
        config
            .global_hotkey_game
            .as_deref()
            .unwrap_or(DEFAULT_GAME_HOTKEY)
    } else {
        config.global_hotkey.as_deref().unwrap_or(DEFAULT_HOTKEY)
    };
    let shortcut = parse_shortcut(hotkey_str)?;

    let mut active = active().lock().unwrap();
    // Unregister previous base shortcuts.
    if let Some(prev) = active.base.take() {
        let _ = app.global_shortcut().unregister(prev);
    }
    if let Some(prev) = active.base_game.take() {
        let _ = app.global_shortcut().unregister(prev);
    }

    app.global_shortcut()
        .register(shortcut)
        .map_err(|e| format!("failed to register base hotkey {}: {}", hotkey_str, e))?;

    if game_mode {
        active.base_game = Some(shortcut);
    } else {
        active.base = Some(shortcut);
    }
    Ok(())
}

/// (Re-)register the screenshot hotkey (Windows and macOS — on Linux the
/// trigger is a managed COSMIC binding that relaunches us with a deep link).
/// Registration failure is non-fatal: the OS or other apps may already own the
/// chosen binding, and the palette command still works.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn register_screenshot_hotkey(app: &AppHandle, config: &AppConfig) {
    let hotkey_str = match config.screenshot_hotkey.as_deref() {
        Some(s) => s,
        #[cfg(target_os = "windows")]
        None => DEFAULT_SCREENSHOT_HOTKEY,
        // macOS has no safe default (PrintScreen keys don't exist; Cmd+Shift+3/4/5
        // are system shortcuts), so leave it unregistered until the user sets one.
        #[cfg(target_os = "macos")]
        None => return,
    };
    let shortcut = match parse_shortcut(hotkey_str) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Invalid screenshot hotkey {hotkey_str}: {e}");
            return;
        }
    };

    let mut active = active().lock().unwrap();
    if let Some(prev) = active.screenshot.take() {
        let _ = app.global_shortcut().unregister(prev);
    }
    match app.global_shortcut().register(shortcut) {
        Ok(_) => active.screenshot = Some(shortcut),
        Err(e) => eprintln!("Failed to register screenshot hotkey {hotkey_str}: {e}"),
    }
}

/// Register per-command shortcuts from overrides. Each shortcut fires a
/// `command-hotkey` event carrying the command id.
pub fn register_command_hotkeys(
    app: &AppHandle,
    overrides: &HashMap<String, CommandOverride>,
) -> Result<(), String> {
    let mut active = active().lock().unwrap();
    for shortcut in active.commands.values() {
        let _ = app.global_shortcut().unregister(*shortcut);
    }
    active.commands.clear();

    for (id, ov) in overrides {
        let Some(hotkey_str) = &ov.hotkey else {
            continue;
        };
        let shortcut = parse_shortcut(hotkey_str)?;

        // Avoid collisions with the base hotkey.
        if active.base == Some(shortcut) || active.base_game == Some(shortcut) {
            continue;
        }

        match app.global_shortcut().register(shortcut) {
            Ok(_) => {
                // Tauri's handler is set at plugin build time, so we use the
                // global handler in lib.rs to emit events for command hotkeys.
                active.commands.insert(id.clone(), shortcut);
            }
            Err(e) => {
                eprintln!(
                    "Failed to register command hotkey {} for {}: {}",
                    hotkey_str, id, e
                );
            }
        }
    }
    Ok(())
}

/// Full setup: read config + overrides and register all shortcuts.
pub fn setup_shortcuts(app: &AppHandle) -> Result<(), String> {
    let config = read_config_sync(app)?;
    register_base_hotkey(app, &config, false)?;
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    register_screenshot_hotkey(app, &config);

    let overrides = read_overrides_sync(app)?;
    register_command_hotkeys(app, &overrides)?;
    Ok(())
}

/// Reload shortcuts after config or overrides change.
pub fn reload_shortcuts(app: &AppHandle, game_mode: bool) -> Result<(), String> {
    let config = read_config_sync(app)?;
    register_base_hotkey(app, &config, game_mode)?;
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    register_screenshot_hotkey(app, &config);

    let overrides = read_overrides_sync(app)?;
    register_command_hotkeys(app, &overrides)?;
    Ok(())
}

/// Returns true if the shortcut is the registered screenshot hotkey.
pub fn is_screenshot_hotkey(shortcut: Shortcut) -> bool {
    active().lock().unwrap().screenshot == Some(shortcut)
}

/// Returns true if the shortcut currently belongs to a registered command
/// hotkey (so the global handler can emit the right event).
pub fn is_command_hotkey(shortcut: Shortcut) -> Option<String> {
    let active = active().lock().unwrap();
    active
        .commands
        .iter()
        .find(|(_, s)| **s == shortcut)
        .map(|(id, _)| id.clone())
}

#[derive(Debug, Deserialize)]
pub struct HotkeyUpdate {
    pub hotkey: String,
    pub game_hotkey: Option<String>,
}

/// Update the stored base hotkeys and re-register them. The binding is
/// validated (parsed) before persisting so an invalid string is rejected with
/// an error the UI can surface. On Linux the COSMIC/GNOME managed binding is
/// rewritten too, so the user-edited hotkey takes effect immediately (not just
/// the hardcoded Ctrl+Space / Alt+Space defaults).
#[tauri::command]
pub async fn set_global_hotkey(
    app: AppHandle,
    update: HotkeyUpdate,
    game_mode: bool,
) -> Result<(), String> {
    let base = update.hotkey.trim().to_string();
    let game = update.game_hotkey.as_deref().map(|g| g.trim().to_string());
    // Validate up front so bad input never reaches config.json.
    parse_shortcut(&base)?;
    if let Some(g) = &game {
        parse_shortcut(g)?;
    }

    let mut config = read_config_sync(&app)?;
    config.global_hotkey = Some(base.clone());
    if let Some(g) = &game {
        config.global_hotkey_game = Some(g.clone());
    }

    let path = config_path(&app)?;
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;

    register_base_hotkey(&app, &config, game_mode)?;

    #[cfg(target_os = "linux")]
    {
        let base = config.global_hotkey.as_deref().unwrap_or("Ctrl+Space");
        let game = config.global_hotkey_game.as_deref().unwrap_or("Alt+Space");
        super::linux_shortcuts::update_toggle_shortcut_with(base, game, game_mode);
    }

    Ok(())
}

/// Update the stored screenshot hotkey and re-register it. The binding is
/// validated (parsed) before persisting so an invalid string is rejected with
/// an error the UI can surface. Registration only takes effect on Windows; on
/// Linux the screenshot trigger is a managed COSMIC binding, but the value is
/// still persisted for cross-build consistency.
#[tauri::command]
pub async fn set_screenshot_hotkey(app: AppHandle, hotkey: String) -> Result<(), String> {
    let hotkey = hotkey.trim().to_string();
    // Validate the binding up front so bad input never reaches config.json.
    parse_shortcut(&hotkey)?;

    let mut config = read_config_sync(&app)?;
    config.screenshot_hotkey = Some(hotkey);

    let path = config_path(&app)?;
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    register_screenshot_hotkey(&app, &config);

    Ok(())
}

/// Set or clear a per-command global shortcut. Stores the hotkey in
/// overrides.json and re-registers command hotkeys.
#[tauri::command]
pub async fn set_command_hotkey(
    app: AppHandle,
    command_id: String,
    hotkey: Option<String>,
) -> Result<(), String> {
    let path = overrides_path(&app)?;
    let mut overrides = read_overrides_sync(&app)?;
    let mut ov = overrides.get(&command_id).cloned().unwrap_or_default();
    ov.hotkey = hotkey;
    if ov.alias.is_none() && ov.pinned.is_none() && ov.hotkey.is_none() {
        overrides.remove(&command_id);
    } else {
        overrides.insert(command_id, ov);
    }

    let json = serde_json::to_string_pretty(&overrides).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;

    register_command_hotkeys(&app, &overrides)
}

#[cfg(test)]
mod tests {
    use super::parse_shortcut;
    use tauri_plugin_global_shortcut::{Code, Modifiers};

    fn assert_shortcut(s: &str, mods: Modifiers, key: Code) {
        let sc = parse_shortcut(s).unwrap();
        assert_eq!(sc.mods, mods, "modifiers mismatch for {}", s);
        assert_eq!(sc.key, key, "key mismatch for {}", s);
    }

    #[test]
    fn parses_simple_modifiers() {
        assert_shortcut("Ctrl+Space", Modifiers::CONTROL, Code::Space);
        assert_shortcut("Alt+G", Modifiers::ALT, Code::KeyG);
        assert_shortcut(
            "Ctrl+Shift+T",
            Modifiers::CONTROL | Modifiers::SHIFT,
            Code::KeyT,
        );
    }

    #[test]
    fn parses_case_insensitive() {
        assert_shortcut("ctrl+space", Modifiers::CONTROL, Code::Space);
        assert_shortcut("ALT+SHIFT+F1", Modifiers::ALT | Modifiers::SHIFT, Code::F1);
    }

    #[test]
    fn rejects_invalid_shortcut() {
        assert!(parse_shortcut("").is_err());
        assert!(parse_shortcut("Foo+Bar").is_err());
    }
}
