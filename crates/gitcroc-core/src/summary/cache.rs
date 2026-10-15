//! `cache.db` in the cache dir (section 10 "Files"): the last summary read
//! of each repo, keyed by its path (PERF-3). The cache is disposable: a
//! database that fails to open, is corrupt or has another schema version is
//! deleted and created again, and without one the app runs uncached.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use gitcroc_git::RepoSummary;
use rusqlite::{Connection, OptionalExtension, params};

use crate::lock;

/// The file name inside the cache dir.
pub const CACHE_FILE: &str = "cache.db";

/// The schema this build reads and writes, kept in SQLite's `user_version`.
/// Any other version is not migrated: the file is created again.
pub const CACHE_SCHEMA: i32 = 1;

/// Where the app keeps its cache. Tests point it at a temporary dir.
pub trait CacheDir: Send + Sync {
    /// The app's own cache dir, or `None` when the platform has none.
    fn cache_dir(&self) -> Option<PathBuf>;
}

/// The platform cache dir with `gitcroc/` appended, for example
/// `~/.cache/gitcroc` on Linux.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlatformCacheDir;

impl CacheDir for PlatformCacheDir {
    fn cache_dir(&self) -> Option<PathBuf> {
        directories::BaseDirs::new().map(|dirs| dirs.cache_dir().join("gitcroc"))
    }
}

/// The open cache, or none when it could not be opened or created.
#[derive(Debug)]
pub struct SummaryCache {
    connection: Mutex<Option<Connection>>,
}

impl SummaryCache {
    /// Opens `cache.db` in `dirs`, creating it when it is missing and again
    /// when it cannot be used. Never fails: without a usable file the cache
    /// is empty and keeps nothing.
    pub fn open(dirs: &dyn CacheDir) -> Self {
        let connection = dirs.cache_dir().and_then(|dir| open_or_recreate(&dir));
        Self {
            connection: Mutex::new(connection),
        }
    }

    /// A cache that keeps nothing.
    pub fn none() -> Self {
        Self {
            connection: Mutex::new(None),
        }
    }

    /// The summary last read for the repo at `path`. A row that cannot be
    /// read, for example one written by another build, counts as missing.
    pub(crate) fn get(&self, path: &Path) -> Option<RepoSummary> {
        let key = path.to_str()?;
        let connection = self.lock();
        let text: String = connection
            .as_ref()?
            .query_row(
                "SELECT summary FROM summaries WHERE path = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .ok()??;
        serde_json::from_str(&text).ok()
    }

    /// Keeps `summary` as the last read of the repo at `path`. A failed
    /// write loses only the cache entry, so it is ignored.
    pub(crate) fn put(&self, path: &Path, summary: &RepoSummary) {
        let Some(key) = path.to_str() else {
            return;
        };
        let Ok(text) = serde_json::to_string(summary) else {
            return;
        };
        let read_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        if let Some(connection) = self.lock().as_ref() {
            let _ = connection.execute(
                "INSERT INTO summaries (path, summary, read_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (path) DO UPDATE SET summary = ?2, read_at = ?3",
                params![key, text, read_at],
            );
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Connection>> {
        // A statement either ran or did not; a panic leaves nothing half done.
        lock(&self.connection)
    }
}

/// Opens the cache in `dir`; on any problem deletes it and tries once more.
fn open_or_recreate(dir: &Path) -> Option<Connection> {
    let path = dir.join(CACHE_FILE);
    if let Some(connection) = open(&path) {
        return Some(connection);
    }
    remove(&path);
    open(&path)
}

/// Opens the cache at `path`; `None` when it cannot be opened, is not a
/// database, or has another schema.
fn open(path: &Path) -> Option<Connection> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).ok()?;
    }
    let connection = Connection::open(path).ok()?;
    // Reading the version reads the header, so a file that is not a
    // database fails here.
    let version: i32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .ok()?;
    match version {
        0 => {
            connection
                .execute_batch(
                    "CREATE TABLE IF NOT EXISTS summaries (
                     path TEXT PRIMARY KEY NOT NULL,
                     summary TEXT NOT NULL,
                     read_at INTEGER NOT NULL
                 ) WITHOUT ROWID;",
                )
                .ok()?;
            connection
                .pragma_update(None, "user_version", CACHE_SCHEMA)
                .ok()?;
        }
        CACHE_SCHEMA => {}
        _ => return None,
    }
    // Proves the table has the columns this build uses.
    connection
        .query_row(
            "SELECT count(*) FROM summaries WHERE path = '' AND summary = '' AND read_at = 0",
            [],
            |row| row.get::<_, i64>(0),
        )
        .ok()?;
    Some(connection)
}

/// Deletes the database and the files SQLite keeps beside it.
fn remove(path: &Path) {
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        let _ = fs::remove_file(PathBuf::from(name));
    }
}
