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

#[tauri::command]
pub async fn list_apps() -> Result<Vec<AppEntry>, String> {
    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(|| {
            let mut apps = apps_folder_entries()
                .filter(|a| !a.is_empty())
                .unwrap_or_else(start_menu_entries);
            apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            Ok(apps)
        })
        .await
        .map_err(|e| e.to_string())?
    }

    #[cfg(not(target_os = "windows"))]
    {
        tokio::task::spawn_blocking(|| {
            let mut apps = desktop_dir_entries();
            apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            Ok(apps)
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

/// Enumerate installed apps from the XDG applications dirs (`~/.local/share`
/// first so user entries shadow system ones, then each `XDG_DATA_DIRS` entry —
/// which on Fedora includes the Flatpak exports). Deduped by desktop-file ID.
#[cfg(not(target_os = "windows"))]
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

    #[cfg(not(target_os = "windows"))]
    {
        tokio::task::spawn_blocking(move || {
            super::desktop::launch_desktop_file(std::path::Path::new(&path))
        })
        .await
        .map_err(|e| e.to_string())?
    }
}
