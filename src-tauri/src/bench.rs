//! `omp bench --json` subprocess runner.
//!
//! Blueprint: `src/main/benchmark-runner.ts`. One run at a time; `bench:abort` kills the child
//! and the in-flight call reports the abort instead of a malformed summary.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const BENCHMARK_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const PROFILES: [&str; 4] = ["mix", "chat", "prefill", "generation"];

fn bounded_integer(name: &str, value: Option<u64>, min: u64, max: u64) -> Result<u64, String> {
    let value = value.ok_or_else(|| format!("{name} is required"))?;
    if value < min || value > max {
        return Err(format!("{name} must be between {min} and {max}"));
    }
    Ok(value)
}

/// Build the argv for one benchmark run, rejecting the same inputs the desktop app rejects.
pub fn benchmark_args(options: &Value) -> Result<Vec<String>, String> {
    let models = options
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| "Select between 1 and 8 models".to_owned())?;
    if models.is_empty() || models.len() > 8 {
        return Err("Select between 1 and 8 models".to_owned());
    }
    let profile = options
        .get("profile")
        .and_then(Value::as_str)
        .ok_or_else(|| "Invalid benchmark profile".to_owned())?;
    if !PROFILES.contains(&profile) {
        return Err("Invalid benchmark profile".to_owned());
    }

    let mut unique: Vec<String> = Vec::new();
    for model in models {
        let text = model
            .as_str()
            .ok_or_else(|| "Invalid model selector".to_owned())?
            .trim()
            .to_owned();
        if text.is_empty() || text.len() > 200 || text.starts_with('-') {
            return Err("Invalid model selector".to_owned());
        }
        if !unique.contains(&text) {
            unique.push(text);
        }
    }

    let runs = bounded_integer("runs", options.get("runs").and_then(Value::as_u64), 1, 20)?;
    let parallel = bounded_integer(
        "parallel",
        options.get("parallel").and_then(Value::as_u64),
        1,
        8,
    )?;

    let mut args = vec!["bench".to_owned()];
    args.extend(unique);
    args.push("--profile".into());
    args.push(profile.to_owned());
    args.push("--runs".into());
    args.push(runs.to_string());
    args.push("--par".into());
    args.push(parallel.to_string());
    args.push("--json".into());
    if let Some(max_tokens) = options.get("maxTokens").and_then(Value::as_u64) {
        let trimmed = bounded_integer("maxTokens", Some(max_tokens), 1, 8192)?;
        args.push("--max-tokens".into());
        args.push(trimmed.to_string());
    }
    Ok(args)
}

/// The child of the run in flight, so `bench:abort` can kill it.
#[derive(Default)]
pub struct BenchRunner {
    child: Mutex<Option<Child>>,
}

impl BenchRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_running(&self) -> bool {
        self.child.lock().is_some()
    }

    /// Kill the in-flight run. `false` when nothing was running.
    pub fn abort(&self) -> bool {
        let mut slot = self.child.lock();
        match slot.as_mut() {
            Some(child) => {
                let _ = child.start_kill();
                true
            }
            None => false,
        }
    }

    /// Run one benchmark to completion. Never panics on bad output; the renderer gets a
    /// `{success:false,error}` result instead.
    pub async fn run(&self, binary: &Path, options: &Value) -> Value {
        if self.is_running() {
            return json!({ "success": false, "error": "A benchmark is already running" });
        }
        let args = match benchmark_args(options) {
            Ok(args) => args,
            Err(error) => return json!({ "success": false, "error": error }),
        };

        let mut command = Command::new(binary);
        command
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .env("PI_NOTIFICATIONS", "off");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.as_std_mut().creation_flags(0x0800_0000);
        }

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                return json!({ "success": false, "error": format!("failed to spawn bench: {error}") });
            }
        };
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let stdout_task = tokio::spawn(read_capped(stdout));
        let stderr_task = tokio::spawn(read_capped(stderr));
        *self.child.lock() = Some(child);

        let deadline = tokio::time::Instant::now() + BENCHMARK_TIMEOUT;
        let exit = loop {
            let status = {
                let mut slot = self.child.lock();
                match slot.as_mut() {
                    Some(child) => child.try_wait(),
                    // Cleared means `abort` took it: report the cancellation.
                    None => {
                        return json!({ "success": false, "error": "Benchmark aborted" });
                    }
                }
            };
            match status {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(error) => {
                    return json!({ "success": false, "error": error.to_string() });
                }
            }
            if tokio::time::Instant::now() >= deadline {
                self.abort();
                return json!({ "success": false, "error": "Benchmark timed out after 15 minutes" });
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        self.child.lock().take();

        let out = stdout_task.await.unwrap_or_default();
        let err = stderr_task.await.unwrap_or_default();
        let stderr_text = String::from_utf8_lossy(&err).trim().to_owned();
        let exit_code = exit.code();

        if !exit.success() {
            let mut result = json!({
                "success": false,
                "error": format!("bench exited with {exit_code:?}"),
            });
            if !stderr_text.is_empty() {
                result["stderr"] = json!(stderr_text);
            }
            return result;
        }

        match serde_json::from_slice::<Value>(&out) {
            Ok(summary) if summary.is_object() => {
                let mut result = json!({ "success": true, "summary": summary, "exitCode": exit_code });
                if !stderr_text.is_empty() {
                    result["stderr"] = json!(stderr_text);
                }
                result
            }
            _ => json!({ "success": false, "error": "Benchmark returned malformed JSON" }),
        }
    }
}

async fn read_capped(stream: Option<impl tokio::io::AsyncRead + Unpin>) -> Vec<u8> {
    let Some(mut stream) = stream else {
        return Vec::new();
    };
    let mut buffer = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        match stream.read(&mut chunk).await {
            Ok(0) => break,
            Ok(read) => {
                if buffer.len() < MAX_OUTPUT_BYTES {
                    let room = MAX_OUTPUT_BYTES - buffer.len();
                    buffer.extend_from_slice(&chunk[..read.min(room)]);
                }
            }
            Err(_) => break,
        }
    }
    buffer
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> Value {
        json!({ "models": ["magpie/group/never-stuck"], "profile": "chat", "runs": 2, "parallel": 2 })
    }

    #[test]
    fn builds_the_upstream_argv() {
        let args = benchmark_args(&options()).unwrap();
        assert_eq!(
            args,
            vec![
                "bench",
                "magpie/group/never-stuck",
                "--profile",
                "chat",
                "--runs",
                "2",
                "--par",
                "2",
                "--json"
            ]
        );
    }

    #[test]
    fn deduplicates_and_adds_max_tokens() {
        let mut opts = options();
        opts["models"] = json!(["a/b", " a/b ", "c/d"]);
        opts["maxTokens"] = json!(128);
        let args = benchmark_args(&opts).unwrap();
        assert_eq!(args.iter().filter(|a| *a == "a/b").count(), 1);
        assert_eq!(args.iter().filter(|a| *a == "c/d").count(), 1);
        assert!(args.windows(2).any(|w| w == ["--max-tokens", "128"]));
    }

    #[test]
    fn rejects_every_out_of_range_input() {
        let mut opts = options();
        opts["models"] = json!([]);
        assert_eq!(benchmark_args(&opts).unwrap_err(), "Select between 1 and 8 models");

        let mut opts = options();
        opts["models"] = json!(["a", "b", "c", "d", "e", "f", "g", "h", "i"]);
        assert_eq!(benchmark_args(&opts).unwrap_err(), "Select between 1 and 8 models");

        let mut opts = options();
        opts["profile"] = json!("throughput");
        assert_eq!(benchmark_args(&opts).unwrap_err(), "Invalid benchmark profile");

        let mut opts = options();
        opts["models"] = json!(["--evil"]);
        assert_eq!(benchmark_args(&opts).unwrap_err(), "Invalid model selector");

        let mut opts = options();
        opts["runs"] = json!(21);
        assert_eq!(benchmark_args(&opts).unwrap_err(), "runs must be between 1 and 20");

        let mut opts = options();
        opts["parallel"] = json!(0);
        assert_eq!(benchmark_args(&opts).unwrap_err(), "parallel must be between 1 and 8");

        let mut opts = options();
        opts["maxTokens"] = json!(9999);
        assert_eq!(benchmark_args(&opts).unwrap_err(), "maxTokens must be between 1 and 8192");
    }

    #[tokio::test]
    async fn abort_reports_when_nothing_is_running() {
        let runner = BenchRunner::new();
        assert!(!runner.abort());
        assert!(!runner.is_running());
    }
}
