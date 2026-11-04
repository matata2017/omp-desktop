//! The two host tools agent tabs get for scheduled tasks, and their argument handling.
//!
//! Blueprint: `src/main/scheduler/{schedule-tool,manage-tasks-tool,host}.ts`. The GUI owns these
//! tools: the sidecar asks, the host performs the change and replies with a text result, so a task
//! exists the moment the agent asks for it rather than after a window round trip.


use serde_json::{Value, json};

pub const SCHEDULE_TASK: &str = "schedule_task";
pub const MANAGE_TASKS: &str = "manage_scheduled_tasks";

/// Tools whose exec approval prompt the host answers itself: the user already asked for the
/// change in the conversation, and runs stay bounded by their tool policy.
pub fn is_scheduler_tool(name: &str) -> bool {
    name == SCHEDULE_TASK || name == MANAGE_TASKS
}

/// `set_host_tools` payload: both definitions, generated from the upstream TypeScript by
/// `scripts/extract-host-tools.mjs`, so the schema cannot drift from the blueprint.
pub fn definitions() -> Value {
    static DEFS: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    DEFS.get_or_init(|| {
        serde_json::from_str(include_str!("host_tools_defs.json"))
            .expect("host_tools_defs.json is generated from the upstream definitions")
    })
    .clone()
}

fn require_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| format!("{key} is required"))
}

/// Everything the tool needs from the host: the session workspace and "now".
pub struct ToolContext {
    pub cwd: std::path::PathBuf,
    pub now: i64,
    /// IANA zone recorded on a cron schedule; the parser itself always runs in local time.
    pub time_zone: String,
}

fn arg_text(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// The schedule from `scheduleKind` plus that kind's own field.
fn parse_schedule(args: &Value, ctx: &ToolContext) -> Result<Value, String> {
    match args.get("scheduleKind").and_then(Value::as_str) {
        Some("cron") => {
            let expr =
                arg_text(args, "cron").ok_or_else(|| "cron is required when scheduleKind is cron".to_owned())?;
            Ok(json!({ "kind": "cron", "expr": expr, "tz": ctx.time_zone }))
        }
        Some("interval") => {
            let minutes = args.get("everyMinutes").and_then(Value::as_f64).unwrap_or(0.0);
            if !(minutes > 0.0) {
                return Err("everyMinutes must be a positive number when scheduleKind is interval".to_owned());
            }
            let every_ms = (minutes.round() as i64) * 60_000;
            Ok(json!({ "kind": "interval", "everyMs": every_ms, "anchor": ctx.now + every_ms }))
        }
        Some("once") => {
            let at = arg_text(args, "at")
                .ok_or_else(|| "at must be an ISO 8601 date-time when scheduleKind is once".to_owned())?;
            let parsed = chrono::DateTime::parse_from_rfc3339(&at)
                .map_err(|_| "at must be an ISO 8601 date-time when scheduleKind is once".to_owned())?;
            Ok(json!({ "kind": "once", "at": parsed.timestamp_millis() }))
        }
        _ => Err("scheduleKind must be cron, interval or once".to_owned()),
    }
}

/// Depth picked by the call; an empty depth keeps the default.
fn parse_depth(args: &Value) -> Result<Value, String> {
    match args.get("depth").and_then(Value::as_str).map(str::trim).unwrap_or_default() {
        "" => Ok(json!({ "kind": "check" })),
        "check" | "goal" => Ok(json!({ "kind": args["depth"].as_str().unwrap_or("check") })),
        "research" => {
            let asked = args.get("rounds").and_then(Value::as_f64).map(|r| r.round() as i64).unwrap_or(0);
            let rounds = if asked > 0 { asked } else { 3 };
            if !(2..=6).contains(&rounds) {
                return Err("rounds must be between 2 and 6".to_owned());
            }
            Ok(json!({ "kind": "research", "rounds": rounds }))
        }
        other => Err(format!("depth must be check, goal or research, not {other}")),
    }
}

/// Turn `schedule_task` arguments into the draft the store expects.
///
/// Runs are always read-only: only the user raises a task's tool policy, in the Tasks panel.
/// That is upstream's rule, not an oversight.
pub fn schedule_draft(args: &Value, ctx: &ToolContext) -> Result<Value, String> {
    let title = require_str(args, "title")?;
    let prompt = require_str(args, "prompt")?;
    let schedule = parse_schedule(args, ctx)?;
    let depth = parse_depth(args)?;
    let model = arg_text(args, "model");
    Ok(json!({
        "title": title,
        "prompt": prompt,
        "cwd": ctx.cwd.to_string_lossy(),
        "schedule": schedule,
        "model": model,
        "tools": "read_only",
        "depth": depth,
        "timeoutMs": 900_000,
        "verify": Value::Null,
        "notify": "on_failure",
        "origin": { "sessionPath": Value::Null },
        "startPaused": false,
    }))
}

/// Perform one call. Returns the text handed back to the agent and whether it is an error.
pub fn handle(
    store: &omp_scheduler::TaskStore,
    tool: &str,
    args: &Value,
    ctx: &ToolContext,
) -> (String, bool) {
    let fail = |err: String| (err, true);
    match tool {
        SCHEDULE_TASK => match schedule_draft(args, ctx) {
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

    fn ctx() -> ToolContext {
        ToolContext {
            cwd: std::path::PathBuf::from("D:/w"),
            now: 1_800_000_000_000,
            time_zone: "Asia/Shanghai".to_owned(),
        }
    }

    fn args() -> Value {
        json!({
            "title": "daily report",
            "prompt": "summarise yesterday's commits",
            "scheduleKind": "cron",
            "cron": "0 9 * * 1-5",
            "everyMinutes": 0,
            "at": "",
            "model": "",
            "depth": "check",
            "rounds": 0,
        })
    }

    #[test]
    fn definitions_come_from_the_generated_upstream_schema() {
        let defs = definitions();
        let items = defs.as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["name"], SCHEDULE_TASK);
        assert_eq!(items[1]["name"], MANAGE_TASKS);
        for def in items {
            // The agent validates this shape: schema lives under `parameters`, never `inputSchema`.
            assert_eq!(def["parameters"]["type"], "object");
            assert!(def.get("inputSchema").is_none());
            assert_eq!(def["loadMode"], "essential");
            assert!(!def["description"].as_str().unwrap_or_default().is_empty());
        }
        // The argument names the agent will actually send.
        for key in ["title", "prompt", "scheduleKind", "cron", "everyMinutes", "at", "depth", "rounds"] {
            assert!(items[0]["parameters"]["properties"].get(key).is_some(), "{key}");
        }
    }

    #[test]
    fn cron_interval_and_once_map_to_store_schedules() {
        let draft = schedule_draft(&args(), &ctx()).unwrap();
        assert_eq!(draft["schedule"]["kind"], "cron");
        assert_eq!(draft["schedule"]["expr"], "0 9 * * 1-5");
        assert_eq!(draft["schedule"]["tz"], "Asia/Shanghai");
        assert_eq!(draft["tools"], "read_only");

        let mut interval = args();
        interval["scheduleKind"] = json!("interval");
        interval["everyMinutes"] = json!(15);
        let draft = schedule_draft(&interval, &ctx()).unwrap();
        assert_eq!(draft["schedule"]["everyMs"], 900_000);
        assert_eq!(draft["schedule"]["anchor"], 1_800_000_000_000i64 + 900_000);

        let mut once = args();
        once["scheduleKind"] = json!("once");
        once["at"] = json!("2027-01-01T00:00:00Z");
        let draft = schedule_draft(&once, &ctx()).unwrap();
        assert_eq!(draft["schedule"]["at"], 1_798_761_600_000i64);
    }

    #[test]
    fn bad_arguments_are_rejected_with_the_upstream_wording() {
        let mut missing_cron = args();
        missing_cron["cron"] = json!("");
        assert_eq!(
            schedule_draft(&missing_cron, &ctx()).unwrap_err(),
            "cron is required when scheduleKind is cron"
        );

        let mut zero_interval = args();
        zero_interval["scheduleKind"] = json!("interval");
        zero_interval["everyMinutes"] = json!(0);
        assert_eq!(
            schedule_draft(&zero_interval, &ctx()).unwrap_err(),
            "everyMinutes must be a positive number when scheduleKind is interval"
        );

        let mut bad_once = args();
        bad_once["scheduleKind"] = json!("once");
        bad_once["at"] = json!("tomorrow");
        assert_eq!(
            schedule_draft(&bad_once, &ctx()).unwrap_err(),
            "at must be an ISO 8601 date-time when scheduleKind is once"
        );

        let mut bad_kind = args();
        bad_kind["scheduleKind"] = json!("weekly");
        assert_eq!(
            schedule_draft(&bad_kind, &ctx()).unwrap_err(),
            "scheduleKind must be cron, interval or once"
        );
    }

    #[test]
    fn depth_and_rounds_follow_the_tool_schema() {
        let draft = schedule_draft(&args(), &ctx()).unwrap();
        assert_eq!(draft["depth"]["kind"], "check");

        let mut research = args();
        research["depth"] = json!("research");
        research["rounds"] = json!(5);
        assert_eq!(schedule_draft(&research, &ctx()).unwrap()["depth"]["rounds"], 5);

        research["rounds"] = json!(9);
        assert_eq!(
            schedule_draft(&research, &ctx()).unwrap_err(),
            "rounds must be between 2 and 6"
        );

        let mut goal = args();
        goal["depth"] = json!("goal");
        assert_eq!(schedule_draft(&goal, &ctx()).unwrap()["depth"]["kind"], "goal");
    }

    #[test]
    fn schedule_task_creates_and_list_shows_it() {
        let (store, dir) = store();
        let (text, is_error) = handle(&store, SCHEDULE_TASK, &args(), &ctx());
        assert!(!is_error, "{text}");
        assert!(text.contains("daily report"), "{text}");

        let (listing, is_error) = handle(&store, MANAGE_TASKS, &json!({ "action": "list" }), &ctx());
        assert!(!is_error, "{listing}");
        assert!(listing.contains("daily report"), "{listing}");
        assert!(listing.contains("read_only"), "{listing}");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn pause_resume_update_and_remove_round_trip() {
        let (store, dir) = store();
        let _ = handle(&store, SCHEDULE_TASK, &args(), &ctx());
        let id = store.list(false).unwrap()[0]["id"].as_str().unwrap().to_owned();

        let (text, is_error) = handle(&store, MANAGE_TASKS, &json!({ "action": "pause", "taskId": id }), &ctx());
        assert!(!is_error);
        assert!(text.contains("paused"));
        assert_eq!(store.list(false).unwrap()[0]["enabled"], false);

        let (text, is_error) = handle(&store, MANAGE_TASKS, &json!({ "action": "resume", "taskId": id }), &ctx());
        assert!(!is_error);
        assert!(text.contains("resumed"));

        let (text, is_error) = handle(
            &store,
            MANAGE_TASKS,
            &json!({ "action": "update", "taskId": id, "patch": { "title": "renamed" } }),
            &ctx(),
        );
        assert!(!is_error, "{text}");
        assert_eq!(store.list(false).unwrap()[0]["title"], "renamed");

        let (text, is_error) = handle(&store, MANAGE_TASKS, &json!({ "action": "remove", "taskId": id }), &ctx());
        assert!(!is_error);
        assert!(text.contains("removed"));
        assert!(store.list(true).unwrap().is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn unknown_actions_and_tools_are_errors() {
        let (store, dir) = store();
        let (text, is_error) = handle(&store, MANAGE_TASKS, &json!({ "action": "explode" }), &ctx());
        assert!(is_error);
        assert!(text.contains("unknown action"));
        let (text, is_error) = handle(&store, "delete_everything", &json!({}), &ctx());
        assert!(is_error);
        assert!(text.contains("unknown host tool"));
        std::fs::remove_dir_all(dir).ok();
    }
}
