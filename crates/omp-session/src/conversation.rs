//! Search over what was said in agent sessions.
//!
//! A derived, GUI-owned cache: an FTS5 (trigram) table of user and assistant message text,
//! kept under the app's data folder and safe to delete. Only message text is indexed — tool
//! output never is. Session files are append-only past their title slot, so a grown file is
//! read from where the last pass stopped and a shrunk or replaced one is read again in full.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use omp_core::{Error, Result};
use parking_lot::Mutex;
use rusqlite::types::Value as SqlValue;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde_json::{Value, json};

const SCHEMA_VERSION: &str = "1";
/// Bytes before the read offset that must still match for an append-only read.
const TAIL_CHECK_BYTES: u64 = 64;
/// Bytes read per chunk.
const CHUNK_BYTES: u64 = 2 * 1024 * 1024;
/// Head bytes holding the title slot and session header.
const HEAD_BYTES: u64 = 8 * 1024;
/// Longest message body kept; longer replies match on their opening.
const MAX_BODY_CHARS: usize = 20_000;
/// Matching messages read per search before grouping.
const MAX_ROWS: usize = 400;
const HITS_PER_SESSION: usize = 3;
const MAX_SESSIONS: usize = 60;
/// Characters of context on each side of the first match in a snippet.
const SNIPPET_CONTEXT: usize = 60;
/// Snippet match delimiters; the renderer turns them into `<mark>`.
const MARK_OPEN: char = '\u{1}';
const MARK_CLOSE: char = '\u{2}';

/// Result of one pass over the sessions folder.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Sessions in the index after the pass.
    pub indexed: usize,
    /// Always `None` once the pass returns; a search running during it reports live progress.
    pub indexing: Option<(usize, usize)>,
}

/// One user or assistant message that matched a query.
struct Hit {
    entry_id: String,
    role: &'static str,
    snippet: String,
    timestamp: i64,
}

struct FileRow {
    signature: String,
    indexed_bytes: u64,
    /// Hex of the [`TAIL_CHECK_BYTES`] before `indexed_bytes`.
    tail: String,
}

/// A session file's identity: modification time and size decide when it is read again.
#[derive(Clone)]
struct FileInfo {
    signature: String,
    size: u64,
    modified: i64,
}

#[derive(Default)]
struct SessionHead {
    session_id: Option<String>,
    cwd: Option<String>,
    title: Option<String>,
    kind: Option<String>,
}

struct ParsedMessage {
    entry_id: String,
    role: &'static str,
    body: String,
    timestamp: i64,
}

pub struct ConversationIndex {
    db: Mutex<Connection>,
    progress: Mutex<Option<(usize, usize)>>,
    /// Serializes passes so two syncs cannot index the same file twice.
    syncing: Mutex<()>,
}

impl ConversationIndex {
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(db_path).map_err(db_error)?;
        connection
            .execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")
            .map_err(db_error)?;
        connection
            .execute_batch("CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .map_err(db_error)?;
        let version: Option<String> = connection
            .query_row("SELECT value FROM meta WHERE key = 'schema'", [], |row| row.get(0))
            .optional()
            .map_err(db_error)?;
        // A cache written by an older build is dropped rather than migrated.
        if version.as_deref() != Some(SCHEMA_VERSION) {
            connection
                .execute_batch("DROP TABLE IF EXISTS files; DROP TABLE IF EXISTS messages;")
                .map_err(db_error)?;
            connection
                .execute(
                    "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema', ?1)",
                    [SCHEMA_VERSION],
                )
                .map_err(db_error)?;
        }
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS files (
                    path TEXT PRIMARY KEY,
                    signature TEXT NOT NULL,
                    indexed_bytes INTEGER NOT NULL,
                    tail TEXT NOT NULL,
                    session_id TEXT,
                    cwd TEXT,
                    title TEXT,
                    kind TEXT,
                    modified INTEGER NOT NULL
                );
                CREATE VIRTUAL TABLE IF NOT EXISTS messages USING fts5(
                    body,
                    session_file UNINDEXED,
                    entry_id UNINDEXED,
                    role UNINDEXED,
                    timestamp UNINDEXED,
                    tokenize = 'trigram'
                );",
            )
            .map_err(db_error)?;
        Ok(Self {
            db: Mutex::new(connection),
            progress: Mutex::new(None),
            syncing: Mutex::new(()),
        })
    }

    /// Bring the index up to date with `sessions_root`.
    ///
    /// A file that only grew is read from the last complete line; a shrunk or replaced one is
    /// read again in full. One unreadable session never stops the pass.
    pub fn sync(&self, sessions_root: &Path) -> SyncReport {
        let _pass = self.syncing.lock();
        let files = scan(sessions_root);
        let _ = self.remove_missing(&files);
        let known = self.known_files().unwrap_or_default();
        let stale: Vec<(PathBuf, FileInfo)> = files
            .iter()
            .filter(|(path, info)| known.get(*path).is_none_or(|row| row.signature != info.signature))
            .map(|(path, info)| (path.clone(), info.clone()))
            .collect();
        let total = stale.len();
        if total > 0 {
            *self.progress.lock() = Some((0, total));
        }
        for (done, (path, info)) in stale.into_iter().enumerate() {
            let _ = self.index_file(&path, &info, known.get(&path));
            if total > 0 {
                *self.progress.lock() = Some((done + 1, total));
            }
        }
        *self.progress.lock() = None;
        SyncReport {
            indexed: self.file_count(),
            indexing: None,
        }
    }

    /// Search indexed messages. Query terms must all match; tool output is never indexed.
    ///
    /// `limit` caps the number of sessions returned. A failure inside the cache answers with
    /// [`Self::unavailable`] instead of failing the whole call.
    pub fn search(&self, query: &str, cwd: Option<&Path>, limit: usize) -> Result<Value> {
        self.search_inner(query, cwd, limit).or_else(|_| Ok(Self::unavailable()))
    }

    /// The response for a run where the index could not be opened.
    pub fn unavailable() -> Value {
        let mut response = response(Vec::new(), 0, None);
        response["unavailable"] = Value::Bool(true);
        response
    }

    fn search_inner(&self, query: &str, cwd: Option<&Path>, limit: usize) -> Result<Value> {
        let progress = *self.progress.lock();
        let connection = self.db.lock();
        let indexed: i64 = connection
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
            .map_err(db_error)?;
        let terms = search_terms(query);
        let session_cap = limit.min(MAX_SESSIONS);
        if terms.is_empty() || session_cap == 0 {
            return Ok(response(Vec::new(), indexed, progress));
        }

        let long: Vec<&String> = terms.iter().filter(|term| term.chars().count() >= 3).collect();
        let short: Vec<&String> = terms.iter().filter(|term| term.chars().count() < 3).collect();
        let mut clauses: Vec<String> = Vec::new();
        let mut values: Vec<SqlValue> = Vec::new();
        if !long.is_empty() {
            clauses.push("messages MATCH ?".to_string());
            let match_query = long.iter().map(|term| phrase(term)).collect::<Vec<_>>().join(" AND ");
            values.push(SqlValue::Text(match_query));
        }
        for term in &short {
            clauses.push("body LIKE ? ESCAPE '\\'".to_string());
            values.push(SqlValue::Text(like_pattern(term)));
        }
        if let Some(cwd) = cwd {
            clauses.push("session_file IN (SELECT path FROM files WHERE cwd = ?)".to_string());
            values.push(SqlValue::Text(cwd.to_string_lossy().into_owned()));
        }
        values.push(SqlValue::Integer(MAX_ROWS as i64));
        let sql = format!(
            "SELECT body, session_file, entry_id, role, CAST(timestamp AS INTEGER) FROM messages \
             WHERE {} ORDER BY CAST(timestamp AS INTEGER) DESC LIMIT ?",
            clauses.join(" AND ")
        );
        let mut statement = connection.prepare(&sql).map_err(db_error)?;
        let rows = statement
            .query_map(params_from_iter(values), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .map_err(db_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(db_error)?;

        // Rows arrive newest first, so the first session seen is the most recently active.
        let mut order: Vec<String> = Vec::new();
        let mut grouped: HashMap<String, Vec<Hit>> = HashMap::new();
        for (body, session_file, entry_id, role, timestamp) in rows {
            if !grouped.contains_key(&session_file) {
                order.push(session_file.clone());
            }
            grouped.entry(session_file).or_default().push(Hit {
                entry_id,
                role: if role == "assistant" { "assistant" } else { "user" },
                snippet: build_snippet(&body, &terms),
                timestamp,
            });
        }

        let mut file_query = connection
            .prepare("SELECT session_id, cwd, title, kind, modified FROM files WHERE path = ?1")
            .map_err(db_error)?;
        let mut results: Vec<Value> = Vec::new();
        for path in order {
            if results.len() >= session_cap {
                break;
            }
            let Some(hits) = grouped.remove(&path) else { continue };
            let file = file_query
                .query_row(params![path.as_str()], |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                })
                .optional()
                .map_err(db_error)?;
            let Some((session_id, cwd, title, kind, modified)) = file else { continue };
            let total_hits = hits.len();
            let mut shown: Vec<Hit> = hits.into_iter().take(HITS_PER_SESSION).collect();
            // Newest hits decided the order; the session reads in conversation order.
            shown.sort_by_key(|hit| hit.timestamp);
            let hits: Vec<Value> = shown
                .iter()
                .map(|hit| {
                    json!({
                        "entryId": hit.entry_id,
                        "role": hit.role,
                        "snippet": hit.snippet,
                        "timestamp": hit.timestamp,
                    })
                })
                .collect();
            let mut result = json!({
                "path": path,
                "sessionId": session_id.unwrap_or_default(),
                "cwd": cwd.unwrap_or_default(),
                "title": title,
                "modified": modified,
                "hits": hits,
                "more": total_hits.saturating_sub(shown.len()),
            });
            if kind.as_deref() == Some("chat") {
                result["kind"] = json!("chat");
            }
            results.push(result);
        }
        Ok(response(results, indexed, progress))
    }

    fn index_file(&self, path: &Path, info: &FileInfo, previous: Option<&FileRow>) -> Result<()> {
        let mut file = File::open(path)?;
        let path_text = path.to_string_lossy().into_owned();
        let head_length = info.size.min(HEAD_BYTES) as usize;
        let mut head = vec![0u8; head_length];
        file.read_exact(&mut head)?;
        let head = parse_session_head(&String::from_utf8_lossy(&head));

        // Appends continue from the last complete line; a file rewritten underneath starts over.
        let append = match previous {
            Some(row) if info.size >= row.indexed_bytes => tail_at(&mut file, row.indexed_bytes)? == row.tail,
            _ => false,
        };
        let mut offset = if append { previous.map_or(0, |row| row.indexed_bytes) } else { 0 };
        file.seek(SeekFrom::Start(offset))?;
        let mut carry: Vec<u8> = Vec::new();
        let mut parsed: Vec<ParsedMessage> = Vec::new();
        while offset < info.size {
            let length = CHUNK_BYTES.min(info.size - offset) as usize;
            let mut buffer = vec![0u8; length];
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            offset += read as u64;
            carry.extend_from_slice(&buffer[..read]);
            // Only whole lines count; a trailing partial line is read again next pass.
            if let Some(end) = carry.iter().rposition(|byte| *byte == b'\n') {
                let complete = &carry[..end];
                for line in complete.split(|byte| *byte == b'\n') {
                    if let Some(message) = parse_message_line(&String::from_utf8_lossy(line)) {
                        parsed.push(message);
                    }
                }
                carry.drain(..=end);
            }
        }
        let indexed_bytes = info.size - carry.len() as u64;
        let tail = tail_at(&mut file, indexed_bytes)?;

        let mut connection = self.db.lock();
        let transaction = connection.transaction().map_err(db_error)?;
        if !append {
            transaction
                .execute("DELETE FROM messages WHERE session_file = ?1", params![path_text.as_str()])
                .map_err(db_error)?;
        }
        {
            let mut insert = transaction
                .prepare(
                    "INSERT INTO messages (body, session_file, entry_id, role, timestamp)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(db_error)?;
            for message in &parsed {
                insert
                    .execute(params![
                        message.body.as_str(),
                        path_text.as_str(),
                        message.entry_id.as_str(),
                        message.role,
                        message.timestamp,
                    ])
                    .map_err(db_error)?;
            }
        }
        transaction
            .execute(
                "INSERT OR REPLACE INTO files
                    (path, signature, indexed_bytes, tail, session_id, cwd, title, kind, modified)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    path_text.as_str(),
                    info.signature,
                    indexed_bytes as i64,
                    tail,
                    head.session_id,
                    head.cwd,
                    head.title,
                    head.kind,
                    info.modified,
                ],
            )
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)
    }

    fn remove_missing(&self, present: &HashMap<PathBuf, FileInfo>) -> Result<()> {
        let known = self.known_files()?;
        let gone: Vec<PathBuf> = known
            .keys()
            .filter(|path| !present.contains_key(*path))
            .cloned()
            .collect();
        if gone.is_empty() {
            return Ok(());
        }
        let mut connection = self.db.lock();
        let transaction = connection.transaction().map_err(db_error)?;
        for path in gone {
            let path = path.to_string_lossy().into_owned();
            transaction
                .execute("DELETE FROM messages WHERE session_file = ?1", params![path.as_str()])
                .map_err(db_error)?;
            transaction
                .execute("DELETE FROM files WHERE path = ?1", params![path.as_str()])
                .map_err(db_error)?;
        }
        transaction.commit().map_err(db_error)
    }

    fn known_files(&self) -> Result<HashMap<PathBuf, FileRow>> {
        let connection = self.db.lock();
        let mut statement = connection
            .prepare("SELECT path, signature, indexed_bytes, tail FROM files")
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    FileRow {
                        signature: row.get(1)?,
                        indexed_bytes: row.get(2)?,
                        tail: row.get(3)?,
                    },
                ))
            })
            .map_err(db_error)?;
        let mut known = HashMap::new();
        for row in rows {
            let (path, file) = row.map_err(db_error)?;
            known.insert(PathBuf::from(path), file);
        }
        Ok(known)
    }

    fn file_count(&self) -> usize {
        self.db
            .lock()
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get::<_, i64>(0))
            .map(|count| count.max(0) as usize)
            .unwrap_or(0)
    }
}

fn db_error(error: rusqlite::Error) -> Error {
    Error::msg(format!("conversation index: {error}"))
}

fn response(results: Vec<Value>, indexed: i64, indexing: Option<(usize, usize)>) -> Value {
    json!({
        "results": results,
        "indexed": indexed,
        "indexing": indexing.map(|(done, total)| json!({"done": done, "total": total})),
    })
}

/// `.jsonl` files directly under `root` and one project folder down.
fn scan(root: &Path) -> HashMap<PathBuf, FileInfo> {
    let mut found = HashMap::new();
    let Ok(entries) = fs::read_dir(root) else { return found };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else { continue };
        if metadata.is_dir() {
            let Ok(children) = fs::read_dir(&path) else { continue };
            for child in children.flatten() {
                let child_path = child.path();
                if child_path.extension().is_none_or(|extension| extension != "jsonl") {
                    continue;
                }
                if let Ok(metadata) = child.metadata() {
                    found.insert(child_path, file_info(&metadata));
                }
            }
        } else if metadata.is_file() && path.extension().is_some_and(|extension| extension == "jsonl") {
            found.insert(path, file_info(&metadata));
        }
    }
    found
}

fn file_info(metadata: &fs::Metadata) -> FileInfo {
    let size = metadata.len();
    // Nanoseconds: two rewrites inside one millisecond must still read as a change.
    let elapsed = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok());
    FileInfo {
        signature: format!(
            "{}:{size}",
            elapsed.map(|duration| duration.as_nanos()).unwrap_or(0)
        ),
        size,
        modified: elapsed.map(|duration| duration.as_millis() as i64).unwrap_or(0),
    }
}

/// Hex of the bytes just before `offset`, which an append must leave untouched.
fn tail_at(file: &mut File, offset: u64) -> std::io::Result<String> {
    let start = offset.saturating_sub(TAIL_CHECK_BYTES);
    let mut buffer = vec![0u8; (offset - start) as usize];
    file.seek(SeekFrom::Start(start))?;
    file.read_exact(&mut buffer)?;
    let mut hex = String::with_capacity(buffer.len() * 2);
    for byte in buffer {
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(hex)
}

/// Title, id, cwd and kind from a session file's opening lines.
fn parse_session_head(head: &str) -> SessionHead {
    let mut result = SessionHead::default();
    for line in head.lines().take(3) {
        let Ok(row) = serde_json::from_str::<Value>(line) else { continue };
        match row.get("type").and_then(Value::as_str) {
            Some("title") => {
                if let Some(title) = row.get("title").and_then(Value::as_str) {
                    if !title.trim().is_empty() {
                        result.title = Some(title.trim().to_string());
                    }
                }
            }
            Some("session") => {
                result.session_id = row.get("id").and_then(Value::as_str).map(str::to_string);
                result.cwd = row.get("cwd").and_then(Value::as_str).map(str::to_string);
                result.kind = row.get("kind").and_then(Value::as_str).map(str::to_string);
                break;
            }
            _ => {}
        }
    }
    result
}

/// Text of a persisted user/assistant message line; `None` for anything else.
fn parse_message_line(line: &str) -> Option<ParsedMessage> {
    // Cheap rejects first: most lines are tool results and other entry kinds.
    if !line.contains("\"type\":\"message\"") {
        return None;
    }
    if !line.contains("\"role\":\"user\"") && !line.contains("\"role\":\"assistant\"") {
        return None;
    }
    let row: Value = serde_json::from_str(line).ok()?;
    if row.get("type").and_then(Value::as_str) != Some("message") {
        return None;
    }
    let entry_id = row.get("id").and_then(Value::as_str)?.to_string();
    let message = row.get("message")?;
    let role = match message.get("role").and_then(Value::as_str)? {
        "user" => "user",
        "assistant" => "assistant",
        _ => return None,
    };
    let body = match message.get("content")? {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    let body = body.trim();
    if body.is_empty() {
        return None;
    }
    let body = match body.char_indices().nth(MAX_BODY_CHARS) {
        Some((end, _)) => &body[..end],
        None => body,
    };
    let timestamp = message
        .get("timestamp")
        .and_then(Value::as_i64)
        .or_else(|| message.get("timestamp").and_then(Value::as_f64).map(|stamp| stamp as i64))
        .or_else(|| row.get("timestamp").and_then(Value::as_i64))
        .or_else(|| {
            row.get("timestamp")
                .and_then(Value::as_str)
                .and_then(parse_time)
        })
        .unwrap_or(0);
    Some(ParsedMessage {
        entry_id,
        role,
        body: body.to_string(),
        timestamp,
    })
}

fn parse_time(text: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.timestamp_millis())
}

/// Whitespace-separated terms, lowercased and de-duplicated; at most eight.
fn search_terms(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut terms = Vec::new();
    for term in query.to_lowercase().split_whitespace() {
        if seen.insert(term.to_string()) {
            terms.push(term.to_string());
            if terms.len() == 8 {
                break;
            }
        }
    }
    terms
}

/// Lowercased characters of `text` and, for each one, the source character it came from.
///
/// Keeps snippet indices aligned when a character lowercases to several characters.
fn fold(text: &str) -> (Vec<char>, Vec<usize>) {
    let mut lower = Vec::new();
    let mut source = Vec::new();
    for (index, character) in text.chars().enumerate() {
        for folded in character.to_lowercase() {
            lower.push(folded);
            source.push(index);
        }
    }
    (lower, source)
}

fn find_chars(haystack: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    haystack.windows(needle.len()).position(|window| window == needle)
}

/// The stretch of `body` around its first matching term, every term wrapped in
/// [`MARK_OPEN`]/[`MARK_CLOSE`], on one line, with ellipses where text was cut.
fn build_snippet(body: &str, terms: &[String]) -> String {
    let flat = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = flat.chars().collect();
    let (flat_lower, flat_source) = fold(&flat);
    let mut needles: Vec<Vec<char>> = terms
        .iter()
        .map(|term| fold(term).0)
        .filter(|term| !term.is_empty())
        .collect();
    needles.sort_by(|left, right| right.len().cmp(&left.len()));

    let mut first: Option<usize> = None;
    for needle in &needles {
        if let Some(at) = find_chars(&flat_lower, needle) {
            let at = flat_source.get(at).copied().unwrap_or(0);
            first = Some(first.map_or(at, |value| value.min(at)));
        }
    }
    let first = first.unwrap_or(0);
    let start = if first <= SNIPPET_CONTEXT { 0 } else { first - SNIPPET_CONTEXT };
    let end = chars.len().min(first + SNIPPET_CONTEXT * 2 + 40);
    let window = &chars[start..end];
    let window_text: String = window.iter().collect();
    let (window_lower, window_source) = fold(&window_text);

    // Mark every occurrence of every term, longest first so overlaps keep the longer one.
    let mut marks: Vec<(usize, usize)> = Vec::new();
    for needle in &needles {
        let mut from = 0;
        while let Some(at) = find_chars(&window_lower[from..], needle) {
            let begin = from + at;
            let stop = begin + needle.len();
            let mapped = (window_source[begin], window_source[stop - 1] + 1);
            if !marks.iter().any(|(start, end)| mapped.0 < *end && mapped.1 > *start) {
                marks.push(mapped);
            }
            from = stop;
        }
    }
    marks.sort_unstable();

    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    let mut cursor = 0;
    for (begin, stop) in marks {
        out.extend(&window[cursor..begin]);
        out.push(MARK_OPEN);
        out.extend(&window[begin..stop]);
        out.push(MARK_CLOSE);
        cursor = stop;
    }
    out.extend(&window[cursor..]);
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// FTS5 phrase for one term: quoted, inner quotes doubled.
fn phrase(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

fn like_pattern(term: &str) -> String {
    let mut pattern = String::with_capacity(term.len() + 2);
    pattern.push('%');
    for character in term.chars() {
        if matches!(character, '\\' | '%' | '_') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern.push('%');
    pattern
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::sync::atomic::{AtomicU64, Ordering};

    use serde_json::json;

    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "omp-conversation-{}-{}-{name}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn title_line(title: &str) -> String {
        json!({"type": "title", "v": 1, "title": title}).to_string()
    }

    fn session_line(id: &str, cwd: &str, kind: &str) -> String {
        json!({"type": "session", "version": 3, "id": id, "cwd": cwd, "kind": kind}).to_string()
    }

    fn message_line(id: &str, role: &str, text: &str, timestamp: &str) -> String {
        json!({
            "type": "message",
            "id": id,
            "timestamp": timestamp,
            "message": {"role": role, "content": [{"type": "text", "text": text}]},
        })
        .to_string()
    }

    fn write_fixture(root: &Path, project: &str, name: &str, lines: &[String]) -> PathBuf {
        let directory = root.join(project);
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(name);
        let mut text = String::new();
        for line in lines {
            text.push_str(line);
            text.push('\n');
        }
        fs::write(&path, text).unwrap();
        path
    }

    fn indexed_bytes(index: &ConversationIndex, path: &Path) -> i64 {
        index
            .db
            .lock()
            .query_row(
                "SELECT indexed_bytes FROM files WHERE path = ?1",
                [path.to_string_lossy().into_owned()],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn message_count(index: &ConversationIndex) -> i64 {
        index
            .db
            .lock()
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn finds_user_and_assistant_messages_and_skips_tool_output() {
        let temp = TempDir::new("hits");
        let root = temp.path().join("sessions");
        let db = temp.path().join("index.sqlite");
        let alpha = write_fixture(
            &root,
            "--D--alpha--",
            "alpha.jsonl",
            &[
                title_line("Alpha title"),
                session_line("sess-a", "D:\\alpha", "chat"),
                message_line("u1", "user", "please look up zanzibar", "2026-01-02T03:04:05.000Z"),
                message_line("a1", "assistant", "zanzibar is an island", "2026-01-02T03:04:06.000Z"),
                json!({
                    "type": "message",
                    "id": "tool",
                    "timestamp": "2026-01-02T03:04:07.000Z",
                    "message": {"role": "toolResult", "content": [{"type": "text", "text": "toolonly"}]},
                })
                .to_string(),
                json!({
                    "type": "toolResult",
                    "id": "t2",
                    "timestamp": "2026-01-02T03:04:08.000Z",
                    "content": [{"type": "text", "text": "toolonly"}],
                })
                .to_string(),
            ],
        );
        write_fixture(
            &root,
            "--D--beta--",
            "beta.jsonl",
            &[
                title_line("Beta title"),
                session_line("sess-b", "D:\\beta", "agent"),
                message_line("u2", "user", "mombasa harbor", "2026-01-02T04:00:00.000Z"),
            ],
        );

        let index = ConversationIndex::open(&db).unwrap();
        let report = index.sync(&root);
        assert_eq!(report.indexed, 2, "both sessions are in the index");
        assert_eq!(report.indexing, None);

        let response = index.search("zanzibar", None, 20).unwrap();
        assert_eq!(response["indexed"], 2);
        assert_eq!(response["indexing"], Value::Null);
        assert!(response.get("unavailable").is_none());
        let results = response["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        let result = &results[0];
        assert_eq!(result["path"], alpha.to_string_lossy().as_ref());
        assert_eq!(result["sessionId"], "sess-a");
        assert_eq!(result["cwd"], "D:\\alpha");
        assert_eq!(result["title"], "Alpha title");
        assert_eq!(result["kind"], "chat");
        assert_eq!(result["more"], 0);
        let hits = result["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0]["entryId"], "u1");
        assert_eq!(hits[0]["role"], "user");
        assert!(hits[0]["snippet"].as_str().unwrap().contains("\u{1}zanzibar\u{2}"));
        assert_eq!(hits[1]["entryId"], "a1");
        assert_eq!(hits[1]["role"], "assistant");

        // Tool output is never indexed.
        let response = index.search("toolonly", None, 20).unwrap();
        assert!(response["results"].as_array().unwrap().is_empty());

        // The agent session omits the chat-only kind and only matches in its own cwd.
        let response = index.search("mombasa", None, 20).unwrap();
        let results = response["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].get("kind").is_none());
        let response = index.search("zanzibar", Some(Path::new("D:\\beta")), 20).unwrap();
        assert!(response["results"].as_array().unwrap().is_empty());
    }

    #[test]
    fn appending_a_message_indexes_only_the_new_line() {
        let temp = TempDir::new("append");
        let root = temp.path().join("sessions");
        let db = temp.path().join("index.sqlite");
        let path = write_fixture(
            &root,
            "--D--alpha--",
            "alpha.jsonl",
            &[
                title_line("Growing"),
                session_line("sess-a", "D:\\alpha", "chat"),
                message_line("u1", "user", "first message about apples", "2026-01-02T03:04:05.000Z"),
            ],
        );

        let index = ConversationIndex::open(&db).unwrap();
        index.sync(&root);
        let before_bytes = indexed_bytes(&index, &path);
        let before_messages = message_count(&index);
        assert_eq!(before_messages, 1);
        assert!(before_bytes > 0);

        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(
            file,
            "{}",
            message_line("u2", "user", "second message about bananas", "2026-01-02T03:05:00.000Z")
        )
        .unwrap();
        drop(file);

        let report = index.sync(&root);
        assert_eq!(report.indexed, 1);
        assert!(indexed_bytes(&index, &path) > before_bytes, "indexed_bytes grew");
        assert_eq!(message_count(&index), before_messages + 1, "only the new line was read");

        let response = index.search("bananas", None, 20).unwrap();
        let hits = response["results"][0]["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["entryId"], "u2");

        let response = index.search("apples", None, 20).unwrap();
        let hits = response["results"][0]["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 1, "the old line was not indexed twice");
        assert_eq!(hits[0]["entryId"], "u1");
    }

    #[test]
    fn replaced_file_is_read_again_in_full() {
        let temp = TempDir::new("replace");
        let root = temp.path().join("sessions");
        let db = temp.path().join("index.sqlite");
        let path = write_fixture(
            &root,
            "--D--alpha--",
            "alpha.jsonl",
            &[
                title_line("Original"),
                session_line("sess-a", "D:\\alpha", "chat"),
                message_line("u1", "user", "apples and bananas", "2026-01-02T03:04:05.000Z"),
            ],
        );
        let index = ConversationIndex::open(&db).unwrap();
        index.sync(&root);

        // The rewritten line is longer, so only the tail check can tell this is not an append.
        let mut rewritten = [
            title_line("Original"),
            session_line("sess-a", "D:\\alpha", "chat"),
            message_line("u3", "user", "grapes and cherries everywhere", "2026-01-02T03:04:05.000Z"),
        ]
        .join("\n");
        rewritten.push('\n');
        fs::write(&path, rewritten).unwrap();

        index.sync(&root);
        assert!(index.search("apples", None, 20).unwrap()["results"].as_array().unwrap().is_empty());
        let response = index.search("grapes", None, 20).unwrap();
        assert_eq!(response["results"][0]["hits"][0]["entryId"], "u3");
        assert_eq!(message_count(&index), 1, "the old line was replaced, not kept");
    }

    #[test]
    fn unwritable_db_path_reports_unavailable() {
        let temp = TempDir::new("unavailable");
        let blocked = temp.path().join("index.sqlite");
        fs::create_dir_all(&blocked).unwrap();
        assert!(ConversationIndex::open(&blocked).is_err(), "a directory is not a database file");

        // What the IPC layer answers when the cache cannot be opened.
        let response = match ConversationIndex::open(&blocked) {
            Ok(index) => index.search("zanzibar", None, 20).unwrap(),
            Err(_) => ConversationIndex::unavailable(),
        };
        assert_eq!(response["results"], json!([]));
        assert_eq!(response["indexed"], 0);
        assert_eq!(response["indexing"], Value::Null);
        assert_eq!(response["unavailable"], true);
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn session_head_reads_title_and_header() {
        let head = concat!(
            "{\"type\":\"title\",\"title\":\"Hello\"}\n",
            "{\"type\":\"session\",\"id\":\"s1\",\"cwd\":\"D:\\\\w\",\"kind\":\"agent\"}\n"
        );
        let parsed = parse_session_head(head);
        assert_eq!(parsed.title.as_deref(), Some("Hello"));
        assert_eq!(parsed.session_id.as_deref(), Some("s1"));
        assert_eq!(parsed.cwd.as_deref(), Some("D:\\w"));
        assert_eq!(parsed.kind.as_deref(), Some("agent"));
    }

    #[test]
    fn session_head_ignores_blank_title_and_stops_at_session_line() {
        let head = concat!(
            "{\"type\":\"title\",\"title\":\"  \"}\n",
            "{\"type\":\"session\",\"id\":\"s1\"}\n",
            "{\"type\":\"title\",\"title\":\"Later\"}\n"
        );
        let parsed = parse_session_head(head);
        assert_eq!(parsed.title, None);
        assert_eq!(parsed.session_id.as_deref(), Some("s1"));
    }

    #[test]
    fn message_line_joins_text_parts_and_skips_tool_output() {
        let line = json!({
            "type": "message",
            "id": "m1",
            "timestamp": "2026-01-02T03:04:05.000Z",
            "message": {
                "role": "user",
                "content": [
                    {"type": "text", "text": "list "},
                    {"type": "text", "text": "files"},
                    {"type": "tool_result", "text": "ignored"}
                ]
            }
        })
        .to_string();
        let parsed = parse_message_line(&line).expect("user message");
        assert_eq!(parsed.body, "list \nfiles");
        assert_eq!(parsed.role, "user");
        assert_eq!(parsed.timestamp, 1_767_323_045_000);

        let tool = json!({
            "type": "message",
            "id": "m2",
            "message": {"role": "tool", "content": "ignored"}
        })
        .to_string();
        assert!(parse_message_line(&tool).is_none());
    }

    #[test]
    fn message_line_rejects_blank_bodies_and_truncates_long_ones() {
        let blank = json!({
            "type": "message",
            "id": "m",
            "message": {"role": "user", "content": [{"type": "text", "text": "   "}]}
        })
        .to_string();
        assert!(parse_message_line(&blank).is_none());

        let long = "a".repeat(MAX_BODY_CHARS + 100);
        let line = json!({
            "type": "message",
            "id": "m",
            "message": {"role": "assistant", "content": long}
        })
        .to_string();
        let parsed = parse_message_line(&line).expect("assistant message");
        assert_eq!(parsed.body.chars().count(), MAX_BODY_CHARS);
    }

    #[test]
    fn parse_time_accepts_rfc3339_only() {
        assert_eq!(parse_time("2026-01-02T03:04:05.000Z"), Some(1_767_323_045_000));
        assert_eq!(parse_time("not a time"), None);
    }

    #[test]
    fn search_terms_lowercase_dedupe_and_cap_at_eight() {
        assert_eq!(search_terms("List list  FILES"), vec!["list", "files"]);
        let many = (0..12).map(|index| format!("t{index}")).collect::<Vec<_>>().join(" ");
        assert_eq!(search_terms(&many).len(), 8);
    }

    #[test]
    fn snippet_marks_the_term_and_flattens_newlines() {
        let snippet = build_snippet("first line\nsecond line with needle here", &["needle".to_owned()]);
        assert!(snippet.contains(MARK_OPEN));
        assert!(snippet.contains("needle"));
        assert!(!snippet.contains('\n'));
    }

    #[test]
    fn snippet_adds_ellipsis_when_the_body_is_cut() {
        let body = format!("{} needle {}", "x ".repeat(200), "y ".repeat(200));
        let snippet = build_snippet(&body, &["needle".to_owned()]);
        assert!(snippet.starts_with('…'));
        assert!(snippet.ends_with('…'));
    }

    #[test]
    fn phrase_and_like_pattern_escape_their_wildcards() {
        assert_eq!(phrase("a\"b"), "\"a\"\"b\"");
        assert_eq!(like_pattern("a%b_c\\"), "%a\\%b\\_c\\\\%");
    }
}
