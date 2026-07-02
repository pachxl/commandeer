//! Self-hosted file index: SQLite + FTS5 (trigram tokenizer) with live
//! filesystem watcher. Replaces the hard dependency on voidtools Everything.
//!
//! Architecture:
//!   - `path_idx` is a contentless FTS5 virtual table using the built-in
//!     trigram tokenizer for typo-tolerant substring matching.
//!   - `indexed_file` stores file metadata keyed by the same rowid.
//!   - A background thread seeds the index from configured roots and then
//!     applies watcher deltas with a small IO pacer so first-run scans are
//!     polite.
//!   - `search_files` in launcher.rs tries this index first, then Everything,
//!     then a plain walkdir fallback.

use notify::Watcher;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const MAX_RESULTS: usize = 100;
const BATCH_SIZE: usize = 256;
const IO_PACE_MS: u64 = 2;
const SCAN_DEPTH: usize = 8;

#[derive(Debug, Clone, Serialize)]
pub struct IndexedFile {
    pub path: String,
    pub modified: i64,
    pub size: i64,
}

fn db_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("file_index.db"))
}

fn open_db(app: &AppHandle) -> Result<Connection, String> {
    let path = db_path(app)?;
    let conn = Connection::open(&path).map_err(|e| e.to_string())?;
    init_schema(&conn)?;
    Ok(conn)
}

/// Create (or migrate) the index schema on `conn`. Split out from `open_db` so
/// the DELETE-dependent code paths can be exercised against an in-memory DB.
fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS indexed_file (
            rowid INTEGER PRIMARY KEY,
            path TEXT UNIQUE NOT NULL,
            modified INTEGER NOT NULL,
            size INTEGER NOT NULL
        )",
        [],
    )
    .map_err(|e| e.to_string())?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_path ON indexed_file(path)",
        [],
    )
    .map_err(|e| e.to_string())?;

    // The FTS5 index must be created with `contentless_delete=1`. A plain
    // contentless (`content=''`) table rejects DELETE ("cannot DELETE from
    // contentless fts5 table"), which would silently break every incremental
    // update, rename, removal, and re-scan after the first population. If an
    // older build created the table without it, drop and rebuild — the index is
    // a regenerable cache, and clearing indexed_file keeps the two tables'
    // rowids in lockstep when the next scan repopulates them.
    let existing_ddl: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'path_idx'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(ddl) = existing_ddl {
        if !ddl.contains("contentless_delete") {
            conn.execute("DROP TABLE path_idx", [])
                .map_err(|e| e.to_string())?;
            conn.execute("DELETE FROM indexed_file", [])
                .map_err(|e| e.to_string())?;
        }
    }
    conn.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS path_idx USING fts5(path, content='', contentless_delete=1, tokenize='trigram')",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Clone)]
pub struct FileIndex {
    conn: Arc<Mutex<Connection>>,
}

impl FileIndex {
    pub fn new(app: &AppHandle) -> Result<Self, String> {
        let conn = open_db(app)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Search the index for paths matching `query`. Returns up to `limit`
    /// candidates ordered by FTS5 rank; callers (frontend) apply fuzzy ranking
    /// and file-relevance multipliers on top.
    ///
    /// The query is split on whitespace and each term is matched independently
    /// (AND), so `downloads pdf` finds `Downloads/report.pdf` rather than only
    /// paths containing the literal substring "downloads pdf". Terms of 3+ chars
    /// go through the trigram FTS5 index; shorter terms (which the trigram
    /// tokenizer cannot represent) fall back to a `LIKE` substring filter so a
    /// 1–2 char query still returns results instead of silently nothing.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<IndexedFile>, String> {
        let terms: Vec<String> = query.split_whitespace().map(|t| t.to_lowercase()).collect();
        if terms.is_empty() {
            return Ok(vec![]);
        }
        let (long, short): (Vec<String>, Vec<String>) =
            terms.into_iter().partition(|t| t.chars().count() >= 3);

        let conn = self.conn.lock().unwrap();

        if long.is_empty() {
            // All terms are 1–2 chars: the trigram index can't help, so scan
            // indexed_file with a LIKE-per-term filter, shortest path first as a
            // cheap relevance proxy.
            return like_only_search(&conn, &short, limit);
        }

        // Candidate rowids from the trigram index: AND the long terms as
        // separate phrases. Over-fetch when short terms will further filter.
        let match_expr = long
            .iter()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ");
        let fetch = if short.is_empty() { limit } else { limit * 5 };
        let mut stmt = conn
            .prepare("SELECT rowid FROM path_idx WHERE path MATCH ? ORDER BY rank LIMIT ?")
            .map_err(|e| e.to_string())?;
        let rowids: Vec<i64> = stmt
            .query_map(params![&match_expr, fetch as i64], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        drop(stmt);

        if rowids.is_empty() {
            return Ok(vec![]);
        }

        let placeholders: Vec<String> = rowids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "SELECT rowid, path, modified, size FROM indexed_file WHERE rowid IN ({})",
            placeholders.join(",")
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let params: Vec<&dyn rusqlite::ToSql> =
            rowids.iter().map(|r| r as &dyn rusqlite::ToSql).collect();
        let mut by_rowid: std::collections::HashMap<i64, IndexedFile> = stmt
            .query_map(&params[..], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    IndexedFile {
                        path: row.get(1)?,
                        modified: row.get(2)?,
                        size: row.get(3)?,
                    },
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<std::collections::HashMap<_, _>, _>>()
            .map_err(|e| e.to_string())?;

        // Preserve FTS5 rank order, drop any candidate missing a short term, and
        // truncate back to the requested limit.
        let ordered: Vec<IndexedFile> = rowids
            .into_iter()
            .filter_map(|rowid| by_rowid.remove(&rowid))
            .filter(|f| {
                let lower = f.path.to_lowercase();
                short.iter().all(|t| lower.contains(t.as_str()))
            })
            .take(limit)
            .collect();
        Ok(ordered)
    }

    /// Remove a path from the index.
    fn remove_path(&self, path: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let rowid: Option<i64> = conn
            .query_row(
                "SELECT rowid FROM indexed_file WHERE path = ?",
                [path],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(rowid) = rowid {
            conn.execute(
                "DELETE FROM path_idx WHERE rowid = ?",
                [rowid],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "DELETE FROM indexed_file WHERE rowid = ?",
                [rowid],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Add or update a single path in the index.
    fn upsert_path(&self, path: &str, modified: i64, size: i64) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let existing: Option<i64> = conn
            .query_row(
                "SELECT rowid FROM indexed_file WHERE path = ?",
                [path],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;

        if let Some(rowid) = existing {
            conn.execute("DELETE FROM path_idx WHERE rowid = ?", [rowid])
                .map_err(|e| e.to_string())?;
            conn.execute("DELETE FROM indexed_file WHERE rowid = ?", [rowid])
                .map_err(|e| e.to_string())?;
        } else {
            conn.execute(
                "INSERT INTO path_idx(path) VALUES (?)",
                [path],
            )
            .map_err(|e| e.to_string())?;
            let rowid = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO indexed_file(rowid, path, modified, size) VALUES (?, ?, ?, ?)",
                params![rowid, path, modified, size],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Mark all entries under a directory prefix as needing validation; paths
    /// whose files no longer exist are removed. Called after a rename/remove.
    fn invalidate_prefix(&self, prefix: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT rowid, path FROM indexed_file WHERE path LIKE ? || '%'")
            .map_err(|e| e.to_string())?;
        let rows: Vec<(i64, String)> = stmt
            .query_map([prefix], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        drop(stmt);

        for (rowid, path) in rows {
            if !Path::new(&path).exists() {
                conn.execute("DELETE FROM path_idx WHERE rowid = ?", [rowid])
                    .map_err(|e| e.to_string())?;
                conn.execute("DELETE FROM indexed_file WHERE rowid = ?", [rowid])
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
}

/// Substring search over indexed_file for queries the trigram index can't serve
/// (all terms under 3 chars). ANDs a `LIKE '%term%'` per term and orders by path
/// length so the shortest (usually most relevant) paths come first.
fn like_only_search(
    conn: &Connection,
    terms: &[String],
    limit: usize,
) -> Result<Vec<IndexedFile>, String> {
    if terms.is_empty() {
        return Ok(vec![]);
    }
    let clause = terms
        .iter()
        .map(|_| "LOWER(path) LIKE ? ESCAPE '\\'")
        .collect::<Vec<_>>()
        .join(" AND ");
    let sql = format!(
        "SELECT path, modified, size FROM indexed_file
         WHERE {clause}
         ORDER BY LENGTH(path) ASC
         LIMIT ?"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = terms
        .iter()
        .map(|t| Box::new(format!("%{}%", escape_like(t))) as Box<dyn rusqlite::ToSql>)
        .collect();
    args.push(Box::new(limit as i64));
    let param_refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
    let rows: Vec<IndexedFile> = stmt
        .query_map(&param_refs[..], |row| {
            Ok(IndexedFile {
                path: row.get(0)?,
                modified: row.get(1)?,
                size: row.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// Escape the LIKE metacharacters `%`, `_`, and the escape char itself so a term
/// is matched literally. Pairs with `ESCAPE '\'` in the query.
fn escape_like(term: &str) -> String {
    term.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn is_significant_event(event: &notify::Event) -> bool {
    use notify::event::ModifyKind;
    use notify::EventKind::*;
    match event.kind {
        Create(_) | Remove(_) => true,
        Modify(ModifyKind::Data(_)) => true,
        Modify(ModifyKind::Metadata(_)) => false,
        Modify(ModifyKind::Name(_)) => true,
        Modify(ModifyKind::Other) => true,
        Modify(ModifyKind::Any) => true,
        Any | Access(_) | Other => true,
    }
}

fn should_index(path: &Path) -> bool {
    // Skip hidden files/dirs, obvious temp dirs, and very deep paths.
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.starts_with('.') || name == "$RECYCLE.BIN" || name == "node_modules" {
            return false;
        }
    }
    true
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

fn default_roots() -> Vec<PathBuf> {
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
}

fn collect_roots(configured: &[String]) -> Vec<PathBuf> {
    if configured.is_empty() {
        default_roots()
    } else {
        configured.iter().map(|p| expand_tilde(p)).collect()
    }
}

/// Run a full incremental scan: insert new files, update changed ones, and
/// remove entries whose paths no longer exist.
fn scan_index(index: &FileIndex, roots: &[PathBuf]) -> Result<usize, String> {
    let start = Instant::now();
    let mut batch: Vec<(String, i64, i64)> = Vec::with_capacity(BATCH_SIZE);
    let mut inserted = 0usize;

    // Gather current indexed paths so we can detect deletions.
    let existing: HashSet<String> = {
        let conn = index.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT path FROM indexed_file").map_err(|e| e.to_string())?;
        let rows: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows.into_iter().collect()
    };
    let mut seen: HashSet<String> = HashSet::with_capacity(existing.len());

    for root in roots {
        if !root.exists() {
            continue;
        }
        let walker = walkdir::WalkDir::new(root)
            .max_depth(SCAN_DEPTH)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok());

        for entry in walker {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if !should_index(path) {
                continue;
            }
            let path_str = path.to_string_lossy().replace('\\', "/");
            seen.insert(path_str.clone());

            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let size = meta.len() as i64;

            batch.push((path_str, modified, size));
            if batch.len() >= BATCH_SIZE {
                flush_batch(index, &batch)?;
                inserted += batch.len();
                batch.clear();
                thread::sleep(Duration::from_millis(IO_PACE_MS));
            }
        }
    }

    if !batch.is_empty() {
        flush_batch(index, &batch)?;
        inserted += batch.len();
    }

    // Remove stale paths.
    let stale: Vec<String> = existing.difference(&seen).cloned().collect();
    for path in stale {
        let _ = index.remove_path(&path);
    }

    eprintln!(
        "File index scan complete: {} inserted/updated, {} removed in {:?}",
        inserted,
        existing.len().saturating_sub(seen.len()),
        start.elapsed()
    );
    Ok(inserted)
}

fn flush_batch(index: &FileIndex, batch: &[(String, i64, i64)]) -> Result<(), String> {
    let mut conn = index.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for (path, modified, size) in batch {
        let existing: Option<(i64, i64, i64)> = tx
            .query_row(
                "SELECT rowid, modified, size FROM indexed_file WHERE path = ?",
                [path],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        match existing {
            // Unchanged since last scan: skip. Avoids needless delete+insert
            // churn, which would also accumulate FTS5 tombstones over time.
            Some((_, m, s)) if m == *modified && s == *size => continue,
            Some((rowid, _, _)) => {
                tx.execute("DELETE FROM path_idx WHERE rowid = ?", [rowid])
                    .map_err(|e| e.to_string())?;
                tx.execute("DELETE FROM indexed_file WHERE rowid = ?", [rowid])
                    .map_err(|e| e.to_string())?;
            }
            None => {}
        }
        tx.execute("INSERT INTO path_idx(path) VALUES (?)", [path])
            .map_err(|e| e.to_string())?;
        let rowid = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO indexed_file(rowid, path, modified, size) VALUES (?, ?, ?, ?)",
            params![rowid, path, modified, size],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

/// Spawn the index manager: scan once, then watch for changes.
pub fn start_index_manager(app: AppHandle, index: FileIndex) {
    thread::spawn(move || {
        let roots = read_roots(&app);
        let _ = scan_index(&index, &roots);

        // Notify the frontend that the index is ready.
        let _ = app.emit("file-index-ready", ());

        // Set up filesystem watcher.
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(move |res| {
            if let Ok(event) = res {
                if is_significant_event(&event) {
                    let _ = tx.send(event);
                }
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("File watcher failed to start: {}", e);
                return;
            }
        };

        for root in &roots {
            if root.exists() {
                let _ = watcher.watch(root, notify::RecursiveMode::Recursive);
            }
        }

        // Debounce and apply watcher deltas.
        loop {
            if let Ok(event) = rx.recv() {
                thread::sleep(Duration::from_millis(200));
                while rx.try_recv().is_ok() {}
                let _ = apply_event(&index, &event);
                // Catch any stragglers from rapid renames.
                thread::sleep(Duration::from_millis(50));
                while let Ok(event) = rx.try_recv() {
                    let _ = apply_event(&index, &event);
                }
            }
        }
    });
}

fn apply_event(index: &FileIndex, event: &notify::Event) -> Result<(), String> {
    match &event.kind {
        notify::EventKind::Create(_) => {
            for path in &event.paths {
                if path.is_file() {
                    if let Ok(meta) = fs::metadata(path) {
                        let modified = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);
                        let size = meta.len() as i64;
                        let path_str = path.to_string_lossy().replace('\\', "/");
                        index.upsert_path(&path_str, modified, size)?;
                    }
                }
            }
        }
        notify::EventKind::Remove(_) => {
            for path in &event.paths {
                let path_str = path.to_string_lossy().replace('\\', "/");
                index.remove_path(&path_str)?;
                if path.is_dir() {
                    index.invalidate_prefix(&path_str)?;
                }
            }
        }
        notify::EventKind::Modify(notify::event::ModifyKind::Name(_)) => {
            // Rename: remove old, add new.
            if event.paths.len() >= 2 {
                let old = event.paths[0].to_string_lossy().replace('\\', "/");
                let new = event.paths[1].to_string_lossy().replace('\\', "/");
                index.remove_path(&old)?;
                if Path::new(&new).is_file() {
                    if let Ok(meta) = fs::metadata(&new) {
                        let modified = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);
                        let size = meta.len() as i64;
                        index.upsert_path(&new, modified, size)?;
                    }
                }
            } else {
                for path in &event.paths {
                    let path_str = path.to_string_lossy().replace('\\', "/");
                    index.remove_path(&path_str)?;
                    index.invalidate_prefix(&path_str)?;
                }
            }
        }
        notify::EventKind::Modify(_) => {
            for path in &event.paths {
                if path.is_file() {
                    if let Ok(meta) = fs::metadata(path) {
                        let modified = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);
                        let size = meta.len() as i64;
                        let path_str = path.to_string_lossy().replace('\\', "/");
                        index.upsert_path(&path_str, modified, size)?;
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn read_roots(app: &AppHandle) -> Vec<PathBuf> {
    let path = match app.path().app_data_dir() {
        Ok(d) => d.join("config.json"),
        Err(_) => return default_roots(),
    };
    if !path.exists() {
        return default_roots();
    }
    let raw = fs::read_to_string(&path).unwrap_or_default();
    let config: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
    if let Some(arr) = config.get("search_paths").and_then(|v| v.as_array()) {
        let paths: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        if !paths.is_empty() {
            return collect_roots(&paths);
        }
    }
    default_roots()
}

/// Tauri command: search the self-hosted index.
#[tauri::command]
pub async fn search_indexed_files(
    index: tauri::State<'_, FileIndex>,
    query: String,
) -> Result<Vec<IndexedFile>, String> {
    let index = (*index).clone();
    tokio::task::spawn_blocking(move || index.search(&query, MAX_RESULTS))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    /// Build a FileIndex backed by an in-memory DB with the real schema, so the
    /// DELETE-dependent methods run exactly as they do in production.
    fn mem_index() -> FileIndex {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        FileIndex {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    fn paths(items: &[IndexedFile]) -> Vec<String> {
        items.iter().map(|i| i.path.clone()).collect()
    }

    /// Guards the file-index schema: the FTS5 table MUST support DELETE by
    /// rowid, or every incremental update, rename, removal, and re-scan after
    /// the first population silently fails. A plain contentless (`content=''`)
    /// table rejects DELETE; `contentless_delete=1` is what makes it work.
    #[test]
    fn fts5_schema_supports_delete() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        conn.execute("INSERT INTO path_idx(path) VALUES ('hello/world.txt')", [])
            .unwrap();
        let rowid = conn.last_insert_rowid();
        conn.execute("DELETE FROM path_idx WHERE rowid = ?", [rowid])
            .expect("FTS5 index must support DELETE by rowid");
    }

    /// The end-to-end lifecycle: insert, re-scan (the path that used to fail),
    /// update, remove, and prune-missing all work against the live schema.
    #[test]
    fn full_lifecycle_of_index_mutations() {
        let index = mem_index();
        let batch = vec![
            ("home/user/report.txt".to_string(), 100, 10),
            ("home/user/photo.png".to_string(), 200, 20),
        ];

        // First population (all INSERTs).
        flush_batch(&index, &batch).unwrap();
        assert_eq!(paths(&index.search("report", 10).unwrap()), vec!["home/user/report.txt"]);

        // Re-scan with identical data: this is the exact path that errored on a
        // contentless table. Must succeed and not duplicate rows.
        flush_batch(&index, &batch).unwrap();
        assert_eq!(index.search("photo", 10).unwrap().len(), 1);

        // Update an existing file (changed mtime/size) — hits the delete+insert
        // branch that a contentless table rejected.
        let updated = vec![("home/user/report.txt".to_string(), 999, 55)];
        flush_batch(&index, &updated).unwrap();
        let found = index.search("report", 10).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].modified, 999);
        assert_eq!(found[0].size, 55);

        // Watcher-style single upsert then removal.
        index.upsert_path("home/user/notes.md", 300, 30).unwrap();
        assert_eq!(index.search("notes", 10).unwrap().len(), 1);
        index.remove_path("home/user/notes.md").unwrap();
        assert!(index.search("notes", 10).unwrap().is_empty());

        // Prune entries whose files no longer exist on disk (paths above are
        // synthetic, so invalidate_prefix should clear everything under it).
        index.invalidate_prefix("home/user").unwrap();
        assert!(index.search("report", 10).unwrap().is_empty());
    }

    /// Whitespace-separated terms are ANDed independently, so a query whose
    /// words live in different path segments still matches — the old
    /// whole-query-as-one-phrase behavior could not do this.
    #[test]
    fn multi_word_terms_are_anded() {
        let index = mem_index();
        flush_batch(
            &index,
            &vec![
                ("home/user/downloads/report.pdf".to_string(), 1, 1),
                ("home/user/desktop/notes.txt".to_string(), 2, 2),
            ],
        )
        .unwrap();

        // "downloads" and "pdf" are in different segments; both must match.
        assert_eq!(
            paths(&index.search("downloads pdf", 10).unwrap()),
            vec!["home/user/downloads/report.pdf"]
        );
        // Order-independent.
        assert_eq!(
            paths(&index.search("pdf downloads", 10).unwrap()),
            vec!["home/user/downloads/report.pdf"]
        );
        // A term present in neither → no match.
        assert!(index.search("downloads doc", 10).unwrap().is_empty());
    }

    /// Queries under the trigram minimum (3 chars) used to return nothing; the
    /// LIKE fallback now serves them.
    #[test]
    fn short_query_falls_back_to_like() {
        let index = mem_index();
        flush_batch(
            &index,
            &vec![
                ("home/user/go.txt".to_string(), 1, 1),
                ("home/user/rust.rs".to_string(), 2, 2),
            ],
        )
        .unwrap();

        assert_eq!(
            paths(&index.search("go", 10).unwrap()),
            vec!["home/user/go.txt"]
        );
        // Still case-insensitive.
        assert_eq!(
            paths(&index.search("GO", 10).unwrap()),
            vec!["home/user/go.txt"]
        );
    }

    /// A long trigram term combined with a short LIKE term: the short term
    /// filters the trigram candidates rather than being dropped.
    #[test]
    fn mixed_long_and_short_terms() {
        let index = mem_index();
        flush_batch(
            &index,
            &vec![
                ("home/user/downloads/readme.md".to_string(), 1, 1),
                ("home/user/downloads/photo.png".to_string(), 2, 2),
            ],
        )
        .unwrap();

        assert_eq!(
            paths(&index.search("downloads md", 10).unwrap()),
            vec!["home/user/downloads/readme.md"]
        );
    }

    #[test]
    fn escape_like_escapes_metacharacters() {
        assert_eq!(escape_like("a%b_c"), "a\\%b\\_c");
        assert_eq!(escape_like("back\\slash"), "back\\\\slash");
        assert_eq!(escape_like("plain"), "plain");
    }

    /// LIKE-metacharacter terms are matched literally, not as wildcards.
    #[test]
    fn short_query_like_metachars_are_literal() {
        let index = mem_index();
        flush_batch(
            &index,
            &vec![
                ("home/user/a_b.txt".to_string(), 1, 1),
                ("home/user/axb.txt".to_string(), 2, 2),
            ],
        )
        .unwrap();

        // "_" must match a literal underscore, not "any char".
        assert_eq!(
            paths(&index.search("_b", 10).unwrap()),
            vec!["home/user/a_b.txt"]
        );
    }

    /// A DB created by the buggy build (contentless, no delete support) must be
    /// silently rebuilt on open, and be fully mutable afterward.
    #[test]
    fn migrates_legacy_contentless_table() {
        let conn = Connection::open_in_memory().unwrap();
        // Simulate the old schema.
        conn.execute(
            "CREATE TABLE indexed_file (rowid INTEGER PRIMARY KEY, path TEXT UNIQUE NOT NULL, modified INTEGER NOT NULL, size INTEGER NOT NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE VIRTUAL TABLE path_idx USING fts5(path, content='', tokenize='trigram')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO path_idx(path) VALUES ('stale/file.txt')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO indexed_file(rowid, path, modified, size) VALUES (?, 'stale/file.txt', 1, 1)",
            [conn.last_insert_rowid()],
        )
        .unwrap();

        // Migrate.
        init_schema(&conn).unwrap();

        // indexed_file was cleared so it rebuilds in lockstep on the next scan.
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM indexed_file", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 0);

        // The rebuilt FTS5 table now supports DELETE.
        let index = FileIndex {
            conn: Arc::new(Mutex::new(conn)),
        };
        flush_batch(&index, &vec![("new/file.txt".to_string(), 5, 5)]).unwrap();
        flush_batch(&index, &vec![("new/file.txt".to_string(), 6, 6)]).unwrap();
        index.remove_path("new/file.txt").unwrap();
        assert!(index.search("file", 10).unwrap().is_empty());
    }
}
