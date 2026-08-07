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

const BATCH_SIZE: usize = 256;
const IO_PACE_MS: u64 = 2;
const SCAN_DEPTH: usize = 8;
const WATCH_DEBOUNCE_MS: u64 = 200;

/// Dependency, VCS, and generated-output directories that are pruned before
/// descent. Hidden entries are handled separately so this list only needs the
/// visible names that commonly dominate a recursive walk.
const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "bower_components",
    "vendor",
    "Pods",
    "__pycache__",
    "venv",
    "target",
    "build",
    "dist",
    "out",
    "$RECYCLE.BIN",
];

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

        // Use a JOIN to fetch both rowids and file data in one query, ordered by FTS5 rank.
        // This is more efficient than the previous two-query approach.
        let match_expr = long
            .iter()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ");
        let fetch = if short.is_empty() { limit } else { limit * 5 };

        // Single JOIN query to get rowid, path, modified, size ordered by rank
        let mut stmt = conn
            .prepare(
                "SELECT i.rowid, i.path, i.modified, i.size 
                 FROM indexed_file i
                 JOIN path_idx p ON i.rowid = p.rowid
                 WHERE p.path MATCH ?
                 ORDER BY p.rank
                 LIMIT ?",
            )
            .map_err(|e| e.to_string())?;

        let candidates: Vec<(i64, IndexedFile)> = stmt
            .query_map(params![&match_expr, fetch as i64], |row| {
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
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        drop(stmt);

        if candidates.is_empty() {
            return Ok(vec![]);
        }

        // Filter by short terms (which trigram can't handle) and truncate to limit
        let ordered: Vec<IndexedFile> = candidates
            .into_iter()
            .map(|(_, file)| file)
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
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let rowid: Option<i64> = tx
            .query_row(
                "SELECT rowid FROM indexed_file WHERE path = ?",
                [path],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(rowid) = rowid {
            tx.execute("DELETE FROM path_idx WHERE rowid = ?", [rowid])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM indexed_file WHERE rowid = ?", [rowid])
                .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }

    /// Add or update a single path in the index.
    fn upsert_path(&self, path: &str, modified: i64, size: i64) -> Result<(), String> {
        flush_batch(self, &[(path.to_string(), modified, size)])
    }

    /// Remove an exact path and every indexed descendant in one transaction.
    /// This does not inspect the filesystem: remove and rename events arrive
    /// after the old directory has disappeared, so `Path::is_dir` cannot tell
    /// whether the event represented a file or a directory.
    fn remove_prefix(&self, prefix: &str) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let mut stmt = tx
            .prepare(
                "SELECT rowid FROM indexed_file
                 WHERE path = ?1 OR path LIKE ?2 ESCAPE '\\'",
            )
            .map_err(|e| e.to_string())?;
        let descendant_pattern = descendant_like_pattern(prefix);
        let rowids: Vec<i64> = stmt
            .query_map(params![prefix, descendant_pattern], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        drop(stmt);

        for rowid in rowids {
            tx.execute("DELETE FROM path_idx WHERE rowid = ?", [rowid])
                .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM indexed_file WHERE rowid = ?", [rowid])
                .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }
}

fn descendant_like_pattern(prefix: &str) -> String {
    let normalized = prefix.trim_end_matches('/');
    if normalized.is_empty() {
        "/%".to_string()
    } else {
        format!("{}/%", escape_like(normalized))
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

fn should_skip_name(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    name.starts_with('.') || SKIP_DIRS.iter().any(|skip| name.eq_ignore_ascii_case(skip))
}

/// Whether a path is inside a configured root, within the scan-depth cap, and
/// contains no ignored component below that root. An explicitly configured
/// hidden root remains valid; only its descendants are evaluated.
fn should_index_in_roots(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|root| {
        let Ok(relative) = path.strip_prefix(root) else {
            return false;
        };
        relative.components().count() <= SCAN_DEPTH
            && relative
                .components()
                .all(|component| !should_skip_name(component.as_os_str()))
    })
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
        let mut stmt = conn
            .prepare("SELECT path FROM indexed_file")
            .map_err(|e| e.to_string())?;
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
            .filter_entry(|entry| entry.depth() == 0 || !should_skip_name(entry.file_name()))
            .filter_map(|e| e.ok());

        for entry in walker {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if !should_index_in_roots(path, roots) {
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
    let removed = stale.len();
    for path in stale {
        index.remove_path(&path)?;
    }

    eprintln!(
        "File index scan complete: {} inserted/updated, {} removed in {:?}",
        inserted,
        removed,
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

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn file_metadata(path: &Path) -> Option<(i64, i64)> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    Some((modified, meta.len() as i64))
}

fn remaining_scan_depth(path: &Path, roots: &[PathBuf]) -> Option<usize> {
    roots
        .iter()
        .filter_map(|root| path.strip_prefix(root).ok())
        .map(|relative| relative.components().count())
        .min()
        .and_then(|depth| SCAN_DEPTH.checked_sub(depth))
}

fn indexed_paths_under(index: &FileIndex, prefix: &str) -> Result<HashSet<String>, String> {
    let conn = index.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT path FROM indexed_file
             WHERE path = ?1 OR path LIKE ?2 ESCAPE '\\'",
        )
        .map_err(|e| e.to_string())?;
    let descendant_pattern = descendant_like_pattern(prefix);
    let paths = stmt
        .query_map(params![prefix, descendant_pattern], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|e| e.to_string())?;
    Ok(paths)
}

/// Reconcile an existing directory and all indexable descendants. Native
/// watchers are allowed to report only the directory for a bulk create or
/// rename, so handling just `event.paths` would miss the files below it.
fn reconcile_subtree(index: &FileIndex, directory: &Path, roots: &[PathBuf]) -> Result<(), String> {
    let directory_str = normalized_path(directory);
    let existing = indexed_paths_under(index, &directory_str)?;
    let mut seen = HashSet::with_capacity(existing.len());
    let Some(max_depth) = remaining_scan_depth(directory, roots) else {
        return index.remove_prefix(&directory_str);
    };

    let walker = walkdir::WalkDir::new(directory)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.depth() == 0 || !should_skip_name(entry.file_name()))
        .filter_map(|entry| entry.ok());
    let mut batch = Vec::with_capacity(BATCH_SIZE);
    for entry in walker {
        let path = entry.path();
        if !path.is_file() || !should_index_in_roots(path, roots) {
            continue;
        }
        let Some((modified, size)) = file_metadata(path) else {
            continue;
        };
        let path = normalized_path(path);
        seen.insert(path.clone());
        batch.push((path, modified, size));
        if batch.len() >= BATCH_SIZE {
            flush_batch(index, &batch)?;
            batch.clear();
        }
    }
    if !batch.is_empty() {
        flush_batch(index, &batch)?;
    }

    for stale in existing.difference(&seen) {
        index.remove_path(stale)?;
    }
    Ok(())
}

fn reconcile_path(index: &FileIndex, path: &Path, roots: &[PathBuf]) -> Result<(), String> {
    let path_str = normalized_path(path);
    if !should_index_in_roots(path, roots) {
        return index.remove_prefix(&path_str);
    }
    if path.is_file() {
        if let Some((modified, size)) = file_metadata(path) {
            return index.upsert_path(&path_str, modified, size);
        }
    } else if path.is_dir() {
        return reconcile_subtree(index, path, roots);
    }

    // Missing paths can be either files or directories. Remove the prefix so
    // both cases are correct without relying on post-removal metadata.
    index.remove_prefix(&path_str)
}

fn collect_debounced_events(
    rx: &std::sync::mpsc::Receiver<notify::Event>,
    first: notify::Event,
    quiet_period: Duration,
) -> Vec<notify::Event> {
    let mut events = vec![first];
    while let Ok(event) = rx.recv_timeout(quiet_period) {
        events.push(event);
    }
    events
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
                let events =
                    collect_debounced_events(&rx, event, Duration::from_millis(WATCH_DEBOUNCE_MS));
                for event in events {
                    if let Err(error) = apply_event(&index, &roots, &event) {
                        eprintln!("File watcher event failed: {error}");
                    }
                }
            }
        }
    });
}

fn apply_event(index: &FileIndex, roots: &[PathBuf], event: &notify::Event) -> Result<(), String> {
    if event.need_rescan() {
        scan_index(index, roots)?;
        return Ok(());
    }

    match &event.kind {
        notify::EventKind::Create(_) => {
            for path in &event.paths {
                reconcile_path(index, path, roots)?;
            }
        }
        notify::EventKind::Remove(_) => {
            for path in &event.paths {
                index.remove_prefix(&normalized_path(path))?;
            }
        }
        notify::EventKind::Modify(notify::event::ModifyKind::Name(_)) => {
            // A paired rename has the old path first and new path last. Remove
            // the entire old subtree, then reconcile the new path recursively.
            if event.paths.len() >= 2 {
                index.remove_prefix(&normalized_path(&event.paths[0]))?;
                reconcile_path(index, &event.paths[event.paths.len() - 1], roots)?;
            } else {
                // Some backends emit rename-from and rename-to separately. A
                // missing path is removed; an existing path is reconciled.
                for path in &event.paths {
                    reconcile_path(index, path, roots)?;
                }
            }
        }
        notify::EventKind::Modify(_)
        | notify::EventKind::Any
        | notify::EventKind::Access(_)
        | notify::EventKind::Other => {
            for path in &event.paths {
                reconcile_path(index, path, roots)?;
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
    use notify::EventKind;
    use rusqlite::Connection;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    static NEXT_TEMP_DIR: AtomicUsize = AtomicUsize::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let id = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "commandeer-file-index-test-{}-{id}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

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

    fn all_paths(index: &FileIndex) -> Vec<String> {
        let conn = index.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT path FROM indexed_file ORDER BY path")
            .unwrap();
        stmt.query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    fn write_test_file(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"test").unwrap();
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
        assert_eq!(
            paths(&index.search("report", 10).unwrap()),
            vec!["home/user/report.txt"]
        );

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

        // Watcher-style single upsert, update, then removal.
        index.upsert_path("home/user/notes.md", 300, 30).unwrap();
        index.upsert_path("home/user/notes.md", 400, 40).unwrap();
        let notes = index.search("notes", 10).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].modified, 400);
        assert_eq!(notes[0].size, 40);
        index.remove_path("home/user/notes.md").unwrap();
        assert!(index.search("notes", 10).unwrap().is_empty());

        // Removing a directory event must clear every descendant without
        // relying on that now-missing directory's metadata.
        index.remove_prefix("home/user").unwrap();
        assert!(index.search("report", 10).unwrap().is_empty());
    }

    #[test]
    fn scan_prunes_hidden_and_build_heavy_subtrees() {
        let root = TestDir::new();
        write_test_file(&root.path().join("visible/report.txt"));
        write_test_file(&root.path().join(".hidden/secret.txt"));
        write_test_file(&root.path().join("node_modules/package/index.js"));
        write_test_file(&root.path().join("target/debug/binary"));
        write_test_file(&root.path().join("build/generated.txt"));

        let index = mem_index();
        scan_index(&index, &[root.path().to_path_buf()]).unwrap();

        assert_eq!(
            all_paths(&index),
            vec![normalized_path(&root.path().join("visible/report.txt"))]
        );
    }

    #[test]
    fn watcher_reconciles_directory_create_rename_and_remove() {
        let root = TestDir::new();
        let old_dir = root.path().join("old-folder");
        let old_file = old_dir.join("nested/report.txt");
        write_test_file(&old_file);
        let roots = vec![root.path().to_path_buf()];
        let index = mem_index();

        let create =
            notify::Event::new(EventKind::Create(CreateKind::Folder)).add_path(old_dir.clone());
        apply_event(&index, &roots, &create).unwrap();
        assert_eq!(all_paths(&index), vec![normalized_path(&old_file)]);

        let new_dir = root.path().join("new-folder");
        fs::rename(&old_dir, &new_dir).unwrap();
        let new_file = new_dir.join("nested/report.txt");
        let rename = notify::Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
            .add_path(old_dir)
            .add_path(new_dir.clone());
        apply_event(&index, &roots, &rename).unwrap();
        assert_eq!(all_paths(&index), vec![normalized_path(&new_file)]);

        fs::remove_dir_all(&new_dir).unwrap();
        let remove = notify::Event::new(EventKind::Remove(RemoveKind::Folder)).add_path(new_dir);
        apply_event(&index, &roots, &remove).unwrap();
        assert!(all_paths(&index).is_empty());
    }

    #[test]
    fn watcher_uses_the_same_exclusions_as_the_scanner() {
        let root = TestDir::new();
        let ignored = root.path().join("node_modules/package/index.js");
        write_test_file(&ignored);
        let roots = vec![root.path().to_path_buf()];
        let index = mem_index();

        let create = notify::Event::new(EventKind::Create(CreateKind::File)).add_path(ignored);
        apply_event(&index, &roots, &create).unwrap();

        assert!(all_paths(&index).is_empty());
    }

    #[test]
    fn prefix_removal_stays_on_directory_boundaries_and_escapes_like_syntax() {
        let index = mem_index();
        flush_batch(
            &index,
            &[
                ("home/user/project/file.txt".to_string(), 1, 1),
                ("home/user/project-copy/keep.txt".to_string(), 1, 1),
                ("home/user/100%/remove.txt".to_string(), 1, 1),
                ("home/user/100-percent/keep.txt".to_string(), 1, 1),
            ],
        )
        .unwrap();

        index.remove_prefix("home/user/project").unwrap();
        index.remove_prefix("home/user/100%").unwrap();

        assert_eq!(
            all_paths(&index),
            vec![
                "home/user/100-percent/keep.txt".to_string(),
                "home/user/project-copy/keep.txt".to_string(),
            ]
        );
    }

    #[test]
    fn debounce_preserves_every_queued_event() {
        let (tx, rx) = std::sync::mpsc::channel();
        let first = notify::Event::new(EventKind::Create(CreateKind::File))
            .add_path(PathBuf::from("first.txt"));
        for name in ["second.txt", "third.txt"] {
            tx.send(
                notify::Event::new(EventKind::Create(CreateKind::File))
                    .add_path(PathBuf::from(name)),
            )
            .unwrap();
        }

        let events = collect_debounced_events(&rx, first, Duration::from_millis(1));

        assert_eq!(events.len(), 3);
        assert_eq!(events[0].paths[0], PathBuf::from("first.txt"));
        assert_eq!(events[1].paths[0], PathBuf::from("second.txt"));
        assert_eq!(events[2].paths[0], PathBuf::from("third.txt"));
    }

    /// Whitespace-separated terms are ANDed independently, so a query whose
    /// words live in different path segments still matches — the old
    /// whole-query-as-one-phrase behavior could not do this.
    #[test]
    fn multi_word_terms_are_anded() {
        let index = mem_index();
        flush_batch(
            &index,
            &[
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
            &[
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
            &[
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
            &[
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
        flush_batch(&index, &[("new/file.txt".to_string(), 5, 5)]).unwrap();
        flush_batch(&index, &[("new/file.txt".to_string(), 6, 6)]).unwrap();
        index.remove_path("new/file.txt").unwrap();
        assert!(index.search("file", 10).unwrap().is_empty());
    }
}
