use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::Manager;

use super::crypto;

const MAX_HISTORY: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardItem {
    pub id: String,
    pub text: String,
    pub copied_at: String,
}

fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn db_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join("clipboard.db"))
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[derive(Clone)]
pub struct ClipboardDb {
    conn: Arc<Mutex<Connection>>,
}

impl ClipboardDb {
    pub fn new(app: &tauri::AppHandle) -> Result<Self, String> {
        let path = db_path(app)?;
        let conn = Connection::open(&path).map_err(|e| e.to_string())?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS clipboard_history (
                id TEXT PRIMARY KEY,
                text BLOB NOT NULL,
                copied_at TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_copied_at ON clipboard_history(copied_at)",
            [],
        )
        .map_err(|e| e.to_string())?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.migrate_json(app)?;
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        db.migrate_plaintext()?;
        Ok(db)
    }

    /// One-time (idempotent) re-encryption of rows written before encryption
    /// existed on Linux/macOS. `decrypt` passes unmarked rows through, so this
    /// is safe to interrupt and re-run.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn migrate_plaintext(&self) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        {
            let mut stmt = tx
                .prepare("SELECT id, text FROM clipboard_history")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
                })
                .map_err(|e| e.to_string())?;
            let plaintext: Vec<(String, Vec<u8>)> = rows
                .filter_map(|r| r.ok())
                .filter(|(_, blob)| !crypto::is_encrypted(blob))
                .collect();
            for (id, blob) in plaintext {
                let encrypted = crypto::encrypt(&blob)?;
                tx.execute(
                    "UPDATE clipboard_history SET text = ? WHERE id = ?",
                    params![&encrypted, &id],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())
    }

    /// Decrypt a stored row. On Windows a failure is fatal (DPAPI always
    /// roundtrips for the same user); on Linux/macOS a lost key must not brick
    /// the whole history, so undecryptable rows are skipped via `None`.
    fn decrypt_row(encrypted: &[u8]) -> Result<Option<String>, String> {
        let result = crypto::decrypt(encrypted).and_then(|bytes| {
            String::from_utf8(bytes).map_err(|e| format!("invalid UTF-8 in clipboard entry: {e}"))
        });
        match result {
            Ok(text) => Ok(Some(text)),
            #[cfg(target_os = "windows")]
            Err(e) => Err(e),
            #[cfg(not(target_os = "windows"))]
            Err(_) => Ok(None),
        }
    }

    /// One-time migration from the legacy plaintext clipboard.json file.
    fn migrate_json(&self, app: &tauri::AppHandle) -> Result<(), String> {
        let json_path = data_dir(app)?.join("clipboard.json");
        if !json_path.exists() {
            return Ok(());
        }
        let raw = fs::read_to_string(&json_path).map_err(|e| e.to_string())?;
        let items: Vec<ClipboardItem> = serde_json::from_str(&raw).unwrap_or_default();

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        for item in items {
            let encrypted = crypto::encrypt(item.text.as_bytes())?;
            let _ = tx.execute(
                "INSERT OR IGNORE INTO clipboard_history (id, text, copied_at) VALUES (?, ?, ?)",
                params![&item.id, &encrypted, &item.copied_at],
            );
        }
        tx.commit().map_err(|e| e.to_string())?;

        // Rename the legacy file so the migration never runs again.
        let mut migrated = json_path.clone();
        migrated.set_extension("json.migrated");
        let _ = fs::rename(&json_path, migrated);
        Ok(())
    }

    /// Read and decrypt up to `limit` history items, newest first.
    pub fn read_all(&self, limit: usize) -> Result<Vec<ClipboardItem>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, text, copied_at FROM clipboard_history
                 ORDER BY copied_at DESC LIMIT ?",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;

        let mut items = Vec::new();
        for row in rows {
            let (id, encrypted, copied_at) = row.map_err(|e| e.to_string())?;
            let Some(text) = Self::decrypt_row(&encrypted)? else {
                continue;
            };
            items.push(ClipboardItem { id, text, copied_at });
        }
        Ok(items)
    }

    /// Insert a new clipboard entry, removing any existing duplicates and
    /// keeping the history at `MAX_HISTORY` items.
    pub fn record(&self, text: &str) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| e.to_string())?;

        // Find duplicates by decrypting existing entries. The history is small
        // (MAX_HISTORY items), so this is cheap and avoids storing plaintext
        // hashes for comparison.
        let mut stmt = tx
            .prepare(
                "SELECT id, text, copied_at FROM clipboard_history ORDER BY copied_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;

        let mut duplicate_ids: Vec<String> = Vec::new();
        let mut kept_count: usize = 0;
        for row in rows {
            let (id, encrypted, _copied_at) = row.map_err(|e| e.to_string())?;
            let Some(decrypted) = Self::decrypt_row(&encrypted)? else {
                // Undecryptable row: not a duplicate, still occupies a slot
                // until the size cap ages it out.
                kept_count += 1;
                continue;
            };
            if decrypted == text {
                duplicate_ids.push(id);
            } else {
                kept_count += 1;
            }
        }
        drop(stmt);

        for id in &duplicate_ids {
            tx.execute("DELETE FROM clipboard_history WHERE id = ?", [id])
                .map_err(|e| e.to_string())?;
        }

        let encrypted = crypto::encrypt(text.as_bytes())?;
        let id = uuid::Uuid::new_v4().to_string();
        let copied_at = now_iso();
        tx.execute(
            "INSERT INTO clipboard_history (id, text, copied_at) VALUES (?, ?, ?)",
            params![&id, &encrypted, &copied_at],
        )
        .map_err(|e| e.to_string())?;

        // Truncate oldest items if we're over the limit.
        if kept_count + 1 > MAX_HISTORY {
            let excess = (kept_count + 1 - MAX_HISTORY) as i64;
            tx.execute(
                "DELETE FROM clipboard_history WHERE id IN (
                    SELECT id FROM clipboard_history ORDER BY copied_at ASC LIMIT ?
                )",
                [excess],
            )
            .map_err(|e| e.to_string())?;
        }

        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Remove all history entries.
    pub fn clear(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM clipboard_history", [])
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
