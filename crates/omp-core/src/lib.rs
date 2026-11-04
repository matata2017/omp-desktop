//! Shared primitives: agent data locations, typed IDs, session state, errors.

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub mod paths {
    use std::path::PathBuf;

    /// `PI_CODING_AGENT_DIR`, else `~/.omp/agent`.
    pub fn agent_dir() -> PathBuf {
        if let Some(dir) = std::env::var_os("PI_CODING_AGENT_DIR") {
            let p = PathBuf::from(dir);
            if !p.as_os_str().is_empty() {
                return p;
            }
        }
        home_dir().join(".omp").join("agent")
    }

    pub fn sessions_dir() -> PathBuf {
        agent_dir().join("sessions")
    }

    pub fn logs_dir() -> PathBuf {
        agent_dir().join("..").join("logs")
    }

    /// GUI-owned data dir, `<app_data>`.
    pub fn data_dir() -> PathBuf {
        if let Some(dir) = std::env::var_os("OMP_DATA_DIR") {
            let p = PathBuf::from(dir);
            if !p.as_os_str().is_empty() {
                return p;
            }
        }
        crate::home_dir()
            .join("AppData")
            .join("Roaming")
            .join("sh.omp.desktop")
    }

    pub fn home_dir() -> PathBuf {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
    }
}

pub use paths::home_dir;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl Error {
    pub fn msg(text: impl Into<String>) -> Self {
        Error::Message(text.into())
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                $name(value.into())
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(v: &str) -> Self {
                $name(v.to_owned())
            }
        }
    };
}

id_type!(TabId);
id_type!(WindowId);

/// Minimal context snapshot parsed out of `get_state`, used by F-OWN and the title bar.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContextUsage {
    #[serde(default, rename = "usedTokens")]
    pub used_tokens: Option<u64>,
    #[serde(default, rename = "maxTokens")]
    pub max_tokens: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionState {
    pub session_file: Option<PathBuf>,
    pub session_id: Option<String>,
    pub model_id: Option<String>,
    pub streaming: bool,
    pub context: Option<ContextUsage>,
}

impl SessionState {
    /// Parse the small subset of a `get_state` response the transport layer needs.
    pub fn from_response(value: &serde_json::Value) -> Self {
        let data = value.get("data").unwrap_or(value);
        let session_file = data
            .get("sessionFile")
            .and_then(|v| v.as_str())
            .map(PathBuf::from);
        let session_id = data
            .get("sessionId")
            .or_else(|| data.get("sessionID"))
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let model_id = data
            .get("model")
            .and_then(|m| m.get("id"))
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let streaming = data
            .get("streaming")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let context = data.get("contextUsage").map(|c| ContextUsage {
            used_tokens: c.get("usedTokens").and_then(|v| v.as_u64()),
            max_tokens: c.get("maxTokens").and_then(|v| v.as_u64()),
        });
        SessionState {
            session_file,
            session_id,
            model_id,
            streaming,
            context,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_state_subset() {
        let frame = serde_json::json!({
            "id": "gui-1",
            "success": true,
            "data": {
                "sessionFile": "C:/x/y.jsonl",
                "sessionId": "abc",
                "model": { "id": "magpie/x" },
                "streaming": true,
                "contextUsage": { "usedTokens": 10, "maxTokens": 200 }
            }
        });
        let state = SessionState::from_response(&frame);
        assert_eq!(state.session_file.unwrap().to_string_lossy(), "C:/x/y.jsonl");
        assert_eq!(state.session_id.as_deref(), Some("abc"));
        assert_eq!(state.model_id.as_deref(), Some("magpie/x"));
        assert!(state.streaming);
        assert_eq!(state.context.unwrap().used_tokens, Some(10));
    }

    #[test]
    fn tolerates_missing_fields() {
        let state = SessionState::from_response(&serde_json::json!({ "data": {} }));
        assert_eq!(state, SessionState::default());
    }

    #[test]
    fn agent_dir_honours_env() {
        // SAFETY: single-threaded test process section; the variable is restored below.
        unsafe { std::env::set_var("PI_CODING_AGENT_DIR", "D:/tmp/agent") };
        assert_eq!(paths::agent_dir(), PathBuf::from("D:/tmp/agent"));
        unsafe { std::env::remove_var("PI_CODING_AGENT_DIR") };
    }
}
