//! Application shell: state, sidecar wiring, and the single `ipc_invoke` bridge command.

pub mod builtin_providers;
pub mod editor;
pub mod ipc;
pub mod models;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use parking_lot::Mutex;
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};

use omp_agent::argv::{Capabilities, LaunchOptions, RpcProbe, TabKind};
use omp_agent::batcher::EventBatcher;
use omp_agent::pool::SidecarPool;
use omp_agent::sidecar::{Sidecar, SidecarOptions};
use omp_core::TabId;
use omp_rpc::RpcClient;

/// Channel names the renderer listens on. Kept in sync with `IPC_EVENTS` in
/// `src/shared/ipc-types.ts`; `scripts/check-channels.mjs` is the guard once Phase 4 lands.
pub mod events {
    pub const EVENTS_BATCH: &str = "rpc:events";
    pub const SIDECAR_STATUS: &str = "sidecar:status";
    pub const TAB_STATUS: &str = "tab:status";
    pub const TERMINAL_FRAME: &str = "terminal:frame";
    pub const SESSION_INFO_UPDATE: &str = "session-info:update";
    pub const EXTENSION_UI: &str = "extension-ui:request";
    pub const HOST_TOOL_CALL: &str = "host-tool:call";
    pub const HOST_URI_REQUEST: &str = "host-uri:request";
    pub const COMMANDS_UPDATE: &str = "commands:update";
    pub const CONFIG_UPDATE: &str = "config:update";
    pub const PROMPT_RESULT: &str = "prompt:result";
    pub const COMMAND_OUTPUT: &str = "command:output";
    pub const EXTENSION_ERROR: &str = "extension:error";
    pub const SUBAGENT_FRAME: &str = "subagent:frame";
    pub const MODEL_CATALOG_UPDATE: &str = "model-catalog:update";
    pub const LIVE_FRAME: &str = "live:frame";
    pub const DICTATION_FRAME: &str = "dictation:frame";
    pub const BTW_FRAME: &str = "btw:frame";
    pub const MENU_ACTION: &str = "menu:action";
    pub const DEEP_LINK: &str = "deep-link";
}

pub struct AppState {
    pub caps: Capabilities,
    pub data_dir: PathBuf,
    pub sidecar_binary: Option<PathBuf>,
    pub pool: SidecarPool<Sidecar>,
    pub clients: Mutex<HashMap<TabId, Arc<RpcClient>>>,
    pub batchers: Mutex<HashMap<TabId, EventBatcher>>,
    pub tab_cwds: Mutex<HashMap<TabId, PathBuf>>,
    pub tab_kinds: Mutex<HashMap<TabId, TabKind>>,
    pub ready_tabs: Mutex<HashSet<TabId>>,
    /// Session file each tab is bound to, so a released tab can resume it.
    pub tab_sessions: Mutex<HashMap<TabId, PathBuf>>,
    /// Last frame time and streaming flag per tab.
    pub tab_activity: Mutex<HashMap<TabId, TabActivity>>,
    /// One idle, already-booted sidecar that new tabs can adopt.
    pub spare: Mutex<Option<SpareSidecar>>,
    pub active_tab: Mutex<TabId>,
    pub next_tab: AtomicU64,
    pub cwd: PathBuf,
}

/// A booted sidecar kept warm for the next tab, so opening a session does not pay
/// the process start cost. Its frames are routed under `route_tab`, which is re-keyed
/// to the adopting tab.
pub struct SpareSidecar {
    pub sidecar: Sidecar,
    pub client: Arc<RpcClient>,
    pub route_tab: Arc<Mutex<TabId>>,
    /// Set once the spare emitted `ready`; adoption must not use a booting process.
    pub ready: Arc<std::sync::atomic::AtomicBool>,
}

/// Push a full tab snapshot; the renderer's tab store listens to `tab:status` and
/// uses this status for the in-place switch guard.
pub(crate) fn emit_tab_status(app: &AppHandle, state: &AppState, tab: &TabId) {
    let active = state.active_tab();
    let status = if state.ready_tabs.lock().contains(tab) { "ready" } else { "starting" };
    let cwd = state
        .tab_cwds
        .lock()
        .get(tab)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| state.cwd.to_string_lossy().into_owned());
    let kind = match state.tab_kinds.lock().get(tab).copied().unwrap_or(TabKind::Agent) {
        TabKind::Chat => "chat",
        TabKind::Agent => "agent",
    };
    let _ = app.emit(
        events::TAB_STATUS,
        json!({
            "tabId": tab,
            "cwd": cwd,
            "status": status,
            "active": tab == &active,
            "visible": tab == &active,
            "kind": kind,
        }),
    );
}

/// Per-tab sidecar activity, used to pick a victim when the sidecar cap is reached.
#[derive(Debug, Clone, Default)]
pub struct TabActivity {
    pub last_frame_ms: u64,
    /// A turn is streaming; such a tab must never be released.
    pub streaming: bool,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

impl AppState {
    pub fn rpc_for(&self, tab: &TabId) -> Result<Arc<RpcClient>, String> {
        self.clients
            .lock()
            .get(tab)
            .cloned()
            .ok_or_else(|| "sidecar not running".to_owned())
    }

    pub fn rpc(&self) -> Result<Arc<RpcClient>, String> {
        let active = self.active_tab();
        self.rpc_for(&active)
    }

    pub fn active_tab(&self) -> TabId {
        self.active_tab.lock().clone()
    }
}

/// How one event channel expects its payload. The renderer's subscriptions are typed against
/// these three shapes in `src/bridge/omp-api.ts`, so emitting the wrong one reads as `undefined`
/// rather than throwing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PayloadShape {
    /// `subscribeActiveTab` / `subscribeTab`: `{ tabId, payload }`.
    Envelope,
    /// `{ request, tabId }` for extension-UI, host-tool and host-URI requests.
    Request,
    /// `{ tabId, frame }` for terminal frames (IpcTerminalFramePayload).
    TerminalFrame,
}

/// Event channels the renderer subscribes to directly instead of through the batched stream.
fn dedicated_channel(frame_type: &str) -> Option<(&'static str, PayloadShape)> {
    match frame_type {
        "extension_ui_request" | "host_tool_call" | "host_uri_request" => {
            Some((frame_type_channel(frame_type)?, PayloadShape::Request))
        }
        "terminal_opened" | "terminal_output" | "terminal_exit" | "terminal_closed" => {
            Some((events::TERMINAL_FRAME, PayloadShape::TerminalFrame))
        }
        _ => Some((frame_type_channel(frame_type)?, PayloadShape::Envelope)),
    }
}

fn frame_type_channel(frame_type: &str) -> Option<&'static str> {
    Some(match frame_type {
        "session_info_update" => events::SESSION_INFO_UPDATE,
        "extension_ui_request" => events::EXTENSION_UI,
        "host_tool_call" => events::HOST_TOOL_CALL,
        "host_uri_request" => events::HOST_URI_REQUEST,
        "available_commands_update" => events::COMMANDS_UPDATE,
        "config_update" => events::CONFIG_UPDATE,
        "prompt_result" => events::PROMPT_RESULT,
        "command_output" => events::COMMAND_OUTPUT,
        "extension_error" => events::EXTENSION_ERROR,
        "subagent_frame" => events::SUBAGENT_FRAME,
        "model_catalog_update" => events::MODEL_CATALOG_UPDATE,
        "live_frame" => events::LIVE_FRAME,
        "dictation_frame" => events::DICTATION_FRAME,
        "btw_frame" => events::BTW_FRAME,
        "terminal_opened" | "terminal_output" | "terminal_exit" | "terminal_closed" => {
            events::TERMINAL_FRAME
        }
        _ => return None,
    })
}

/// Build the payload a given channel's subscription expects.
///
/// The renderer subscribes in two shapes (`src/bridge/omp-api.ts`): an active-tab envelope
/// `{ tabId, payload }`, a flat `{ request, tabId }` for extension-UI/host-tool/host-URI,
/// or `{ tabId, frame }` for terminal frames.
/// `commands:update` additionally delivers the inner array, not the raw frame.
fn event_payload(shape: PayloadShape, frame_type: &str, tab: &TabId, frame: Value) -> Value {
    match shape {
        PayloadShape::Request => json!({ "tabId": tab, "request": frame }),
        PayloadShape::TerminalFrame => json!({ "tabId": tab, "frame": frame }),
        PayloadShape::Envelope if frame_type == "available_commands_update" => {
            let commands = frame.get("commands").cloned().unwrap_or(json!([]));
            json!({ "tabId": tab, "payload": commands })
        }
        PayloadShape::Envelope => json!({ "tabId": tab, "payload": frame }),
    }
}

/// Route one sidecar frame: responses wake their waiter, everything else reaches the renderer.
fn route_frame(app: &AppHandle, tab: &TabId, frame: Value) {
    let state = app.state::<AppState>();
    let frame_type = frame
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    {
        let mut activity = state.tab_activity.lock();
        let entry = activity.entry(tab.clone()).or_default();
        entry.last_frame_ms = now_ms();
        match frame_type.as_str() {
            "agent_start" | "turn_start" | "message_start" | "tool_execution_start" => {
                entry.streaming = true;
            }
            "agent_end" | "turn_end" => entry.streaming = false,
            _ => {}
        }
    }
    if frame_type == "response" {
        if let Some(client) = state.clients.lock().get(tab).cloned() {
            client.on_response(&frame);
        }
        return;
    }
    // Requests raised by a sidecar keep their origin tab so the reply goes back there.
    if matches!(
        frame_type.as_str(),
        "extension_ui_request" | "host_tool_call" | "host_uri_request"
    ) {
        if let Some(id) = frame.get("id").and_then(Value::as_str) {
            state.pool.register_request(id, tab);
        }
    }

    if frame_type == "ready" {
        state.ready_tabs.lock().insert(tab.clone());
        tracing::info!(tab = %tab, "sidecar ready");
        let _ = app.emit(
            events::SIDECAR_STATUS,
            json!({ "tabId": tab, "payload": { "status": "ready" } }),
        );
        emit_tab_status(app, &state, tab);
        return;
    }

    if let Some((channel, shape)) = dedicated_channel(&frame_type) {
        let _ = app.emit(channel, event_payload(shape, &frame_type, tab, frame));
        return;
    }

    let has_batcher = state.batchers.lock().contains_key(tab);
    if !has_batcher {
        let emitter = app.clone();
        let batch_tab = tab.clone();
        let batcher = EventBatcher::new(Arc::new(move |batch: Vec<Value>| {
            let _ = emitter.emit(
                events::EVENTS_BATCH,
                json!({ "tabId": batch_tab, "payload": batch }),
            );
        }));
        state.batchers.lock().insert(tab.clone(), batcher);
    }
    if let Some(batcher) = state.batchers.lock().get(tab) {
        batcher.push(frame);
    }
}

/// Spawn a sidecar for one tab and install its per-tab client.
pub(crate) fn start_tab(app: &AppHandle, tab: TabId, launch: LaunchOptions) -> Result<(), String> {
    let state = app.state::<AppState>();
    let binary = state
        .sidecar_binary
        .clone()
        .ok_or_else(|| "Built-in omp not found".to_owned())?;
    let cwd = launch.cwd.clone();
    let kind = launch.kind;
    let caps = state.caps.clone();
    let argv = omp_agent::argv::build(&caps, &launch);
    // Phantom-tab investigation: `tab:spawn` reaches this path directly, so the trace must carry
    // the full launch shape, not just the argv.
    tracing::info!(
        tab = %tab,
        binary = %binary.display(),
        cwd = %launch.cwd.display(),
        kind = ?launch.kind,
        resume = ?launch.resume_session.as_ref().map(|p| p.display().to_string()),
        fresh = launch.fresh_launch,
        ?argv,
        "spawning sidecar"
    );
    let tab_for_frames = tab.clone();
    let app_for_frames = app.clone();
    let on_frame: omp_agent::sidecar::FrameHandler = Arc::new(move |frame: Value| {
        route_frame(&app_for_frames, &tab_for_frames, frame);
    });
    let sidecar = Sidecar::spawn(
        &SidecarOptions {
            binary,
            launch,
            caps,
        },
        on_frame,
    )
    .map_err(|e| e.to_string())?;
    let client = Arc::new(RpcClient::new(sidecar.frame_sink()));
    state.clients.lock().insert(tab.clone(), client);
    state.tab_cwds.lock().insert(tab.clone(), cwd);
    state.tab_kinds.lock().insert(tab.clone(), kind);
    state.pool.attach(tab, sidecar);
    Ok(())
}

/// Boot a warm spare after a short delay when there is room for another process.
#[allow(dead_code)] // kept for the next warm-spare iteration; boot is disabled for now
pub(crate) fn spawn_spare(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let state = app.state::<AppState>();
        if state.spare.lock().is_some() {
            return;
        }
        if state.clients.lock().len() + 1 > crate::ipc::max_sidecars() {
            return;
        }
        let Some(binary) = state.sidecar_binary.clone() else {
            return;
        };
        let caps = state.caps.clone();
        let launch = LaunchOptions {
            cwd: state.cwd.clone(),
            kind: TabKind::Agent,
            fresh_launch: true,
            ..LaunchOptions::default()
        };
        let route_tab = Arc::new(Mutex::new(TabId::new("spare")));
        let ready = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let app_for_frames = app.clone();
        let tab_for_frames = route_tab.clone();
        let ready_for_frames = ready.clone();
        let on_frame: omp_agent::sidecar::FrameHandler = Arc::new(move |frame: Value| {
            if frame.get("type").and_then(Value::as_str) == Some("ready") {
                ready_for_frames.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            let tab = tab_for_frames.lock().clone();
            route_frame(&app_for_frames, &tab, frame);
        });
        match Sidecar::spawn(&SidecarOptions { binary, launch, caps }, on_frame) {
            Ok(sidecar) => {
                let client = Arc::new(RpcClient::new(sidecar.frame_sink()));
                *state.spare.lock() = Some(SpareSidecar {
                    sidecar,
                    client,
                    route_tab,
                    ready,
                });
                tracing::info!("warm spare booted");
            }
            Err(error) => tracing::warn!(%error, "warm spare failed"),
        }
    });
}

/// Start a tab's sidecar if it is not running yet, emitting the same status events the
/// renderer expects. Called from `tab:set-active` / `tab:set-view`, never from setup.
pub(crate) fn ensure_tab_started(
    app: &AppHandle,
    state: &AppState,
    tab: &TabId,
    origin: &str,
) -> Result<(), String> {
    let cwd = state
        .tab_cwds
        .lock()
        .get(tab)
        .cloned()
        .unwrap_or_else(|| state.cwd.clone());
    let kind = state
        .tab_kinds
        .lock()
        .get(tab)
        .copied()
        .unwrap_or(TabKind::Agent);
    let pending_session = state.tab_sessions.lock().get(tab).cloned();
    // Phantom-tab investigation: log every call, including the no-op ones, so the trace shows
    // which surface asked for which tab (an unexpected tab then has a caller, not just an id).
    tracing::info!(
        tab = %tab,
        origin,
        cwd = %cwd.display(),
        kind = ?kind,
        session = ?pending_session.as_ref().map(|p| p.display().to_string()),
        tabs = state.pool.tab_count(),
        already_started = state.pool.is_started(tab),
        "ensure_tab_started"
    );
    if state.pool.is_started(tab) {
        return Ok(());
    }
    if state.clients.lock().len() >= crate::ipc::max_sidecars()
        && !crate::ipc::evict_idle_sidecar(app, state)
    {
        return Err("sidecar limit reached".to_owned());
    }
    state.ready_tabs.lock().remove(tab);
    let _ = app.emit(
        events::SIDECAR_STATUS,
        json!({
            "tabId": tab,
            "payload": { "status": "starting", "cwd": cwd.to_string_lossy() }
        }),
    );
    emit_tab_status(app, state, tab);
    let resume = state.tab_sessions.lock().get(tab).cloned();
    let launch = LaunchOptions {
        cwd: cwd.clone(),
        kind,
        resume_session: resume.clone(),
        fresh_launch: resume.is_none(),
        ..LaunchOptions::default()
    };
    start_tab(app, tab.clone(), launch).map_err(|err| {
        state.ready_tabs.lock().remove(tab);
        let _ = app.emit(
            events::SIDECAR_STATUS,
            json!({
                "tabId": tab,
                "payload": { "status": "error", "message": err, "cwd": cwd.to_string_lossy() }
            }),
        );
        err
    })
}

fn probe_capabilities(binary: &std::path::Path) -> Capabilities {
    let help = run_help(binary, &["--help"]);
    let stats_help = run_help(binary, &["stats", "--help"]);
    let mut caps = Capabilities::from_help(&help, &stats_help, RpcProbe::default());
    // The bundled sidecar is the fork. `--help` omits `--no-auto-resume`, but the
    // protocol probe spawns with it and reaches ready; without it a fresh tab
    // silently auto-resumes the most recently used session (cross-workspace bleed).
    if caps.session_flag && std::env::var_os("ROMP_STOCK_SIDECAR").is_none() {
        caps.no_auto_resume = true;
    }
    caps
}
fn run_help(binary: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new(binary)
        .args(args)
        .stdin(std::process::Stdio::null())
        .output();
    match output {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            text
        }
        Err(_) => String::new(),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let log_dir = omp_core::paths::data_dir();
    std::fs::create_dir_all(&log_dir).ok();
    let file_appender = tracing_appender::rolling::never(&log_dir, "runtime.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();
    tracing::info!("romp run() start");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = omp_core::paths::data_dir();
            std::fs::create_dir_all(&data_dir).ok();

            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(PathBuf::from));
            let dev_root = std::env::current_dir().ok();
            let sidecar_binary =
                omp_agent::resolve_sidecar(exe_dir.as_deref(), dev_root.as_deref());
            let caps = sidecar_binary
                .as_deref()
                .map(probe_capabilities)
                .unwrap_or_default();

            let launch_cwd = std::env::current_dir().unwrap_or_else(|_| omp_core::home_dir());
            let cwd = if launch_cwd.file_name().and_then(|name| name.to_str()) == Some("src-tauri") {
                launch_cwd
                    .parent()
                    .map(PathBuf::from)
                    .unwrap_or(launch_cwd)
            } else {
                launch_cwd
            };
            let first_tab = TabId::new("tab-1");
            let state = AppState {
                caps,
                data_dir,
                sidecar_binary,
                pool: SidecarPool::new(),
                clients: Mutex::new(HashMap::new()),
                batchers: Mutex::new(HashMap::new()),
                tab_cwds: Mutex::new(HashMap::from([(first_tab.clone(), cwd.clone())])),
                tab_kinds: Mutex::new(HashMap::from([(first_tab.clone(), TabKind::Agent)])),
                ready_tabs: Mutex::new(HashSet::new()),
                tab_sessions: Mutex::new(HashMap::new()),
                tab_activity: Mutex::new(HashMap::new()),
                spare: Mutex::new(None),
                active_tab: Mutex::new(first_tab.clone()),
                next_tab: AtomicU64::new(2),
                cwd: cwd.clone(),
            };
            app.manage(state);
            app.state::<AppState>().pool.register_deferred(first_tab);
            // Warm spare stays disabled: the sidecar cannot switch across workspaces,
            // and same-workspace switches happen in place on the existing sidecar.
            // The sidecar starts when the renderer activates tab-1 (tab:set-active /
            // tab:set-view), so the ready event cannot race the renderer's subscription.
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![ipc::ipc_invoke])
        .run(tauri::generate_context!())
        .expect("error while running Romp");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_shape_matches_subscribe_active_tab() {
        let tab = TabId::new("tab-1");
        let payload = event_payload(
            PayloadShape::Envelope,
            "session_info_update",
            &tab,
            json!({ "type": "session_info_update", "title": "t" }),
        );
        assert_eq!(payload["tabId"], "tab-1");
        assert_eq!(payload["payload"]["type"], "session_info_update");
    }

    #[test]
    fn commands_update_delivers_inner_array() {
        let tab = TabId::new("tab-1");
        let payload = event_payload(
            PayloadShape::Envelope,
            "available_commands_update",
            &tab,
            json!({ "type": "available_commands_update", "commands": [{"name": "compact"}] }),
        );
        assert_eq!(payload["payload"][0]["name"], "compact");
        assert!(payload["payload"].is_array());
    }

    #[test]
    fn request_shape_carries_the_frame_flat() {
        let tab = TabId::new("tab-1");
        let payload = event_payload(
            PayloadShape::Request,
            "extension_ui_request",
            &tab,
            json!({ "type": "extension_ui_request", "id": "r1" }),
        );
        assert_eq!(payload["tabId"], "tab-1");
        assert_eq!(payload["request"]["id"], "r1");
        assert!(payload.get("payload").is_none());
    }

    #[test]
    fn every_dedicated_channel_declares_a_shape() {
        for frame_type in [
            "session_info_update",
            "extension_ui_request",
            "host_tool_call",
            "host_uri_request",
            "available_commands_update",
            "config_update",
            "prompt_result",
            "command_output",
            "extension_error",
            "subagent_frame",
            "model_catalog_update",
            "live_frame",
            "dictation_frame",
            "btw_frame",
            "terminal_opened",
            "terminal_output",
            "terminal_exit",
            "terminal_closed",
        ] {
            assert!(dedicated_channel(frame_type).is_some(), "{frame_type}");
        }
        assert!(dedicated_channel("message_update").is_none());
    }

    #[test]
    fn terminal_frames_use_the_flat_frame_shape() {
        let tab = TabId::new("tab-1");
        let frame = json!({
            "type": "terminal_output",
            "terminalId": "t1",
            "data": "hello",
            "seq": 1,
        });
        let payload = event_payload(
            PayloadShape::TerminalFrame,
            "terminal_output",
            &tab,
            frame.clone(),
        );
        assert_eq!(payload["tabId"], "tab-1");
        assert_eq!(payload["frame"], frame);
        assert!(payload.get("payload").is_none());
        for frame_type in ["terminal_opened", "terminal_output", "terminal_exit", "terminal_closed"] {
            assert_eq!(
                frame_type_channel(frame_type),
                Some(events::TERMINAL_FRAME),
                "{frame_type}"
            );
        }
    }
}
