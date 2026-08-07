//! Configurable global shortcuts: the base palette hotkey plus per-command
//! shortcuts stored in overrides.json.

use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

use super::config::AppConfig;
use super::persistence::atomic_write;
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

/// Small registration seam that keeps the transition logic testable without a
/// running Tauri application or access to the operating system's hotkey API.
trait ShortcutRegistry {
    fn register(&self, shortcut: Shortcut) -> Result<(), String>;
    fn unregister(&self, shortcut: Shortcut) -> Result<(), String>;
}

struct AppShortcutRegistry<'a>(&'a AppHandle);

impl ShortcutRegistry for AppShortcutRegistry<'_> {
    fn register(&self, shortcut: Shortcut) -> Result<(), String> {
        self.0
            .global_shortcut()
            .register(shortcut)
            .map_err(|e| e.to_string())
    }

    fn unregister(&self, shortcut: Shortcut) -> Result<(), String> {
        self.0
            .global_shortcut()
            .unregister(shortcut)
            .map_err(|e| e.to_string())
    }
}

/// Replace one active shortcut without releasing the working binding until the
/// replacement is known to be available. If unregistering the old binding
/// fails, remove the newly registered binding again and leave the caller's
/// tracked state unchanged.
fn replace_registered_shortcut<R: ShortcutRegistry>(
    registry: &R,
    current: Option<Shortcut>,
    desired: Option<Shortcut>,
    label: &str,
) -> Result<Option<Shortcut>, String> {
    if current == desired {
        return Ok(current);
    }

    if let Some(next) = desired {
        registry
            .register(next)
            .map_err(|e| format!("failed to register {label}: {e}"))?;
    }

    if let Some(previous) = current {
        if let Err(error) = registry.unregister(previous) {
            let rollback = desired
                .and_then(|next| registry.unregister(next).err())
                .map(|rollback_error| format!("; rollback also failed: {rollback_error}"))
                .unwrap_or_default();
            return Err(format!(
                "failed to unregister the previous {label}: {error}{rollback}"
            ));
        }
    }

    Ok(desired)
}

fn sorted_shortcuts(shortcuts: &HashMap<String, Shortcut>) -> Vec<(&str, Shortcut)> {
    let mut entries: Vec<_> = shortcuts
        .iter()
        .map(|(id, shortcut)| (id.as_str(), *shortcut))
        .collect();
    entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
    entries
}

/// Replace a group of command shortcuts atomically from the caller's point of
/// view. Command shortcuts can swap bindings with each other, so the old set
/// must be released first. Any failure removes the partial new set and restores
/// the complete previous set before returning an error.
fn replace_registered_commands<R: ShortcutRegistry>(
    registry: &R,
    current: &HashMap<String, Shortcut>,
    desired: &HashMap<String, Shortcut>,
) -> Result<(), String> {
    if current == desired {
        return Ok(());
    }

    let current_entries = sorted_shortcuts(current);
    let desired_entries = sorted_shortcuts(desired);
    let mut removed = Vec::new();
    for (id, shortcut) in &current_entries {
        if let Err(error) = registry.unregister(*shortcut) {
            let mut rollback_errors = Vec::new();
            for (removed_id, removed_shortcut) in &removed {
                if let Err(rollback_error) = registry.register(*removed_shortcut) {
                    rollback_errors.push(format!("{removed_id}: {rollback_error}"));
                }
            }
            let rollback = if rollback_errors.is_empty() {
                String::new()
            } else {
                format!("; rollback also failed for {}", rollback_errors.join(", "))
            };
            return Err(format!(
                "failed to unregister command hotkey for {id}: {error}{rollback}"
            ));
        }
        removed.push((*id, *shortcut));
    }

    let mut registered = Vec::new();
    for (id, shortcut) in &desired_entries {
        if let Err(error) = registry.register(*shortcut) {
            let mut rollback_errors = Vec::new();
            for (registered_id, registered_shortcut) in &registered {
                if let Err(rollback_error) = registry.unregister(*registered_shortcut) {
                    rollback_errors.push(format!("remove {registered_id}: {rollback_error}"));
                }
            }
            for (removed_id, removed_shortcut) in &removed {
                if let Err(rollback_error) = registry.register(*removed_shortcut) {
                    rollback_errors.push(format!("restore {removed_id}: {rollback_error}"));
                }
            }
            let rollback = if rollback_errors.is_empty() {
                String::new()
            } else {
                format!("; rollback also failed for {}", rollback_errors.join(", "))
            };
            return Err(format!(
                "failed to register command hotkey for {id}: {error}{rollback}"
            ));
        }
        registered.push((*id, *shortcut));
    }

    Ok(())
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

fn validate_configured_hotkey_collisions(
    config: &AppConfig,
    commands: &HashMap<String, Shortcut>,
) -> Result<HashSet<Shortcut>, String> {
    let base = parse_shortcut(config.global_hotkey.as_deref().unwrap_or(DEFAULT_HOTKEY))?;
    let game = parse_shortcut(
        config
            .global_hotkey_game
            .as_deref()
            .unwrap_or(DEFAULT_GAME_HOTKEY),
    )?;
    let builtins = HashSet::from([base, game]);

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let builtins = {
        let mut builtins = builtins;
        if let Some(screenshot_str) = config.screenshot_hotkey.as_deref().or({
            #[cfg(target_os = "windows")]
            {
                Some(DEFAULT_SCREENSHOT_HOTKEY)
            }
            #[cfg(target_os = "macos")]
            {
                None
            }
        }) {
            let screenshot = parse_shortcut(screenshot_str)?;
            if screenshot == base || screenshot == game {
                return Err(format!(
                    "screenshot hotkey {screenshot_str} conflicts with a palette hotkey"
                ));
            }
            builtins.insert(screenshot);
        }
        builtins
    };

    for (id, shortcut) in commands {
        if builtins.contains(shortcut) {
            return Err(format!(
                "command hotkey for {id} conflicts with a configured built-in hotkey"
            ));
        }
    }

    Ok(builtins)
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
    let previous = active.base.or(active.base_game);
    let registry = AppShortcutRegistry(app);
    let registered = replace_registered_shortcut(
        &registry,
        previous,
        Some(shortcut),
        &format!("base hotkey {hotkey_str}"),
    )?;

    active.base = None;
    active.base_game = None;
    if game_mode {
        active.base_game = registered;
    } else {
        active.base = registered;
    }
    Ok(())
}

/// (Re-)register the screenshot hotkey (Windows and macOS — on Linux the
/// trigger is a managed COSMIC binding that relaunches us with a deep link).
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn register_screenshot_hotkey(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let hotkey_str = match config.screenshot_hotkey.as_deref() {
        Some(s) => s,
        #[cfg(target_os = "windows")]
        None => DEFAULT_SCREENSHOT_HOTKEY,
        // macOS has no safe default (PrintScreen keys don't exist; Cmd+Shift+3/4/5
        // are system shortcuts), so leave it unregistered until the user sets one.
        #[cfg(target_os = "macos")]
        None => "",
    };
    let desired = if hotkey_str.is_empty() {
        None
    } else {
        Some(parse_shortcut(hotkey_str)?)
    };

    let mut active = active().lock().unwrap();
    validate_configured_hotkey_collisions(config, &active.commands)?;
    let registry = AppShortcutRegistry(app);
    active.screenshot = replace_registered_shortcut(
        &registry,
        active.screenshot,
        desired,
        &format!("screenshot hotkey {hotkey_str}"),
    )?;
    Ok(())
}

fn parse_command_hotkeys(
    overrides: &HashMap<String, CommandOverride>,
) -> Result<HashMap<String, Shortcut>, String> {
    let mut entries: Vec<_> = overrides.iter().collect();
    entries.sort_unstable_by(|left, right| left.0.cmp(right.0));

    let mut desired = HashMap::new();
    let mut owners = HashMap::<Shortcut, String>::new();
    for (id, ov) in entries {
        let Some(hotkey_str) = &ov.hotkey else {
            continue;
        };
        let shortcut = parse_shortcut(hotkey_str)
            .map_err(|e| format!("invalid command hotkey for {id}: {e}"))?;
        if let Some(previous_id) = owners.insert(shortcut, id.clone()) {
            return Err(format!(
                "command hotkey {hotkey_str} is assigned to both {previous_id} and {id}"
            ));
        }
        desired.insert(id.clone(), shortcut);
    }
    Ok(desired)
}

/// Register per-command shortcuts from overrides. Each shortcut fires a
/// `command-hotkey` event carrying the command id.
pub fn register_command_hotkeys(
    app: &AppHandle,
    overrides: &HashMap<String, CommandOverride>,
) -> Result<(), String> {
    let desired = parse_command_hotkeys(overrides)?;
    let config = read_config_sync(app)?;
    let mut reserved = validate_configured_hotkey_collisions(&config, &desired)?;
    let mut active = active().lock().unwrap();
    reserved.extend(
        [active.base, active.base_game, active.screenshot]
            .into_iter()
            .flatten(),
    );
    for (id, shortcut) in &desired {
        if reserved.contains(shortcut) {
            return Err(format!(
                "command hotkey for {id} conflicts with an active built-in hotkey"
            ));
        }
    }

    let registry = AppShortcutRegistry(app);
    replace_registered_commands(&registry, &active.commands, &desired)?;
    // Tauri's handler is set at plugin build time, so the global handler in
    // lib.rs resolves these active bindings to command ids when they fire.
    active.commands = desired;
    Ok(())
}

/// Full setup: read config + overrides and register all shortcuts.
pub fn setup_shortcuts(app: &AppHandle) -> Result<(), String> {
    let config = read_config_sync(app)?;
    register_base_hotkey(app, &config, false)?;
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    if let Err(error) = register_screenshot_hotkey(app, &config) {
        eprintln!("Screenshot hotkey is unavailable: {error}");
    }

    let overrides = read_overrides_sync(app)?;
    // A stale per-command binding owned by another application must not make
    // Commandeer itself fail to launch. The transactional registrar guarantees
    // that either the full command set is active or none of it is.
    if let Err(error) = register_command_hotkeys(app, &overrides) {
        eprintln!("Command hotkeys are unavailable: {error}");
    }
    Ok(())
}

/// Reload shortcuts after config or overrides change.
pub fn reload_shortcuts(app: &AppHandle, game_mode: bool) -> Result<(), String> {
    let config = read_config_sync(app)?;
    register_base_hotkey(app, &config, game_mode)?;
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    if let Err(error) = register_screenshot_hotkey(app, &config) {
        eprintln!("Screenshot hotkey could not be reloaded: {error}");
    }

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

    let previous_config = read_config_sync(&app)?;
    let mut config = previous_config.clone();
    config.global_hotkey = Some(base.clone());
    if let Some(g) = &game {
        config.global_hotkey_game = Some(g.clone());
    }

    let path = config_path(&app)?;
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    let commands = active().lock().unwrap().commands.clone();
    validate_configured_hotkey_collisions(&config, &commands)?;

    // Activate the proposed binding first. A collision leaves both the live
    // registration and config.json untouched.
    register_base_hotkey(&app, &config, game_mode)?;

    if let Err(error) = atomic_write(&path, json) {
        let rollback = register_base_hotkey(&app, &previous_config, game_mode)
            .err()
            .map(|rollback_error| format!("; active-hotkey rollback also failed: {rollback_error}"))
            .unwrap_or_default();
        return Err(format!("failed to persist the hotkey: {error}{rollback}"));
    }

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
/// an error the UI can surface. Registration takes effect on Windows/macOS; on
/// Linux the screenshot trigger is a managed compositor binding, but the value
/// is still persisted for cross-build consistency.
#[tauri::command]
pub async fn set_screenshot_hotkey(app: AppHandle, hotkey: String) -> Result<(), String> {
    let hotkey = hotkey.trim().to_string();
    // Validate the binding up front so bad input never reaches config.json.
    parse_shortcut(&hotkey)?;

    let previous_config = read_config_sync(&app)?;
    let mut config = previous_config.clone();
    config.screenshot_hotkey = Some(hotkey);

    let path = config_path(&app)?;
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    let commands = active().lock().unwrap().commands.clone();
    validate_configured_hotkey_collisions(&config, &commands)?;

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    register_screenshot_hotkey(&app, &config)?;

    if let Err(error) = atomic_write(&path, json) {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        let rollback = register_screenshot_hotkey(&app, &previous_config)
            .err()
            .map(|rollback_error| format!("; active-hotkey rollback also failed: {rollback_error}"))
            .unwrap_or_default();
        #[cfg(target_os = "linux")]
        let rollback = String::new();
        return Err(format!("failed to persist the hotkey: {error}{rollback}"));
    }

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
    let hotkey = hotkey
        .map(|binding| binding.trim().to_string())
        .filter(|binding| !binding.is_empty());
    if let Some(binding) = &hotkey {
        parse_shortcut(binding)?;
    }

    let path = overrides_path(&app)?;
    let previous_overrides = read_overrides_sync(&app)?;
    let mut overrides = previous_overrides.clone();
    let mut ov = overrides.get(&command_id).cloned().unwrap_or_default();
    ov.hotkey = hotkey;
    if ov.alias.is_none() && ov.pinned.is_none() && ov.hotkey.is_none() {
        overrides.remove(&command_id);
    } else {
        overrides.insert(command_id, ov);
    }

    let json = serde_json::to_string_pretty(&overrides).map_err(|e| e.to_string())?;

    // Registration validates the complete desired set and rolls back any
    // partial transition before overrides.json is touched.
    register_command_hotkeys(&app, &overrides)?;

    if let Err(error) = atomic_write(&path, json) {
        let rollback = register_command_hotkeys(&app, &previous_overrides)
            .err()
            .map(|rollback_error| {
                format!("; command-hotkey rollback also failed: {rollback_error}")
            })
            .unwrap_or_default();
        return Err(format!(
            "failed to persist command hotkeys: {error}{rollback}"
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        parse_command_hotkeys, parse_shortcut, replace_registered_commands,
        replace_registered_shortcut, validate_configured_hotkey_collisions, ShortcutRegistry,
        DEFAULT_GAME_HOTKEY, DEFAULT_HOTKEY,
    };
    use crate::commands::config::AppConfig;
    use crate::commands::store::CommandOverride;
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

    #[derive(Default)]
    struct MockRegistry {
        registered: Mutex<HashSet<Shortcut>>,
        fail_register: Mutex<HashSet<Shortcut>>,
        fail_unregister: Mutex<HashSet<Shortcut>>,
    }

    impl MockRegistry {
        fn with_registered(shortcuts: impl IntoIterator<Item = Shortcut>) -> Self {
            Self {
                registered: Mutex::new(shortcuts.into_iter().collect()),
                ..Self::default()
            }
        }

        fn registered(&self) -> HashSet<Shortcut> {
            self.registered.lock().unwrap().clone()
        }
    }

    impl ShortcutRegistry for MockRegistry {
        fn register(&self, shortcut: Shortcut) -> Result<(), String> {
            if self.fail_register.lock().unwrap().contains(&shortcut) {
                return Err("injected registration failure".to_string());
            }
            if !self.registered.lock().unwrap().insert(shortcut) {
                return Err("already registered".to_string());
            }
            Ok(())
        }

        fn unregister(&self, shortcut: Shortcut) -> Result<(), String> {
            if self.fail_unregister.lock().unwrap().contains(&shortcut) {
                return Err("injected unregistration failure".to_string());
            }
            if !self.registered.lock().unwrap().remove(&shortcut) {
                return Err("not registered".to_string());
            }
            Ok(())
        }
    }

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

    #[test]
    fn single_replacement_keeps_working_binding_when_registration_fails() {
        let previous = parse_shortcut("Ctrl+A").unwrap();
        let desired = parse_shortcut("Ctrl+B").unwrap();
        let registry = MockRegistry::with_registered([previous]);
        registry.fail_register.lock().unwrap().insert(desired);

        let error =
            replace_registered_shortcut(&registry, Some(previous), Some(desired), "test hotkey")
                .unwrap_err();

        assert!(error.contains("failed to register test hotkey"));
        assert_eq!(registry.registered(), HashSet::from([previous]));
    }

    #[test]
    fn single_replacement_removes_new_binding_if_old_cannot_be_released() {
        let previous = parse_shortcut("Ctrl+A").unwrap();
        let desired = parse_shortcut("Ctrl+B").unwrap();
        let registry = MockRegistry::with_registered([previous]);
        registry.fail_unregister.lock().unwrap().insert(previous);

        replace_registered_shortcut(&registry, Some(previous), Some(desired), "test hotkey")
            .unwrap_err();

        assert_eq!(registry.registered(), HashSet::from([previous]));
    }

    #[test]
    fn command_replacement_restores_complete_old_set_after_failure() {
        let old_a = parse_shortcut("Ctrl+A").unwrap();
        let old_b = parse_shortcut("Ctrl+B").unwrap();
        let new_a = parse_shortcut("Ctrl+C").unwrap();
        let new_b = parse_shortcut("Ctrl+D").unwrap();
        let current = HashMap::from([("old-a".to_string(), old_a), ("old-b".to_string(), old_b)]);
        let desired = HashMap::from([("new-a".to_string(), new_a), ("new-b".to_string(), new_b)]);
        let registry = MockRegistry::with_registered([old_a, old_b]);
        registry.fail_register.lock().unwrap().insert(new_b);

        let error = replace_registered_commands(&registry, &current, &desired).unwrap_err();

        assert!(error.contains("failed to register command hotkey for new-b"));
        assert_eq!(registry.registered(), HashSet::from([old_a, old_b]));
    }

    #[test]
    fn duplicate_command_bindings_are_rejected_before_registration() {
        let overrides = HashMap::from([
            (
                "first".to_string(),
                CommandOverride {
                    hotkey: Some("Ctrl+K".to_string()),
                    ..CommandOverride::default()
                },
            ),
            (
                "second".to_string(),
                CommandOverride {
                    hotkey: Some("ctrl+k".to_string()),
                    ..CommandOverride::default()
                },
            ),
        ]);

        let error = parse_command_hotkeys(&overrides).unwrap_err();
        assert!(error.contains("assigned to both first and second"));
    }

    #[test]
    fn command_bindings_cannot_claim_inactive_palette_hotkeys() {
        let config = AppConfig::default();
        let commands = HashMap::from([
            ("base".to_string(), parse_shortcut(DEFAULT_HOTKEY).unwrap()),
            (
                "game".to_string(),
                parse_shortcut(DEFAULT_GAME_HOTKEY).unwrap(),
            ),
        ]);

        let error = validate_configured_hotkey_collisions(&config, &commands).unwrap_err();
        assert!(error.contains("conflicts with a configured built-in hotkey"));
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    #[test]
    fn screenshot_binding_cannot_claim_an_inactive_palette_hotkey() {
        let config = AppConfig {
            screenshot_hotkey: Some(DEFAULT_GAME_HOTKEY.to_string()),
            ..AppConfig::default()
        };

        let error = validate_configured_hotkey_collisions(&config, &HashMap::new()).unwrap_err();
        assert!(error.contains("conflicts with a palette hotkey"));
    }
}
