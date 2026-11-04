//! `scheduler.db`: the connection, its schema version, and the row helpers the store builds on.

use std::collections::hash_map::RandomState;
use std::fs;
use std::hash::{BuildHasher, Hasher};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use omp_core::{Error, Result};
use parking_lot::Mutex;
use rusqlite::{Connection, params};
use serde_json::{Map, Value};

/// Bumped whenever the tables change; a database from a newer build is refused, not guessed at.
const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS tasks (
    id         TEXT PRIMARY KEY NOT NULL,
    config     TEXT NOT NULL,
    enabled    INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    archived   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS tasks_by_creation ON tasks (created_at);
CREATE TABLE IF NOT EXISTS runs (
    id           TEXT PRIMARY KEY NOT NULL,
    task_id      TEXT NOT NULL,
    status       TEXT NOT NULL,
    started_at   INTEGER NOT NULL,
    ended_at     INTEGER,
    session_path TEXT,
    report       TEXT,
    review       TEXT,
    cost         REAL
);
CREATE INDEX IF NOT EXISTS runs_by_task ON runs (task_id, started_at DESC);
CREATE TABLE IF NOT EXISTS revisions (
    task_id TEXT NOT NULL,
    rev     INTEGER NOT NULL,
    config  TEXT NOT NULL,
    at      INTEGER NOT NULL,
    PRIMARY KEY (task_id, rev)
);
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
";

/// A handle on `scheduler.db`. Cheap to share: the connection is serialized behind a mutex.
pub struct TaskStore {
    pub(crate) db: Mutex<Connection>,
}

impl TaskStore {
    /// Open (creating if needed) the database at `path`; `":memory:"` opens a scratch one.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if path != Path::new(":memory:")
            && let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path).map_err(db_error)?;
        connection
            .execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")
            .map_err(db_error)?;
        migrate(&connection)?;
        Ok(Self { db: Mutex::new(connection) })
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let db = self.db.lock();
        match db.query_row("SELECT value FROM meta WHERE key = ?1", params![key], |row| row.get(0)) {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(db_error(error)),
        }
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        let db = self.db.lock();
        db.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(db_error)?;
        Ok(())
    }
}

/// [`TaskStore::open`] spelled as a function, for callers that start from the database path.
pub fn open(path: impl AsRef<Path>) -> Result<TaskStore> {
    TaskStore::open(path)
}

fn migrate(connection: &Connection) -> Result<()> {
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(db_error)?;
    if version > SCHEMA_VERSION {
        return Err(Error::msg(format!(
            "scheduler.db schema {version} is newer than this build understands ({SCHEMA_VERSION})"
        )));
    }
    if version < 1 {
        connection.execute_batch(SCHEMA).map_err(db_error)?;
    }
    if version != SCHEMA_VERSION {
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(db_error)?;
    }
    Ok(())
}

pub(crate) fn db_error(error: rusqlite::Error) -> Error {
    Error::msg(format!("scheduler db: {error}"))
}

pub(crate) fn missing(kind: &str, id: &str) -> Error {
    Error::msg(format!("scheduler {kind} {id} not found"))
}

pub(crate) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// The stored config as a JSON object. Unknown keys survive, so a newer build can add fields.
pub(crate) fn config_object(text: &str) -> Result<Map<String, Value>> {
    match serde_json::from_str::<Value>(text)? {
        Value::Object(object) => Ok(object),
        _ => Err(Error::msg("scheduler task config is not a JSON object")),
    }
}

static NEXT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// `<kind>_<millis><salt>` in base 36: roughly age-ordered and unique across processes.
pub(crate) fn new_id(kind: &str) -> String {
    let millis = now_ms().max(0) as u64;
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(NEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed));
    hasher.write_u64(millis);
    let salt = base36(hasher.finish() & 0xffff_ffff, 6);
    format!("{kind}_{}{salt}", base36(millis, 0))
}

fn base36(mut value: u64, min_width: usize) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut buffer = [b'0'; 13];
    let mut index = buffer.len();
    loop {
        index -= 1;
        buffer[index] = DIGITS[(value % 36) as usize];
        value /= 36;
        if value == 0 && buffer.len() - index >= min_width {
            break;
        }
    }
    String::from_utf8_lossy(&buffer[index..]).into_owned()
}

#[cfg(test)]
pub(crate) fn temp_path(tag: &str) -> std::path::PathBuf {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "omp-scheduler-{tag}-{}-{sequence}.db",
        std::process::id()
    ))
}

#[cfg(test)]
pub(crate) fn remove_db(path: &Path) {
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(path.with_extension("db-wal"));
    let _ = fs::remove_file(path.with_extension("db-shm"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fresh_database_is_version_one() {
        let path = temp_path("fresh");
        let store = TaskStore::open(&path).unwrap();
        drop(store);
        let connection = Connection::open(&path).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        remove_db(&path);
    }

    #[test]
    fn reopening_preserves_tasks_revisions_runs_and_meta() {
        let path = temp_path("reopen");
        let id;
        {
            let store = TaskStore::open(&path).unwrap();
            id = store
                .create(&json!({
                    "title": "persisted",
                    "schedule": {"kind": "interval", "everyMs": 60000, "anchor": 0},
                }))
                .unwrap();
            store.update(&id, &json!({"title": "persisted 2"})).unwrap();
            let run = store
                .record_run(&json!({"taskId": id, "status": "running", "startedAt": 5}))
                .unwrap();
            store
                .finish_run(
                    &run,
                    "succeeded",
                    Some(&json!({"status": "done", "summary": "ok"})),
                    None,
                    Some(1.5),
                    Some("p.jsonl"),
                    9,
                )
                .unwrap();
            store.set_meta("paused", "false").unwrap();
        }
        {
            let store = TaskStore::open(&path).unwrap();
            let tasks = store.list(false).unwrap();
            assert_eq!(tasks.len(), 1);
            assert_eq!(tasks[0]["title"], json!("persisted 2"));
            assert_eq!(tasks[0]["schedule"]["everyMs"], json!(60000));
            assert_eq!(tasks[0]["rev"], json!(2));
            assert_eq!(store.revisions(&id).unwrap().len(), 2);
            let runs = store.runs(&id, 10).unwrap();
            assert_eq!(runs[0]["report"]["summary"], json!("ok"));
            assert_eq!(runs[0]["cost"], json!(1.5));
            assert_eq!(store.get_meta("paused").unwrap().as_deref(), Some("false"));
        }
        let connection = Connection::open(&path).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        drop(connection);
        remove_db(&path);
    }

    #[test]
    fn refuses_a_database_from_a_newer_build() {
        let path = temp_path("newer");
        {
            let connection = Connection::open(&path).unwrap();
            connection.execute_batch("PRAGMA user_version = 7;").unwrap();
        }
        let error = match TaskStore::open(&path) {
            Ok(_) => panic!("a newer schema must not be opened"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("newer"), "{error}");
        remove_db(&path);
    }

    #[test]
    fn ids_are_prefixed_and_distinct() {
        let first = new_id("task");
        let second = new_id("task");
        assert!(first.starts_with("task_"), "{first}");
        assert_ne!(first, second);
        assert_eq!(base36(0, 0), "0");
        assert_eq!(base36(1, 3), "001");
        assert_eq!(base36(36, 0), "10");
    }
}
