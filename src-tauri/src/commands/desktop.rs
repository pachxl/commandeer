//! Shared freedesktop `.desktop`-entry helpers used by the scripts list,
//! the app launcher, and icon resolution. Linux/Unix only.

use std::fs;
use std::path::{Path, PathBuf};

use super::fs::base64_encode;

/// Read a single key from the `[Desktop Entry]` group of a .desktop file.
/// Localized variants (e.g. `Name[de]=`) are ignored in favour of the default.
pub(crate) fn desktop_entry_value(content: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    let mut in_entry = false;
    for raw in content.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        } else if in_entry {
            if let Some(val) = line.strip_prefix(&prefix) {
                return Some(val.trim().to_string());
            }
        }
    }
    None
}

/// Human-friendly name declared inside a .desktop file (`Name=`).
pub(crate) fn resolve_desktop_name(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let name = desktop_entry_value(&content, "Name")?;
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Resolve the `Icon=` of a .desktop file to a base64 data URL, if findable.
pub(crate) fn resolve_desktop_icon(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let icon = desktop_entry_value(&content, "Icon")?;
    if icon.is_empty() {
        return None;
    }

    let icon_path = if icon.starts_with('/') {
        let p = PathBuf::from(&icon);
        if p.is_file() {
            Some(p)
        } else {
            None
        }
    } else {
        find_themed_icon(&icon)
    }?;

    icon_file_to_data_url(&icon_path)
}

/// Read an icon file into a `data:` URL (png assumed unless the ext says svg).
pub(crate) fn icon_file_to_data_url(icon_path: &Path) -> Option<String> {
    let bytes = fs::read(icon_path).ok()?;
    let mime = match icon_path.extension().and_then(|e| e.to_str()) {
        Some("svg") => "image/svg+xml",
        _ => "image/png",
    };
    Some(format!("data:{mime};base64,{}", base64_encode(&bytes)))
}

/// Best-effort lookup of a freedesktop icon name across the common theme roots.
pub(crate) fn find_themed_icon(name: &str) -> Option<PathBuf> {
    find_themed_icon_in(name, &["apps"])
}

/// Like [`find_themed_icon`] but searching the given theme context dirs
/// (e.g. `apps`, `mimetypes`, `places`) in order.
pub(crate) fn find_themed_icon_in(name: &str, contexts: &[&str]) -> Option<PathBuf> {
    let sizes = [
        "scalable", "512x512", "256x256", "128x128", "96x96", "64x64", "48x48", "32x32", "24x24",
        "16x16",
    ];
    let exts = ["png", "svg"];

    // Flat pixmaps directory (no theme/size structure).
    for ext in exts {
        let p = PathBuf::from(format!("/usr/share/pixmaps/{name}.{ext}"));
        if p.is_file() {
            return Some(p);
        }
    }

    for root in theme_roots() {
        for context in contexts {
            for size in sizes {
                for ext in exts {
                    let p = PathBuf::from(format!("{root}/{size}/{context}/{name}.{ext}"));
                    if p.is_file() {
                        return Some(p);
                    }
                    // Some themes (Adwaita, Papirus) nest size under context
                    // (theme/context/size) or use size-first with symbolic
                    // fallbacks; try the common inverted layout too.
                    let p = PathBuf::from(format!("{root}/{context}/{size}/{name}.{ext}"));
                    if p.is_file() {
                        return Some(p);
                    }
                }
            }
        }
    }

    None
}

/// Icon-theme roots in preference order: the active theme (when cheaply
/// detectable), then Adwaita (ships mimetype/place icons that hicolor lacks),
/// then hicolor, across user + system icon dirs.
fn theme_roots() -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            let xdg_data_home = std::env::var("XDG_DATA_HOME")
                .ok()
                .filter(|v| !v.is_empty())
                .unwrap_or(format!("{home}/.local/share"));
            dirs.push(format!("{xdg_data_home}/icons"));
            dirs.push(format!("{home}/.icons"));
        }
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or("/usr/local/share:/usr/share".to_string());
    for d in data_dirs.split(':').filter(|d| !d.is_empty()) {
        dirs.push(format!("{}/icons", d.trim_end_matches('/')));
    }

    let mut themes: Vec<String> = Vec::new();
    if let Some(active) = active_icon_theme() {
        themes.push(active);
    }
    for fallback in ["Adwaita", "hicolor"] {
        if !themes.iter().any(|t| t == fallback) {
            themes.push(fallback.to_string());
        }
    }

    let mut roots = Vec::with_capacity(dirs.len() * themes.len());
    for theme in &themes {
        for dir in &dirs {
            roots.push(format!("{dir}/{theme}"));
        }
    }
    roots
}

/// The desktop's configured icon theme, when cheaply readable (COSMIC config
/// file; GNOME via gsettings). Cached: the theme doesn't change mid-session.
fn active_icon_theme() -> Option<String> {
    use std::sync::OnceLock;
    static THEME: OnceLock<Option<String>> = OnceLock::new();
    THEME
        .get_or_init(|| {
            // COSMIC stores the raw theme name in a plain file.
            if let Ok(home) = std::env::var("HOME") {
                let p = format!("{home}/.config/cosmic/com.system76.CosmicTk/v1/icon_theme");
                if let Ok(content) = fs::read_to_string(p) {
                    let name = content.trim().trim_matches('"').to_string();
                    if !name.is_empty() {
                        return Some(name);
                    }
                }
            }
            // GNOME (and some others): gsettings.
            let out = std::process::Command::new("gsettings")
                .args(["get", "org.gnome.desktop.interface", "icon-theme"])
                .output()
                .ok()?;
            if !out.status.success() {
                return None;
            }
            let name = String::from_utf8_lossy(&out.stdout)
                .trim()
                .trim_matches('\'')
                .to_string();
            if name.is_empty() {
                None
            } else {
                Some(name)
            }
        })
        .clone()
}

/// Whether a .desktop entry should be shown in an app list, honoring
/// `NoDisplay`, `Hidden`, and `OnlyShowIn`/`NotShowIn` vs the current desktop.
pub(crate) fn is_displayable(content: &str) -> bool {
    let truthy = |key: &str| {
        desktop_entry_value(content, key)
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    };
    if truthy("NoDisplay") || truthy("Hidden") {
        return false;
    }

    let current: Vec<String> = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_lowercase())
        .collect();
    let listed = |key: &str| -> Option<bool> {
        desktop_entry_value(content, key).map(|v| {
            v.split(';')
                .filter(|s| !s.is_empty())
                .any(|d| current.iter().any(|c| c == &d.to_ascii_lowercase()))
        })
    };
    if let Some(matched) = listed("OnlyShowIn") {
        if !matched {
            return false;
        }
    }
    if let Some(matched) = listed("NotShowIn") {
        if matched {
            return false;
        }
    }
    true
}

/// Tokenize a Desktop Entry `Exec=` line: split on unquoted whitespace,
/// honour double quotes with `\`-escapes, and drop the spec's field codes
/// (`%f`, `%u`, `%i` and friends — `%i` also consumes its `--icon <name>`
/// expansion source, which is simply dropped since we pass no file/URL).
pub(crate) fn parse_exec(exec: &str) -> Option<Vec<String>> {
    let mut args: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut chars = exec.chars().peekable();
    let mut in_quotes = false;
    let mut has_token = false;

    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                has_token = true;
            }
            '\\' if in_quotes => {
                if let Some(&next) = chars.peek() {
                    cur.push(next);
                    chars.next();
                }
            }
            '%' if !in_quotes => {
                // "%%" is a literal '%'; any other field code is dropped
                // (we launch with no args).
                if let Some('%') = chars.next() {
                    cur.push('%');
                    has_token = true;
                }
            }
            c if c.is_whitespace() && !in_quotes => {
                if has_token && !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                }
                cur.clear();
                has_token = false;
            }
            c => {
                cur.push(c);
                has_token = true;
            }
        }
    }
    if has_token && !cur.is_empty() {
        args.push(cur);
    }

    if args.is_empty() {
        None
    } else {
        Some(args)
    }
}

/// Standard freedesktop application directories, in user-before-system order.
fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            let data_home = std::env::var("XDG_DATA_HOME")
                .ok()
                .filter(|value| !value.is_empty())
                .unwrap_or(format!("{home}/.local/share"));
            dirs.push(PathBuf::from(data_home).join("applications"));
        }
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or("/usr/local/share:/usr/share".to_string());
    dirs.extend(
        data_dirs
            .split(':')
            .filter(|directory| !directory.is_empty())
            .map(|directory| PathBuf::from(directory).join("applications")),
    );
    // Snap does not always add its desktop export root to XDG_DATA_DIRS.
    dirs.push(PathBuf::from("/var/lib/snapd/desktop/applications"));
    dirs
}

/// Match a live `/proc/<pid>/exe` path to the app's `.desktop` entry. Process
/// rows otherwise get the generic executable glyph because ELF binaries do not
/// embed application artwork the way Windows executables do.
fn desktop_for_executable(path: &Path) -> Option<PathBuf> {
    use std::collections::HashMap;
    use std::sync::OnceLock;

    static EXECUTABLES: OnceLock<HashMap<String, PathBuf>> = OnceLock::new();
    let entries = EXECUTABLES.get_or_init(|| {
        let mut entries = HashMap::new();
        for directory in application_dirs() {
            let walker = walkdir::WalkDir::new(directory)
                .max_depth(2)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok);
            for entry in walker {
                let desktop_path = entry.path();
                if desktop_path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    != Some("desktop")
                {
                    continue;
                }
                let Ok(content) = fs::read_to_string(desktop_path) else {
                    continue;
                };
                if !is_displayable(&content) {
                    continue;
                }
                let Some(exec) = desktop_entry_value(&content, "Exec") else {
                    continue;
                };
                let Some(arguments) = parse_exec(&exec) else {
                    continue;
                };
                let Some(program) = arguments.first() else {
                    continue;
                };
                let program_path = Path::new(program);
                let desktop_path = desktop_path.to_path_buf();
                entries
                    .entry(program.to_ascii_lowercase())
                    .or_insert_with(|| desktop_path.clone());
                if let Some(name) = program_path.file_name().and_then(|name| name.to_str()) {
                    entries
                        .entry(name.to_ascii_lowercase())
                        .or_insert(desktop_path);
                }
            }
        }
        entries
    });

    let exact = path.to_string_lossy().to_ascii_lowercase();
    entries.get(&exact).cloned().or_else(|| {
        let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
        entries.get(&name).cloned()
    })
}

/// Icon for an arbitrary filesystem path as a data URL: `.desktop` files get
/// their declared `Icon=`; everything else resolves through the shared-mime
/// database (via gio's content-type guess, filename-only — no I/O) to a themed
/// icon such as `text-x-rust` or `application-pdf`.
pub(crate) fn icon_for_path(path: &str) -> Option<String> {
    let p = Path::new(path);
    if p.extension().and_then(|e| e.to_str()) == Some("desktop") {
        return resolve_desktop_icon(p);
    }

    if p.is_dir() {
        if let Some(hit) = find_themed_icon_in("folder", &["places"]) {
            return icon_file_to_data_url(&hit);
        }
        return None;
    }

    if p.extension().is_none() {
        if let Some(desktop) = desktop_for_executable(p) {
            if let Some(icon) = resolve_desktop_icon(&desktop) {
                return Some(icon);
            }
        }
    }

    // Executables without an extension (the common case for /usr/bin) guess as
    // octet-stream by filename alone; give them the standard executable icon.
    use gtk::glib::Cast;
    let (content_type, uncertain) = gtk::gio::functions::content_type_guess(Some(p), &[]);
    let mut names: Vec<String> = Vec::new();
    if !uncertain || content_type != "application/octet-stream" {
        let icon = gtk::gio::functions::content_type_get_icon(&content_type);
        if let Ok(themed) = icon.downcast::<gtk::gio::ThemedIcon>() {
            names.extend(themed.names().iter().map(|n| n.to_string()));
        }
    }
    names.push("application-x-executable".to_string());

    for name in &names {
        if let Some(hit) = find_themed_icon_in(name, &["mimetypes", "apps", "devices", "places"]) {
            return icon_file_to_data_url(&hit);
        }
    }
    None
}

/// Launch a .desktop entry. Prefers `gio launch` (correct handling of
/// `Terminal=true`, DBusActivatable, and Flatpak wrappers); when gio is not
/// installed, falls back to parsing `Exec=` and spawning detached.
pub(crate) fn launch_desktop_file(path: &Path) -> Result<(), String> {
    match std::process::Command::new("gio")
        .arg("launch")
        .arg(path)
        .spawn()
    {
        Ok(_) => return Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {} // no gio: fall through
        Err(e) => return Err(format!("gio launch failed: {e}")),
    }

    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    if desktop_entry_value(&content, "Terminal")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return Err("terminal app: install glib2 (gio) to launch it".to_string());
    }
    let exec = desktop_entry_value(&content, "Exec")
        .ok_or_else(|| "desktop entry has no Exec line".to_string())?;
    let args =
        parse_exec(&exec).ok_or_else(|| "desktop entry has an empty Exec line".to_string())?;

    let mut cmd = std::process::Command::new(&args[0]);
    cmd.args(&args[1..]);
    if let Some(dir) = desktop_entry_value(&content, "Path").filter(|p| !p.is_empty()) {
        cmd.current_dir(dir);
    }
    // Detach into its own process group so the app outlives the palette.
    use std::os::unix::process::CommandExt;
    cmd.process_group(0)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to launch '{}': {e}", args[0]))?;
    Ok(())
}
