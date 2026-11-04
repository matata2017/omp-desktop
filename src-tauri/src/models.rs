//! `models.yml` reads and writes for the GUI provider editor.
//!
//! Blueprint: `src/main/models-config.ts`. The file is a YAML document with a `providers`
//! mapping; unknown keys are preserved on write (the agent owns fields the GUI does not edit),
//! but comments are not — the GUI becomes the owner of this file once it writes it.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use serde_yaml::{Mapping, Value as Yaml};

use crate::builtin_providers;

const DEFAULT_API: &str = "openai-completions";
const AUTH_MODES: [&str; 3] = ["apiKey", "none", "oauth"];
const MASK_PREVIEW_LEN: usize = 4;

pub fn models_path() -> PathBuf {
    omp_core::paths::agent_dir().join("models.yml")
}

fn read_document(path: &Path) -> Result<Yaml, String> {
    match std::fs::read_to_string(path) {
        Ok(text) if !text.trim().is_empty() => {
            serde_yaml::from_str::<Yaml>(&text).map_err(|e| format!("invalid models.yml: {e}"))
        }
        _ => Ok(Yaml::Mapping(Mapping::new())),
    }
}

fn providers_of(doc: &Yaml) -> Option<&Mapping> {
    doc.get("providers").and_then(Yaml::as_mapping)
}

/// Write through a unique temp file and rename over the target.
///
/// The agent watches this path and hot-reloads it, so a partially written file would be read as
/// the user's configuration; rename keeps the swap atomic. The temp name carries pid + counter
/// so two writers can never collide, and a failed write removes its own temp file.
fn write_document(path: &Path, doc: &Yaml) -> Result<(), String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_yaml::to_string(doc).map_err(|e| e.to_string())?;
    let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("yml.tmp-{}-{nonce}", std::process::id()));
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    if let Err(err) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(err.to_string());
    }
    Ok(())
}

fn as_str(value: Option<&Yaml>) -> Option<String> {
    match value {
        Some(Yaml::String(text)) if !text.is_empty() => Some(text.clone()),
        Some(Yaml::Number(number)) => Some(number.to_string()),
        _ => None,
    }
}

fn as_bool(value: Option<&Yaml>) -> Option<bool> {
    value.and_then(Yaml::as_bool)
}

fn as_string_map(value: Option<&Yaml>) -> Option<Map<String, Value>> {
    let map = value?.as_mapping()?;
    let mut out = Map::new();
    for (key, value) in map {
        if let (Some(key), Some(value)) = (key.as_str(), value.as_str()) {
            out.insert(key.to_owned(), Value::String(value.to_owned()));
        }
    }
    (!out.is_empty()).then_some(out)
}

fn yaml_to_json(value: &Yaml) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn mask_api_key(raw: &Yaml) -> (bool, Option<String>) {
    match raw.get("apiKey").and_then(Yaml::as_str) {
        Some(key) if !key.is_empty() => {
            let tail: String = key
                .chars()
                .rev()
                .take(MASK_PREVIEW_LEN)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            (true, Some(format!("•••{tail}")))
        }
        _ => (false, None),
    }
}

/// One provider as the GUI shows it: the API key is masked, everything else is verbatim.
pub fn to_view(id: &str, raw: &Yaml) -> Value {
    let (has_api_key, api_key_preview) = mask_api_key(raw);
    let auth = as_str(raw.get("auth")).filter(|mode| AUTH_MODES.contains(&mode.as_str()));
    let transport = as_str(raw.get("transport")).filter(|mode| mode == "pi-native");
    let extra_body = raw
        .get("compat")
        .and_then(|compat| compat.get("extraBody"))
        .filter(|value| value.as_mapping().is_some_and(|map| !map.is_empty()))
        .map(yaml_to_json);
    let models = raw
        .get("models")
        .map(yaml_to_json)
        .unwrap_or_else(|| json!([]));

    let mut view = Map::new();
    view.insert("id".into(), json!(id));
    view.insert(
        "api".into(),
        json!(as_str(raw.get("api")).unwrap_or_else(|| DEFAULT_API.to_owned())),
    );
    view.insert("baseUrl".into(), json!(as_str(raw.get("baseUrl")).unwrap_or_default()));
    view.insert("hasApiKey".into(), json!(has_api_key));
    if let Some(preview) = api_key_preview {
        view.insert("apiKeyPreview".into(), json!(preview));
    }
    if let Some(auth) = auth {
        view.insert("auth".into(), json!(auth));
    }
    if let Some(header) = as_bool(raw.get("authHeader")) {
        view.insert("authHeader".into(), json!(header));
    }
    if let Some(headers) = as_string_map(raw.get("headers")) {
        view.insert("headers".into(), Value::Object(headers));
    }
    if let Some(discovery) = raw.get("discovery") {
        view.insert("discovery".into(), yaml_to_json(discovery));
    }
    if let Some(strict) = as_bool(raw.get("disableStrictTools")) {
        view.insert("disableStrictTools".into(), json!(strict));
    }
    if let Some(transport) = transport {
        view.insert("transport".into(), json!(transport));
    }
    if let Some(extra) = extra_body {
        view.insert("extraBody".into(), extra);
    }
    view.insert("models".into(), models);
    view.insert("builtin".into(), json!(builtin_providers::is_builtin(id)));
    Value::Object(view)
}

/// Every configured provider, sorted by id so the editor's order is stable.
pub fn list(path: &Path) -> Result<Value, String> {
    let doc = read_document(path)?;
    let mut views = Vec::new();
    if let Some(providers) = providers_of(&doc) {
        let mut ids: Vec<&str> = providers
            .keys()
            .filter_map(Yaml::as_str)
            .collect();
        ids.sort_unstable();
        for id in ids {
            let raw = providers.get(id).expect("key came from this mapping");
            views.push(to_view(id, raw));
        }
    }
    Ok(Value::Array(views))
}

fn entry_mapping<'a>(providers: &'a mut Mapping, id: &str) -> &'a mut Mapping {
    let key = Yaml::String(id.to_owned());
    if !matches!(providers.get(&key), Some(Yaml::Mapping(_))) {
        providers.insert(key.clone(), Yaml::Mapping(Mapping::new()));
    }
    match providers.get_mut(&key) {
        Some(Yaml::Mapping(map)) => map,
        _ => unreachable!("inserted a mapping above"),
    }
}

fn set_str(entry: &mut Mapping, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        entry.insert(Yaml::String(key.to_owned()), Yaml::String(value.to_owned()));
    }
}

/// Insert or update one provider. Fields the input omits keep their stored value.
pub fn upsert(path: &Path, input: &Value) -> Result<Value, String> {
    let id = input
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "provider requires an id".to_owned())?
        .to_owned();
    if builtin_providers::is_builtin(&id) {
        return Err(format!(
            "\"{id}\" is a built-in provider id; choose a distinct custom id."
        ));
    }
    let models = input
        .get("models")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let clear_key = input
        .get("clearApiKey")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let api_key = input
        .get("apiKey")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty());
    let auth = input.get("auth").and_then(Value::as_str);

    let mut doc = read_document(path)?;
    if doc.get("providers").is_none() {
        doc.as_mapping_mut()
            .ok_or_else(|| "models.yml is not a mapping".to_owned())?
            .insert(Yaml::String("providers".into()), Yaml::Mapping(Mapping::new()));
    }
    let providers = doc
        .get_mut("providers")
        .and_then(Yaml::as_mapping_mut)
        .ok_or_else(|| "models.yml providers is not a mapping".to_owned())?;

    {
        let entry = entry_mapping(providers, &id);
        set_str(entry, "api", input.get("api").and_then(Value::as_str));
        set_str(entry, "baseUrl", input.get("baseUrl").and_then(Value::as_str));
        if let Some(key) = api_key {
            entry.insert(Yaml::String("apiKey".into()), Yaml::String(key.to_owned()));
        } else if clear_key {
            entry.remove(Yaml::String("apiKey".into()));
        }
        set_str(entry, "auth", auth);
        if let Some(header) = input.get("authHeader").and_then(Value::as_bool) {
            entry.insert(Yaml::String("authHeader".into()), Yaml::Bool(header));
        }
        if let Some(strict) = input.get("disableStrictTools").and_then(Value::as_bool) {
            entry.insert(
                Yaml::String("disableStrictTools".into()),
                Yaml::Bool(strict),
            );
        }
        set_str(entry, "transport", input.get("transport").and_then(Value::as_str));
        if let Some(headers) = input.get("headers").and_then(Value::as_object) {
            let mut map = Mapping::new();
            for (key, value) in headers {
                if let Some(text) = value.as_str() {
                    map.insert(Yaml::String(key.clone()), Yaml::String(text.to_owned()));
                }
            }
            entry.insert(Yaml::String("headers".into()), Yaml::Mapping(map));
        }
        if let Some(discovery) = input.get("discovery") {
            entry.insert(
                Yaml::String("discovery".into()),
                serde_yaml::to_value(discovery).map_err(|e| e.to_string())?,
            );
        }
        if let Some(extra) = input.get("extraBody") {
            let compat_key = Yaml::String("compat".into());
            if !matches!(entry.get(&compat_key), Some(Yaml::Mapping(_))) {
                entry.insert(compat_key.clone(), Yaml::Mapping(Mapping::new()));
            }
            let compat = entry
                .get_mut(&compat_key)
                .and_then(Yaml::as_mapping_mut)
                .expect("inserted a mapping above");
            compat.insert(
                Yaml::String("extraBody".into()),
                serde_yaml::to_value(extra).map_err(|e| e.to_string())?,
            );
        }
        let models_yaml = serde_yaml::to_value(&models).map_err(|e| e.to_string())?;
        entry.insert(Yaml::String("models".into()), models_yaml);
    }

    // The agent refuses a provider that lists models but cannot authenticate.
    let entry = providers
        .get(Yaml::String(id.clone()))
        .and_then(Yaml::as_mapping)
        .cloned()
        .unwrap_or_default();
    let has_key = entry
        .get("apiKey")
        .and_then(Yaml::as_str)
        .is_some_and(|key| !key.is_empty());
    let models_listed = entry
        .get("models")
        .and_then(Yaml::as_sequence)
        .is_some_and(|list| !list.is_empty());
    let auth_none = auth == Some("none");
    if models_listed && !has_key && !auth_none {
        return Err(format!(
            "\"{id}\" lists models but has no API key. Add a key, or set the auth mode to none for a server that needs no key."
        ));
    }

    write_document(path, &doc)?;
    Ok(to_view(&id, &Yaml::Mapping(entry)))
}

/// Remove one provider; `false` when it was not configured.
pub fn delete(path: &Path, id: &str) -> Result<bool, String> {
    if builtin_providers::is_builtin(id) {
        return Err(format!(
            "\"{id}\" is a built-in provider and cannot be removed from the GUI."
        ));
    }
    let mut doc = read_document(path)?;
    let Some(providers) = doc.get_mut("providers").and_then(Yaml::as_mapping_mut) else {
        return Ok(false);
    };
    let removed = providers.remove(Yaml::String(id.to_owned())).is_some();
    if removed {
        write_document(path, &doc)?;
    }
    Ok(removed)
}

/// Create the file when missing so the system editor opens a real document.
pub fn ensure_file(path: &Path) -> Result<PathBuf, String> {
    if !path.exists() {
        write_document(path, &Yaml::Mapping(Mapping::new()))?;
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("romp-models-{tag}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lists_sorted_views_with_masked_key() {
        let dir = temp_dir("list");
        let path = dir.join("models.yml");
        std::fs::write(
            &path,
            "providers:\n  anthropic:\n    baseUrl: https://z.example\n    apiKey: sk-abcdefgh\n  alpha:\n    baseUrl: https://a.example\n",
        )
        .unwrap();
        let views = list(&path).unwrap();
        let views = views.as_array().unwrap();
        assert_eq!(views[0]["id"], "alpha");
        assert_eq!(views[0]["api"], DEFAULT_API);
        assert_eq!(views[1]["id"], "anthropic");
        assert_eq!(views[1]["hasApiKey"], true);
        assert_eq!(views[1]["apiKeyPreview"], "•••efgh");
        assert_eq!(views[1]["builtin"], true);
        assert_eq!(views[0]["builtin"], false);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn upsert_preserves_unknown_fields_and_merges() {
        let dir = temp_dir("upsert");
        let path = dir.join("models.yml");
        std::fs::write(
            &path,
            "providers:\n  mine:\n    apiKey: sk-old\n    note: keep me\n",
        )
        .unwrap();
        upsert(
            &path,
            &json!({
                "id": "mine",
                "api": "openai-completions",
                "baseUrl": "https://m.example/v1",
                "apiKey": "sk-new",
                "models": [{ "id": "m1" }]
            }),
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("note: keep me"), "{text}");
        assert!(text.contains("sk-new"));
        assert!(!text.contains("sk-old"));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn upsert_can_clear_the_key() {
        let dir = temp_dir("clear");
        let path = dir.join("models.yml");
        std::fs::write(&path, "providers:\n  mine:\n    apiKey: sk-old\n").unwrap();
        upsert(&path, &json!({ "id": "mine", "clearApiKey": true, "models": [] })).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("sk-old"), "{text}");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn models_without_auth_are_rejected() {
        let dir = temp_dir("validate");
        let path = dir.join("models.yml");
        let err = upsert(
            &path,
            &json!({ "id": "mine", "models": [{ "id": "m1" }] }),
        )
        .unwrap_err();
        assert!(err.contains("lists models but has no API key"), "{err}");
        // auth: none makes the same provider acceptable.
        upsert(
            &path,
            &json!({ "id": "mine", "auth": "none", "models": [{ "id": "m1" }] }),
        )
        .unwrap();
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn builtin_ids_cannot_be_upserted_or_deleted() {
        let dir = temp_dir("builtin");
        let path = dir.join("models.yml");
        let err = upsert(&path, &json!({ "id": "anthropic", "models": [] })).unwrap_err();
        assert!(err.contains("is a built-in provider id"), "{err}");
        std::fs::write(&path, "providers:\n  anthropic:\n    apiKey: sk-1\n").unwrap();
        let err = delete(&path, "anthropic").unwrap_err();
        assert!(err.contains("cannot be removed from the GUI"), "{err}");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn writes_leave_no_temp_files() {
        let dir = temp_dir("temp");
        let path = dir.join("models.yml");
        upsert(&path, &json!({ "id": "mine", "auth": "none", "models": [] })).unwrap();
        let leftovers: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn delete_reports_whether_it_existed() {
        let dir = temp_dir("delete");
        let path = dir.join("models.yml");
        std::fs::write(&path, "providers:\n  mine:\n    apiKey: sk-1\n").unwrap();
        assert_eq!(delete(&path, "mine").unwrap(), true);
        assert_eq!(delete(&path, "mine").unwrap(), false);
        std::fs::remove_dir_all(dir).ok();
    }
}
