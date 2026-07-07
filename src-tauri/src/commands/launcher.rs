//! App launcher: enumerates the shell AppsFolder — the same source the Start
//! menu's "All apps" uses — so win32 and UWP/Store apps both appear, with
//! localized display names and shell-side dedup. Falls back to a Start-Menu
//! .lnk/.url walk if COM enumeration yields nothing. Entries carry a
//! `shell:AppsFolder\<id>` parsing path; launching goes through ShellExecuteW
//! (no process spawn) and icons resolve lazily per visible row via path_icon.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct AppEntry {
    pub name: String,
    /// Either `shell:AppsFolder\<parsing name>` or a .lnk/.url path (fallback)
    pub path: String,
}

#[cfg(target_os = "windows")]
fn apps_folder_entries() -> Option<Vec<AppEntry>> {
    use windows::core::w;
    use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        IEnumShellItems, IShellItem, SHCreateItemFromParsingName, BHID_EnumItems, SIGDN,
        SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING,
    };

    unsafe fn display_name(item: &IShellItem, kind: SIGDN) -> Option<String> {
        unsafe {
            let pw = item.GetDisplayName(kind).ok()?;
            let s = pw.to_string().ok();
            CoTaskMemFree(Some(pw.0 as _));
            s
        }
    }

    unsafe {
        // COM may already be initialized on this thread; ignore that.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let folder: IShellItem = SHCreateItemFromParsingName(w!("shell:AppsFolder"), None).ok()?;
        let enumerator: IEnumShellItems = folder.BindToHandler(None, &BHID_EnumItems).ok()?;

        let mut out = Vec::new();
        loop {
            let mut slot: [Option<IShellItem>; 1] = [None];
            let mut fetched = 0u32;
            if enumerator.Next(&mut slot, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(item) = slot[0].take() else { break };
            let name = display_name(&item, SIGDN_NORMALDISPLAY);
            let parse = display_name(&item, SIGDN_PARENTRELATIVEPARSING);
            if let (Some(name), Some(parse)) = (name, parse) {
                if !name.is_empty() && !parse.is_empty() {
                    out.push(AppEntry {
                        name,
                        path: format!("shell:AppsFolder\\{parse}"),
                    });
                }
            }
        }
        Some(out)
    }
}

/// Legacy-style Start-Menu walk, used only when AppsFolder enumeration fails.
/// User-dir shortcuts shadow machine-wide ones with the same name.
#[cfg(target_os = "windows")]
fn start_menu_entries() -> Vec<AppEntry> {
    use std::path::PathBuf;

    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("Microsoft/Windows/Start Menu/Programs"));
    }
    if let Ok(programdata) = std::env::var("PROGRAMDATA") {
        dirs.push(PathBuf::from(programdata).join("Microsoft/Windows/Start Menu/Programs"));
    }

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for dir in dirs {
        let walker = walkdir::WalkDir::new(&dir)
            .max_depth(3)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok());
        for entry in walker {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();
            if ext != "lnk" && ext != "url" {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if !seen.insert(name.to_lowercase()) {
                continue;
            }
            out.push(AppEntry {
                name: name.to_string(),
                path: path.to_string_lossy().replace('\\', "/"),
            });
        }
    }
    out
}

/// Scan the standard application folders for `.app` bundles. Earlier roots win
/// on name collisions (a user-installed app shadows the system copy).
#[cfg(target_os = "macos")]
fn mac_app_entries() -> Vec<AppEntry> {
    use std::path::PathBuf;

    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }
    roots.push(PathBuf::from("/Applications"));
    roots.push(PathBuf::from("/System/Applications"));

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for root in roots {
        // Depth 3 covers subfolders like /Applications/Utilities without
        // crawling the whole disk.
        let mut walker = walkdir::WalkDir::new(&root)
            .max_depth(3)
            .follow_links(false)
            .into_iter();
        while let Some(entry) = walker.next() {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("app") {
                continue;
            }
            // An .app is one launchable unit — never descend into the bundle
            // (helper apps inside would show up as bogus entries).
            if entry.file_type().is_dir() {
                walker.skip_current_dir();
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if !seen.insert(name.to_lowercase()) {
                continue;
            }
            out.push(AppEntry {
                name: name.to_string(),
                path: path.to_string_lossy().into_owned(),
            });
        }
    }
    out
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    // Exercises the real filesystem walk over the application folders.
    #[test]
    fn smoke_mac_app_entries() {
        let apps = super::mac_app_entries();
        assert!(!apps.is_empty(), "no .app bundles found");
        assert!(apps.iter().all(|a| a.path.ends_with(".app")));
        // Ships with every macOS install, lives under /System/Applications.
        assert!(
            apps.iter().any(|a| a.name == "Calculator"),
            "expected a stock app in the list"
        );
    }

    // Cross-references the real installed-app list against the live process
    // table. On a dev Mac at least one /Applications app is always open, and
    // every match must be a path the app list actually contains.
    #[test]
    fn smoke_running_app_paths() {
        let apps = super::app_entries();
        let procs = crate::commands::process::snapshot_processes();
        let running = super::match_running_apps(&apps, &procs);
        let known: std::collections::HashSet<&str> =
            apps.iter().map(|a| a.path.as_str()).collect();
        assert!(
            running.iter().all(|p| known.contains(p.as_str())),
            "matcher returned a path not in the app list"
        );
        assert!(
            !running.is_empty(),
            "no installed app matched a running process"
        );
    }
}

/// The platform's installed-app list, name-sorted. Shared by `list_apps` and
/// the running-app matcher so both see the exact same `path` identities.
fn app_entries() -> Vec<AppEntry> {
    #[cfg(target_os = "windows")]
    let mut apps = apps_folder_entries()
        .filter(|a| !a.is_empty())
        .unwrap_or_else(start_menu_entries);
    #[cfg(target_os = "macos")]
    let mut apps = mac_app_entries();
    #[cfg(target_os = "linux")]
    let mut apps = desktop_dir_entries();

    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

#[tauri::command]
pub async fn list_apps() -> Result<Vec<AppEntry>, String> {
    tokio::task::spawn_blocking(app_entries)
        .await
        .map_err(|e| e.to_string())
}

/// Cross-reference installed apps with running processes and return the subset
/// of app `path`s (same identity as `list_apps`) that currently have a live
/// process — powers the running-app indicator dot in the root list. Matching is
/// per-platform and best-effort: a miss just omits the dot.
///
/// - **macOS**: a process whose executable lives inside the `Foo.app` bundle.
/// - **Linux**: the desktop entry's `Exec` binary basename matches a running
///   executable/`comm` name.
/// - **Windows**: AppsFolder `path`s aren't real exe paths, so match the app's
///   display name against the running executable basename (normalized).
#[tauri::command]
pub async fn running_app_paths() -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(|| {
        let apps = app_entries();
        let procs = super::process::snapshot_processes();
        Ok(match_running_apps(&apps, &procs))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(target_os = "macos")]
fn match_running_apps(
    apps: &[AppEntry],
    procs: &[crate::commands::process::ProcessInfo],
) -> Vec<String> {
    apps.iter()
        .filter(|app| {
            // /Applications/Foo.app  →  a process exe under /Applications/Foo.app/
            let prefix = format!("{}/", app.path);
            procs
                .iter()
                .any(|p| p.exe_path.as_deref().is_some_and(|e| e.starts_with(&prefix)))
        })
        .map(|app| app.path.clone())
        .collect()
}

#[cfg(target_os = "linux")]
fn match_running_apps(
    apps: &[AppEntry],
    procs: &[crate::commands::process::ProcessInfo],
) -> Vec<String> {
    use std::collections::HashSet;
    use std::path::Path;

    // Running executable basenames + comm names, lowercased for matching.
    let mut running: HashSet<String> = HashSet::new();
    for p in procs {
        running.insert(p.name.to_lowercase());
        if let Some(exe) = &p.exe_path {
            if let Some(base) = Path::new(exe).file_name() {
                running.insert(base.to_string_lossy().to_lowercase());
            }
        }
    }

    apps.iter()
        .filter(|app| {
            let Ok(content) = std::fs::read_to_string(&app.path) else {
                return false;
            };
            let Some(exec) = super::desktop::desktop_entry_value(&content, "Exec") else {
                return false;
            };
            let Some(args) = super::desktop::parse_exec(&exec) else {
                return false;
            };
            // First non-env token is the binary; comm truncates to 15 chars, so
            // also accept a prefix match against the running name.
            let bin = Path::new(&args[0])
                .file_name()
                .map(|b| b.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            !bin.is_empty()
                && running
                    .iter()
                    .any(|r| r == &bin || (r.len() == 15 && bin.starts_with(r.as_str())))
        })
        .map(|app| app.path.clone())
        .collect()
}

#[cfg(target_os = "windows")]
fn match_running_apps(
    apps: &[AppEntry],
    procs: &[crate::commands::process::ProcessInfo],
) -> Vec<String> {
    // Squash to lowercase alphanumerics so "Windows Terminal" ↔ "WindowsTerminal.exe".
    fn norm(s: &str) -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect()
    }
    let running: std::collections::HashSet<String> = procs
        .iter()
        .map(|p| norm(p.name.trim_end_matches(".exe")))
        .collect();
    apps.iter()
        .filter(|app| {
            let n = norm(&app.name);
            !n.is_empty() && running.contains(&n)
        })
        .map(|app| app.path.clone())
        .collect()
}

/// Enumerate installed apps from the XDG applications dirs (`~/.local/share`
/// first so user entries shadow system ones, then each `XDG_DATA_DIRS` entry —
/// which on Fedora includes the Flatpak exports). Deduped by desktop-file ID.
#[cfg(target_os = "linux")]
fn desktop_dir_entries() -> Vec<AppEntry> {
    use std::collections::HashSet;
    use std::path::PathBuf;

    let mut app_dirs: Vec<PathBuf> = Vec::new();
    let home = std::env::var("HOME").unwrap_or_default();
    let xdg_data_home = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or(format!("{home}/.local/share"));
    app_dirs.push(PathBuf::from(format!("{xdg_data_home}/applications")));
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or("/usr/local/share:/usr/share".to_string());
    for d in data_dirs.split(':').filter(|d| !d.is_empty()) {
        app_dirs.push(PathBuf::from(format!("{}/applications", d.trim_end_matches('/'))));
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<AppEntry> = Vec::new();
    for dir in &app_dirs {
        // The spec allows one level of vendor subdirs (e.g. kde4/), hence depth 2.
        for entry in walkdir::WalkDir::new(dir)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            // Desktop-file ID: path relative to the applications dir, '/' → '-'.
            let id = match path.strip_prefix(dir) {
                Ok(rel) => rel.to_string_lossy().replace('/', "-"),
                Err(_) => continue,
            };
            if !seen.insert(id) {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(path) else {
                continue;
            };
            if super::desktop::desktop_entry_value(&content, "Type").as_deref()
                != Some("Application")
            {
                continue;
            }
            if !super::desktop::is_displayable(&content) {
                continue;
            }
            let Some(name) =
                super::desktop::desktop_entry_value(&content, "Name").filter(|n| !n.is_empty())
            else {
                continue;
            };
            out.push(AppEntry {
                name,
                path: path.to_string_lossy().into_owned(),
            });
        }
    }
    out
}

#[tauri::command]
pub async fn run_app(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(move || {
            use windows::core::PCWSTR;
            use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
            use windows::Win32::UI::Shell::ShellExecuteW;
            use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

            let wide: Vec<u16> = path
                .replace('/', "\\")
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                // Default verb: launches .lnk/.url targets and shell:AppsFolder
                // entries (win32 and UWP) alike, no intermediate process.
                let h = ShellExecuteW(
                    None,
                    PCWSTR::null(),
                    PCWSTR(wide.as_ptr()),
                    PCWSTR::null(),
                    PCWSTR::null(),
                    SW_SHOWNORMAL,
                );
                // Per the ShellExecute contract, values > 32 mean success
                if h.0 as isize > 32 {
                    Ok(())
                } else {
                    Err(format!("failed to launch '{}' (code {})", path, h.0 as isize))
                }
            }
        })
        .await
        .map_err(|e| e.to_string())?
    }

    #[cfg(target_os = "macos")]
    {
        tokio::task::spawn_blocking(move || {
            // `open` goes through LaunchServices: it activates a running
            // instance instead of launching a second one, same as Finder.
            let out = std::process::Command::new("open")
                .arg(&path)
                .output()
                .map_err(|e| format!("open failed to run: {e}"))?;
            if out.status.success() {
                Ok(())
            } else {
                Err(format!(
                    "failed to launch '{}': {}",
                    path,
                    String::from_utf8_lossy(&out.stderr).trim()
                ))
            }
        })
        .await
        .map_err(|e| e.to_string())?
    }

    #[cfg(target_os = "linux")]
    {
        tokio::task::spawn_blocking(move || {
            super::desktop::launch_desktop_file(std::path::Path::new(&path))
        })
        .await
        .map_err(|e| e.to_string())?
    }
}
