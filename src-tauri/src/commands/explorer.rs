//! File search in the active Explorer folder: resolve which folder was open
//! in the File Explorer window that had focus before the palette was shown
//! (via the IShellWindows COM enumeration), and list a folder's contents
//! recursively for the palette to filter client-side.

use serde::Serialize;

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

/// Folder open in the File Explorer window that was focused when the palette
/// was shown, or None if that window isn't an Explorer folder view.
#[tauri::command]
pub async fn explorer_location() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let hwnd = super::paste::previous_foreground();
        if hwnd == 0 {
            return None;
        }
        // COM enumeration off the async runtime thread
        tokio::task::spawn_blocking(move || location_for_hwnd(hwnd))
            .await
            .ok()
            .flatten()
    }

    #[cfg(not(target_os = "windows"))]
    None
}

/// Recursively list everything under `path`, capped at `max` entries.
/// Parallel walk (jwalk), symlinks/junctions not followed. Sorted shallowest
/// first so the palette's empty-query view shows the folder's top level.
#[tauri::command]
pub async fn list_files_recursive(path: String, max: usize) -> Result<Vec<FileEntry>, String> {
    tokio::task::spawn_blocking(move || {
        let root = std::path::PathBuf::from(&path);
        if !root.is_dir() {
            return Err(format!("Not a folder: {path}"));
        }

        let mut out: Vec<(usize, FileEntry)> = Vec::new();
        for entry in jwalk::WalkDir::new(&root)
            .skip_hidden(false)
            .follow_links(false)
        {
            let Ok(e) = entry else { continue };
            if e.depth() == 0 {
                continue;
            }
            let p = e.path();
            let rel = p
                .strip_prefix(&root)
                .map(|r| r.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.push((
                e.depth(),
                FileEntry {
                    name: e.file_name().to_string_lossy().into_owned(),
                    path: p.to_string_lossy().into_owned(),
                    rel,
                    is_dir: e.file_type().is_dir(),
                },
            ));
            if out.len() >= max {
                break;
            }
        }

        out.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.rel.cmp(&b.1.rel)));
        Ok(out.into_iter().map(|(_, f)| f).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}
