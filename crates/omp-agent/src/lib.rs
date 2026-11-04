//! Sidecar lifecycle: capability-gated argv, process plumbing, event batching.

pub mod argv;
pub mod batcher;
pub mod pool;
pub mod sidecar;

pub use argv::{
    Capabilities, LaunchOptions, RpcProbe, TabKind, build, build_stats, resolve_sidecar,
    strip_denylisted_flags,
};
pub use batcher::{BATCH_INTERVAL, EventBatcher, MAX_BUFFER_SIZE, is_lifecycle};
pub use pool::{SidecarHandle, SidecarPool, ViewState};
pub use sidecar::{RESTART_BACKOFF, RestartPolicy, Sidecar, SidecarOptions, STDERR_TAIL_LINES};
