//! On-disk session index for the sidebar.
//!
//! Sessions are JSONL files whose first line is the title slot and whose second line is the
//! session header. Reading a whole session is wasteful (they grow to megabytes), so the index
//! only reads a head window (title, header, first user message) and a tail window (message
//! count, status), and caches the result per (size, mtime) like the upstream index does.

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::UNIX_EPOCH;

use serde::Serialize;
use serde_json::Value;

pub mod conversation;

const TITLE_SLOT_BYTES: usize = 256;
const HEAD_BYTES: usize = 32 * 1024;
const TAIL_BYTES: usize = 32 * 1024;
/// Longest header line read; imported sessions can carry kilobytes of metadata.
const MAX_HEADER_BYTES: usize = 1024 * 1024;
const MAX_PARSE_CACHE_ENTRIES: usize = 4096;
/// Per-file cap for full-content search reads; larger sessions match on this prefix.
const SEARCH_READ_BYTES: u64 = 8 * 1024 * 1024;
/// Total bytes of cached search text.
const SEARCH_CACHE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub path: String,
    pub id: String,
    pub title: Option<String>,
    pub cwd: String,
    pub created: String,
    pub modified: String,
    pub message_count: usize,
    pub size: u64,
    pub status: String,
    pub first_message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

struct ParseCache {
    entries: HashMap<PathBuf, (u64, u64, SessionInfo)>,
    order: VecDeque<PathBuf>,
}

impl ParseCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&self, path: &Path, size: u64, mtime: u64) -> Option<SessionInfo> {
        self.entries
            .get(path)
            .filter(|(cached_size, cached_mtime, _)| *cached_size == size && *cached_mtime == mtime)
            .map(|(_, _, info)| info.clone())
    }

    fn put(&mut self, path: PathBuf, size: u64, mtime: u64, info: SessionInfo) {
        if self.entries.insert(path.clone(), (size, mtime, info)).is_none() {
            self.order.push_back(path);
        }
        while self.order.len() > MAX_PARSE_CACHE_ENTRIES {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }
}

struct SearchCache {
    entries: HashMap<PathBuf, (u64, u64, String)>,
    bytes: u64,
}

impl SearchCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            bytes: 0,
        }
    }

    fn get(&self, path: &Path, size: u64, mtime: u64) -> Option<String> {
        self.entries
            .get(path)
            .filter(|(cached_size, cached_mtime, _)| *cached_size == size && *cached_mtime == mtime)
            .map(|(_, _, text)| text.clone())
    }

    fn put(&mut self, path: PathBuf, size: u64, mtime: u64, text: String) {
        let length = text.len() as u64;
        if let Some((_, _, previous)) = self.entries.remove(&path) {
            self.bytes = self.bytes.saturating_sub(previous.len() as u64);
        }
        self.entries.insert(path.clone(), (size, mtime, text));
        self.bytes += length;
        while self.bytes > SEARCH_CACHE_BYTES {
            let mut oldest = None;
            for key in self.entries.keys() {
                oldest = Some(key.clone());
                break;
            }
            if let Some(key) = oldest {
                if let Some((_, _, text)) = self.entries.remove(&key) {
                    self.bytes = self.bytes.saturating_sub(text.len() as u64);
                }
            } else {
                break;
            }
        }
    }
}

static PARSE_CACHE: LazyLock<Mutex<ParseCache>> = LazyLock::new(|| Mutex::new(ParseCache::new()));
static SEARCH_CACHE: LazyLock<Mutex<SearchCache>> = LazyLock::new(|| Mutex::new(SearchCache::new()));

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// Root of the agent's saved sessions.
pub fn sessions_root() -> PathBuf {
    omp_core::paths::sessions_dir()
}

/// List sessions, newest activity first. `scope = "local"` keeps the current cwd only.
pub fn list(scope: &str, cwd: &Path) -> Vec<SessionInfo> {
    let root = sessions_root();
    let mut out = Vec::new();
    let Ok(dirs) = fs::read_dir(&root) else {
        return out;
    };
    for entry in dirs.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(files) = fs::read_dir(&dir) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(info) = read_session(&path) else {
                continue;
            };
            if scope == "local" && !same_path(Path::new(&info.cwd), cwd) {
                continue;
            }
            // Upstream drops empty sessions (no messages, no title, tiny file) so
            // scratch files never clutter the sidebar.
            if info.message_count == 0 && info.title.is_none() && info.size <= TAIL_BYTES as u64 {
                continue;
            }
            out.push(info);
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

/// Full-content search over saved sessions. All whitespace-separated tokens must match.
pub fn search(query: &str, scope: &str, cwd: &Path) -> Vec<String> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|token| token.to_lowercase())
        .collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    list(scope, cwd)
        .into_iter()
        .filter(|info| {
            read_search_text(Path::new(&info.path), info.size)
                .map(|text| matches_all(&text, &tokens))
                .unwrap_or(false)
        })
        .map(|info| info.path)
        .collect()
}

fn read_search_text(path: &Path, size: u64) -> Option<String> {
    let mtime = modified_nanos(path)?;
    if let Some(text) = lock(&SEARCH_CACHE).get(path, size, mtime) {
        return Some(text);
    }
    let mut file = File::open(path).ok()?;
    let read_len = size.min(SEARCH_READ_BYTES) as usize;
    let mut buffer = vec![0u8; read_len];
    file.read_exact(&mut buffer).ok()?;
    let text = String::from_utf8_lossy(&buffer).into_owned();
    lock(&SEARCH_CACHE).put(path.to_path_buf(), size, mtime, text.clone());
    Some(text)
}

fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase()
    };
    norm(a) == norm(b)
}

fn modified_nanos(path: &Path) -> Option<u64> {
    let meta = fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    let since = modified.duration_since(UNIX_EPOCH).ok()?;
    Some(since.as_nanos() as u64)
}

fn read_session(path: &Path) -> Option<SessionInfo> {
    let meta = fs::metadata(path).ok()?;
    let size = meta.len();
    if size == 0 {
        return None;
    }
    let mtime = modified_nanos(path)?;
    if let Some(info) = lock(&PARSE_CACHE).get(path, size, mtime) {
        return Some(info);
    }
    let info = parse_session(path, size)?;
    lock(&PARSE_CACHE).put(path.to_path_buf(), size, mtime, info.clone());
    Some(info)
}

fn parse_session(path: &Path, size: u64) -> Option<SessionInfo> {
    let mut file = File::open(path).ok()?;

    // Head window: title slot, session header, first user message.
    let head_len = size.min(HEAD_BYTES as u64) as usize;
    let mut head = vec![0u8; head_len];
    file.read_exact(&mut head).ok()?;
    let head_text = String::from_utf8_lossy(&head).into_owned();
    let mut lines = head_text.split('\n');
    let title_line = lines.next().unwrap_or("");
    let title_slot = &title_line[..title_line.len().min(TITLE_SLOT_BYTES)];
    let title = serde_json::from_str::<Value>(title_slot)
        .ok()
        .filter(|value| value.get("type").and_then(Value::as_str) == Some("title"))
        .and_then(|value| {
            value
                .get("title")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
        });

    // The header is the second line; imported sessions can carry a long one.
    let header = read_header(&mut file, &head_text, size)?;
    let id = header
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if id.is_empty() {
        return None;
    }
    let cwd = header
        .get("cwd")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let created = header
        .get("timestamp")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let kind = header
        .get("kind")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let parent_session_path = header
        .get("parentSession")
        .and_then(Value::as_str)
        .map(str::to_owned);

    let first_message = first_user_message(&head_text);

    // Tail window: message count and the session status.
    let tail_len = size.min(TAIL_BYTES as u64) as usize;
    let mut tail = vec![0u8; tail_len];
    if tail_len > 0 {
        file.seek(SeekFrom::Start(size - tail_len as u64)).ok()?;
        file.read_exact(&mut tail).ok()?;
    }
    let (status, message_count) = parse_tail(&String::from_utf8_lossy(&tail));

    let modified = fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .map(|time| {
            let dt: chrono::DateTime<chrono::Utc> = time.into();
            dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        })
        .unwrap_or_default();

    Some(SessionInfo {
        path: path.to_string_lossy().into_owned(),
        id,
        title,
        cwd,
        created: created.unwrap_or_else(|| modified.clone()),
        modified,
        message_count,
        size,
        status,
        first_message,
        parent_session_path,
        kind,
    })
}

fn read_header(file: &mut File, head_text: &str, size: u64) -> Option<Value> {
    let mut lines = head_text.split('\n');
    lines.next()?; // title slot
    if let Some(header_line) = lines.next()
        && let Ok(value) = serde_json::from_str::<Value>(header_line)
    {
        return Some(value);
    }
    // The header is longer than the head window: read until its newline.
    let read_len = size.min(MAX_HEADER_BYTES as u64) as usize;
    let mut buffer = vec![0u8; read_len];
    file.seek(SeekFrom::Start(0)).ok()?;
    file.read_exact(&mut buffer).ok()?;
    let text = String::from_utf8_lossy(&buffer).into_owned();
    let mut iter = text.split('\n');
    iter.next();
    serde_json::from_str::<Value>(iter.next().unwrap_or("")).ok()
}

fn first_user_message(head: &str) -> String {
    for line in head.split('\n') {
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if entry.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let message = entry.get("message");
        let role = message
            .and_then(|value| value.get("role"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if role != "user" {
            continue;
        }
        let content = message.and_then(|value| value.get("content"));
        if let Some(text) = content.and_then(|value| value.as_str()) {
            return text.chars().take(200).collect();
        }
        if let Some(parts) = content.and_then(|value| value.as_array()) {
            for part in parts {
                if part.get("type").and_then(Value::as_str) == Some("text")
                    && let Some(text) = part.get("text").and_then(Value::as_str)
                {
                    return text.chars().take(200).collect();
                }
            }
        }
    }
    String::new()
}

fn parse_tail(tail: &str) -> (String, usize) {
    let mut message_count = 0usize;
    let mut status = "unknown".to_owned();
    let mut last_type: Option<String> = None;
    let mut last_role: Option<String> = None;

    for line in tail.split('\n') {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(line) else {
            // Tail reads can begin or end in the middle of a JSON line.
            continue;
        };
        let entry_type = entry.get("type").and_then(Value::as_str).map(str::to_owned);
        if entry_type.as_deref() == Some("message") {
            message_count += 1;
        }
        if entry_type.is_some() {
            last_type = entry_type;
            last_role = entry
                .get("message")
                .and_then(|value| value.get("role"))
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
    }

    if let Some(last) = last_type.as_deref() {
        status = match last {
            "agent_end" | "turn_end" => "complete",
            "error" => "error",
            "aborted" => "aborted",
            "interrupted" => "interrupted",
            "message_start" | "tool_execution_start" => "pending",
            "message" => {
                if last_role.as_deref() == Some("assistant") {
                    "complete"
                } else {
                    "pending"
                }
            }
            _ => "unknown",
        }
        .to_owned();
    }
    (status, message_count)
}

fn matches_all(text: &str, tokens: &[String]) -> bool {
    let lower = text.to_lowercase();
    tokens.iter().all(|token| lower.contains(token))
}

#[cfg(test)]
mod search_tests {
    use super::*;

    #[test]
    fn requires_every_token() {
        let tokens = vec!["list".to_owned(), "crates".to_owned()];
        assert!(matches_all("please LIST files in CRATES", &tokens));
        assert!(!matches_all("list files in src", &tokens));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_title_session_and_first_message() {
        let path = std::env::temp_dir().join(format!("omp-session-test-{}.jsonl", std::process::id()));
        let body = concat!(
            "{\"type\":\"title\",\"title\":\"Hello\"}\n",
            "{\"type\":\"session\",\"id\":\"abc\",\"timestamp\":\"2026-01-02T03:04:05.000Z\",\"cwd\":\"D:\\\\w\"}\n",
            "{\"type\":\"message\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"list files\"}]}}\n",
            "{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"ok\"}]}}\n"
        );
        fs::write(&path, body).unwrap();
        let info = read_session(&path).unwrap();
        assert_eq!(info.id, "abc");
        assert_eq!(info.title.as_deref(), Some("Hello"));
        assert_eq!(info.first_message, "list files");
        assert!(info.message_count >= 1);
        let _ = fs::remove_file(path);
    }
}
