//! Event batching: ~30 Hz coalescing with backpressure that never drops lifecycle frames.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::Value;

pub const BATCH_INTERVAL: Duration = Duration::from_millis(32);
pub const MAX_BUFFER_SIZE: usize = 1000;

/// Events that must survive backpressure even at buffer capacity.
const NEVER_DROP: &[&str] = &["message_update"];

type Flush = Arc<dyn Fn(Vec<Value>) + Send + Sync>;

struct State {
    pending: Vec<Value>,
    scheduled: bool,
    disposed: bool,
}

#[derive(Clone)]
pub struct EventBatcher {
    state: Arc<Mutex<State>>,
    flush: Flush,
    interval: Duration,
    /// Captured at construction so `push` never panics outside a runtime.
    handle: tokio::runtime::Handle,
}

impl EventBatcher {
    pub fn new(flush: Flush) -> Self {
        Self::with_interval(flush, BATCH_INTERVAL)
    }

    pub fn with_interval(flush: Flush, interval: Duration) -> Self {
        EventBatcher {
            state: Arc::new(Mutex::new(State {
                pending: Vec::new(),
                scheduled: false,
                disposed: false,
            })),
            flush,
            interval,
            handle: tokio::runtime::Handle::current(),
        }
    }

    fn event_type(event: &Value) -> &str {
        event.get("type").and_then(Value::as_str).unwrap_or("")
    }

    /// Queue one event; schedules a flush on the first push of a window.
    pub fn push(&self, event: Value) {
        let mut schedule = false;
        {
            let mut state = self.state.lock();
            if state.disposed {
                return;
            }
            // Backpressure: drop intermediate tool_execution_update only.
            if state.pending.len() >= MAX_BUFFER_SIZE
                && Self::event_type(&event) == "tool_execution_update"
            {
                return;
            }
            state.pending.push(event);
            if !state.scheduled {
                state.scheduled = true;
                schedule = true;
            }
        }
        if schedule {
            let batcher = self.clone();
            self.handle.spawn(async move {
                tokio::time::sleep(batcher.interval).await;
                batcher.do_flush();
            });
        }
    }

    fn do_flush(&self) {
        let batch = {
            let mut state = self.state.lock();
            state.scheduled = false;
            if state.pending.is_empty() || state.disposed {
                return;
            }
            std::mem::take(&mut state.pending)
        };
        (self.flush)(batch);
    }

    /// Flush immediately, e.g. when the sidecar disconnects.
    pub fn flush_now(&self) {
        self.do_flush();
    }

    pub fn dispose(&self) {
        let mut state = self.state.lock();
        state.disposed = true;
        state.scheduled = false;
        state.pending.clear();
    }

    pub fn pending_len(&self) -> usize {
        self.state.lock().pending.len()
    }
}

/// Convenience predicate used by the pool when deciding what a background tab receives.
pub fn is_lifecycle(event_type: &str) -> bool {
    NEVER_DROP.contains(&event_type) || event_type.ends_with("_end") || event_type.ends_with("_error")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    fn batcher() -> (EventBatcher, Arc<StdMutex<Vec<Value>>>) {
        let batches = Arc::new(StdMutex::new(Vec::new()));
        let sink = batches.clone();
        let b = EventBatcher::with_interval(
            Arc::new(move |events: Vec<Value>| sink.lock().unwrap().extend(events)),
            Duration::from_millis(10),
        );
        (b, batches)
    }

    #[tokio::test]
    async fn coalesces_into_one_batch() {
        let (b, flushed) = batcher();
        for i in 0..5 {
            b.push(serde_json::json!({ "type": "message_update", "i": i }));
        }
        assert_eq!(b.pending_len(), 5);
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert_eq!(flushed.lock().unwrap().len(), 5);
        assert_eq!(b.pending_len(), 0);
    }

    #[tokio::test]
    async fn drops_tool_updates_at_capacity_but_keeps_message_updates() {
        let (b, flushed) = batcher();
        for _ in 0..MAX_BUFFER_SIZE {
            b.push(serde_json::json!({ "type": "tool_execution_update" }));
        }
        b.push(serde_json::json!({ "type": "tool_execution_update" }));
        b.push(serde_json::json!({ "type": "message_update" }));
        assert_eq!(b.pending_len(), MAX_BUFFER_SIZE + 1);
        b.flush_now();
        let out = flushed.lock().unwrap();
        assert_eq!(out.len(), MAX_BUFFER_SIZE + 1);
        assert_eq!(out.last().unwrap()["type"], "message_update");
    }

    #[tokio::test]
    async fn flush_now_emits_without_waiting() {
        let (b, flushed) = batcher();
        b.push(serde_json::json!({ "type": "session_info_update" }));
        b.flush_now();
        assert_eq!(flushed.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn dispose_stops_accepting() {
        let (b, flushed) = batcher();
        b.dispose();
        b.push(serde_json::json!({ "type": "message_update" }));
        b.flush_now();
        assert!(flushed.lock().unwrap().is_empty());
        assert_eq!(b.pending_len(), 0);
    }

    #[test]
    fn lifecycle_classification() {
        assert!(is_lifecycle("message_update"));
        assert!(is_lifecycle("agent_end"));
        assert!(is_lifecycle("extension_error"));
        assert!(!is_lifecycle("tool_execution_update"));
    }
}
