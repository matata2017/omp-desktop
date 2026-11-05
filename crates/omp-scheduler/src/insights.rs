//! Aggregations behind the Insights tab and the notification bell.
//!
//! Blueprint: `src/main/scheduler/insights.ts` and the notices half of `state.ts`. Everything here
//! is a pure function over already-read rows, so the numbers can be unit tested without a database
//! or a clock.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{Value, json};

/// Run states that count as a failure for the success rate.
const FAILED_STATES: [&str; 3] = ["failed", "timed_out", "blocked_config"];
/// States that mean "the run happened but was not a pass or a fail".
const SKIPPED_STATES: [&str; 3] = ["skipped_overlap", "skipped_backlog", "missed"];

fn state_of(run: &Value) -> &str {
    run.get("state")
        .or_else(|| run.get("status"))
        .and_then(Value::as_str)
        .unwrap_or("unknown")
}

fn number(run: &Value, key: &str) -> Option<i64> {
    run.get(key).and_then(Value::as_i64)
}

fn succeeded(run: &Value) -> bool {
    matches!(state_of(run), "succeeded" | "ok")
}

fn failed(run: &Value) -> bool {
    FAILED_STATES.contains(&state_of(run))
}

fn median(values: &mut [i64]) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let mid = values.len() / 2;
    Some(if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) / 2
    } else {
        values[mid]
    })
}

/// `SchedulerInsights` for a window ending at `now_ms`.
pub fn compute_insights(period_days: u64, now_ms: i64, runs: &[Value], tasks: &[Value]) -> Value {
    let window_start = now_ms - (period_days as i64) * 86_400_000;
    let in_window: Vec<&Value> = runs
        .iter()
        .filter(|run| number(run, "started_at").or_else(|| number(run, "startedAt")).unwrap_or(0) >= window_start)
        .collect();

    let mut succeeded_count = 0u64;
    let mut failed_count = 0u64;
    let mut durations: Vec<i64> = Vec::new();
    let mut cost_total = 0.0f64;
    let mut verification_pass = 0u64;
    let mut verification_total = 0u64;
    let mut review_accept = 0u64;
    let mut review_total = 0u64;
    let mut per_day: BTreeMap<String, (u64, u64, u64, u64)> = BTreeMap::new();
    let mut cost_by_task: HashMap<String, (f64, u64)> = HashMap::new();
    let mut recent_failures: Vec<Value> = Vec::new();

    for run in &in_window {
        if succeeded(run) {
            succeeded_count += 1;
        } else if failed(run) {
            failed_count += 1;
            recent_failures.push((*run).clone());
        }

        let started = number(run, "started_at").or_else(|| number(run, "startedAt"));
        let ended = number(run, "ended_at").or_else(|| number(run, "endedAt"));
        if let (Some(started), Some(ended)) = (started, ended)
            && ended >= started
        {
            durations.push(ended - started);
        }

        let cost = run.get("cost").and_then(Value::as_f64).unwrap_or(0.0);
        cost_total += cost;
        let task_id = run.get("task_id").or_else(|| run.get("taskId")).and_then(Value::as_str).unwrap_or_default();
        let entry = cost_by_task.entry(task_id.to_owned()).or_insert((0.0, 0));
        entry.0 += cost;
        entry.1 += 1;

        match run.get("verification").and_then(|v| v.get("state")).and_then(Value::as_str) {
            Some("passed") => {
                verification_pass += 1;
                verification_total += 1;
            }
            Some("failed") | Some("error") => verification_total += 1,
            _ => {}
        }
        match run.get("review").and_then(|r| r.get("state")).and_then(Value::as_str) {
            Some("accepted") => {
                review_accept += 1;
                review_total += 1;
            }
            Some("rejected") => review_total += 1,
            _ => {}
        }

        if let Some(started) = started {
            let day = chrono::DateTime::from_timestamp_millis(started)
                .map(|time| time.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
                .unwrap_or_default();
            let bucket = per_day.entry(day).or_insert((0, 0, 0, 0));
            if succeeded(run) {
                bucket.0 += 1;
            } else if failed(run) {
                bucket.1 += 1;
            } else if SKIPPED_STATES.contains(&state_of(run)) {
                bucket.3 += 1;
            } else {
                bucket.2 += 1;
            }
        }
    }

    let decided = succeeded_count + failed_count;
    let success_rate = (decided > 0).then(|| succeeded_count as f64 / decided as f64);

    let titles: HashMap<&str, &str> = tasks
        .iter()
        .filter_map(|task| {
            Some((
                task.get("id")?.as_str()?,
                task.get("title").and_then(Value::as_str).unwrap_or(""),
            ))
        })
        .collect();

    let mut cost_rows: Vec<Value> = cost_by_task
        .into_iter()
        .map(|(task_id, (cost, count))| {
            json!({
                "taskId": task_id,
                "title": titles.get(task_id.as_str()).copied().unwrap_or(""),
                "costUsd": cost,
                "runs": count,
            })
        })
        .collect();
    cost_rows.sort_by(|a, b| {
        b["costUsd"]
            .as_f64()
            .unwrap_or(0.0)
            .total_cmp(&a["costUsd"].as_f64().unwrap_or(0.0))
    });

    recent_failures.sort_by_key(|run| -(number(run, "started_at").or_else(|| number(run, "startedAt")).unwrap_or(0)));
    recent_failures.truncate(5);

    json!({
        "periodDays": period_days,
        "runs": in_window.len(),
        "succeeded": succeeded_count,
        "failed": failed_count,
        "successRate": success_rate,
        "verificationPassRate": (verification_total > 0)
            .then(|| verification_pass as f64 / verification_total as f64),
        "reviewAcceptRate": (review_total > 0).then(|| review_accept as f64 / review_total as f64),
        "costUsd": cost_total,
        "medianDurationMs": median(&mut durations),
        "perDay": per_day
            .into_iter()
            .map(|(day, (ok, bad, quiet, skipped))| json!({
                "day": day,
                "succeeded": ok,
                "failed": bad,
                "quiet": quiet,
                "skipped": skipped,
            }))
            .collect::<Vec<_>>(),
        "costByTask": cost_rows,
        "recentFailures": recent_failures,
    })
}

/// Notices for the bell: runs waiting on a person, and runs that broke.
///
/// `read_ids` is the set already acknowledged; a notice keeps its id so reading it never makes the
/// list jump around.
pub fn derive_notices(runs: &[Value], tasks: &[Value], read_ids: &HashSet<String>, limit: usize) -> Vec<Value> {
    let titles: HashMap<&str, &str> = tasks
        .iter()
        .filter_map(|task| {
            Some((
                task.get("id")?.as_str()?,
                task.get("title").and_then(Value::as_str).unwrap_or(""),
            ))
        })
        .collect();

    let mut notices: Vec<Value> = Vec::new();
    for run in runs {
        let review_state = run.get("review").and_then(|r| r.get("state")).and_then(Value::as_str).unwrap_or("none");
        let tone = if review_state == "pending" {
            Some("review")
        } else if failed(run) {
            Some("failed")
        } else {
            None
        };
        let Some(tone) = tone else { continue };

        let run_id = run.get("id").and_then(Value::as_str).unwrap_or_default();
        let task_id = run
            .get("task_id")
            .or_else(|| run.get("taskId"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let title = titles.get(task_id).copied().unwrap_or("");
        let created_at = number(run, "ended_at")
            .or_else(|| number(run, "endedAt"))
            .or_else(|| number(run, "started_at"))
            .or_else(|| number(run, "startedAt"))
            .unwrap_or(0);
        let id = format!("{run_id}:{tone}");
        notices.push(json!({
            "id": id,
            "target": { "view": "runs", "taskId": task_id, "runId": run_id },
            "title": if title.is_empty() { "Scheduled task".to_owned() } else { title.to_owned() },
            "body": if tone == "review" {
                format!("Run finished and is waiting for your review")
            } else {
                format!("Run {}", state_of(run).replace('_', " "))
            },
            "tone": tone,
            "createdAt": created_at,
            "read": read_ids.contains(&id),
        }));
    }

    notices.sort_by_key(|notice| -notice["createdAt"].as_i64().unwrap_or(0));
    notices.truncate(limit);
    notices
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(id: &str, state: &str, started: i64, ended: i64, cost: f64) -> Value {
        json!({
            "id": id,
            "taskId": "t1",
            "state": state,
            "startedAt": started,
            "endedAt": ended,
            "cost": cost,
        })
    }

    fn tasks() -> Value {
        json!([{ "id": "t1", "title": "daily report" }])
    }

    #[test]
    fn empty_history_reports_nulls_not_zeroes() {
        let insights = compute_insights(7, 1_800_000_000_000, &[], &[]);
        assert_eq!(insights["runs"], 0);
        assert_eq!(insights["successRate"], Value::Null);
        assert_eq!(insights["reviewAcceptRate"], Value::Null);
        assert_eq!(insights["medianDurationMs"], Value::Null);
        assert_eq!(insights["costUsd"], 0.0);
        assert_eq!(insights["perDay"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn counts_states_and_ignores_runs_outside_the_window() {
        let now = 1_800_000_000_000i64;
        let runs = vec![
            run("a", "succeeded", now - 1_000, now - 500, 0.10),
            run("b", "failed", now - 2_000, now - 1_500, 0.20),
            run("c", "timed_out", now - 3_000, now - 3_000, 0.0),
            run("d", "skipped_overlap", now - 4_000, now - 4_000, 0.0),
            // Outside a 7-day window.
            run("old", "succeeded", now - 30 * 86_400_000, now, 9.0),
        ];
        let tasks = tasks();
        let insights = compute_insights(7, now, &runs, tasks.as_array().unwrap());

        assert_eq!(insights["runs"], 4);
        assert_eq!(insights["succeeded"], 1);
        assert_eq!(insights["failed"], 2);
        assert!((insights["successRate"].as_f64().unwrap() - 1.0 / 3.0).abs() < 1e-9);
        assert!((insights["costUsd"].as_f64().unwrap() - 0.30).abs() < 1e-9);
        assert_eq!(insights["recentFailures"].as_array().unwrap().len(), 2);
        assert_eq!(insights["costByTask"][0]["taskId"], "t1");
        assert_eq!(insights["costByTask"][0]["title"], "daily report");
        let day = &insights["perDay"].as_array().unwrap()[0];
        assert_eq!(day["succeeded"], 1);
        assert_eq!(day["failed"], 2);
        assert_eq!(day["skipped"], 1);
    }

    #[test]
    fn median_handles_even_and_odd_counts() {
        let mut odd = vec![30, 10, 20];
        assert_eq!(median(&mut odd), Some(20));
        let mut even = vec![10, 20, 30, 40];
        assert_eq!(median(&mut even), Some(25));
        assert_eq!(median(&mut []), None);
    }

    #[test]
    fn verification_and_review_rates_only_count_decided_runs() {
        let now = 1_800_000_000_000i64;
        let runs = vec![
            json!({ "id": "a", "taskId": "t1", "state": "succeeded", "startedAt": now,
                    "verification": { "state": "passed" }, "review": { "state": "accepted" } }),
            json!({ "id": "b", "taskId": "t1", "state": "succeeded", "startedAt": now,
                    "verification": { "state": "failed" }, "review": { "state": "rejected" } }),
            json!({ "id": "c", "taskId": "t1", "state": "failed", "startedAt": now,
                    "verification": { "state": "none" }, "review": { "state": "none" } }),
        ];
        let tasks = tasks();
        let insights = compute_insights(30, now, &runs, tasks.as_array().unwrap());
        assert!((insights["verificationPassRate"].as_f64().unwrap() - 0.5).abs() < 1e-9);
        assert!((insights["reviewAcceptRate"].as_f64().unwrap() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn notices_cover_review_and_failures_and_respect_read_state() {
        let now = 1_800_000_000_000i64;
        let runs = vec![
            json!({ "id": "r1", "taskId": "t1", "state": "succeeded", "endedAt": now,
                    "review": { "state": "pending" } }),
            json!({ "id": "r2", "taskId": "t1", "state": "failed", "endedAt": now - 1,
                    "review": { "state": "none" } }),
            json!({ "id": "r3", "taskId": "t1", "state": "succeeded", "endedAt": now - 2,
                    "review": { "state": "accepted" } }),
        ];
        let tasks = tasks();
        let mut read = HashSet::new();
        read.insert("r2:failed".to_owned());
        let notices = derive_notices(&runs, tasks.as_array().unwrap(), &read, 10);

        assert_eq!(notices.len(), 2, "an accepted run is not a notice");
        assert_eq!(notices[0]["id"], "r1:review");
        assert_eq!(notices[0]["tone"], "review");
        assert_eq!(notices[0]["read"], false);
        assert_eq!(notices[1]["id"], "r2:failed");
        assert_eq!(notices[1]["read"], true);
        assert_eq!(notices[1]["title"], "daily report");
        assert_eq!(notices[1]["target"]["runId"], "r2");
    }

    #[test]
    fn notices_cap_the_list_newest_first() {
        let runs: Vec<Value> = (0..5)
            .map(|i| json!({ "id": format!("r{i}"), "taskId": "t1", "state": "failed", "endedAt": 100 + i }))
            .collect();
        let tasks = tasks();
        let notices = derive_notices(&runs, tasks.as_array().unwrap(), &HashSet::new(), 3);
        assert_eq!(notices.len(), 3);
        assert_eq!(notices[0]["id"], "r4:failed");
    }
}
