//! Application shell: state, sidecar wiring, and the single `ipc_invoke` bridge command.

pub mod ipc;

use std::path::PathBuf;
use std::sync::Arc;

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
    pub client: Mutex<Option<Arc<RpcClient>>>,
    pub active_tab: Mutex<TabId>,
    pub batcher: Mutex<Option<EventBatcher>>,
    /// Tab the pending batch belongs to; one batch channel serves the visible tab.
    pub batch_tab: Mutex<TabId>,
    pub cwd: PathBuf,
}

impl AppState {
    pub fn rpc(&self) -> Result<Arc<RpcClient>, String> {
        self.client
            .lock()
            .clone()
            .ok_or_else(|| "sidecar not running".to_owned())
    }

    pub fn active_tab(&self) -> TabId {
        self.active_tab.lock().clone()
    }
}

/// How one event channel expects its payload. The renderer's subscriptions are typed against
/// these two shapes in `src/bridge/omp-api.ts`, so emitting the wrong one reads as `undefined`
/// rather than throwing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PayloadShape {
    /// `subscribeActiveTab` / `subscribeTab`: `{ tabId, payload }`.
    Envelope,
    /// `{ request, tabId }` for extension-UI, host-tool and host-URI requests.
    Request,
}

/// Event channels the renderer subscribes to directly instead of through the batched stream.
fn dedicated_channel(frame_type: &str) -> Option<(&'static str, PayloadShape)> {
    let envelope = Some((frame_type_channel(frame_type)?, PayloadShape::Envelope));
    match frame_type {
        "extension_ui_request" | "host_tool_call" | "host_uri_request" => {
            Some((frame_type_channel(frame_type)?, PayloadShape::Request))
        }
        _ => envelope,
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
        _ => return None,
    })
}

/// Build the payload a given channel's subscription expects.
///
/// The renderer subscribes in two shapes (`src/bridge/omp-api.ts`): an active-tab envelope
/// `{ tabId, payload }`, or a flat `{ request, tabId }` for extension-UI/host-tool/host-URI.
/// `commands:update` additionally delivers the inner array, not the raw frame.
fn event_payload(shape: PayloadShape, frame_type: &str, tab: &TabId, frame: Value) -> Value {
    match shape {
        PayloadShape::Request => json!({ "tabId": tab, "request": frame }),
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
    if frame.get("id").is_some() {
        if let Some(client) = state.client.lock().as_ref() {
            client.on_response(&frame);
        }
        return;
    }
    let frame_type = frame
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();

    if frame_type == "ready" {
        let _ = app.emit(
            events::SIDECAR_STATUS,
            json!({ "tabId": tab, "payload": { "status": "ready" } }),
        );
        *state.batch_tab.lock() = tab.clone();
        return;
    }

    if let Some((channel, shape)) = dedicated_channel(&frame_type) {
        let _ = app.emit(channel, event_payload(shape, &frame_type, tab, frame));
        return;
    }

    if let Some(batcher) = state.batcher.lock().clone() {
        *state.batch_tab.lock() = tab.clone();
        batcher.push(frame);
    }
}

/// Spawn a sidecar for one tab and install its client in the shared slot.
fn start_tab(app: &AppHandle, tab: TabId, cwd: PathBuf) -> Result<(), String> {
    let state = app.state::<AppState>();
    let binary = state
        .sidecar_binary
        .clone()
        .ok_or_else(|| "Built-in omp not found".to_owned())?;
    let launch = LaunchOptions {
        cwd,
        kind: TabKind::Agent,
        fresh_launch: true,
        ..LaunchOptions::default()
    };
    let caps = state.caps.clone();
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
    *state.client.lock() = Some(client);
    state.pool.attach(tab.clone(), sidecar);
    *state.active_tab.lock() = tab;
    Ok(())
}

fn probe_capabilities(binary: &std::path::Path) -> Capabilities {
    let help = run_help(binary, &["--help"]);
    let stats_help = run_help(binary, &["stats", "--help"]);
    Capabilities::from_help(&help, &stats_help, RpcProbe::default())
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
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

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

            let cwd = std::env::current_dir().unwrap_or_else(|_| omp_core::home_dir());
            let state = AppState {
                caps,
                data_dir,
                sidecar_binary,
                pool: SidecarPool::new(),
                client: Mutex::new(None),
                active_tab: Mutex::new(TabId::new("tab-1")),
                batcher: Mutex::new(None),
                batch_tab: Mutex::new(TabId::new("tab-1")),
                cwd: cwd.clone(),
            };
            app.manage(state);

            // The synchronous `setup` hook runs outside Tauri's async runtime, and both the
            // batcher's timer and the sidecar's stdout pump need to be spawned on it.
            let app_handle = app.handle().clone();
            let first_tab = TabId::new("tab-1");
            app.state::<AppState>().pool.register_deferred(first_tab.clone());
            tauri::async_runtime::spawn(async move {
                let emitter = app_handle.clone();
                let batcher = EventBatcher::new(Arc::new(move |batch: Vec<Value>| {
                    let tab = emitter.state::<AppState>().batch_tab.lock().clone();
                    let _ = emitter.emit(
                        events::EVENTS_BATCH,
                        json!({ "tabId": tab, "payload": batch }),
                    );
                }));
                *app_handle.state::<AppState>().batcher.lock() = Some(batcher);

                if let Err(err) = start_tab(&app_handle, first_tab, cwd) {
                    tracing::error!("sidecar start failed: {err}");
                    let _ = app_handle.emit(
                        events::SIDECAR_STATUS,
                        json!({ "tabId": "tab-1", "payload": { "status": "error", "message": err } }),
                    );
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![ipc::ipc_invoke])
        .run(tauri::generate_context!())
        .expect("error while running omp desktop");
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
        ] {
            assert!(dedicated_channel(frame_type).is_some(), "{frame_type}");
        }
        assert!(dedicated_channel("message_update").is_none());
    }
}
