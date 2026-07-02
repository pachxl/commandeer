//! File search in the active Explorer folder: resolve which folder was open
//! in the File Explorer window that had focus before the palette was shown
//! (via the IShellWindows COM enumeration), and list a folder's contents
//! recursively for the palette to filter client-side.

use serde::Serialize;
use std::sync::Mutex;

#[derive(Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    /// Path relative to the searched folder (used for display + matching)
    pub rel: String,
    pub is_dir: bool,
}

/// Turn an Explorer LocationURL (`file:///C:/Users/...`, percent-encoded
/// UTF-8) into a Windows path. Virtual locations (This PC, Recycle Bin, ...)
/// have non-file URLs and yield None.
#[cfg(target_os = "windows")]
fn decode_file_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("file://")?;

    let bytes = rest.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    let s = String::from_utf8(decoded).ok()?;

    // file:///C:/dir -> "/C:/dir" (strip the slash); file://server/share stays
    // host-first and becomes a UNC path.
    let path = if let Some(local) = s.strip_prefix('/') {
        local.replace('/', "\\")
    } else {
        format!("\\\\{}", s.replace('/', "\\"))
    };
    if path.is_empty() { None } else { Some(path) }
}

/// Find the Explorer window whose HWND matches `target` and return its
/// current folder path.
#[cfg(target_os = "windows")]
fn location_for_hwnd(target: isize) -> Option<String> {
    use windows::core::{Interface, VARIANT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{IShellWindows, IWebBrowser2, ShellWindows};

    unsafe {
        // May return S_FALSE if the thread is already initialized; harmless.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let shell: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL).ok()?;
        let count = shell.Count().unwrap_or(0);
        for i in 0..count {
            let Ok(disp) = shell.Item(&VARIANT::from(i)) else { continue };
            let Ok(browser) = disp.cast::<IWebBrowser2>() else { continue };
            let Ok(hwnd) = browser.HWND() else { continue };
            if hwnd.0 != target {
                continue;
            }
            let url = browser.LocationURL().ok()?;
            return decode_file_url(&url.to_string());
        }
        None
    }
}

/// Folder resolved at the moment the palette was shown (see
/// `capture_location`); None until the resolve lands or when the previous
/// window wasn't an Explorer folder view.
static LAST_LOCATION: Mutex<Option<String>> = Mutex::new(None);

/// Called right before the palette is shown (after `capture_foreground`).
/// Clears the snapshot synchronously — so readers can never see a stale
/// folder from the previous show — then resolves the new one on a worker
/// thread (COM enumeration, typically ~ms, lands well before the frontend
/// asks).
pub fn capture_location() {
    *LAST_LOCATION.lock().unwrap() = None;

    #[cfg(target_os = "windows")]
    {
        let hwnd = super::paste::previous_foreground();
        if hwnd == 0 {
            return;
        }
        std::thread::spawn(move || {
            let loc = location_for_hwnd(hwnd);
            *LAST_LOCATION.lock().unwrap() = loc;
        });
    }
}

/// Folder open in the File Explorer window that was focused when the palette
/// was shown. Instant: returns the snapshot taken at show time.
#[tauri::command]
pub fn explorer_location() -> Option<String> {
    LAST_LOCATION.lock().unwrap().clone()
}

/// Dependency/VCS/cache directories that are never worth searching. Pruned at
/// descent time so the walker skips the whole subtree.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    ".svn",
    ".hg",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    "bower_components",
    ".pnpm-store",
    ".yarn",
    "vendor",
    "Pods",
    ".gradle",
    ".m2",
    ".nuget",
    ".cargo",
    ".terraform",
    ".bundle",
];

fn is_skipped_dir(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    SKIP_DIRS.iter().any(|s| name.eq_ignore_ascii_case(s))
}

/// Recursively list everything under `path`, capped at `max` entries.
/// Breadth-first, one level at a time with the directory reads of each level
/// parallelized (rayon) — so when the cap is hit, it's always the deepest
/// entries that are cut, never shallow ones (a depth-first walk could burn
/// the whole budget inside one big early subtree and miss shallow files).
/// Symlinks/junctions are not descended into.
#[tauri::command]
pub async fn list_files_recursive(path: String, max: usize) -> Result<Vec<FileEntry>, String> {
    use rayon::prelude::*;

    tokio::task::spawn_blocking(move || {
        let root = std::path::PathBuf::from(&path);
        if !root.is_dir() {
            return Err(format!("Not a folder: {path}"));
        }

        let mut out: Vec<FileEntry> = Vec::new();
        let mut level: Vec<std::path::PathBuf> = vec![root.clone()];

        while !level.is_empty() && out.len() < max {
            let mut entries: Vec<(std::path::PathBuf, String, bool)> = level
                .par_iter()
                .flat_map_iter(|dir| {
                    let mut found = Vec::new();
                    if let Ok(rd) = std::fs::read_dir(dir) {
                        for e in rd.flatten() {
                            let Ok(ft) = e.file_type() else { continue };
                            // Junctions/symlinks report is_symlink; skip to avoid cycles
                            if ft.is_symlink() {
                                continue;
                            }
                            if ft.is_dir() && is_skipped_dir(&e.file_name()) {
                                continue;
                            }
                            found.push((
                                e.path(),
                                e.file_name().to_string_lossy().into_owned(),
                                ft.is_dir(),
                            ));
                        }
                    }
                    found
                })
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));

            level = Vec::new();
            for (p, name, is_dir) in entries {
                if out.len() >= max {
                    break;
                }
                let rel = p
                    .strip_prefix(&root)
                    .map(|r| r.to_string_lossy().into_owned())
                    .unwrap_or_default();
                out.push(FileEntry {
                    name,
                    path: p.to_string_lossy().into_owned(),
                    rel,
                    is_dir,
                });
                if is_dir {
                    level.push(p);
                }
            }
        }

        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}
