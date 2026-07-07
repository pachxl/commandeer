//! Cross-platform browser bookmarks reader.
//!
//! Chromium-based browsers keep bookmarks in a JSON `Bookmarks` file inside the
//! profile directory; Firefox keeps them in `places.sqlite`. We read whichever
//! files exist and return a flat, deduplicated list.

use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tauri::Manager;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Bookmark {
    pub name: String,
    pub url: String,
    pub browser: String,
}

#[derive(Debug, Deserialize)]
struct ChromiumRoot {
    roots: serde_json::Value,
}

fn walk_chromium_node(node: &serde_json::Value, out: &mut Vec<Bookmark>, browser: &str) {
    if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
        for child in children {
            if let Some(type_str) = child.get("type").and_then(|t| t.as_str()) {
                if type_str == "url" {
                    if let (Some(name), Some(url)) = (
                        child.get("name").and_then(|n| n.as_str()),
                        child.get("url").and_then(|u| u.as_str()),
                    ) {
                        if !name.is_empty() && !url.is_empty() {
                            out.push(Bookmark {
                                name: name.to_string(),
                                url: url.to_string(),
                                browser: browser.to_string(),
                            });
                        }
                    }
                } else if type_str == "folder" {
                    walk_chromium_node(child, out, browser);
                }
            }
        }
    }
}

fn read_chromium_bookmarks(path: &Path, browser: &str, out: &mut Vec<Bookmark>) {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(root): Result<ChromiumRoot, _> = serde_json::from_str(&raw) else {
        return;
    };
    if let Some(obj) = root.roots.as_object() {
        for (_, node) in obj {
            walk_chromium_node(node, out, browser);
        }
    }
}

/// Read every bookmarks store inside a single Chromium profile directory.
///
/// A local-only profile writes a plain `Bookmarks` JSON file. A signed-in
/// profile stores its account-synced bookmarks in `AccountBookmarks` instead
/// (and may not write `Bookmarks` at all). Both use the identical schema, so we
/// read whichever exist; the later dedup-by-URL collapses any overlap.
fn read_chromium_profile(profile_dir: &Path, browser: &str, out: &mut Vec<Bookmark>) {
    read_chromium_bookmarks(&profile_dir.join("Bookmarks"), browser, out);
    read_chromium_bookmarks(&profile_dir.join("AccountBookmarks"), browser, out);
}

fn firefox_profile_dir(parent: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(parent).ok()?;
    let mut fallback = None;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.ends_with(".default-release") || name_str.ends_with(".default") {
            // Prefer a release profile over a dev edition / nightly default.
            if name_str.contains("default-release") {
                return Some(entry.path());
            }
            fallback = Some(entry.path());
        }
    }
    fallback
}

fn read_firefox_bookmarks(profile_dir: &Path, out: &mut Vec<Bookmark>) {
    let places = profile_dir.join("places.sqlite");
    if !places.exists() {
        return;
    }

    // Firefox may have the database locked; copy it to a transient location
    // inside the app cache before reading.
    let cache_dir = std::env::temp_dir().join("commandeer_bookmarks");
    let _ = std::fs::create_dir_all(&cache_dir);
    let tmp = cache_dir.join("places.sqlite");
    let _ = std::fs::remove_file(&tmp);
    if std::fs::copy(&places, &tmp).is_err() {
        return;
    }

    let Ok(conn) = rusqlite::Connection::open_with_flags(
        &tmp,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) else {
        return;
    };

    let mut stmt = match conn.prepare(
        "SELECT b.title, p.url
         FROM moz_bookmarks b
         JOIN moz_places p ON b.fk = p.id
         WHERE b.type = 1 AND p.url NOT NULL AND p.url <> ''"
    ) {
        Ok(s) => s,
        Err(_) => return,
    };

    let rows = stmt.query_map([], |row| {
        let title: Option<String> = row.get(0)?;
        let url: String = row.get(1)?;
        Ok((title.unwrap_or_default(), url))
    });

    if let Ok(rows) = rows {
        for row in rows.flatten() {
            let (title, url) = row;
            let name = if title.trim().is_empty() {
                url.clone()
            } else {
                title
            };
            out.push(Bookmark {
                name,
                url,
                browser: "Firefox".to_string(),
            });
        }
    }
}

fn chromium_browser_paths(home: &Path) -> Vec<(String, PathBuf)> {
    // Windows resolves via %LOCALAPPDATA% rather than the home dir.
    #[cfg(target_os = "windows")]
    let _ = home;
    let mut browsers: Vec<(String, PathBuf)> = Vec::new();

    #[cfg(target_os = "macos")]
    {
        let base = home.join("Library/Application Support");
        browsers.push(("Chrome".to_string(), base.join("Google/Chrome")));
        browsers.push(("Chrome Canary".to_string(), base.join("Google/Chrome Canary")));
        browsers.push(("Edge".to_string(), base.join("Microsoft Edge")));
        browsers.push(("Brave".to_string(), base.join("BraveSoftware/Brave-Browser")));
        browsers.push(("Arc".to_string(), base.join("Arc/User Data")));
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let base = PathBuf::from(local_app_data);
            // On Windows, Chromium browsers nest their profiles under a
            // "User Data" subdirectory (macOS/Linux put them at the root).
            browsers.push(("Chrome".to_string(), base.join("Google/Chrome/User Data")));
            browsers.push((
                "Chrome Canary".to_string(),
                base.join("Google/Chrome SxS/User Data"),
            ));
            browsers.push(("Edge".to_string(), base.join("Microsoft/Edge/User Data")));
            browsers.push((
                "Brave".to_string(),
                base.join("BraveSoftware/Brave-Browser/User Data"),
            ));
        }
    }

    #[cfg(target_os = "linux")]
    {
        let base = home.join(".config");
        browsers.push(("Chrome".to_string(), base.join("google-chrome")));
        browsers.push(("Chrome Canary".to_string(), base.join("google-chrome-canary")));
        browsers.push(("Edge".to_string(), base.join("microsoft-edge")));
        browsers.push(("Brave".to_string(), base.join("BraveSoftware/Brave-Browser")));
        browsers.push(("Chromium".to_string(), base.join("chromium")));
    }

    browsers
}

fn firefox_parent_dir(home: &Path) -> Option<PathBuf> {
    // Windows resolves via %APPDATA% rather than the home dir.
    #[cfg(target_os = "windows")]
    let _ = home;
    #[cfg(target_os = "macos")]
    {
        Some(home.join("Library/Application Support/Firefox/Profiles"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .ok()
            .map(|p| PathBuf::from(p).join("Mozilla/Firefox/Profiles"))
    }
    #[cfg(target_os = "linux")]
    {
        Some(home.join(".mozilla/firefox"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        None
    }
}

#[tauri::command]
pub async fn list_bookmarks(app: tauri::AppHandle) -> Result<Vec<Bookmark>, String> {
    tokio::task::spawn_blocking(move || {
        let home = app
            .path()
            .home_dir()
            .map_err(|e| e.to_string())?;

        let mut out: Vec<Bookmark> = Vec::new();

        for (name, root) in chromium_browser_paths(&home) {
            // Some Chromium variants keep profiles under "Default", others use
            // "Profile 1", etc. Read Default if present; also read Profile N.
            let default_profile = root.join("Default");
            if default_profile.exists() {
                read_chromium_profile(&default_profile, &name, &mut out);
            }
            if let Ok(entries) = std::fs::read_dir(&root) {
                for entry in entries.flatten() {
                    let file_name = entry.file_name();
                    let fname = file_name.to_string_lossy();
                    if fname.starts_with("Profile ") {
                        read_chromium_profile(&entry.path(), &name, &mut out);
                    }
                }
            }
        }

        if let Some(parent) = firefox_parent_dir(&home) {
            if let Some(profile) = firefox_profile_dir(&parent) {
                read_firefox_bookmarks(&profile, &mut out);
            }
        }

        // Dedupe by URL, keeping the first (usually Chrome before Edge, etc.).
        let mut seen = HashSet::new();
        out.retain(|b| seen.insert(b.url.clone()));

        out.sort_by_key(|a| a.name.to_lowercase());
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}
