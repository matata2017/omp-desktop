//! Id-correlated RPC command client, mirroring `src/main/rpc-client.ts`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::Value;
use tokio::sync::oneshot;

use omp_core::{Error, Result};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(8);

/// Commands that resolve only after long-running model, OAuth, transcript or human-interaction
/// work. The 8 s default would report a false timeout while the sidecar keeps mutating state.
/// Table copied from `RPC_COMMAND_TIMEOUTS` in `src/shared/rpc-client.ts`.
const COMMAND_TIMEOUTS: &[(&str, u64)] = &[
    ("bash", 660_000),
    ("eval", 660_000),
    ("compact", 660_000),
    ("handoff", 660_000),
    ("plan_approval", 660_000),
    ("switch_leaf", 660_000),
    ("login", 660_000),
    ("export_html", 120_000),
    ("reload_plugins", 120_000),
    ("list_reset_credits", 60_000),
    ("redeem_reset_credit", 60_000),
    ("set_model", 30_000),
    ("cycle_model", 30_000),
    ("get_transcript", 30_000),
    ("get_messages", 30_000),
    ("get_transcript_page", 30_000),
];

/// Per-command timeout override, or `None` to use the 8 s default.
pub fn timeout_for_command(command_type: &str) -> Option<Duration> {
    COMMAND_TIMEOUTS
        .iter()
        .find(|(name, _)| *name == command_type)
        .map(|(_, ms)| Duration::from_millis(*ms))
}

pub type FrameSink = Arc<dyn Fn(&Value) + Send + Sync>;

struct Pending {
    tx: oneshot::Sender<Result<Value>>,
    seq: u64,
}

pub struct RpcClient {
    pending: Mutex<HashMap<String, Pending>>,
    next: AtomicU64,
    send: FrameSink,
    timeout: Duration,
}

impl RpcClient {
    pub fn new(send: FrameSink) -> Self {
        Self::with_timeout(send, DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(send: FrameSink, timeout: Duration) -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(0),
            send,
            timeout,
        }
    }

    fn next_id(&self) -> (String, u64) {
        let seq = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        (format!("gui-{seq}"), seq)
    }

    /// Send a command and await its correlated response.
    pub async fn command(&self, cmd: &mut Value, timeout: Option<Duration>) -> Result<Value> {
        let (id, seq) = self.next_id();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().insert(id.clone(), Pending { tx, seq });
        if let Some(obj) = cmd.as_object_mut() {
            obj.insert("id".to_owned(), Value::String(id.clone()));
        }
        (self.send)(cmd);

        let wait = timeout.unwrap_or(self.timeout);
        match tokio::time::timeout(wait, rx).await {
            Ok(Ok(Ok(frame))) => Ok(frame),
            Ok(Ok(Err(err))) => Err(err),
            Ok(Err(_)) => Err(Error::msg(format!("RPC cancelled: {id}"))),
            Err(_) => {
                self.pending.lock().remove(&id);
                let kind = cmd
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                Err(Error::msg(format!(
                    "RPC timeout ({}ms): {kind}",
                    wait.as_millis()
                )))
            }
        }
    }

    /// Fire a command without waiting for its response (used by `bash`).
    pub fn fire(&self, cmd: &mut Value) -> String {
        let (id, _) = self.next_id();
        if let Some(obj) = cmd.as_object_mut() {
            obj.insert("id".to_owned(), Value::String(id.clone()));
        }
        (self.send)(cmd);
        id
    }

    /// Route an inbound response frame to its waiting promise. `true` when a waiter matched.
    pub fn on_response(&self, frame: &Value) -> bool {
        let Some(id) = frame.get("id").and_then(Value::as_str) else {
            return false;
        };
        let Some(entry) = self.pending.lock().remove(id) else {
            return false;
        };
        // A matched waiter counts as delivered even if it already gave up (timeout race).
        let _ = entry.tx.send(Ok(frame.clone()));
        true
    }

    /// Reject every pending request (sidecar died or was disposed). Waiters see an error, not a
    /// synthetic `success:false` frame — matching `rejectAll` in `src/main/rpc-client.ts`.
    pub fn reject_all(&self, reason: &str) {
        let drained: Vec<Pending> = self.pending.lock().drain().map(|(_, v)| v).collect();
        for entry in drained {
            let _ = entry.tx.send(Err(Error::msg(reason.to_owned())));
            let _ = entry.seq;
        }
    }

    pub fn pending_count(&self) -> usize {
        self.pending.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    fn recording_client() -> (Arc<RpcClient>, Arc<StdMutex<Vec<Value>>>) {
        let sent = Arc::new(StdMutex::new(Vec::new()));
        let sink = sent.clone();
        let client = Arc::new(RpcClient::with_timeout(
            Arc::new(move |frame: &Value| sink.lock().unwrap().push(frame.clone())),
            Duration::from_millis(50),
        ));
        (client, sent)
    }

    #[tokio::test]
    async fn injects_sequential_ids() {
        let (client, sent) = recording_client();
        let mut a = serde_json::json!({ "type": "get_state" });
        let mut b = serde_json::json!({ "type": "get_state" });
        let _ = client.fire(&mut a);
        let _ = client.fire(&mut b);
        let frames = sent.lock().unwrap();
        assert_eq!(frames[0]["id"], "gui-1");
        assert_eq!(frames[1]["id"], "gui-2");
    }

    #[tokio::test]
    async fn resolves_matching_response() {
        let (client, _sent) = recording_client();
        let waiter = {
            let client = client.clone();
            tokio::spawn(async move {
                let mut cmd = serde_json::json!({ "type": "get_state" });
                client.command(&mut cmd, None).await
            })
        };
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(client.on_response(&serde_json::json!({ "id": "gui-1", "success": true })));
        let value = waiter.await.unwrap().unwrap();
        assert_eq!(value["success"], true);
        assert_eq!(client.pending_count(), 0);
    }

    #[tokio::test]
    async fn times_out_and_drops_pending() {
        let (client, _sent) = recording_client();
        let mut cmd = serde_json::json!({ "type": "get_available_models" });
        let err = client
            .command(&mut cmd, Some(Duration::from_millis(20)))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("RPC timeout (20ms): get_available_models"));
        assert_eq!(client.pending_count(), 0);
    }

    #[tokio::test]
    async fn reject_all_unblocks_waiters() {
        let (client, _sent) = recording_client();
        let waiter = {
            let client = client.clone();
            tokio::spawn(async move {
                let mut cmd = serde_json::json!({ "type": "get_state" });
                client.command(&mut cmd, None).await
            })
        };
        tokio::time::sleep(Duration::from_millis(10)).await;
        client.reject_all("sidecar exited");
        let err = waiter.await.unwrap().unwrap_err();
        assert!(err.to_string().contains("sidecar exited"));
        assert_eq!(client.pending_count(), 0);
    }
}
