//! The single bridge command. Every renderer→main call arrives here as `(channel, payload)` with
//! the channel string unchanged from `IPC_COMMANDS`, so `src/shared/ipc-types.ts` stays the only
//! contract and `src/bridge/omp-api.ts` needs no per-method mapping.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tauri::{AppHandle, State};

use crate::{AppState, ensure_tab_started};
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
    tracing::info!(channel = %channel, "ipc_invoke");
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
            let client = state.rpc()?;
            let mut cmd = command;
            client
                .command(&mut cmd, timeout)
                .await
                .map_err(|e| e.to_string())
        }
        "rpc:command-for-tab" => {
            // Phase 2 has one sidecar per window; Phase 3 routes by `tabId`.
            let command = payload
                .get("command")
                .cloned()
                .ok_or_else(|| "rpc:command-for-tab requires a command".to_owned())?;
            let timeout = payload
                .get("timeoutMs")
                .and_then(Value::as_u64)
                .map(std::time::Duration::from_millis);
            let client = state.rpc()?;
            let mut cmd = command;
            client
                .command(&mut cmd, timeout)
                .await
                .map_err(|e| e.to_string())
        }

        // ---- Tabs -----------------------------------------------------------------------
        "tab:get-all" => {
            let active = state.active_tab();
            ensure_tab_started(&app, &state, &active)?;
            let ready = state.ready_tabs.lock();
            let tabs: Vec<Value> = state
                .pool
                .tabs()
                .into_iter()
                .map(|tab| {
                    let status = if ready.contains(&tab) { "ready" } else { "starting" };
                    json!({
                        "tabId": tab,
                        "cwd": state.cwd.to_string_lossy(),
                        "status": status,
                        "active": tab == active,
                        "visible": tab == active,
                        "kind": "agent",
                    })
                })
                .collect();
            Ok(json!(tabs))
        }
        "tab:set-active" => {
            let Some(tab) = payload.get("tabId").and_then(Value::as_str).map(TabId::new) else {
                return Ok(json!(false));
            };
            *state.active_tab.lock() = tab.clone();
            ensure_tab_started(&app, &state, &tab)?;
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
            Ok(json!({
                "status": if ready { "ready" } else { "starting" },
                "cwd": state.cwd.to_string_lossy(),
            }))
        }
        "sidecar:restart" => {
            let active = state.active_tab();
            state.ready_tabs.lock().remove(&active);
            let client = state.rpc()?;
            client.reject_all("sidecar restarting");
            Ok(json!({ "ok": true }))
        }

        // ---- Sessions -------------------------------------------------------------------
        "sessions:list" => {
            let scope = payload.get("scope").and_then(Value::as_str).unwrap_or("local");
            serde_json::to_value(omp_session::list(scope, &state.cwd)).map_err(|e| e.to_string())
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

        other => Err(format!("unknown channel: {other}")),
    }
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
}
