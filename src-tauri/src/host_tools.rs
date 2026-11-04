//! The two host tools agent tabs get for scheduled tasks, and their argument handling.
//!
//! Blueprint: `src/main/scheduler/{schedule-tool,manage-tasks-tool,host}.ts`. The GUI owns these
//! tools: the sidecar asks, the host performs the change and replies with a text result, so a task
//! exists the moment the agent asks for it rather than after a window round trip.

use std::path::Path;

use serde_json::{Value, json};

pub const SCHEDULE_TASK: &str = "schedule_task";
pub const MANAGE_TASKS: &str = "manage_scheduled_tasks";

/// Tools whose exec approval prompt the host answers itself: the user already asked for the
/// change in the conversation, and runs stay bounded by their tool policy.
pub fn is_scheduler_tool(name: &str) -> bool {
    name == SCHEDULE_TASK || name == MANAGE_TASKS
}

/// `set_host_tools` payload: both definitions, sent when an agent tab comes up.
pub fn definitions() -> Value {
    json!([
        {
            "name": SCHEDULE_TASK,
            "label": "Schedule task",
            "loadMode": "essential",
            "description": "Create a scheduled task the desktop app will run on a schedule. Use for recurring work the user wants run later or repeatedly (a daily report, a periodic check). Runs are unattended: pick the narrowest tool policy that still does the job.",
            "parameters": {
                "type": "object",
                "required": ["title", "prompt", "schedule"],
                "properties": {
                    "title": { "type": "string", "description": "Short human label for the task." },
                    "prompt": { "type": "string", "description": "The instruction each run executes, written as if to a fresh agent." },
                    "cwd": { "type": "string", "description": "Workspace directory; defaults to this session's cwd." },
                    "schedule": {
                        "type": "object",
                        "description": "Either {kind:'once', at:<ms>}, {kind:'interval', everyMs:<ms>, anchor:<ms>} or {kind:'cron', expr:'<5-field>', tz:'<IANA>'}.",
                        "properties": {
                            "kind": { "type": "string", "enum": ["once", "interval", "cron"] },
                            "at": { "type": "number" },
                            "everyMs": { "type": "number" },
                            "anchor": { "type": "number" },
                            "expr": { "type": "string" },
                            "tz": { "type": "string" }
                        },
                        "required": ["kind"]
                    },
                    "tools": {
                        "type": "string",
                        "enum": ["read_only", "workspace", "full"],
                        "description": "read_only (default) can only read; workspace may edit files; full is unrestricted."
                    },
                    "model": { "type": "string", "description": "Optional model selector for the run." },
                    "timeoutMs": { "type": "number", "description": "Per-run timeout, default 15 minutes." },
                    "startPaused": { "type": "boolean", "description": "Create the task disabled until the user enables it." }
                }
            }
        },
        {
            "name": MANAGE_TASKS,
            "label": "Manage scheduled tasks",
            "loadMode": "essential",
            "description": "List, pause, resume, update or remove scheduled tasks the user already created. Use when the user asks what is scheduled or wants a task changed.",
            "parameters": {
                "type": "object",
                "required": ["action"],
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["list", "pause", "resume", "update", "remove", "run_now"]
                    },
                    "taskId": { "type": "string" },
                    "patch": { "type": "object", "description": "Fields to change for `update`." }
                }
            }
        }
    ])
}

fn require_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| format!("{key} is required"))
}

/// Turn `schedule_task` arguments into the draft the store expects.
pub fn schedule_draft(args: &Value, fallback_cwd: &Path) -> Result<Value, String> {
    let title = require_str(args, "title")?;
    let prompt = require_str(args, "prompt")?;
    let schedule = args
        .get("schedule")
        .filter(|value| value.is_object())
        .ok_or_else(|| "schedule is required".to_owned())?;
    let kind = schedule
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "schedule.kind is required".to_owned())?;
    if !matches!(kind, "once" | "interval" | "cron") {
        return Err(format!("unknown schedule kind: {kind}"));
    }
    let tools = args
        .get("tools")
        .and_then(Value::as_str)
        .unwrap_or("read_only");
    if !matches!(tools, "read_only" | "workspace" | "full") {
        return Err(format!("unknown tool policy: {tools}"));
    }
    Ok(json!({
        "title": title,
        "prompt": prompt,
        "cwd": args.get("cwd").and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| fallback_cwd.to_string_lossy().into_owned()),
        "schedule": schedule,
        "model": args.get("model").cloned().unwrap_or(Value::Null),
        "tools": tools,
        "depth": { "kind": "check" },
        "timeoutMs": args.get("timeoutMs").and_then(Value::as_u64).unwrap_or(900_000),
        "verify": Value::Null,
        "notify": "on_failure",
        "origin": { "sessionPath": Value::Null },
        "startPaused": args.get("startPaused").and_then(Value::as_bool).unwrap_or(false),
    }))
}

/// Perform one call. Returns the text handed back to the agent and whether it is an error.
pub fn handle(
    store: &omp_scheduler::TaskStore,
    tool: &str,
    args: &Value,
    fallback_cwd: &Path,
) -> (String, bool) {
    let fail = |err: String| (err, true);
    match tool {
        SCHEDULE_TASK => match schedule_draft(args, fallback_cwd) {
            Err(err) => fail(err),
            Ok(draft) => match store.create(&draft) {
                Ok(id) => {
                    let title = draft.get("title").and_then(Value::as_str).unwrap_or("");
                    (
                        format!("Created scheduled task \"{title}\" ({id}). It is visible in the desktop app's Tasks panel."),
                        false,
                    )
                }
                Err(err) => fail(err.to_string()),
            },
        },
        MANAGE_TASKS => {
            let action = args
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let task_id = args.get("taskId").and_then(Value::as_str);
            match action {
                "list" => match store.list(false) {
                    Ok(tasks) if tasks.is_empty() => ("No scheduled tasks.".to_owned(), false),
                    Ok(tasks) => {
                        let lines: Vec<String> = tasks
                            .iter()
                            .map(|task| {
                                format!(
                                    "- {} ({}) tools={} enabled={}",
                                    task.get("title").and_then(Value::as_str).unwrap_or(""),
                                    task.get("id").and_then(Value::as_str).unwrap_or(""),
                                    task.get("tools").and_then(Value::as_str).unwrap_or("?"),
                                    task.get("enabled").and_then(Value::as_bool).unwrap_or(false),
                                )
                            })
                            .collect();
                        (lines.join("\n"), false)
                    }
                    Err(err) => fail(err.to_string()),
                },
                "pause" | "resume" => match task_id {
                    None => fail("taskId is required".to_owned()),
                    Some(id) => {
                        let enabled = action == "resume";
                        match store.set_enabled(id, enabled) {
                            Ok(()) => (format!("Task {id} {}", if enabled { "resumed" } else { "paused" }), false),
                            Err(err) => fail(err.to_string()),
                        }
                    }
                },
                "update" => match (task_id, args.get("patch")) {
                    (Some(id), Some(patch)) => match store.update(id, patch) {
                        Ok(rev) => (format!("Task {id} updated to revision {rev}."), false),
                        Err(err) => fail(err.to_string()),
                    },
                    _ => fail("taskId and patch are required".to_owned()),
                },
                "remove" => match task_id {
                    None => fail("taskId is required".to_owned()),
                    Some(id) => match store.delete(id) {
                        Ok(()) => (format!("Task {id} removed."), false),
                        Err(err) => fail(err.to_string()),
                    },
                },
                other => fail(format!("unknown action: {other}")),
            }
        }
        other => fail(format!("unknown host tool: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (omp_scheduler::TaskStore, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "romp-host-tools-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scheduler.db");
        (omp_scheduler::TaskStore::open(&path).unwrap(), dir)
    }

    fn args() -> Value {
        json!({
            "title": "daily report",
            "prompt": "summarise yesterday's commits",
            "schedule": { "kind": "cron", "expr": "0 9 * * 1-5", "tz": "Asia/Shanghai" },
            "tools": "read_only",
        })
    }

    #[test]
    fn definitions_expose_both_tools() {
        let defs = definitions();
        let names: Vec<&str> = defs
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|def| def.get("name").and_then(Value::as_str))
            .collect();
        assert_eq!(names, vec![SCHEDULE_TASK, MANAGE_TASKS]);
        for def in defs.as_array().unwrap() {
            // The sidecar rejects a definition whose schema is not under `parameters`.
            assert!(def.get("parameters").is_some_and(|schema| schema.is_object()));
            assert!(def.get("inputSchema").is_none());
            assert_eq!(def["loadMode"], "essential");
        }
        assert!(is_scheduler_tool(SCHEDULE_TASK));
        assert!(!is_scheduler_tool("bash"));
    }

    #[test]
    fn draft_defaults_to_the_narrow_policy_and_the_session_cwd() {
        let draft = schedule_draft(&args(), Path::new("D:/w")).unwrap();
        assert_eq!(draft["tools"], "read_only");
        assert_eq!(draft["cwd"], "D:/w");
        assert_eq!(draft["timeoutMs"], 900_000);
        assert_eq!(draft["startPaused"], false);
    }

    #[test]
    fn missing_required_arguments_are_rejected() {
        let err = schedule_draft(&json!({ "prompt": "x", "schedule": { "kind": "cron", "expr": "0 9 * * *" } }), Path::new("D:/w"))
            .unwrap_err();
        assert_eq!(err, "title is required");
        let err = schedule_draft(&json!({ "title": "t", "prompt": "x", "schedule": { "kind": "weekly" } }), Path::new("D:/w"))
            .unwrap_err();
        assert!(err.contains("unknown schedule kind"), "{err}");
        let err = schedule_draft(&json!({ "title": "t", "prompt": "x", "schedule": { "kind": "cron", "expr": "* * * * *" }, "tools": "god" }), Path::new("D:/w"))
            .unwrap_err();
        assert!(err.contains("unknown tool policy"), "{err}");
    }

    #[test]
    fn schedule_task_creates_and_list_shows_it() {
        let (store, dir) = store();
        let (text, is_error) = handle(&store, SCHEDULE_TASK, &args(), &dir);
        assert!(!is_error, "{text}");
        assert!(text.contains("daily report"), "{text}");

        let (listing, is_error) = handle(&store, MANAGE_TASKS, &json!({ "action": "list" }), &dir);
        assert!(!is_error, "{listing}");
        assert!(listing.contains("daily report"), "{listing}");
        assert!(listing.contains("read_only"), "{listing}");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn pause_resume_update_and_remove_round_trip() {
        let (store, dir) = store();
        let (_, _) = handle(&store, SCHEDULE_TASK, &args(), &dir);
        let id = store.list(false).unwrap()[0]["id"].as_str().unwrap().to_owned();

        let (text, is_error) = handle(&store, MANAGE_TASKS, &json!({ "action": "pause", "taskId": id }), &dir);
        assert!(!is_error);
        assert!(text.contains("paused"));
        assert_eq!(store.list(false).unwrap()[0]["enabled"], false);

        let (text, is_error) = handle(&store, MANAGE_TASKS, &json!({ "action": "resume", "taskId": id }), &dir);
        assert!(!is_error);
        assert!(text.contains("resumed"));

        let (text, is_error) = handle(
            &store,
            MANAGE_TASKS,
            &json!({ "action": "update", "taskId": id, "patch": { "title": "renamed" } }),
            &dir,
        );
        assert!(!is_error, "{text}");
        assert_eq!(store.list(false).unwrap()[0]["title"], "renamed");

        let (text, is_error) = handle(&store, MANAGE_TASKS, &json!({ "action": "remove", "taskId": id }), &dir);
        assert!(!is_error);
        assert!(text.contains("removed"));
        assert!(store.list(true).unwrap().is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn unknown_actions_and_tools_are_errors() {
        let (store, dir) = store();
        let (text, is_error) = handle(&store, MANAGE_TASKS, &json!({ "action": "explode" }), &dir);
        assert!(is_error);
        assert!(text.contains("unknown action"));
        let (text, is_error) = handle(&store, "delete_everything", &json!({}), &dir);
        assert!(is_error);
        assert!(text.contains("unknown host tool"));
        std::fs::remove_dir_all(dir).ok();
    }
}
