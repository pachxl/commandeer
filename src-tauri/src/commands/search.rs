//! Global file search: self-hosted FTS5 index first, then the Everything
//! (voidtools) index when running, then a plain walkdir fallback. Also hosts
//! file_info (size/modified/thumbnail) for the detail pane.

use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct FileResult {
    pub name: String,
    pub path: String,
    pub icon: Option<String>,
}

fn is_current_executable(path: &str) -> bool {
    let Ok(current) = std::env::current_exe() else {
        return false;
    };
    let requested = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
    let current = std::fs::canonicalize(&current).unwrap_or(current);
    requested == current
}

fn embedded_app_icon() -> String {
    let png = include_bytes!("../../icons/128x128.png");
    format!(
        "data:image/png;base64,{}",
        crate::commands::fs::base64_encode(png)
    )
}

/// Shell icon for a single path, resolved lazily by the frontend (folder
/// search returns up to 50k entries — inlining a data URL per entry would
/// balloon the IPC payload, so rows fetch icons on demand and cache them
/// per extension client-side). Executables and shortcuts carry their own
/// embedded icon, so they resolve per file instead of per extension.
#[tauri::command]
pub async fn path_icon(path: String) -> Option<String> {
    // The running app's own process row must not depend on platform shell
    // caches or desktop-entry association. All builds embed this PNG generated
    // from the same favicon.svg as the ICNS/ICO/package artwork.
    if is_current_executable(&path) {
        return Some(embedded_app_icon());
    }

    #[cfg(target_os = "windows")]
    {
        tokio::task::spawn_blocking(move || {
            let lower = path.to_lowercase();
            if lower.starts_with("shell:") {
                // AppsFolder entries (app launcher) have no filesystem path
                crate::commands::icons::icon_for_shell_item(&path)
            } else if lower.ends_with(".exe") || lower.ends_with(".lnk") {
                crate::commands::icons::icon_for_file(&path)
            } else {
                crate::commands::icons::icon_for_path(&path)
            }
        })
        .await
        .ok()
        .flatten()
    }
    #[cfg(target_os = "macos")]
    {
        tokio::task::spawn_blocking(move || crate::commands::icons::icon_for_path(&path))
            .await
            .ok()
            .flatten()
    }

    #[cfg(target_os = "linux")]
    {
        tokio::task::spawn_blocking(move || linux_icons::cached_icon_for_path(&path))
            .await
            .ok()
            .flatten()
    }
}

#[cfg(test)]
mod app_icon_tests {
    #[test]
    fn current_process_uses_embedded_app_icon() {
        let current = std::env::current_exe().expect("current executable");
        assert!(super::is_current_executable(&current.to_string_lossy()));
        assert!(super::embedded_app_icon().starts_with("data:image/png;base64,"));
    }
}

/// Server-side icon cache mirroring the frontend's: keyed per extension for
/// ordinary files (one theme lookup covers every .rs file), per path for
/// .desktop entries and extensionless files, whose icons are individual.
#[cfg(target_os = "linux")]
mod linux_icons {
    use std::collections::HashMap;
    use std::sync::Mutex;

    static CACHE: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

    pub(super) fn cached_icon_for_path(path: &str) -> Option<String> {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let key = if ext.is_empty() || ext == "desktop" {
            path.to_string()
        } else {
            format!("ext:{ext}")
        };

        let mut guard = CACHE.lock().unwrap();
        let cache = guard.get_or_insert_with(HashMap::new);
        if let Some(hit) = cache.get(&key) {
            return hit.clone();
        }
        let icon = crate::commands::desktop::icon_for_path(path);
        cache.insert(key, icon.clone());
        icon
    }
}

/// Query the Everything (voidtools) index over its WM_COPYDATA IPC protocol.
/// No SDK DLL needed: we speak the wire format directly. Returns None when
/// Everything isn't running (callers fall back to the walkdir scan).
#[cfg(target_os = "windows")]
mod everything {
    use super::FileResult;
    use std::cell::RefCell;
    use std::sync::Once;
    use std::time::{Duration, Instant};
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::DataExchange::COPYDATASTRUCT;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW,
        MsgWaitForMultipleObjects, PeekMessageW, RegisterClassW, SendMessageTimeoutW,
        TranslateMessage, HWND_MESSAGE, MSG, PM_REMOVE, QS_ALLINPUT, SMTO_ABORTIFHUNG, SMTO_BLOCK,
        WINDOW_EX_STYLE, WINDOW_STYLE, WM_COPYDATA, WNDCLASSW,
    };

    /// dwData tag Everything echoes back in its reply so we can recognise it.
    const REPLY_ID: usize = 0xC0DE;
    /// EVERYTHING_IPC_COPYDATAQUERYW
    const COPYDATA_QUERYW: usize = 2;
    /// EVERYTHING_IPC_LISTW header: totitems, numitems, offset (3 × u32)
    const HEADER_SIZE: usize = 12;
    /// EVERYTHING_IPC_ITEMW: flags, filename_offset, path_offset (3 × u32)
    const ITEM_SIZE: usize = 12;

    // The reply lands in the wndproc on the thread that pumps the messages,
    // which is the same thread that runs query() — a thread-local hand-off
    // avoids any global state.
    thread_local! {
        static REPLY: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
    }

    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_COPYDATA {
            let cds = &*(lparam.0 as *const COPYDATASTRUCT);
            if cds.dwData == REPLY_ID {
                let bytes =
                    std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize)
                        .to_vec();
                REPLY.with(|r| *r.borrow_mut() = Some(bytes));
                return LRESULT(1);
            }
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    fn reply_class() -> PCWSTR {
        static REGISTER: Once = Once::new();
        let class = w!("commandeer_everything_ipc");
        REGISTER.call_once(|| unsafe {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
        });
        class
    }

    fn wide_str_at(data: &[u8], offset: usize) -> String {
        let mut out: Vec<u16> = Vec::new();
        let mut i = offset;
        while i + 1 < data.len() {
            let ch = u16::from_le_bytes([data[i], data[i + 1]]);
            if ch == 0 {
                break;
            }
            out.push(ch);
            i += 2;
        }
        String::from_utf16_lossy(&out)
    }

    fn parse_reply(data: &[u8]) -> Option<Vec<FileResult>> {
        let u32_at = |off: usize| -> Option<u32> {
            data.get(off..off + 4)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        };
        let numitems = u32_at(4)? as usize;
        let mut results = Vec::with_capacity(numitems);
        for i in 0..numitems {
            let base = HEADER_SIZE + i * ITEM_SIZE;
            let name_off = u32_at(base + 4)? as usize;
            let path_off = u32_at(base + 8)? as usize;
            let name = wide_str_at(data, name_off);
            let dir = wide_str_at(data, path_off);
            if name.is_empty() {
                continue;
            }
            let full = if dir.is_empty() {
                name.clone()
            } else {
                format!("{}\\{}", dir, name)
            };
            results.push(FileResult {
                name,
                path: full.replace('\\', "/"),
                icon: None,
            });
        }
        Some(results)
    }

    /// Send `search` to Everything and wait for the reply. Returns None if
    /// Everything is not running or doesn't answer within `timeout`.
    pub fn query(search: &str, max_results: u32, timeout: Duration) -> Option<Vec<FileResult>> {
        unsafe {
            let target = FindWindowW(w!("EVERYTHING_TASKBAR_NOTIFICATION"), PCWSTR::null()).ok()?;
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                reply_class(),
                PCWSTR::null(),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                None,
                None,
                None,
            )
            .ok()?;
            REPLY.with(|r| *r.borrow_mut() = None);

            // EVERYTHING_IPC_QUERYW: reply_hwnd (DWORD — handles fit in 32 bits),
            // reply_copydata_message, search_flags, offset, max_results, search…\0
            let mut buf: Vec<u8> = Vec::new();
            buf.extend_from_slice(&(hwnd.0 as usize as u32).to_le_bytes());
            buf.extend_from_slice(&(REPLY_ID as u32).to_le_bytes());
            buf.extend_from_slice(&0u32.to_le_bytes());
            buf.extend_from_slice(&0u32.to_le_bytes());
            buf.extend_from_slice(&max_results.to_le_bytes());
            for ch in search.encode_utf16().chain(std::iter::once(0)) {
                buf.extend_from_slice(&ch.to_le_bytes());
            }
            let cds = COPYDATASTRUCT {
                dwData: COPYDATA_QUERYW,
                cbData: buf.len() as u32,
                lpData: buf.as_ptr() as *mut _,
            };
            let sent = SendMessageTimeoutW(
                target,
                WM_COPYDATA,
                WPARAM(hwnd.0 as usize),
                LPARAM(&cds as *const _ as isize),
                SMTO_BLOCK | SMTO_ABORTIFHUNG,
                timeout.as_millis() as u32,
                None,
            );

            let reply = if sent.0 == 0 {
                None
            } else {
                let deadline = Instant::now() + timeout;
                loop {
                    let mut msg = MSG::default();
                    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    if let Some(bytes) = REPLY.with(|r| r.borrow_mut().take()) {
                        break Some(bytes);
                    }
                    let now = Instant::now();
                    if now >= deadline {
                        break None;
                    }
                    MsgWaitForMultipleObjects(
                        None,
                        false,
                        (deadline - now).as_millis() as u32,
                        QS_ALLINPUT,
                    );
                }
            };
            let _ = DestroyWindow(hwnd);
            reply.as_deref().and_then(parse_reply)
        }
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        #[cfg(target_os = "windows")]
        {
            if let Ok(userprofile) = std::env::var("USERPROFILE") {
                return PathBuf::from(userprofile).join(rest);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            if let Ok(home) = std::env::var("HOME") {
                return PathBuf::from(home).join(rest);
            }
        }
    }
    PathBuf::from(path)
}

#[tauri::command]
pub async fn search_files(
    query: String,
    paths: Vec<String>,
    index: tauri::State<'_, crate::commands::file_index::FileIndex>,
) -> Result<Vec<FileResult>, String> {
    if query.is_empty() {
        return Ok(vec![]);
    }

    let index = (*index).clone();
    tokio::task::spawn_blocking(move || {
        // 1. Prefer the self-hosted SQLite+FTS5 index.
        match index.search(&query, 100) {
            Ok(results) if !results.is_empty() => {
                let mut out: Vec<FileResult> = results
                    .into_iter()
                    .map(|r| FileResult {
                        name: Path::new(&r.path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .to_string(),
                        path: r.path,
                        icon: None,
                    })
                    .collect();
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                for r in &mut out {
                    r.icon = crate::commands::icons::icon_for_path(&r.path);
                }
                #[cfg(target_os = "linux")]
                for r in &mut out {
                    r.icon = linux_icons::cached_icon_for_path(&r.path);
                }
                return Ok(out);
            }
            _ => {}
        }

        // 2. Fall back to the Everything (voidtools) index when it's running.
        #[cfg(target_os = "windows")]
        {
            let scoped = if paths.is_empty() {
                query.clone()
            } else {
                let scopes: Vec<String> = paths
                    .iter()
                    .map(|p| {
                        let mut win = expand_tilde(p).to_string_lossy().replace('/', "\\");
                        if !win.ends_with('\\') {
                            win.push('\\');
                        }
                        format!("\"{}\"", win)
                    })
                    .collect();
                format!("<{}> {}", scopes.join("|"), query)
            };
            if let Some(mut results) =
                everything::query(&scoped, 50, std::time::Duration::from_millis(600))
            {
                for r in &mut results {
                    r.icon = crate::commands::icons::icon_for_path(&r.path);
                }
                return Ok(results);
            }
        }

        let query_lower = query.to_lowercase();
        let mut results: Vec<FileResult> = Vec::new();
        let max_results = 20;

        let search_roots: Vec<PathBuf> = if paths.is_empty() {
            #[cfg(target_os = "windows")]
            {
                let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
                vec![
                    PathBuf::from(&home).join("Desktop"),
                    PathBuf::from(&home).join("Documents"),
                    PathBuf::from(&home).join("Downloads"),
                ]
            }
            #[cfg(not(target_os = "windows"))]
            {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                vec![
                    PathBuf::from(&home).join("Desktop"),
                    PathBuf::from(&home).join("Documents"),
                    PathBuf::from(&home).join("Downloads"),
                ]
            }
        } else {
            paths.iter().map(|p| expand_tilde(p)).collect()
        };

        for root in search_roots {
            if !root.exists() {
                continue;
            }
            let walker = walkdir::WalkDir::new(&root)
                .max_depth(4)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok());

            for entry in walker {
                if results.len() >= max_results {
                    break;
                }
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if name.contains(&query_lower) {
                    results.push(FileResult {
                        name: path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .to_string(),
                        path: path.to_string_lossy().replace('\\', "/"),
                        icon: None,
                    });
                }
            }
            if results.len() >= max_results {
                break;
            }
        }

        #[cfg(any(target_os = "windows", target_os = "macos"))]
        for r in &mut results {
            r.icon = crate::commands::icons::icon_for_path(&r.path);
        }
        #[cfg(target_os = "linux")]
        for r in &mut results {
            r.icon = linux_icons::cached_icon_for_path(&r.path);
        }

        Ok(results)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
pub struct FileInfo {
    pub size: u64,
    pub modified: Option<String>,
    pub is_dir: bool,
    /// Data URL of the raw image for common formats (detail-pane thumbnail)
    pub thumbnail: Option<String>,
}

#[tauri::command]
pub async fn file_info(path: String) -> Result<FileInfo, String> {
    tokio::task::spawn_blocking(move || {
        let p = Path::new(&path);
        let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
        let modified = meta
            .modified()
            .ok()
            .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339());

        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let mime = match ext.as_str() {
            "png" => Some("image/png"),
            "jpg" | "jpeg" => Some("image/jpeg"),
            "gif" => Some("image/gif"),
            "webp" => Some("image/webp"),
            "bmp" => Some("image/bmp"),
            "ico" => Some("image/x-icon"),
            _ => None,
        };
        let thumbnail = match mime {
            Some(mime) if !meta.is_dir() && meta.len() < 5 * 1024 * 1024 => {
                std::fs::read(p).ok().map(|bytes| {
                    format!(
                        "data:{};base64,{}",
                        mime,
                        crate::commands::fs::base64_encode(&bytes)
                    )
                })
            }
            _ => None,
        };

        Ok(FileInfo {
            size: meta.len(),
            modified,
            is_dir: meta.is_dir(),
            thumbnail,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
