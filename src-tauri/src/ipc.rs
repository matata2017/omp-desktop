//! The single bridge command. Every renderer→main call arrives here as `(channel, payload)` with
//! the channel string unchanged from `IPC_COMMANDS`, so `src/shared/ipc-types.ts` stays the only
//! contract and `src/bridge/omp-api.ts` needs no per-method mapping.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, State};
use crate::{AppState, ensure_tab_started, start_tab};
use omp_agent::argv::{LaunchOptions, TabKind};
use omp_core::{TabId, WindowId};

const FS_READ_CAP: u64 = 8 * 1024 * 1024;
const FS_LIST_MAX_DEPTH: usize = 8;
const FS_LIST_MAX_ENTRIES: usize = 2000;

#[tauri::command]
pub async fn ipc_invoke(
    app: AppHandle,
    state: State<'_, AppState>,
    channel: String,
    payload: Option<Value>,
) -> Result<Value, String> {
    let payload = payload.unwrap_or_else(|| json!({}));
    tracing::debug!(channel = %channel, "ipc_invoke");
    if channel == "runtime:error-report" {
        tracing::warn!(?payload, "renderer runtime error");
    }
    match channel.as_str() {
        // ---- RPC: the whole conversation flows through these two -------------------------
        "rpc:command" => {
            let command = payload
                .get("command")
                .cloned()
                .ok_or_else(|| "rpc:command requires a command".to_owned())?;
            let timeout = payload
                .get("timeoutMs")
                .and_then(Value::as_u64)
                .map(std::time::Duration::from_millis);
            let tab = state.active_tab();
            let client = state.rpc_for(&tab)?;
            let mut cmd = command;
            let response = client
                .command(&mut cmd, timeout)
                .await
                .map_err(|e| e.to_string())?;
            sync_switch_binding(&state, &tab, &cmd, &response);
            Ok(response)
        }
        "rpc:command-for-tab" => {
            let command = payload
                .get("command")
                .cloned()
                .ok_or_else(|| "rpc:command-for-tab requires a command".to_owned())?;
            let timeout = payload
                .get("timeoutMs")
                .and_then(Value::as_u64)
                .map(std::time::Duration::from_millis);
            let tab = payload
                .get("tabId")
                .and_then(Value::as_str)
                .map(TabId::new)
                .unwrap_or_else(|| state.active_tab());
            let client = state.rpc_for(&tab)?;
            let mut cmd = command;
            let response = client
                .command(&mut cmd, timeout)
                .await
                .map_err(|e| e.to_string())?;
            sync_switch_binding(&state, &tab, &cmd, &response);
            Ok(response)
        }

        // ---- Side-channel replies (extension UI / host tool / host URI) ------------------
        "extension-ui:respond" => {
            route_side_channel(&state, &payload, "response", true)?;
            Ok(Value::Null)
        }
        "host-tool:result" => {
            route_side_channel(&state, &payload, "result", true)?;
            Ok(Value::Null)
        }
        "host-tool:update" => {
            route_side_channel(&state, &payload, "update", false)?;
            Ok(Value::Null)
        }
        "host-uri:result" => {
            route_side_channel(&state, &payload, "result", true)?;
            Ok(Value::Null)
        }

        // ---- Tabs -----------------------------------------------------------------------
        "tab:get-all" => {
            let active = state.active_tab();
            ensure_tab_started(&app, &state, &active)?;
            let ready = state.ready_tabs.lock();
            let cwds = state.tab_cwds.lock();
            let kinds = state.tab_kinds.lock();
            let tabs: Vec<Value> = state
                .pool
                .tabs()
                .into_iter()
                .map(|tab| {
                    let status = if ready.contains(&tab) { "ready" } else { "starting" };
                    let cwd = cwds
                        .get(&tab)
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_else(|| state.cwd.to_string_lossy().into_owned());
                    let kind = match kinds.get(&tab).copied().unwrap_or(TabKind::Agent) {
                        TabKind::Chat => "chat",
                        TabKind::Agent => "agent",
                    };
                    json!({
                        "tabId": tab,
                        "cwd": cwd,
                        "status": status,
                        "active": tab == active,
                        "visible": tab == active,
                        "kind": kind,
                    })
                })
                .collect();
            Ok(json!(tabs))
        }
        "tab:spawn" => {
            // Bound the number of live sidecars: each one costs a full agent process.
            // Evict the least-recently-active idle tab first; only answer null when no
            // tab can be released.
            if state.clients.lock().len() >= max_sidecars() && !evict_idle_sidecar(&app, &state) {
                return Ok(Value::Null);
            }
            let cwd = payload
                .get("cwd")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .unwrap_or_else(|| state.cwd.clone());
            let kind = match payload.get("kind").and_then(Value::as_str) {
                Some("chat") => TabKind::Chat,
                _ => TabKind::Agent,
            };
            let session_path = payload
                .get("sessionPath")
                .and_then(Value::as_str)
                .map(PathBuf::from);
            if let Some(path) = &session_path {
                if let Some(owner) = state.pool.session_owner(path) {
                    return Ok(json!({ "tabId": null, "ownerTabId": owner, "ownerWinId": 1, "refusal": "owned" }));
                }
            }
            let number = state.next_tab.fetch_add(1, Ordering::SeqCst);
            let tab = TabId::new(format!("tab-{number}"));
            state.tab_cwds.lock().insert(tab.clone(), cwd.clone());
            state.tab_kinds.lock().insert(tab.clone(), kind);
            if let Some(path) = &session_path {
                state.tab_sessions.lock().insert(tab.clone(), path.clone());
            }
            let launch = LaunchOptions {
                cwd: cwd.clone(),
                kind,
                resume_session: session_path.clone(),
                fresh_launch: session_path.is_none(),
                ..LaunchOptions::default()
            };
            start_tab(&app, tab.clone(), launch)?;
            // The renderer switches to a spawned tab right away; keep main's active view
            // in sync so a reconcile does not snap back to the previous tab.
            *state.active_tab.lock() = tab.clone();
            if let Some(path) = session_path {
                let _ = state.pool.claim_session(&tab, &path);
            }
            Ok(json!({ "tabId": tab, "cwd": cwd.to_string_lossy() }))
        }
        "tab:close" => {
            let Some(tab) = payload.get("tabId").and_then(Value::as_str).map(TabId::new) else {
                return Ok(json!(false));
            };
            let existed = state.pool.close(&tab);
            state.clients.lock().remove(&tab);
            if let Some(batcher) = state.batchers.lock().remove(&tab) {
                batcher.dispose();
            }
            state.tab_cwds.lock().remove(&tab);
            state.tab_kinds.lock().remove(&tab);
            state.tab_sessions.lock().remove(&tab);
            state.tab_activity.lock().remove(&tab);
            state.ready_tabs.lock().remove(&tab);
            Ok(json!(existed))
        }
        "tab:set-order" => Ok(json!(true)),
        "tab:set-active" => {
            let Some(tab) = payload.get("tabId").and_then(Value::as_str).map(TabId::new) else {
                return Ok(json!(false));
            };
            *state.active_tab.lock() = tab.clone();
            ensure_tab_started(&app, &state, &tab)?;
            replay_ready(&app, &state, &tab);
            Ok(json!(true))
        }
        "tab:set-view" => {
            let Some(focused) = payload
                .get("focusedTabId")
                .and_then(Value::as_str)
                .map(TabId::new)
            else {
                return Ok(json!(false));
            };
            let visible: Vec<TabId> = payload
                .get("visibleTabIds")
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(Value::as_str)
                        .map(TabId::new)
                        .collect()
                })
                .unwrap_or_else(|| vec![focused.clone()]);
            *state.active_tab.lock() = focused.clone();
            state
                .pool
                .set_view(&WindowId::new("main"), focused.clone(), &visible)
                .map_err(|e| e.to_string())?;
            for tab in &visible {
                ensure_tab_started(&app, &state, tab)?;
                replay_ready(&app, &state, tab);
            }
            Ok(json!(true))
        }
        "tab:get-session-owner" => {
            let session = payload.get("sessionPath").and_then(Value::as_str);
            let owner = session.and_then(|s| state.pool.session_owner(Path::new(s)));
            Ok(match owner {
                Some(tab) => json!({ "tabId": tab }),
                None => Value::Null,
            })
        }

        // ---- Sidecar --------------------------------------------------------------------
        "sidecar:status-get" => {
            let active = state.active_tab();
            let ready = state.ready_tabs.lock().contains(&active);
            let cwd = state
                .tab_cwds
                .lock()
                .get(&active)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| state.cwd.to_string_lossy().into_owned());
            Ok(json!({
                "status": if ready { "ready" } else { "starting" },
                "cwd": cwd,
            }))
        }
        "sidecar:restart" => {
            for client in state.clients.lock().values() {
                client.reject_all("sidecar restarting");
            }
            state.ready_tabs.lock().clear();
            Ok(json!({ "ok": true }))
        }

        // ---- Sessions -------------------------------------------------------------------
        "sessions:list" => {
            let scope = payload.get("scope").and_then(Value::as_str).unwrap_or("local");
            serde_json::to_value(omp_session::list(scope, &state.cwd)).map_err(|e| e.to_string())
        }
        "sessions:search" => {
            let query = payload.get("query").and_then(Value::as_str).unwrap_or("");
            let scope = payload.get("scope").and_then(Value::as_str).unwrap_or("local");
            Ok(json!(omp_session::search(query, scope, &state.cwd)))
        }
        "sessions:delete" => {
            let Some(path) = payload.get("sessionPath").and_then(Value::as_str) else {
                return Err("sessions:delete requires sessionPath".to_owned());
            };
            if !path.ends_with(".jsonl") {
                return Err("Invalid session path".to_owned());
            }
            let path = PathBuf::from(path);
            if let Some(owner) = state.pool.session_owner(&path) {
                let client = state.rpc_for(&owner)?;
                let mut command = json!({ "type": "drop_session", "sessionPath": path });
                let response = client
                    .command(&mut command, None)
                    .await
                    .map_err(|e| e.to_string())?;
                if response.get("success").and_then(Value::as_bool) != Some(true) {
                    return Err(response
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("drop_session failed")
                        .to_owned());
                }
                if response
                    .get("data")
                    .and_then(|data| data.get("cancelled"))
                    .and_then(Value::as_bool)
                    == Some(true)
                {
                    return Err("Session deletion was cancelled".to_owned());
                }
                return Ok(Value::Null);
            }
            delete_session_file(&path)?;
            Ok(Value::Null)
        }
        "sessions:rename" => {
            let Some(path) = payload.get("sessionPath").and_then(Value::as_str) else {
                return Err("sessions:rename requires sessionPath".to_owned());
            };
            if !path.ends_with(".jsonl") {
                return Err("Invalid session path".to_owned());
            }
            let name = payload
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            if name.is_empty() {
                return Err("Session name cannot be empty".to_owned());
            }
            let tab = state
                .pool
                .session_owner(Path::new(path))
                .unwrap_or_else(|| state.active_tab());
            let client = state.rpc_for(&tab)?;
            let mut command = json!({
                "type": "set_session_name",
                "name": name,
                "sessionPath": path,
            });
            let response = client
                .command(&mut command, None)
                .await
                .map_err(|e| e.to_string())?;
            if response.get("success").and_then(Value::as_bool) != Some(true) {
                return Err(response
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("set_session_name failed")
                    .to_owned());
            }
            Ok(Value::Null)
        }

        // ---- Prefs ----------------------------------------------------------------------
        "prefs:get" => Ok(read_json_or(&state.data_dir.join("prefs.json"), json!({}))),
        "prefs:set" => {
            let merged = merge_json(read_json_or(&state.data_dir.join("prefs.json"), json!({})), payload);
            write_json(&state.data_dir.join("prefs.json"), &merged)?;
            Ok(merged)
        }

        // ---- System ---------------------------------------------------------------------
        "system:info" => Ok(json!({
            "appVersion": env!("CARGO_PKG_VERSION"),
            "platform": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "osRelease": std::env::consts::FAMILY,
            // The renderer only displays these; the host no longer ships Chromium or Node.
            "electron": "",
            "chrome": "",
            "node": "",
        })),
        "system:open-external" => {
            if let Some(url) = payload.get("url").and_then(Value::as_str) {
                tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())?;
            }
            Ok(Value::Null)
        }
        "app:quit" => {
            app.exit(0);
            Ok(Value::Null)
        }

        // ---- Updater (Phase 9 owns the real implementation) ------------------------------
        "updater:version" => Ok(json!(env!("CARGO_PKG_VERSION"))),
        "updater:getStatus" => Ok(json!({ "status": "idle" })),

        // ---- Files ----------------------------------------------------------------------
        "fs:read" => read_file(state.cwd.clone(), payload),
        "fs:list" => list_tree(state.cwd.clone(), payload),

        // ---- Logs -----------------------------------------------------------------------
        "runtime:log-path" => Ok(json!(state.data_dir.join("runtime.log").to_string_lossy())),
        "log:snapshot" => Ok(json!({ "lines": [] })),

        // ---- Paths, themes, preferences, workspace ---------------------------------------
        "system:open-path" => open_path_in_os(&state, &payload, false),
        "system:reveal-path" => open_path_in_os(&state, &payload, true),
        "system:resolve-paths" => {
            let tab = payload.get("tabId").and_then(Value::as_str).map(TabId::new);
            let cwd = tab_cwd(&state, tab.as_ref());
            let paths = payload
                .get("paths")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let resolved: Vec<Value> = paths
                .iter()
                .filter_map(|entry| {
                    let raw = entry.as_str()?;
                    let candidate = if Path::new(raw).is_absolute() {
                        PathBuf::from(raw)
                    } else {
                        cwd.join(raw)
                    };
                    let meta = fs::metadata(&candidate).ok()?;
                    let kind = if meta.is_dir() { "directory" } else { "file" };
                    let mut item = json!({
                        "absolutePath": candidate.to_string_lossy(),
                        "kind": kind,
                    });
                    if let Ok(relative) = candidate.strip_prefix(&cwd) {
                        item["relativePath"] = json!(relative.to_string_lossy());
                    }
                    Some(item)
                })
                .collect();
            Ok(json!(resolved))
        }
        "themes:list" => {
            let dir = omp_core::home_dir().join(".omp").join("gui").join("themes");
            let mut files = Vec::new();
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                        continue;
                    }
                    let file_name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    let id = path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    let (data, error) = match fs::read_to_string(&path) {
                        Ok(text) => match serde_json::from_str::<Value>(&text) {
                            Ok(value) => (Some(value), None),
                            Err(err) => (None, Some(err.to_string())),
                        },
                        Err(err) => (None, Some(err.to_string())),
                    };
                    files.push(json!({
                        "id": id,
                        "fileName": file_name,
                        "data": data,
                        "error": error,
                    }));
                }
            }
            Ok(json!({ "dir": dir.to_string_lossy(), "files": files }))
        }
        "themes:open-folder" => {
            let dir = omp_core::home_dir().join(".omp").join("gui").join("themes");
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            open_in_os(&dir, false)?;
            Ok(json!({ "ok": true, "resolvedPath": dir.to_string_lossy() }))
        }
        "prefs:update-launch-profile" => {
            let cwd = payload
                .get("cwd")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let patch = payload.get("patch").cloned().unwrap_or_else(|| json!({}));
            let path = state.data_dir.join("prefs.json");
            let mut prefs = read_json_or(&path, json!({}));
            let mut profiles = prefs
                .get("launchProfiles")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let profile = profiles.get(&cwd).cloned().unwrap_or_else(|| json!({}));
            let profile = merge_json(profile, patch);
            profiles[&cwd] = profile.clone();
            prefs["launchProfiles"] = profiles;
            write_json(&path, &prefs)?;
            Ok(profile)
        }
        "sidecar:default-workspace" => {
            let dir = omp_core::home_dir().join(".omp").join("work");
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            Ok(json!(dir.to_string_lossy()))
        }
        "stats:url" => Ok(json!({ "url": Value::Null, "unavailable": true })),
        "stats:fetch" => Err("stats server not running".to_owned()),
        "runtime:error-report" => {
            append_log(
                &state.data_dir.join("runtime.log"),
                &format!("renderer error: {payload}\n"),
            );
            Ok(Value::Null)
        }
        "runtime:log-tail" => {
            let max = payload
                .get("maxLines")
                .and_then(Value::as_u64)
                .unwrap_or(200) as usize;
            let text = fs::read_to_string(state.data_dir.join("runtime.log")).unwrap_or_default();
            let mut lines: Vec<String> = text
                .lines()
                .rev()
                .take(max)
                .map(str::to_owned)
                .collect();
            lines.reverse();
            Ok(json!(lines))
        }
        "tray:state-push" => {
            let path = state.data_dir.join("prefs.json");
            let mut prefs = read_json_or(&path, json!({}));
            prefs["trayState"] = payload.clone();
            write_json(&path, &prefs)?;
            Ok(Value::Null)
        }
        "progress:set" => Ok(Value::Null),
        "session:consume-pending" => Ok(Value::Null),
        "session:open-new-window" => Ok(json!(false)),

        other => Err(format!("unknown channel: {other}")),
    }
}

/// Re-emit the cached ready state when a tab becomes active: the original push may
/// have arrived while the renderer was still focused on another tab.
fn replay_ready(app: &AppHandle, state: &AppState, tab: &TabId) {
    if state.ready_tabs.lock().contains(tab) {
        let _ = app.emit(
            crate::events::SIDECAR_STATUS,
            json!({ "tabId": tab, "payload": { "status": "ready" } }),
        );
        crate::emit_tab_status(app, state, tab);
    }
}

/// An in-place `switch_session` changes the tab's session file; keep the resume
/// binding and the pool's ownership map in sync with it.
fn sync_switch_binding(state: &AppState, tab: &TabId, command: &Value, response: &Value) {
    if command.get("type").and_then(Value::as_str) != Some("switch_session") {
        return;
    }
    if response.get("success").and_then(Value::as_bool) != Some(true) {
        return;
    }
    if response
        .get("data")
        .and_then(|data| data.get("cancelled"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        return;
    }
    let Some(path) = command
        .get("sessionPath")
        .and_then(Value::as_str)
        .map(PathBuf::from)
    else {
        return;
    };
    if let Some(previous) = state.tab_sessions.lock().insert(tab.clone(), path.clone()) {
        state.pool.release_session(&previous);
    }
    let _ = state.pool.claim_session(tab, &path);
}

/// Workspace root of a tab, falling back to the window's cwd.
fn tab_cwd(state: &AppState, tab: Option<&TabId>) -> PathBuf {
    tab.and_then(|id| state.tab_cwds.lock().get(id).cloned())
        .unwrap_or_else(|| state.cwd.clone())
}

/// Upper bound on live sidecars; every one of them is a full agent process.
pub(crate) fn max_sidecars() -> usize {
    std::env::var("ROMP_MAX_SIDECARS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(6)
}

/// Release the least-recently-active sidecar that is safe to stop: not the active tab,
/// not streaming a turn, and bound to a session file it can resume later.
pub(crate) fn evict_idle_sidecar(app: &AppHandle, state: &AppState) -> bool {
    let active = state.active_tab();
    let mut victim: Option<(u64, TabId)> = None;
    {
        let activity = state.tab_activity.lock();
        let sessions = state.tab_sessions.lock();
        let clients = state.clients.lock();
        for tab in state.pool.tabs() {
            if tab == active || !clients.contains_key(&tab) || !sessions.contains_key(&tab) {
                continue;
            }
            // A tab with no frames yet is still booting and safe to release.
            let (last_frame_ms, streaming) = activity
                .get(&tab)
                .map(|info| (info.last_frame_ms, info.streaming))
                .unwrap_or((0, false));
            if streaming {
                continue;
            }
            let candidate = (last_frame_ms, tab.clone());
            if victim
                .as_ref()
                .map(|(oldest, _)| candidate.0 < *oldest)
                .unwrap_or(true)
            {
                victim = Some(candidate);
            }
        }
    }
    let Some((_, tab)) = victim else {
        return false;
    };
    state.pool.release_process(&tab);
    state.clients.lock().remove(&tab);
    if let Some(batcher) = state.batchers.lock().remove(&tab) {
        batcher.dispose();
    }
    state.ready_tabs.lock().remove(&tab);
    let _ = app.emit(
        crate::events::SIDECAR_STATUS,
        json!({ "tabId": tab, "payload": { "status": "starting" } }),
    );
    tracing::info!(tab = %tab, "released idle sidecar");
    true
}

/// Open a path with the OS file manager; `reveal` selects it inside its folder.
fn open_in_os(path: &Path, reveal: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let mut command = std::process::Command::new("explorer.exe");
        if reveal {
            command.arg(format!("/select,{}", path.display()));
        } else {
            command.arg(path);
        }
        command.spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn open_path_in_os(state: &AppState, payload: &Value, reveal: bool) -> Result<Value, String> {
    let Some(raw) = payload.get("path").and_then(Value::as_str) else {
        return Err("path is required".to_owned());
    };
    let candidate = if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        state.cwd.join(raw)
    };
    if !candidate.exists() {
        return Ok(json!({ "ok": false, "error": "path does not exist" }));
    }
    open_in_os(&candidate, reveal)?;
    Ok(json!({ "ok": true, "resolvedPath": candidate.to_string_lossy() }))
}

fn append_log(path: &Path, line: &str) {
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        use std::io::Write;
        let _ = file.write_all(line.as_bytes());
    }
}

/// Send a side-channel reply back to the sidecar that raised the request.
fn route_side_channel(
    state: &AppState,
    payload: &Value,
    key: &str,
    unregister: bool,
) -> Result<(), String> {
    let Some(frame) = payload.get(key) else {
        return Err(format!("missing {key}"));
    };
    let request_id = frame.get("id").and_then(Value::as_str).unwrap_or("");
    let owner = if unregister {
        state.pool.take_request(request_id)
    } else {
        state.pool.request_owner(request_id)
    };
    let tab = owner.unwrap_or_else(|| state.active_tab());
    state.pool.dispatch(&tab, frame).map_err(|e| e.to_string())
}

fn read_json_or(path: &Path, fallback: Value) -> Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(fallback)
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Shallow merge: `prefs:set` carries only the keys the renderer changed.
fn merge_json(mut base: Value, patch: Value) -> Value {
    if let (Some(base_map), Some(patch_map)) = (base.as_object_mut(), patch.as_object()) {
        for (key, value) in patch_map {
            base_map.insert(key.clone(), value.clone());
        }
    }
    base
}

fn resolve_in(cwd: &Path, path: &str) -> PathBuf {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        candidate
    } else {
        cwd.join(candidate)
    }
}

fn read_file(cwd: PathBuf, payload: Value) -> Result<Value, String> {
    let path = payload
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "fs:read requires a path".to_owned())?;
    let max_bytes = payload
        .get("maxBytes")
        .and_then(Value::as_u64)
        .unwrap_or(FS_READ_CAP)
        .min(FS_READ_CAP);
    let resolved = resolve_in(&cwd, path);
    let bytes = std::fs::read(&resolved).map_err(|e| e.to_string())?;
    let size = bytes.len() as u64;
    let truncated = size > max_bytes;
    let slice = &bytes[..size.min(max_bytes) as usize];
    let binary = slice.contains(&0);
    Ok(json!({
        "ok": true,
        "content": if binary { String::new() } else { String::from_utf8_lossy(slice).into_owned() },
        "truncated": truncated,
        "binary": binary,
        "size": size,
    }))
}

fn list_tree(cwd: PathBuf, payload: Value) -> Result<Value, String> {
    let root = payload
        .get("path")
        .and_then(Value::as_str)
        .map(|p| resolve_in(&cwd, p))
        .unwrap_or(cwd);
    let mut entries = Vec::new();
    let mut truncated = false;
    let mut stack = vec![(root.clone(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if depth > FS_LIST_MAX_DEPTH {
            continue;
        }
        let read = match std::fs::read_dir(&dir) {
            Ok(read) => read,
            Err(_) => continue,
        };
        for entry in read.flatten() {
            if entries.len() >= FS_LIST_MAX_ENTRIES {
                truncated = true;
                break;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if matches!(name.as_str(), ".git" | "node_modules" | "target") {
                continue;
            }
            let path = entry.path();
            let is_dir = path.is_dir();
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            entries.push(json!({ "path": rel, "name": name, "dir": is_dir }));
            if is_dir {
                stack.push((path, depth + 1));
            }
        }
        if truncated {
            break;
        }
    }
    Ok(json!({ "ok": true, "entries": entries, "truncated": truncated }))
}

fn delete_session_file(path: &Path) -> Result<(), String> {
    let root = omp_session::sessions_root()
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(&root) {
        return Err("Invalid session path".to_owned());
    }
    fs::remove_file(&path).map_err(|e| e.to_string())?;
    let sidecar_dir = path.with_extension("");
    if sidecar_dir.is_dir() {
        fs::remove_dir_all(&sidecar_dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_json_overrides_only_present_keys() {
        let base = json!({ "theme": "dark", "lang": "en" });
        let merged = merge_json(base, json!({ "lang": "zh" }));
        assert_eq!(merged, json!({ "theme": "dark", "lang": "zh" }));
    }

    #[test]
    fn resolve_in_keeps_absolute_paths() {
        let cwd = PathBuf::from("D:/w");
        assert_eq!(resolve_in(&cwd, "a/b.txt"), PathBuf::from("D:/w").join("a/b.txt"));
        let absolute = resolve_in(&cwd, "C:/x/y.txt");
        assert!(absolute.is_absolute());
    }

    #[test]
    fn list_tree_skips_heavy_dirs() {
        let root = std::env::temp_dir().join("omp-list-test");
        std::fs::create_dir_all(root.join("keep")).unwrap();
        std::fs::create_dir_all(root.join("node_modules")).unwrap();
        std::fs::write(root.join("keep/a.txt"), "x").unwrap();
        std::fs::write(root.join("node_modules/b.txt"), "x").unwrap();
        let result = list_tree(root.clone(), json!({})).unwrap();
        let entries = result["entries"].as_array().unwrap();
        assert!(entries.iter().any(|e| e["path"] == "keep/a.txt"));
        assert!(!entries.iter().any(|e| e["name"] == "node_modules"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn delete_session_file_rejects_outside_root() {
        let path = std::env::temp_dir().join(format!("romp-outside-{}.jsonl", std::process::id()));
        std::fs::write(&path, "{}").unwrap();
        let error = delete_session_file(&path).unwrap_err();
        assert!(error.contains("Invalid session path"));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn delete_session_file_removes_file_and_sidecar_dir() {
        let root = omp_session::sessions_root();
        std::fs::create_dir_all(&root).unwrap();
        let dir = root.join(format!("--romp-test-{}--", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("case.jsonl");
        std::fs::write(&file, "{}").unwrap();
        let sidecar_dir = dir.join("case");
        std::fs::create_dir_all(&sidecar_dir).unwrap();
        std::fs::write(sidecar_dir.join("__advisor.jsonl"), "{}").unwrap();
        delete_session_file(&file).unwrap();
        assert!(!file.exists());
        assert!(!sidecar_dir.exists());
        std::fs::remove_dir_all(dir).ok();
    }
}
