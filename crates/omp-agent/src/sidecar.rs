//! Sidecar process: spawn, stdio plumbing, stderr ring, restart backoff.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use parking_lot::Mutex;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc;

use omp_core::{Error, Result};

use crate::argv::{Capabilities, LaunchOptions, build};

/// Restart delays after an unexpected exit; attempts beyond this are terminal.
pub const RESTART_BACKOFF: [u64; 3] = [1, 2, 4];
pub const STDERR_TAIL_LINES: usize = 20;

pub type FrameHandler = Arc<dyn Fn(Value) + Send + Sync>;
/// Borrow-taking sink shape required by [`omp_rpc::RpcClient`].
pub type CommandSink = Arc<dyn Fn(&Value) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct SidecarOptions {
    pub binary: PathBuf,
    pub launch: LaunchOptions,
    pub caps: Capabilities,
}

pub struct Sidecar {
    child: Child,
    tx: mpsc::UnboundedSender<Value>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
    generation: u32,
}

impl Sidecar {
    /// Spawn the sidecar and start pumping stdout frames into `on_frame`.
    pub fn spawn(opts: &SidecarOptions, on_frame: FrameHandler) -> Result<Self> {
        let args = build(&opts.caps, &opts.launch);
        let mut cmd = Command::new(&opts.binary);
        cmd.args(&args)
            .current_dir(&opts.launch.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .env("PI_RPC_EMIT_TITLE", "1")
            .env("PI_NO_PTY", "1")
            .env("PI_NOTIFICATIONS", "off");

        // No console window per spawn: the sidecar is a GUI child process.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.as_std_mut().creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd.spawn().map_err(|e| {
            Error::msg(format!(
                "failed to spawn {}: {e}",
                opts.binary.display()
            ))
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::msg("sidecar stdin unavailable"))?;
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(writer_task(stdin, rx));
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::msg("sidecar stdout unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::msg("sidecar stderr unavailable"))?;

        let tail = Arc::new(Mutex::new(VecDeque::with_capacity(STDERR_TAIL_LINES)));
        let tail_sink = tail.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut buf = tail_sink.lock();
                if buf.len() == STDERR_TAIL_LINES {
                    buf.pop_front();
                }
                buf.push_back(line);
            }
        });

        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let _ = omp_rpc::pump(reader, on_frame).await;
        });

        Ok(Sidecar {
            child,
            tx,
            stderr_tail: tail,
            generation: 0,
        })
    }

    /// OS pid of the child, for liveness checks from outside the struct.
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn stderr_tail(&self) -> Vec<String> {
        self.stderr_tail.lock().iter().cloned().collect()
    }

    /// Queue one frame for the writer task; ordering is preserved by the channel.
    pub fn send(&self, frame: &Value) -> Result<()> {
        self.tx
            .send(frame.clone())
            .map_err(|_| Error::msg("sidecar stdin closed"))
    }

    /// A sync sink for `RpcClient`: every command it emits is queued to this child.
    pub fn frame_sink(&self) -> CommandSink {
        let tx = self.tx.clone();
        Arc::new(move |frame: &Value| {
            let _ = tx.send(frame.clone());
        })
    }

    pub fn try_wait(&mut self) -> Result<Option<i32>> {
        Ok(self
            .child
            .try_wait()
            .map_err(Error::from)?
            .and_then(|status| status.code()))
    }

    /// Synchronous teardown for callers with no runtime (pool `close`).
    pub fn terminate(mut self) {
        drop(self.tx);
        let _ = self.child.start_kill();
    }

    /// Detach listeners first, then kill: no frame arrives after `dispose` returns.
    pub async fn dispose(mut self) {
        drop(self.tx);
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
    }
}

/// Restart budget for one tab's sidecar.
#[derive(Debug, Default)]
pub struct RestartPolicy {
    attempts: AtomicU32,
    exhausted: AtomicBool,
}

impl RestartPolicy {
    /// Delay before the next start, or `None` once the budget is spent.
    pub fn next_delay(&self) -> Option<std::time::Duration> {
        if self.exhausted.load(Ordering::Relaxed) {
            return None;
        }
        let attempt = self.attempts.fetch_add(1, Ordering::Relaxed) as usize;
        match RESTART_BACKOFF.get(attempt) {
            Some(secs) => Some(std::time::Duration::from_secs(*secs)),
            None => {
                self.exhausted.store(true, Ordering::Relaxed);
                None
            }
        }
    }

    pub fn reset(&self) {
        self.attempts.store(0, Ordering::Relaxed);
        self.exhausted.store(false, Ordering::Relaxed);
    }

    pub fn attempts(&self) -> u32 {
        self.attempts.load(Ordering::Relaxed)
    }
}

/// Long-lived writer task: receives frames over a channel and writes them in order.
pub async fn writer_task(mut stdin: ChildStdin, mut rx: mpsc::UnboundedReceiver<Value>) {
    while let Some(frame) = rx.recv().await {
        if omp_rpc::write_frame(&mut stdin, &frame).await.is_err() {
            return;
        }
    }
    let _ = stdin.shutdown().await;
}

pub fn binary_exists(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn restart_backoff_is_one_two_four_then_exhausted() {
        let policy = RestartPolicy::default();
        assert_eq!(policy.next_delay(), Some(Duration::from_secs(1)));
        assert_eq!(policy.next_delay(), Some(Duration::from_secs(2)));
        assert_eq!(policy.next_delay(), Some(Duration::from_secs(4)));
        assert_eq!(policy.next_delay(), None);
        assert_eq!(policy.next_delay(), None);
        policy.reset();
        assert_eq!(policy.next_delay(), Some(Duration::from_secs(1)));
    }

    #[test]
    fn sidecar_options_carry_argv() {
        let opts = SidecarOptions {
            binary: PathBuf::from("omp.exe"),
            launch: LaunchOptions {
                fresh_launch: true,
                ..LaunchOptions::default()
            },
            caps: Capabilities::fork(),
        };
        let args = build(&opts.caps, &opts.launch);
        assert_eq!(args.first().map(String::as_str), Some("--mode"));
    }
}
