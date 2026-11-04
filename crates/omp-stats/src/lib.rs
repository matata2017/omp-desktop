//! The bundled `omp stats` dashboard: lifecycle for the server process and a whitelisted client.
//!
//! Blueprints: `src/main/stats-server.ts` and `src/main/stats-client.ts`. The server is spawned
//! from the same binary as the agent sidecar, binds a private ephemeral port, and is killed when
//! the app quits. Nothing outside this module talks HTTP to it.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use omp_core::{Error, Result};

/// GET endpoints the dashboard exposes; anything else is refused before it reaches the server.
pub const VALID_PATHS: &[&str] = &[
    "/api/stats/overview",
    "/api/stats/model-dashboard",
    "/api/stats/costs",
    "/api/stats/frustration",
    "/api/stats/tools",
    "/api/stats/providers",
    "/api/stats/provider-windows",
    "/api/stats/recent",
    "/api/stats/requests",
    "/api/stats/errors",
    "/api/stats/models",
    "/api/stats/folders",
    "/api/stats/timeseries",
    "/api/stats/gain",
    "/api/stats",
    "/api/status",
    "/api/sync",
];

/// Endpoints that mutate state and must be POSTed.
pub const POST_PATHS: &[&str] = &["/api/sync"];

pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
pub const MAX_RESTART_ATTEMPTS: u32 = 3;

/// `/api/request/<id>` is a dynamic path the dashboard serves per row; everything else is an
/// exact match against the fixed table. Mirrors `stats-client.ts`.
pub fn is_valid_path(path: &str) -> bool {
    VALID_PATHS.contains(&path) || is_request_path(path)
}

fn is_request_path(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/api/request/") else {
        return false;
    };
    !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
}

pub fn is_post_path(path: &str) -> bool {
    POST_PATHS.contains(&path)
}

/// Strip ANSI colour codes: the dashboard prints its banner through a colouring logger.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            // CSI: ESC [ ... final byte in @..~
            if chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

/// Pull the port out of the banner line, e.g.
/// `Dashboard available at: http://127.0.0.1:60122`.
pub fn parse_port(line: &str) -> Option<u16> {
    let text = strip_ansi(line);
    let marker = "http://127.0.0.1:";
    let start = text.find(marker)? + marker.len();
    let digits: String = text[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// Consecutive-failure budget. A successful start resets it, so a long session cannot run out
/// of restarts just because it respawned once on each of three separate days.
#[derive(Debug, Default)]
pub struct RestartBudget {
    attempts: std::sync::atomic::AtomicU32,
}

impl RestartBudget {
    /// Reserve one attempt; `false` once the budget is spent.
    pub fn try_acquire(&self) -> bool {
        use std::sync::atomic::Ordering;
        let used = self.attempts.fetch_add(1, Ordering::Relaxed);
        used < MAX_RESTART_ATTEMPTS
    }

    /// A healthy start clears the failure streak.
    pub fn on_success(&self) {
        self.attempts.store(0, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn consecutive_failures(&self) -> u32 {
        self.attempts.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// One server process plus the port it announced.
pub struct StatsServer {
    binary: PathBuf,
    child: Mutex<Option<Child>>,
    port: Arc<Mutex<Option<u16>>>,
    budget: RestartBudget,
}

impl StatsServer {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        StatsServer {
            binary: binary.into(),
            child: Mutex::new(None),
            port: Arc::new(Mutex::new(None)),
            budget: RestartBudget::default(),
        }
    }

    pub fn port(&self) -> Option<u16> {
        *self.port.lock()
    }

    /// Whether the child we spawned is still alive.
    fn child_alive(&self) -> bool {
        let mut slot = self.child.lock();
        match slot.as_mut() {
            Some(child) => matches!(child.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// Forget the port and the child so the next call spawns again.
    fn forget_dead_child(&self) {
        *self.port.lock() = None;
        *self.child.lock() = None;
    }

    /// Spawn the server unless one is already running and healthy. Returns the port once
    /// announced. A crashed child is noticed here, so a dead dashboard heals on the next read
    /// instead of answering with a stale port until the app restarts.
    pub async fn ensure_running(&self) -> Result<u16> {
        if let Some(port) = self.port() {
            if self.child_alive() {
                return Ok(port);
            }
            self.forget_dead_child();
        }
        if !self.budget.try_acquire() {
            return Err(Error::msg("stats server restart budget exhausted"));
        }

        let mut child = Command::new(&self.binary)
            .args([
                "stats",
                "--host",
                "127.0.0.1",
                "--port",
                "0",
                "--no-open",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .env("PI_NOTIFICATIONS", "off")
            .spawn()
            .map_err(|e| Error::msg(format!("failed to spawn stats server: {e}")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::msg("stats server stdout unavailable"))?;
        let port_slot = self.port.clone();
        let (tx, rx) = tokio::sync::oneshot::channel::<u16>();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            let mut sender = Some(tx);
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!(line = %strip_ansi(&line), "stats server output");
                if let Some(port) = parse_port(&line) {
                    if let Some(sender) = sender.take() {
                        *port_slot.lock() = Some(port);
                        let _ = sender.send(port);
                    }
                }
            }
        });

        *self.child.lock() = Some(child);
        match tokio::time::timeout(Duration::from_secs(15), rx).await {
            Ok(Ok(port)) => {
                self.budget.on_success();
                Ok(port)
            }
            Ok(Err(_)) => Err(Error::msg("stats server exited before announcing a port")),
            Err(_) => Err(Error::msg("stats server did not announce a port in 15s")),
        }
    }

    /// Kill the server without awaiting the exit. Used from synchronous teardown paths (window
    /// close, `app:quit`) where blocking on a runtime would deadlock or panic.
    pub fn terminate_now(&self) {
        let child = self.child.lock().take();
        *self.port.lock() = None;
        if let Some(mut child) = child {
            let _ = child.start_kill();
        }
    }

    /// Stop the server; the app calls this on quit.
    pub async fn stop(&self) {
        let child = self.child.lock().take();
        *self.port.lock() = None;
        if let Some(mut child) = child {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
    }
}

/// Whitelisted HTTP client for the dashboard API.
pub struct StatsClient {
    port: u16,
    base: String,
    http: reqwest::Client,
}

impl StatsClient {
    pub fn new(port: u16) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| Error::msg(e.to_string()))?;
        Ok(StatsClient {
            port,
            base: format!("http://127.0.0.1:{port}"),
            http,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// GET one whitelisted path with optional query parameters.
    pub async fn get(&self, path: &str, params: &[(String, String)]) -> Result<Value> {
        if !is_valid_path(path) {
            return Err(Error::msg(format!("invalid stats path: {path}")));
        }
        if is_post_path(path) {
            return Err(Error::msg(format!("{path} requires POST")));
        }
        let response = self
            .http
            .get(format!("{}{}", self.base, path))
            .query(params)
            .send()
            .await
            .map_err(|e| Error::msg(e.to_string()))?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(Error::msg(format!("stats {path} -> {status}: {text}")));
        }
        serde_json::from_str(&text).map_err(|e| Error::msg(format!("stats {path}: {e}")))
    }

    /// POST /api/sync, the only mutating endpoint.
    pub async fn sync(&self) -> Result<Value> {
        let response = self
            .http
            .post(format!("{}/api/sync", self.base))
            .send()
            .await
            .map_err(|e| Error::msg(e.to_string()))?;
        let text = response.text().await.unwrap_or_default();
        Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
    }
}

/// `<binary>` path used by the GUI when no explicit server binary is configured.
pub fn default_binary(sidecar: Option<&Path>) -> Option<PathBuf> {
    sidecar.map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist_has_the_seventeen_upstream_paths() {
        assert_eq!(VALID_PATHS.len(), 17);
        assert_eq!(VALID_PATHS.iter().filter(|p| POST_PATHS.contains(p)).count(), 1);
        assert!(is_valid_path("/api/stats/overview"));
        assert!(is_valid_path("/api/status"));
        assert!(!is_valid_path("/api/stats/../../etc/passwd"));
        assert!(is_valid_path("/api/request/1"));
        assert!(is_valid_path("/api/request/42"));
        assert!(!is_valid_path("/api/request/"));
        assert!(!is_valid_path("/api/request/abc"));
        assert!(!is_valid_path("/api/request/1/extra"));
        assert!(is_post_path("/api/sync"));
    }

    #[test]
    fn port_is_parsed_from_the_banner_with_ansi() {
        assert_eq!(
            parse_port("Dashboard available at: http://127.0.0.1:60122"),
            Some(60122)
        );
        assert_eq!(
            parse_port("\u{1b}[32mDashboard available at:\u{1b}[0m http://127.0.0.1:8081/"),
            Some(8081)
        );
        assert_eq!(parse_port("no banner here"), None);
        assert_eq!(parse_port("http://127.0.0.1:not-a-port"), None);
    }

    #[test]
    fn ansi_stripping_leaves_plain_text() {
        assert_eq!(strip_ansi("\u{1b}[1;32mok\u{1b}[0m done"), "ok done");
        assert_eq!(strip_ansi("plain"), "plain");
    }

    #[test]
    fn budget_spends_consecutive_failures_and_resets_on_success() {
        let budget = RestartBudget::default();
        assert!(budget.try_acquire());
        assert!(budget.try_acquire());
        assert!(budget.try_acquire());
        assert!(!budget.try_acquire());
        assert_eq!(budget.consecutive_failures(), 4);
        budget.on_success();
        assert_eq!(budget.consecutive_failures(), 0);
        assert!(budget.try_acquire(), "a healthy start restores the budget");
    }

    #[tokio::test]
    async fn invalid_paths_are_refused_before_any_request() {
        let client = StatsClient::new(1).unwrap();
        let err = client.get("/etc/passwd", &[]).await.unwrap_err();
        assert!(err.to_string().contains("invalid stats path"));
        let err = client.get("/api/sync", &[]).await.unwrap_err();
        assert!(err.to_string().contains("requires POST"));
    }

    #[tokio::test]
    async fn ensure_running_reports_a_missing_binary() {
        let server = StatsServer::new("definitely-not-a-real-binary.exe");
        let err = server.ensure_running().await.unwrap_err();
        assert!(err.to_string().contains("failed to spawn stats server"));
    }
}
