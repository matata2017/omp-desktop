//! On-disk session index for the sidebar.
//!
//! Reads `<home>/.omp/agent/sessions/<encoded-cwd>/<file>.jsonl` and extracts the title
//! slot, session header, first user message, message count, and timestamps. This is the
//! minimum needed to render the sidebar without a running sidecar; richer caches and
//! search indexes land in later batches.

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

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
}

/// Root of the agent's saved sessions.
pub fn sessions_root() -> PathBuf {
    omp_core::home_dir().join(".omp").join("agent").join("sessions")
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
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(info) = read_session(&path) else {
                continue;
            };
            if scope == "local" && !same_path(Path::new(&info.cwd), cwd) {
                continue;
            }
            out.push(info);
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
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

fn read_session(path: &Path) -> Option<SessionInfo> {
    let meta = fs::metadata(path).ok()?;
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);

    let mut title: Option<String> = None;
    let mut id = String::new();
    let mut cwd = String::new();
    let mut created: Option<String> = None;
    let mut message_count = 0usize;
    let mut first_message = String::new();
    let mut line = String::new();

    loop {
        line.clear();
        let read = reader.read_line(&mut line).ok()?;
        if read == 0 {
            break;
        }
        if line.contains("\"type\":\"title\"") {
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                let value = value.get("title").and_then(Value::as_str).unwrap_or("");
                if !value.is_empty() {
                    title = Some(value.to_owned());
                }
            }
        } else if line.contains("\"type\":\"session\"") {
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                id = value
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                cwd = value
                    .get("cwd")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                created = value
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }
        } else if line.contains("\"type\":\"message\"") {
            message_count += 1;
            if first_message.is_empty() {
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    let message = value.get("message");
                    let role = message
                        .and_then(|m| m.get("role"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if role == "user" {
                        first_message = message_text(message).unwrap_or_default();
                    }
                }
            }
        }
    }

    if id.is_empty() {
        return None;
    }
    let modified = meta
        .modified()
        .ok()
        .map(|time| {
            let dt: chrono::DateTime<chrono::Utc> = time.into();
            dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        })
        .or_else(|| created.clone())
        .unwrap_or_default();
    Some(SessionInfo {
        path: path.to_string_lossy().into_owned(),
        id,
        title,
        cwd,
        created: created.unwrap_or_else(|| modified.clone()),
        modified,
        message_count,
        size: meta.len(),
        status: "complete".to_owned(),
        first_message,
    })
}

fn message_text(message: Option<&Value>) -> Option<String> {
    let content = message?.get("content")?;
    if let Some(text) = content.as_str() {
        return Some(text.to_owned());
    }
    let parts = content.as_array()?;
    for part in parts {
        if part.get("type").and_then(Value::as_str) == Some("text") {
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                return Some(text.to_owned());
            }
        }
    }
    None
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
            fs::read_to_string(&info.path)
                .map(|text| matches_all(&text, &tokens))
                .unwrap_or(false)
        })
        .map(|info| info.path)
        .collect()
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
        assert_eq!(info.message_count, 2);
        let _ = fs::remove_file(path);
    }
}
