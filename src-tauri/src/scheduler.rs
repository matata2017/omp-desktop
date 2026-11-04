//! Scheduled-task runtime: the code-controlled argv for a run, plus the service that spawns,
//! tracks and records runs.
//!
//! Blueprint: `src/main/scheduler/{policy.ts,runner.ts}`. Two rules make this safe to leave
//! unattended, and both live here:
//!
//! 1. The approval mode is decided by the task's tool policy, never by the user's launch profile.
//!    A profile flag that could widen it (`--approval-mode`, `--yolo`, `--tools`, `--model`, …) is
//!    stripped before the argv is assembled.
//! 2. Every run gets its own session directory, so a scheduled run can never resume and continue
//!    an interactive conversation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::process::Child;

use omp_agent::argv::{FlagDenylist, strip_denylisted_flags};

/// Tools a read-only run may use. Anything that writes, spawns or approves is absent.
pub const READ_ONLY_TOOLS: [&str; 6] = ["read", "glob", "grep", "lsp", "web_search", "todo"];

/// Denied on top of the tab denylist for every run sidecar.
pub const RUN_DENIED_FLAGS: [&str; 8] = [
    "--approval-mode",
    "--yolo",
    "--auto-approve",
    "--plan-yolo",
    "--tools",
    "--no-tools",
    "--model",
    "--continue",
];

/// The subset of the above that consumes a separate value token.
pub const RUN_DENIED_WITH_VALUE: [&str; 3] = ["--approval-mode", "--tools", "--model"];

/// Flags for one run: the approval mode follows the tool policy, plus an optional model pin.
pub fn policy_flags(policy: &str, model: Option<&str>) -> Result<Vec<String>, String> {
    let mut flags: Vec<String> = Vec::new();
    match policy {
        "read_only" => {
            flags.push("--approval-mode".into());
            flags.push("always-ask".into());
            flags.push("--tools".into());
            flags.push(READ_ONLY_TOOLS.join(","));
        }
        "workspace" => {
            flags.push("--approval-mode".into());
            flags.push("write".into());
        }
        "full" => {
            flags.push("--approval-mode".into());
            flags.push("yolo".into());
        }
        other => return Err(format!("unknown tool policy: {other}")),
    }
    if let Some(model) = model.filter(|m| !m.trim().is_empty()) {
        flags.push("--model".into());
        flags.push(model.trim().to_owned());
    }
    Ok(flags)
}

/// Full argv for one run: mode, its own session dir, the policy flags, then the user flags with
/// every approval-widening flag stripped.
pub fn run_argv(
    policy: &str,
    model: Option<&str>,
    session_dir: &Path,
    user_flags: &[String],
) -> Result<Vec<String>, String> {
    let mut args = vec!["--mode".to_owned(), "rpc-ui".to_owned()];
    args.push("--session-dir".into());
    args.push(session_dir.to_string_lossy().into_owned());
    args.push("--no-auto-resume".into());
    args.extend(policy_flags(policy, model)?);
    let deny = FlagDenylist {
        names: RUN_DENIED_FLAGS.iter().map(|f| (*f).to_owned()).collect(),
        with_value: RUN_DENIED_WITH_VALUE.iter().map(|f| (*f).to_owned()).collect(),
    };
    args.extend(strip_denylisted_flags(user_flags, Some(&deny)));
    Ok(args)
}

/// One live run, so it can be cancelled and reported.
pub struct LiveRun {
    pub run_id: String,
    pub task_id: String,
    pub child: Child,
    pub started_at: i64,
    /// Set by `cancel`, read by the supervisor so the run ends as `cancelled` rather than
    /// `failed` when the user stopped it.
    pub cancel_requested: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct SchedulerService {
    live: Mutex<HashMap<String, LiveRun>>,
    paused: AtomicBool,
    seq: AtomicU64,
}

impl SchedulerService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Relaxed);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn running(&self) -> Vec<Value> {
        self.live
            .lock()
            .values()
            .map(|run| {
                json!({
                    "runId": run.run_id,
                    "taskId": run.task_id,
                    "startedAt": run.started_at,
                })
            })
            .collect()
    }

    /// Stable-enough run id for the scheduler database.
    pub fn next_run_id(&self) -> String {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        format!("run_{}_{seq}", chrono::Utc::now().timestamp_millis())
    }

    pub fn track(&self, run: LiveRun) {
        self.live.lock().insert(run.run_id.clone(), run);
    }

    /// Drop the registry entry without killing: the supervisor calls this once the child is gone.
    pub fn forget(&self, run_id: &str) {
        self.live.lock().remove(run_id);
    }

    /// Kill one run. `true` when it was live.
    pub fn cancel(&self, run_id: &str) -> bool {
        let mut live = self.live.lock();
        match live.remove(run_id) {
            Some(mut run) => {
                run.cancel_requested.store(true, Ordering::Relaxed);
                let _ = run.child.start_kill();
                true
            }
            None => false,
        }
    }

    /// Reap any child that already exited; returns their ids so the store can be updated.
    pub fn reap(&self) -> Vec<(String, Option<i32>)> {
        let mut finished = Vec::new();
        let mut live = self.live.lock();
        let ids: Vec<String> = live.keys().cloned().collect();
        for id in ids {
            let status = match live.get_mut(&id) {
                Some(run) => run.child.try_wait(),
                None => continue,
            };
            if let Ok(Some(status)) = status {
                live.remove(&id);
                finished.push((id, status.code()));
            }
        }
        finished
    }
}

/// The report a finished run carries: the last non-empty assistant message body.
///
/// `agent_end` frames carry the whole message list, so no delta accumulation is needed.
pub fn last_assistant_text(messages: &[Value]) -> Option<String> {
    for message in messages.iter().rev() {
        if message.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let text = match message.get("content") {
            Some(Value::String(text)) => text.clone(),
            Some(Value::Array(parts)) => parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(""),
            _ => String::new(),
        };
        if !text.trim().is_empty() {
            return Some(text);
        }
    }
    None
}

/// Terminal states a scheduled run can record.
pub fn cancel_reason(cancelled: bool) -> Option<&'static str> {
    cancelled.then_some("user")
}

/// Session directory for one run: isolated per task, under the app's data folder.
pub fn run_session_dir(data_dir: &Path, task_id: &str, run_id: &str) -> PathBuf {
    data_dir.join("scheduler").join(task_id).join(run_id)
}

/// A task's last assistant text is its report.
pub fn extract_report(transcript: &Value) -> Option<String> {
    let messages = transcript.get("messages")?.as_array()?;
    for message in messages.iter().rev() {
        if message.get("role").and_then(Value::as_str) != Some("assistant") {
            continue;
        }
        let text = message
            .get("content")
            .and_then(|content| content.as_array())
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|part| part.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();
        if !text.trim().is_empty() {
            return Some(text);
        }
    }
    None
}

/// Kept for the store: a paused scheduler reports no next fire time.
pub fn next_fire_or_null(paused: bool, computed: Option<i64>) -> Value {
    match (paused, computed) {
        (true, _) => Value::Null,
        (false, Some(ms)) => json!(ms),
        (false, None) => Value::Null,
    }
}

/// Thin wrapper so callers do not need to know the store type.
pub type Store = Arc<omp_scheduler::TaskStore>;


/// Read the run's stdout to EOF (which cancel and a crash both produce), stop at `agent_end`,
/// and report how the run ended. Returns the status the store should record.
pub async fn supervise_run(
    stdout: Option<tokio::process::ChildStdout>,
    timeout_ms: u64,
    cancel_requested: Arc<AtomicBool>,
) -> (&'static str, Option<String>, bool) {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let Some(stdout) = stdout else {
        return ("failed", None, false);
    };
    let mut lines = BufReader::new(stdout).lines();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms.max(1));
    let mut report: Option<String> = None;

    loop {
        let read = tokio::time::timeout_at(deadline, lines.next_line()).await;
        match read {
            Err(_) => {
                // Timed out: the caller's child is killed by cancel; report the timeout.
                return ("timeout", report, true);
            }
            Ok(Ok(None)) => {
                // EOF: either the agent exited or the run was cancelled.
                return (
                    if cancel_requested.load(Ordering::Relaxed) {
                        "cancelled"
                    } else if report.is_some() {
                        "ok"
                    } else {
                        "failed"
                    },
                    report,
                    true,
                );
            }
            Ok(Ok(Some(line))) => {
                let Ok(frame) = serde_json::from_str::<Value>(line.trim()) else {
                    continue;
                };
                match frame.get("type").and_then(Value::as_str) {
                    Some("agent_end") => {
                        if let Some(messages) = frame.get("messages").and_then(Value::as_array) {
                            report = last_assistant_text(messages).or(report);
                        }
                        return ("ok", report, true);
                    }
                    // `message_update` is delta mode: its `message` carries only the role, so it
                    // cannot supply a report. `turn_end` carries a whole message and is a usable
                    // fallback when a run ends without `agent_end`.
                    Some("turn_end") => {
                        if let Some(message) = frame.get("message")
                            && let Some(text) = last_assistant_text(std::slice::from_ref(message))
                        {
                            report = Some(text);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Err(_)) => return ("failed", report, true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_runs_get_the_narrow_tool_list() {
        let flags = policy_flags("read_only", None).unwrap();
        assert_eq!(
            flags,
            vec!["--approval-mode", "always-ask", "--tools", "read,glob,grep,lsp,web_search,todo"]
        );
    }

    #[test]
    fn write_and_full_policies_map_to_their_approval_modes() {
        assert_eq!(
            policy_flags("workspace", None).unwrap(),
            vec!["--approval-mode", "write"]
        );
        assert_eq!(
            policy_flags("full", None).unwrap(),
            vec!["--approval-mode", "yolo"]
        );
        assert!(policy_flags("nonsense", None).is_err());
    }

    #[test]
    fn a_model_pin_rides_along() {
        let flags = policy_flags("workspace", Some("magpie/x")).unwrap();
        assert_eq!(flags, vec!["--approval-mode", "write", "--model", "magpie/x"]);
        let blank = policy_flags("workspace", Some("   ")).unwrap();
        assert_eq!(blank, vec!["--approval-mode", "write"]);
    }

    #[test]
    fn a_user_profile_cannot_widen_the_approval_mode() {
        let user_flags: Vec<String> = [
            "--approval-mode",
            "yolo",
            "--yolo",
            "--auto-approve",
            "--plan-yolo",
            "--tools",
            "bash,write",
            "--model",
            "evil/model",
            "--no-tools",
            "--continue",
            "--system-prompt",
            "--approval-mode",
        ]
        .iter()
        .map(|flag| (*flag).to_string())
        .collect();
        let args = run_argv("read_only", None, Path::new("D:/data/run1"), &user_flags).unwrap();

        // Exactly one approval mode, and it is the policy's.
        let modes: Vec<&String> = args
            .iter()
            .enumerate()
            .filter(|(index, _)| index > &0 && args[index - 1] == "--approval-mode")
            .map(|(_, value)| value)
            .collect();
        assert_eq!(modes, vec!["always-ask"]);
        assert!(!args.iter().any(|arg| arg == "--yolo"));
        assert!(!args.iter().any(|arg| arg == "--auto-approve"));
        assert!(!args.iter().any(|arg| arg == "--no-tools"));
        assert!(!args.iter().any(|arg| arg == "--continue"));
        // The smuggled tool list and model are gone; the policy's tool list survives.
        assert!(!args.iter().any(|arg| arg == "bash,write"));
        assert!(!args.iter().any(|arg| arg == "evil/model"));
        assert!(args
            .iter()
            .any(|arg| arg == "read,glob,grep,lsp,web_search,todo"));
        // A prompt that merely reads like a flag survives as data.
        assert!(args.windows(2).any(|pair| pair == ["--system-prompt", "--approval-mode"]));
        // Every run is isolated in its own session directory.
        assert!(args.windows(2).any(|pair| pair == ["--session-dir", "D:/data/run1"]));
        assert!(args.iter().any(|arg| arg == "--no-auto-resume"));
    }

    #[test]
    fn report_is_the_last_non_empty_assistant_text() {
        let transcript = json!({
            "messages": [
                { "role": "user", "content": [{ "text": "do the thing" }] },
                { "role": "assistant", "content": [{ "text": "first" }] },
                { "role": "assistant", "content": [{ "text": "" }] },
                { "role": "assistant", "content": [{ "text": "final report" }] },
            ]
        });
        assert_eq!(extract_report(&transcript).as_deref(), Some("final report"));
        assert_eq!(extract_report(&json!({ "messages": [] })), None);
        assert_eq!(extract_report(&json!({})), None);
    }

    #[test]
    fn paused_scheduler_reports_no_next_fire() {
        assert_eq!(next_fire_or_null(true, Some(1234)), Value::Null);
        assert_eq!(next_fire_or_null(false, Some(1234)), json!(1234));
        assert_eq!(next_fire_or_null(false, None), Value::Null);
    }

    #[test]
    fn report_comes_from_the_agent_end_message_list() {
        let messages = json!([
            { "role": "user", "content": "do it" },
            { "role": "assistant", "content": [{ "text": "working" }] },
            { "role": "assistant", "content": [{ "text": "final answer" }] }
        ]);
        assert_eq!(
            last_assistant_text(messages.as_array().unwrap()).as_deref(),
            Some("final answer")
        );
        // A string body works too, and empty tails are skipped.
        let messages = json!([
            { "role": "assistant", "content": "plain" },
            { "role": "assistant", "content": "   " }
        ]);
        assert_eq!(
            last_assistant_text(messages.as_array().unwrap()).as_deref(),
            Some("plain")
        );
        assert_eq!(last_assistant_text(&[]), None);
        assert_eq!(cancel_reason(true), Some("user"));
        assert_eq!(cancel_reason(false), None);
    }

    #[test]
    fn service_tracks_and_cancels_nothing_safely() {
        let service = SchedulerService::new();
        assert!(!service.cancel("run_none"));
        assert!(service.running().is_empty());
        assert!(service.reap().is_empty());
        let first = service.next_run_id();
        let second = service.next_run_id();
        assert_ne!(first, second);
    }
}
