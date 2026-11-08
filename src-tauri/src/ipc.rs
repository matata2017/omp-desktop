//! The single bridge command. Every renderer→main call arrives here as `(channel, payload)` with
//! the channel string unchanged from `IPC_COMMANDS`, so `src/shared/ipc-types.ts` stays the only
//! contract and `src/bridge/omp-api.ts` needs no per-method mapping.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
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
            sync_session_binding(&state, &tab, &cmd, &response);
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
            sync_session_binding(&state, &tab, &cmd, &response);
            Ok(response)
        }

        // ---- Side-channel replies (extension UI / host tool / host URI) ------------------
        "extension-ui:respond" => {
            route_side_channel(&state, &payload, "response", true)?;
            // The prompt is answered: the tab is a candidate for eviction again. The renderer
            // sends `{ response: { type, id, … } }`, so the id lives one level down.
            if let Some(request_id) = response_request_id(&payload) {
                crate::release_blocking_request(&state, request_id);
            }
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
            ensure_tab_started(&app, &state, &active, "tab:get-all")?;
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
            // Phantom-tab investigation: record who asked for this tab and for what.
            tracing::info!(
                tab = %tab,
                cwd = %cwd.display(),
                kind = ?kind,
                session = ?session_path.as_ref().map(|p| p.display().to_string()),
                tabs = state.pool.tab_count(),
                "tab:spawn"
            );
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
            // Persist last: the saved activeIndex must point at the tab the renderer just opened.
            crate::persist_layout(&state);
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
            state.browser.close_tab(tab.as_str());
            state.blocking_requests.lock().remove(&tab);
            state.ready_tabs.lock().remove(&tab);
            crate::persist_layout(&state);
            Ok(json!(existed))
        }
        "tab:set-order" => {
            crate::persist_layout(&state);
            Ok(json!(true))
        }
        "tab:set-active" => {
            let Some(tab) = payload.get("tabId").and_then(Value::as_str).map(TabId::new) else {
                return Ok(json!(false));
            };
            *state.active_tab.lock() = tab.clone();
            ensure_tab_started(&app, &state, &tab, "tab:set-active")?;
            replay_ready(&app, &state, &tab);
            crate::persist_layout(&state);
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
                ensure_tab_started(&app, &state, tab, "tab:set-view")?;
                replay_ready(&app, &state, tab);
            }
            crate::persist_layout(&state);
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
            // Same teardown as a window close: kill the agents and the dashboard first, then
            // leave; a bare exit(0) orphans every child process.
            state.bench.abort();
            for tab in state.pool.tabs() {
                state.pool.close(&tab);
            }
            state.clients.lock().clear();
            // `ipc_invoke` already runs on the async runtime: block_on here would panic with
            // "cannot block the current thread from within a runtime".
            state.stats.terminate_now();
            app.exit(0);
            Ok(Value::Null)
        }

        // ---- Updater (Phase 9 owns the real implementation) ------------------------------
        "updater:version" => Ok(json!(env!("CARGO_PKG_VERSION"))),
        "updater:getStatus" => Ok(json!({ "status": "idle" })),

        // ---- Files ----------------------------------------------------------------------
        "fs:read" => read_file(state.cwd.clone(), payload),
        "fs:list" => list_tree(state.cwd.clone(), payload),
        "fs:read-image" => {
            let tab = payload.get("tabId").and_then(Value::as_str).map(TabId::new);
            read_image(tab_cwd(&state, tab.as_ref()), &payload)
        }
        "fs:read-plan" => {
            let tab = payload.get("tabId").and_then(Value::as_str).map(TabId::new);
            read_plan(tab_cwd(&state, tab.as_ref()), &payload)
        }

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
                    let candidate = resolve_target(raw, &cwd)?;
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
        // ---- Window recording ------------------------------------------------------------
        "recorder:save" => {
            let path = payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "recorder:save requires a path".to_owned())?;
            let data = payload
                .get("data")
                .ok_or_else(|| "recorder:save requires data".to_owned())?;
            match decode_bytes(data) {
                Ok(bytes) => match std::fs::write(path, bytes) {
                    Ok(()) => Ok(json!({ "ok": true })),
                    Err(err) => Ok(json!({ "ok": false, "error": err.to_string() })),
                },
                Err(err) => Ok(json!({ "ok": false, "error": err })),
            }
        }

        // ---- models.yml provider editor --------------------------------------------------
        // ---- Native dialogs, clipboard, notifications ------------------------------------
        "system:save-dialog" => {
            let default_path = payload
                .get("defaultPath")
                .and_then(Value::as_str)
                .filter(|p| !p.is_empty())
                .map(str::to_owned);
            let filters = dialog_filters(&payload);
            let app_for_dialog = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let mut dialog = app_for_dialog.dialog().file();
                if let Some(name) = default_path {
                    dialog = dialog.set_file_name(name);
                }
                for (name, extensions) in filters {
                    let refs: Vec<&str> = extensions.iter().map(String::as_str).collect();
                    dialog = dialog.add_filter(name, &refs);
                }
                dialog
                    .blocking_save_file()
                    .map(|path| path.to_string())
            })
            .await
            .map_err(|e| e.to_string())
            .map(|path| json!(path))
        }
        "system:open-dialog" => {
            let filters = dialog_filters(&payload);
            let directory = payload
                .get("options")
                .and_then(|options| options.get("directory"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let app_for_dialog = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let mut dialog = app_for_dialog.dialog().file();
                for (name, extensions) in filters {
                    let refs: Vec<&str> = extensions.iter().map(String::as_str).collect();
                    dialog = dialog.add_filter(name, &refs);
                }
                if directory {
                    dialog
                        .blocking_pick_folder()
                        .map(|path| vec![path.to_string()])
                        .unwrap_or_default()
                } else {
                    dialog
                        .blocking_pick_files()
                        .map(|paths| paths.iter().map(|p| p.to_string()).collect())
                        .unwrap_or_default()
                }
            })
            .await
            .map_err(|e| e.to_string())
            .map(|paths: Vec<String>| {
                if paths.is_empty() {
                    Value::Null
                } else {
                    json!(paths)
                }
            })
        }
        "system:clipboard-read" => {
            use tauri_plugin_clipboard_manager::ClipboardExt;
            app.clipboard()
                .read_text()
                .map(Value::String)
                .map_err(|e| e.to_string())
        }
        "system:notify" => {
            use tauri_plugin_notification::NotificationExt;
            let title = payload
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("omp")
                .to_owned();
            let body = payload
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let _ = app
                .notification()
                .builder()
                .title(title)
                .body(body)
                .show();
            Ok(Value::Null)
        }

        // ---- External editor round trip -----------------------------------------------
        "editor:open-external" => {
            let content = payload
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            // The editor is a blocking GUI process; keep it off the async runtime.
            tauri::async_runtime::spawn_blocking(move || crate::editor::open_external(&content))
                .await
                .map_err(|e| e.to_string())
        }

        "models:providers-list" => crate::models::list(&crate::models::models_path()),
        "models:provider-upsert" => crate::models::upsert(&crate::models::models_path(), &payload),
        "models:provider-delete" => {
            let id = payload
                .as_str()
                .ok_or_else(|| "models:provider-delete requires an id".to_owned())?;
            let removed = crate::models::delete(&crate::models::models_path(), id)?;
            Ok(json!({ "removed": removed }))
        }
        "models:config-open" => {
            let path = crate::models::ensure_file(&crate::models::models_path())?;
            let opened = tauri_plugin_opener::open_path(path.as_path(), None::<&str>).is_ok();
            Ok(json!({ "path": path.to_string_lossy(), "opened": opened }))
        }

        // ---- tray ------------------------------------------------------------------------
        "tray:state-push" => {
            // The renderer owns the tray model; rebuild the native menu from it so the tray and
            // the app never disagree about status, language or actions.
            let state_json = payload.get("state").cloned().unwrap_or(payload.clone());
            tracing::info!(
                language = ?state_json.get("language"),
                status = ?state_json.get("status"),
                "tray state pushed"
            );
            *state.tray_state.lock() = Some(state_json.clone());
            crate::rebuild_tray(&app, Some(&state_json)).map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }

        // ---- browser panel ---------------------------------------------------------------
        "browser:get-state" => {
            let tab = payload.get("tabId").and_then(Value::as_str).unwrap_or_default();
            state.browser.refresh_active_history(tab);
            Ok(state.browser.state(tab, false))
        }
        "browser:open" => {
            let tab = payload
                .get("tabId")
                .and_then(Value::as_str)
                .ok_or_else(|| "browser:open requires a tabId".to_owned())?
                .to_owned();
            let url = payload
                .get("url")
                .and_then(Value::as_str)
                .ok_or_else(|| "browser:open requires a url".to_owned())?
                .to_owned();
            let window = app
                .get_window("main")
                .ok_or_else(|| "main window is gone".to_owned())?;
            state.browser.open(&app, &window, &tab, &url, "user").map_err(|e| e.to_string())
        }
        "browser:action" => {
            let tab = payload
                .get("tabId")
                .and_then(Value::as_str)
                .ok_or_else(|| "browser:action requires a tabId".to_owned())?
                .to_owned();
            let page_id = payload
                .get("pageId")
                .and_then(Value::as_u64)
                .ok_or_else(|| "browser:action requires a pageId".to_owned())?;
            state
                .browser
                .action(Some(&app), &tab, page_id, &payload, false)
                .map_err(|e| e.to_string())
        }
        "browser:set-viewport" => {
            let tab = payload
                .get("tabId")
                .and_then(Value::as_str)
                .ok_or_else(|| "browser:set-viewport requires a tabId".to_owned())?
                .to_owned();
            let bounds = crate::browser::Bounds::from_value(payload.get("bounds"));
            state
                .browser
                .set_viewport(&tab, bounds)
                .map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "browser:capture" => {
            let tab = payload.get("tabId").and_then(Value::as_str).unwrap_or_default();
            Ok(match state.browser.capture(tab) {
                Some(data_url) => Value::String(data_url),
                None => Value::Null,
            })
        }
        // Keymap chords are handled by the app window; a child webview keeps its own keys.
        "browser:set-shortcuts" => Ok(Value::Null),

        // ---- scheduled tasks ------------------------------------------------------------
        "scheduler:get-snapshot" => {
            let store = scheduler_store(&state)?;
            let tasks = store.list(true).map_err(|e| e.to_string())?;
            let mut runs: Vec<Value> = Vec::new();
            for task in &tasks {
                let task_id = task.get("id").and_then(Value::as_str).unwrap_or_default();
                if let Ok(mut task_runs) = store.runs(task_id, 10) {
                    runs.append(&mut task_runs);
                }
            }
            runs.truncate(50);
            let review_count = runs
                .iter()
                .filter(|run| run.get("status").and_then(Value::as_str) == Some("review"))
                .count();
            Ok(json!({
                "tasks": tasks,
                "runs": runs,
                "paused": state.scheduler_runs.is_paused(),
                "health": { "state": "ok", "heartbeatAt": chrono::Utc::now().timestamp_millis(), "nextFireAt": Value::Null },
                "limits": { "maxConcurrent": 2, "maxPerDay": 100, "maxRuntimeMs": 900_000 },
                "reviewCount": review_count,
                "live": state.scheduler_runs.running(),
            }))
        }
        "scheduler:create" => {
            let draft = payload.get("draft").cloned().unwrap_or(payload.clone());
            let store = scheduler_store(&state)?;
            let id = store.create(&draft).map_err(|e| e.to_string())?;
            Ok(json!({ "id": id, "problems": [] }))
        }
        "scheduler:update" => {
            let id = payload.get("taskId").and_then(Value::as_str).ok_or_else(|| "update requires a taskId".to_owned())?;
            let patch = payload.get("config").cloned().unwrap_or_else(|| json!({}));
            let store = scheduler_store(&state)?;
            let revision = store.update(id, &patch).map_err(|e| e.to_string())?;
            Ok(json!({ "revision": revision, "problems": [] }))
        }
        "scheduler:set-enabled" => {
            let id = payload.get("taskId").and_then(Value::as_str).ok_or_else(|| "set-enabled requires a taskId".to_owned())?;
            let enabled = payload.get("enabled").and_then(Value::as_bool).unwrap_or(true);
            scheduler_store(&state)?.set_enabled(id, enabled).map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "scheduler:archive" => {
            let id = payload.get("taskId").and_then(Value::as_str).ok_or_else(|| "archive requires a taskId".to_owned())?;
            scheduler_store(&state)?.archive(id).map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "scheduler:delete" => {
            let id = payload.get("taskId").and_then(Value::as_str).ok_or_else(|| "delete requires a taskId".to_owned())?;
            scheduler_store(&state)?.delete(id).map_err(|e| e.to_string())?;
            Ok(Value::Null)
        }
        "scheduler:task-runs" => {
            let id = payload.get("taskId").and_then(Value::as_str).ok_or_else(|| "task-runs requires taskId".to_owned())?;
            let limit = payload.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
            Ok(json!(scheduler_store(&state)?.runs(id, limit).map_err(|e| e.to_string())?))
        }
        "scheduler:revisions" => {
            let id = payload.get("taskId").and_then(Value::as_str).ok_or_else(|| "revisions requires taskId".to_owned())?;
            Ok(json!(scheduler_store(&state)?.revisions(id).map_err(|e| e.to_string())?))
        }
        "scheduler:set-paused" => {
            let paused = payload.get("paused").and_then(Value::as_bool).unwrap_or(false);
            state.scheduler_runs.set_paused(paused);
            Ok(Value::Null)
        }
        "scheduler:cancel-run" => {
            let run_id = payload.get("runId").and_then(Value::as_str).unwrap_or_default();
            Ok(json!(state.scheduler_runs.cancel(run_id)))
        }
        "scheduler:insights" => {
            let days = payload
                .get("periodDays")
                .and_then(Value::as_u64)
                .unwrap_or(7)
                .clamp(1, 365);
            let only_task = payload.get("taskId").and_then(Value::as_str).map(str::to_owned);
            let store = scheduler_store(&state)?;
            let now = chrono::Utc::now().timestamp_millis();
            tauri::async_runtime::spawn_blocking(move || {
                let mut runs = store
                    .all_runs(now - (days as i64) * 86_400_000, 5000)
                    .map_err(|e| e.to_string())?;
                let mut tasks = store.list(true).map_err(|e| e.to_string())?;
                if let Some(task_id) = &only_task {
                    runs.retain(|run| {
                        run.get("taskId").and_then(Value::as_str) == Some(task_id.as_str())
                    });
                    tasks.retain(|task| {
                        task.get("id").and_then(Value::as_str) == Some(task_id.as_str())
                    });
                }
                Ok(omp_scheduler::compute_insights(days, now, &runs, &tasks))
            })
            .await
            .map_err(|e| e.to_string())?
        }
        "scheduler:get-notices" => {
            let store = scheduler_store(&state)?;
            let now = chrono::Utc::now().timestamp_millis();
            tauri::async_runtime::spawn_blocking(move || notifications(&store, now))
                .await
                .map_err(|e| e.to_string())?
        }
        "scheduler:mark-notices-read" => {
            let ids: Option<Vec<String>> = payload
                .get("ids")
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(Value::as_str).map(str::to_owned).collect());
            let store = scheduler_store(&state)?;
            let now = chrono::Utc::now().timestamp_millis();
            let app_for_notices = app.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let current = notifications(&store, now)?;
                let unread: Vec<String> = current
                    .as_array()
                    .map(|list| {
                        list.iter()
                            .filter(|notice| notice.get("read").and_then(Value::as_bool) == Some(false))
                            .filter_map(|notice| notice.get("id").and_then(Value::as_str).map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
                store
                    .mark_notices_read(ids.as_deref(), &unread)
                    .map_err(|e| e.to_string())?;
                let refreshed = notifications(&store, now)?;
                let _ = app_for_notices.emit(crate::events::SCHEDULER_NOTICES, refreshed);
                Ok(Value::Null)
            })
            .await
            .map_err(|e| e.to_string())?
        }
        "scheduler:review" => {
            let run_id = payload
                .get("runId")
                .and_then(Value::as_str)
                .ok_or_else(|| "review requires a runId".to_owned())?
                .to_owned();
            let decision = payload
                .get("decision")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if decision != "accepted" && decision != "rejected" {
                return Err(format!("decision must be accepted or rejected, not {decision}"));
            }
            let note = payload.get("note").and_then(Value::as_str).map(str::to_owned);
            let store = scheduler_store(&state)?;
            let now = chrono::Utc::now().timestamp_millis();
            let run_id_for_read = run_id.clone();
            tauri::async_runtime::spawn_blocking(move || {
                store
                    .review_run(&run_id, &decision, note.as_deref(), now)
                    .map_err(|e| e.to_string())?;
                let runs = store.all_runs(0, 500).map_err(|e| e.to_string())?;
                Ok(runs
                    .into_iter()
                    .find(|run| run.get("id").and_then(Value::as_str) == Some(run_id_for_read.as_str()))
                    .unwrap_or(Value::Null))
            })
            .await
            .map_err(|e| e.to_string())?
        }
        "scheduler:get-run" => {
            let run_id = payload.get("runId").and_then(Value::as_str).unwrap_or_default();
            let store = scheduler_store(&state)?;
            let tasks = store.list(true).map_err(|e| e.to_string())?;
            for task in tasks {
                let task_id = task.get("id").and_then(Value::as_str).unwrap_or_default();
                if let Ok(runs) = store.runs(task_id, 200)
                    && let Some(run) = runs
                        .into_iter()
                        .find(|r| r.get("id").and_then(Value::as_str) == Some(run_id))
                {
                    return Ok(run);
                }
            }
            Ok(Value::Null)
        }
        "scheduler:run-now" => {
            let task_id = payload
                .get("taskId")
                .and_then(Value::as_str)
                .ok_or_else(|| "run-now requires taskId".to_owned())?;
            let store = scheduler_store(&state)?;
            let tasks = store.list(true).map_err(|e| e.to_string())?;
            let task = tasks
                .into_iter()
                .find(|t| t.get("id").and_then(Value::as_str) == Some(task_id))
                .ok_or_else(|| format!("unknown task: {task_id}"))?;
            let policy = task.get("tools").and_then(Value::as_str).unwrap_or("read_only");
            let model = task.get("model").and_then(Value::as_str);
            let prompt = task.get("prompt").and_then(Value::as_str).unwrap_or_default().to_owned();
            let run_cwd = task
                .get("cwd")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .unwrap_or_else(|| state.cwd.clone());
            let binary = state
                .sidecar_binary
                .clone()
                .ok_or_else(|| "Built-in omp not found".to_owned())?;
            let run_id = state.scheduler_runs.next_run_id();
            let session_dir = crate::scheduler::run_session_dir(&state.data_dir, task_id, &run_id);
            std::fs::create_dir_all(&session_dir).map_err(|e| e.to_string())?;
            let user_flags: Vec<String> = Vec::new();
            let args = crate::scheduler::run_argv(policy, model, &session_dir, &user_flags)
                .map_err(|e| e.to_string())?;
            let mut command = tokio::process::Command::new(&binary);
            command
                .args(&args)
                .current_dir(&run_cwd)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .env("PI_NOTIFICATIONS", "off");
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.as_std_mut().creation_flags(0x0800_0000);
            }
            let mut child = command.spawn().map_err(|e| e.to_string())?;
            if let Some(stdin) = child.stdin.as_mut() {
                use tokio::io::AsyncWriteExt;
                let frame = json!({ "type": "prompt", "message": prompt }).to_string() + "\n";
                let _ = stdin.write_all(frame.as_bytes()).await;
                let _ = stdin.flush().await;
            }
            let started_at = chrono::Utc::now().timestamp_millis();
            store
                .record_run(&json!({
                    "id": run_id,
                    "taskId": task_id,
                    "status": "running",
                    "startedAt": started_at,
                    "sessionPath": session_dir.to_string_lossy(),
                }))
                .map_err(|e| e.to_string())?;
            let cancel_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let stdout = child.stdout.take();
            state.scheduler_runs.track(crate::scheduler::LiveRun {
                run_id: run_id.clone(),
                task_id: task_id.to_owned(),
                child,
                started_at,
                cancel_requested: cancel_flag.clone(),
            });

            // Supervise the run to a terminal state. Without this the record would sit at
            // `running` forever: no endedAt, no report, and nothing to cancel.
            let supervisor = app.clone();
            let store_for_run = store.clone();
            let task_id_for_run = task_id.to_owned();
            let run_id_for_run = run_id.clone();
            let timeout_ms = task
                .get("timeoutMs")
                .and_then(Value::as_u64)
                .unwrap_or(900_000);
            tauri::async_runtime::spawn(async move {
                let (status, report, done) =
                    crate::scheduler::supervise_run(stdout, timeout_ms, cancel_flag).await;
                let ended_at = chrono::Utc::now().timestamp_millis();
                let state = supervisor.state::<AppState>();
                state.scheduler_runs.forget(&run_id_for_run);
                let report_json = report.as_ref().map(|text| json!({ "text": text }));
                let _ = store_for_run.finish_run(
                    &run_id_for_run,
                    status,
                    report_json.as_ref(),
                    None,
                    None,
                    None,
                    ended_at,
                );
                let _ = supervisor.emit(
                    crate::events::SCHEDULER_PROGRESS,
                    json!({ "runId": run_id_for_run, "progress": { "status": status, "endedAt": ended_at } }),
                );
                tracing::info!(run = %run_id_for_run, task = %task_id_for_run, status, "scheduled run finished");
                let _ = done;
            });
            Ok(json!({
                "id": run_id,
                "taskId": task_id,
                "status": "running",
                "startedAt": started_at,
            }))
        }

        // ---- conversation search ---------------------------------------------------------
        "conversations:search" => {
            let query = payload
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let cwd = payload
                .get("cwd")
                .and_then(Value::as_str)
                .map(PathBuf::from);
            let cached = state.conversations.lock().clone();
            let index = match cached {
                Some(index) => index,
                None => {
                    let db = state.data_dir.join("conversation-index.db");
                    match omp_session::conversation::ConversationIndex::open(&db) {
                        Ok(index) => {
                            let index = Arc::new(index);
                            *state.conversations.lock() = Some(index.clone());
                            index
                        }
                        Err(err) => {
                            tracing::warn!("conversation index unavailable: {err}");
                            return Ok(omp_session::conversation::ConversationIndex::unavailable());
                        }
                    }
                }
            };
            // Indexing and searching both touch the disk; keep them off the async runtime.
            tauri::async_runtime::spawn_blocking(move || {
                let sessions = omp_core::paths::sessions_dir();
                index.sync(&sessions);
                index.search(&query, cwd.as_deref(), 60)
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
        }

        // ---- benchmarks ------------------------------------------------------------------
        "bench:run" => {
            let binary = state
                .sidecar_binary
                .clone()
                .ok_or_else(|| "Built-in omp not found".to_owned())?;
            Ok(state.bench.run(&binary, &payload).await)
        }
        "bench:abort" => Ok(json!(state.bench.abort())),

        // ---- stats dashboard -------------------------------------------------------------
        // Starting the dashboard only when something asks for it keeps 97 MB out of the idle
        // footprint. /trace reads this URL before any stats component mounts, so the lazy start
        // has to live here too, not only in stats:fetch.
        "stats:url" => match state.stats.ensure_running().await {
            Ok(port) => Ok(json!({ "url": format!("http://127.0.0.1:{port}/") })),
            Err(err) => Ok(json!({ "url": Value::Null, "unavailable": true, "error": err.to_string() })),
        },
        "stats:fetch" => {
            let path = payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "stats:fetch requires a path".to_owned())?
                .to_owned();
            if !omp_stats::is_valid_path(&path) {
                return Err(format!("invalid stats path: {path}"));
            }
            let params: Vec<(String, String)> = payload
                .get("params")
                .and_then(Value::as_object)
                .map(|map| {
                    map.iter()
                        .filter_map(|(key, value)| Some((key.clone(), value.as_str()?.to_owned())))
                        .collect()
                })
                .unwrap_or_default();
            // The dashboard expects stats:fetch to RESOLVE failures as { error, unavailable }
            // (stats-sync.ts bridgeFailure, use-stats.ts unavailableError). Rejecting here
            // bypasses its 90s booting-retry budget and shows a dead-end error toast on the
            // first lazy start. Mirrors upstream main/ipc.ts STATS_FETCH.
            let port = match state.stats.ensure_running().await {
                Ok(port) => port,
                Err(err) => return Ok(json!({ "error": err.to_string(), "unavailable": true })),
            };
            let client = {
                let mut slot = state.stats_client.lock();
                if slot.as_ref().is_none_or(|c| c.port() != port) {
                    match omp_stats::StatsClient::new(port) {
                        Ok(client) => *slot = Some(Arc::new(client)),
                        Err(err) => {
                            return Ok(json!({ "error": err.to_string(), "unavailable": true }));
                        }
                    }
                }
                slot.clone().expect("just populated")
            };
            match client.get(&path, &params).await {
                Ok(value) => Ok(value),
                // A request that reached the server is a real failure, not a booting server.
                Err(err) => Ok(json!({ "error": err.to_string(), "unavailable": false })),
            }
        }
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

/// Keep the resume binding and the pool's ownership map in sync with the session
/// the sidecar is actually on: a confirmed in-place switch or a `get_state` snapshot
/// (which carries `sessionFile`, including for a fresh tab's first session).
fn sync_session_binding(state: &AppState, tab: &TabId, command: &Value, response: &Value) {
    let path = match command.get("type").and_then(Value::as_str) {
        Some("switch_session") => confirmed_switch_path(command, response),
        Some("get_state")
            if response.get("success").and_then(Value::as_bool) == Some(true) =>
        {
            response
                .get("data")
                .and_then(|data| data.get("sessionFile"))
                .and_then(Value::as_str)
                .map(PathBuf::from)
        }
        _ => None,
    };
    let Some(path) = path else {
        return;
    };
    if state.tab_sessions.lock().get(tab) == Some(&path) {
        return;
    }
    if let Some(previous) = state.tab_sessions.lock().insert(tab.clone(), path.clone()) {
        // Only drop an ownership entry this tab actually holds; a stale binding
        // must never release another tab's live session.
        if state.pool.session_owner(&previous).as_ref() == Some(tab) {
            state.pool.release_session(&previous);
        }
    }
    let _ = state.pool.claim_session(tab, &path);
}

/// The session path a sidecar explicitly confirmed it switched to. A vetoed or
/// unconfirmed response must never update ownership.
fn confirmed_switch_path(command: &Value, response: &Value) -> Option<PathBuf> {
    if command.get("type").and_then(Value::as_str) != Some("switch_session") {
        return None;
    }
    if response.get("success").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let cancelled = response
        .get("data")
        .and_then(|data| data.get("cancelled"))
        .and_then(Value::as_bool);
    if cancelled != Some(false) {
        return None;
    }
    command
        .get("sessionPath")
        .and_then(Value::as_str)
        .map(PathBuf::from)
}

/// Workspace root of a tab, falling back to the window's cwd.
fn tab_cwd(state: &AppState, tab: Option<&TabId>) -> PathBuf {
    tab.and_then(|id| state.tab_cwds.lock().get(id).cloned())
        .unwrap_or_else(|| state.cwd.clone())
}

/// Resolve a path named by the renderer the way the upstream main process did.
///
/// `file:///E:/x` and `/E:/x` reach here with a leading slash (`classifyImageSrc`
/// keeps it), which Windows reads as a rooted path on the current drive
/// (`C:\E:\x`) instead of drive E. A drive path written that way loses the
/// slash, `~` expands, absolute paths pass through, and relative ones must stay
/// inside the tab's workspace.
fn resolve_target(raw: &str, cwd: &Path) -> Option<PathBuf> {
    let raw = strip_url_style_drive(raw);
    let expanded = if raw == "~" {
        omp_core::home_dir()
    } else if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        omp_core::home_dir().join(rest)
    } else {
        PathBuf::from(raw)
    };
    if expanded.is_absolute() {
        return Some(normalize_path(&expanded));
    }
    let root = normalize_path(&std::path::absolute(cwd).ok()?);
    let candidate = normalize_path(&root.join(expanded));
    if candidate != root && !candidate.starts_with(&root) {
        return None;
    }
    Some(candidate)
}

/// `/E:/x` and `\E:\x` become `E:/x` / `E:\x`; other inputs are unchanged.
#[cfg(windows)]
fn strip_url_style_drive(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    if bytes.len() >= 4
        && (bytes[0] == b'/' || bytes[0] == b'\\')
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
        && (bytes[3] == b'/' || bytes[3] == b'\\')
    {
        &raw[1..]
    } else {
        raw
    }
}

#[cfg(not(windows))]
fn strip_url_style_drive(raw: &str) -> &str {
    raw
}

/// Lexical `.`/`..` removal, so a workspace-escape check cannot be fooled.
fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

const FS_IMAGE_MAX_BYTES: u64 = 25_000_000;

/// `fs:read-image`: sniff, then return the file as a data URL. Only real image
/// bytes pass, so a markdown `![alt](path)` cannot turn into a generic file read.
fn read_image(cwd: PathBuf, payload: &Value) -> Result<Value, String> {
    let fail = |error: &str| {
        json!({ "ok": false, "dataUrl": Value::Null, "mime": Value::Null, "size": 0, "error": error })
    };
    let Some(raw) = payload.get("path").and_then(Value::as_str).filter(|path| !path.is_empty()) else {
        return Ok(fail("Invalid path"));
    };
    let Some(path) = resolve_target(raw, &cwd) else {
        return Ok(fail("Invalid path"));
    };
    let Ok(meta) = fs::metadata(&path) else {
        return Ok(fail("Invalid path"));
    };
    if !meta.is_file() {
        return Ok(fail("Not a file"));
    }
    if meta.len() > FS_IMAGE_MAX_BYTES {
        return Ok(fail("Image too large"));
    }
    let Ok(bytes) = fs::read(&path) else {
        return Ok(fail("Invalid path"));
    };
    let Some(mime) = sniff_image_mime(&bytes) else {
        return Ok(fail("Not a supported image"));
    };
    use base64::prelude::{Engine as _, BASE64_STANDARD};
    let data_url = format!("data:{mime};base64,{}", BASE64_STANDARD.encode(&bytes));
    Ok(json!({ "ok": true, "dataUrl": data_url, "mime": mime, "size": bytes.len() }))
}

/// Image type by magic bytes; SVG only when the text actually starts as one.
fn sniff_image_mime(header: &[u8]) -> Option<&'static str> {
    if header.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some("image/png");
    }
    if header.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }
    if header.starts_with(b"GIF89a") || header.starts_with(b"GIF87a") {
        return Some("image/gif");
    }
    if header.starts_with(b"RIFF") && header.get(8..12) == Some(&b"WEBP"[..]) {
        return Some("image/webp");
    }
    if header.get(4..8) == Some(&b"ftyp"[..]) {
        let brand = header.get(8..12);
        if brand == Some(&b"avif"[..]) || brand == Some(&b"avis"[..]) {
            return Some("image/avif");
        }
    }
    if header.starts_with(b"BM") {
        return Some("image/bmp");
    }
    if header.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        return Some("image/x-icon");
    }
    let text = String::from_utf8_lossy(&header[..header.len().min(512)]);
    let text = text.trim_start().to_ascii_lowercase();
    if text.starts_with("<svg") || (text.starts_with("<?xml") && text.contains("<svg")) {
        return Some("image/svg+xml");
    }
    None
}

/// `fs:read-plan`: read the configured plan file, else the newest `*plan.md`
/// under the session-local root. Confined to the workspace and the sessions dir.
fn read_plan(cwd: PathBuf, payload: &Value) -> Result<Value, String> {
    let fail =
        |error: &str| json!({ "ok": false, "path": Value::Null, "content": Value::Null, "error": error });
    let Some(raw) = payload.get("fsPath").and_then(Value::as_str).filter(|path| !path.is_empty()) else {
        return Ok(fail("Invalid path"));
    };
    let sessions = omp_session::sessions_root();
    let Some(target) =
        resolve_within(&cwd, raw).or_else(|| resolve_within(&sessions, raw))
    else {
        return Ok(fail("Path escapes allowed roots"));
    };
    let local_root =
        match payload.get("localRoot").and_then(Value::as_str).filter(|root| !root.is_empty()) {
            Some(root) => match resolve_within(&cwd, root).or_else(|| resolve_within(&sessions, root)) {
                Some(path) => Some(path),
                None => return Ok(fail("Path escapes allowed roots")),
            },
            None => None,
        };
    let picked = if target.is_file() {
        Some(target)
    } else {
        local_root.as_deref().and_then(newest_plan_file)
    };
    let Some(picked) = picked else {
        return Ok(json!({ "ok": true, "path": Value::Null, "content": Value::Null }));
    };
    match fs::read_to_string(&picked) {
        Ok(content) => Ok(json!({ "ok": true, "path": picked.to_string_lossy(), "content": content })),
        Err(err) => Ok(fail(&err.to_string())),
    }
}

/// Newest top-level `*plan.md` in `root`; the agent names plans itself, so the
/// configured path alone often misses the one being shown.
fn newest_plan_file(root: &Path) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in fs::read_dir(root).ok()?.flatten() {
        if !entry.file_name().to_string_lossy().to_ascii_lowercase().ends_with("plan.md") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        if best.as_ref().is_none_or(|(time, _)| modified > *time) {
            best = Some((modified, entry.path()));
        }
    }
    best.map(|(_, path)| path)
}

/// Resolve `raw` inside `root`; absolute paths must already be inside it.
fn resolve_within(root: &Path, raw: &str) -> Option<PathBuf> {
    let root = normalize_path(&std::path::absolute(root).ok()?);
    let candidate = normalize_path(&if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        root.join(raw)
    });
    (candidate == root || candidate.starts_with(&root)).then_some(candidate)
}

/// What the eviction sweep needs to know about one tab. Split out from the sweep so the rules can
/// be exercised without an app handle: releasing a sidecar discards whatever it was holding.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EvictionFacts {
    pub is_active: bool,
    pub has_client: bool,
    /// A tab whose session is not on disk yet cannot resume after being released.
    pub has_session: bool,
    pub streaming: bool,
    /// A tool call is waiting for the user's answer.
    pub pending_approval: bool,
    /// The Browser panel has pages in this tab; those pages live in the sidecar's browser host.
    pub browser_pages: usize,
}

/// A sidecar may only be released when nothing in it would be lost.
pub(crate) fn is_evictable(facts: &EvictionFacts) -> bool {
    !facts.is_active
        && facts.has_client
        && facts.has_session
        && !facts.streaming
        && !facts.pending_approval
        && facts.browser_pages == 0
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
            let facts = EvictionFacts {
                is_active: false,
                has_client: true,
                has_session: true,
                streaming,
                pending_approval: state
                    .blocking_requests
                    .lock()
                    .get(&tab)
                    .is_some_and(|ids| !ids.is_empty()),
                browser_pages: state.browser.page_count(tab.as_str()),
            };
            if !is_evictable(&facts) {
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

/// Bytes crossing the bridge arrive either as a JSON number array (what `ArrayBuffer` becomes
/// after `invoke` serialisation) or as base64. Accept both; anything else is a hard error.
fn decode_bytes(value: &Value) -> Result<Vec<u8>, String> {
    if let Some(text) = value.as_str() {
        use base64::Engine as _;
        return base64::engine::general_purpose::STANDARD
            .decode(text)
            .map_err(|e| format!("invalid base64 payload: {e}"));
    }
    let Some(items) = value.as_array() else {
        return Err("recorder:save data must be a byte array or a base64 string".to_owned());
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let byte = item
            .as_u64()
            .filter(|b| *b <= 255)
            .ok_or_else(|| "recorder:save data contains a non-byte value".to_owned())?;
        out.push(byte as u8);
    }
    Ok(out)
}

/// The request id an `extension-ui:respond` payload answers.
fn response_request_id(payload: &Value) -> Option<&str> {
    payload
        .get("response")
        .and_then(|response| response.get("id"))
        .and_then(Value::as_str)
        .or_else(|| payload.get("requestId").and_then(Value::as_str))
}

/// `[{ name, extensions: string[] }]` as the dialog plugin wants it.
fn dialog_filters(payload: &Value) -> Vec<(String, Vec<String>)> {
    payload
        .get("filters")
        .and_then(Value::as_array)
        .map(|filters| {
            filters
                .iter()
                .filter_map(|filter| {
                    let name = filter.get("name").and_then(Value::as_str)?.to_owned();
                    let extensions = filter
                        .get("extensions")
                        .and_then(Value::as_array)
                        .map(|list| {
                            list.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default();
                    Some((name, extensions))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The bell's current notices: runs waiting on a person plus runs that broke.
fn notifications(
    store: &omp_scheduler::TaskStore,
    now: i64,
) -> Result<Value, String> {
    let runs = store
        .all_runs(now - 7 * 86_400_000, 200)
        .map_err(|e| e.to_string())?;
    let tasks = store.list(true).map_err(|e| e.to_string())?;
    let read = store.read_notice_ids().map_err(|e| e.to_string())?;
    Ok(json!(omp_scheduler::derive_notices(&runs, &tasks, &read, 20)))
}

/// Open the scheduler database on first use; an unreadable one disables the feature rather than
/// blocking the whole app.
fn scheduler_store(state: &AppState) -> Result<Arc<omp_scheduler::TaskStore>, String> {
    if let Some(store) = state.scheduler.lock().clone() {
        return Ok(store);
    }
    let path = state.data_dir.join("scheduler.db");
    let store = Arc::new(omp_scheduler::TaskStore::open(&path).map_err(|e| e.to_string())?);
    *state.scheduler.lock() = Some(store.clone());
    Ok(store)
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
    let resolved = resolve_target(path, &cwd).ok_or_else(|| "path escapes the workspace".to_owned())?;
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
    let root = match payload.get("path").and_then(Value::as_str) {
        Some(path) => {
            resolve_target(path, &cwd).ok_or_else(|| "path escapes the workspace".to_owned())?
        }
        None => cwd,
    };
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

    fn free_tab() -> EvictionFacts {
        EvictionFacts {
            is_active: false,
            has_client: true,
            has_session: true,
            streaming: false,
            pending_approval: false,
            browser_pages: 0,
        }
    }

    #[test]
    fn an_idle_tab_with_a_session_is_evictable() {
        assert!(is_evictable(&free_tab()));
    }

    #[test]
    fn every_lease_blocks_eviction() {
        let cases: [(&str, EvictionFacts); 4] = [
            ("active", EvictionFacts { is_active: true, ..free_tab() }),
            ("streaming", EvictionFacts { streaming: true, ..free_tab() }),
            ("awaiting approval", EvictionFacts { pending_approval: true, ..free_tab() }),
            ("browser pages", EvictionFacts { browser_pages: 1, ..free_tab() }),
        ];
        for (name, facts) in cases {
            assert!(!is_evictable(&facts), "{name} must block eviction");
        }
        // A tab without a client or a session is not a candidate at all.
        assert!(!is_evictable(&EvictionFacts { has_client: false, ..free_tab() }));
        assert!(!is_evictable(&EvictionFacts { has_session: false, ..free_tab() }));
    }


    #[test]
    fn respond_payload_id_is_read_from_the_nested_response() {
        assert_eq!(
            response_request_id(&json!({ "response": { "type": "confirm", "id": "req-7", "value": true } })),
            Some("req-7")
        );
        // Legacy flat shape still resolves, and a payload without an id does not panic.
        assert_eq!(response_request_id(&json!({ "requestId": "req-8" })), Some("req-8"));
        assert_eq!(response_request_id(&json!({ "response": { "type": "confirm" } })), None);
    }

    #[test]
    fn merge_json_overrides_only_present_keys() {
        let base = json!({ "theme": "dark", "lang": "en" });
        let merged = merge_json(base, json!({ "lang": "zh" }));
        assert_eq!(merged, json!({ "theme": "dark", "lang": "zh" }));
    }

    #[test]
    fn decode_bytes_accepts_arrays_and_base64() {
        assert_eq!(
            decode_bytes(&json!([0, 1, 255])).unwrap(),
            vec![0u8, 1, 255]
        );
        assert_eq!(decode_bytes(&json!("AQID")).unwrap(), vec![1u8, 2, 3]);
    }

    #[test]
    fn decode_bytes_rejects_other_shapes() {
        assert!(decode_bytes(&json!([256])).is_err());
        assert!(decode_bytes(&json!([-1])).is_err());
        assert!(decode_bytes(&json!({ "length": 2 })).is_err());
        assert!(decode_bytes(&json!("not base64!!")).is_err());
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

    #[test]
    fn switch_binding_requires_explicit_confirmation() {
        let command = json!({ "type": "switch_session", "sessionPath": "/s.jsonl" });
        assert_eq!(
            confirmed_switch_path(
                &command,
                &json!({ "success": true, "data": { "cancelled": false } })
            ),
            Some(PathBuf::from("/s.jsonl"))
        );
        assert!(
            confirmed_switch_path(
                &command,
                &json!({ "success": true, "data": { "cancelled": true } })
            )
            .is_none()
        );
        assert!(
            confirmed_switch_path(&command, &json!({ "success": true, "data": {} })).is_none()
        );
        assert!(
            confirmed_switch_path(
                &json!({ "type": "get_state" }),
                &json!({ "success": true, "data": { "cancelled": false } })
            )
            .is_none()
        );
    }

    #[test]
    fn resolve_target_keeps_relative_paths_inside_the_workspace() {
        let workspace = std::env::temp_dir().join(format!("romp-resolve-{}", std::process::id()));
        std::fs::create_dir_all(&workspace).unwrap();
        assert_eq!(
            resolve_target("src/app.ts", &workspace),
            Some(workspace.join("src").join("app.ts"))
        );
        assert!(resolve_target("../escape.ts", &workspace).is_none());
        std::fs::remove_dir_all(workspace).ok();
    }

    #[test]
    fn resolve_target_expands_home() {
        let cwd = std::env::temp_dir();
        assert_eq!(
            resolve_target("~/notes.md", &cwd),
            Some(omp_core::home_dir().join("notes.md"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn resolve_target_strips_the_slash_before_a_drive() {
        let cwd = Path::new("C:\\work");
        assert_eq!(
            resolve_target("/E:/ymt/scan.js", cwd),
            Some(PathBuf::from("E:\\ymt\\scan.js"))
        );
        assert_eq!(
            resolve_target("\\E:\\ymt\\scan.js", cwd),
            Some(PathBuf::from("E:\\ymt\\scan.js"))
        );
        assert_eq!(
            resolve_target("E:\\ymt\\scan.js", cwd),
            Some(PathBuf::from("E:\\ymt\\scan.js"))
        );
        assert_eq!(
            resolve_target("\\\\nas\\share\\doc.md", cwd),
            Some(PathBuf::from("\\\\nas\\share\\doc.md"))
        );
    }

    #[test]
    fn sniff_image_mime_recognises_the_shipped_formats() {
        assert_eq!(sniff_image_mime(&[0x89, b'P', b'N', b'G', 0, 0, 0, 0]), Some("image/png"));
        assert_eq!(sniff_image_mime(&[0xff, 0xd8, 0xff, 0]), Some("image/jpeg"));
        assert_eq!(sniff_image_mime(b"GIF89a...."), Some("image/gif"));
        assert_eq!(sniff_image_mime(b"RIFF....WEBP"), Some("image/webp"));
        assert_eq!(sniff_image_mime(b"....ftypavif"), Some("image/avif"));
        assert_eq!(sniff_image_mime(b"BM......"), Some("image/bmp"));
        assert_eq!(sniff_image_mime(&[0x00, 0x00, 0x01, 0x00]), Some("image/x-icon"));
        assert_eq!(sniff_image_mime(b"  <svg xmlns=\"\">"), Some("image/svg+xml"));
        assert_eq!(sniff_image_mime(b"plain text"), None);
    }

    #[test]
    fn resolve_within_refuses_escapes() {
        let root = std::env::temp_dir();
        assert!(resolve_within(&root, "plan.md").is_some());
        assert!(resolve_within(&root, "../escape.md").is_none());
        assert!(resolve_within(&root, "..").is_none());
    }

    #[test]
    fn newest_plan_file_ignores_other_names_and_directories() {
        let root = std::env::temp_dir().join(format!("romp-plan-{}", std::process::id()));
        std::fs::create_dir_all(root.join("dir-plan.md")).unwrap();
        std::fs::write(root.join("notes.md"), "x").unwrap();
        std::fs::write(root.join("case-plan.md"), "plan").unwrap();
        assert_eq!(newest_plan_file(&root), Some(root.join("case-plan.md")));
        std::fs::remove_dir_all(root).ok();
    }
}
