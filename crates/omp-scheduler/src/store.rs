//! Task and run CRUD. The config rides as JSON next to the columns queries filter on, so the
//! task shape stays the caller's; `enabled` and `archived` are owned by their columns and win
//! over any same-named key in the config.

use omp_core::{Error, Result};
use rusqlite::{OptionalExtension, params};
use serde_json::{Map, Value, json};

use crate::db::{config_object, db_error, missing, new_id, now_ms};

pub use crate::db::TaskStore;

/// `updated_at` moves forward on every write, even when two writes share a millisecond.
fn bump(param: usize) -> String {
    format!("updated_at = CASE WHEN ?{param} > updated_at THEN ?{param} ELSE updated_at + 1 END")
}

impl TaskStore {
    /// Insert a draft as revision 1; `enabled`/`archived` in the draft seed those columns.
    pub fn create(&self, draft: &Value) -> Result<String> {
        let object = draft
            .as_object()
            .ok_or_else(|| Error::msg("task draft must be a JSON object"))?;
        let id = new_id("task");
        let enabled = object.get("enabled").and_then(Value::as_bool).unwrap_or(true);
        let archived = object.get("archived").and_then(Value::as_bool).unwrap_or(false);
        let config = draft.to_string();
        let now = now_ms();
        let mut db = self.db.lock();
        let transaction = db.transaction().map_err(db_error)?;
        transaction
            .execute(
                "INSERT INTO tasks (id, config, enabled, created_at, updated_at, archived)
                 VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
                params![id, config, i64::from(enabled), now, i64::from(archived)],
            )
            .map_err(db_error)?;
        transaction
            .execute(
                "INSERT INTO revisions (task_id, rev, config, at) VALUES (?1, 1, ?2, ?3)",
                params![id, config, now],
            )
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)?;
        Ok(id)
    }

    /// Merge a patch into the config, freeze it as a new revision, and return the new rev.
    pub fn update(&self, id: &str, patch: &Value) -> Result<i64> {
        let patch = patch
            .as_object()
            .ok_or_else(|| Error::msg("task patch must be a JSON object"))?;
        let mut db = self.db.lock();
        let transaction = db.transaction().map_err(db_error)?;
        let current: Option<(String, i64, i64)> = transaction
            .query_row(
                "SELECT config, enabled, archived FROM tasks WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(db_error)?;
        let Some((config_text, enabled, archived)) = current else {
            return Err(missing("task", id));
        };
        let mut config = config_object(&config_text)?;
        let mut enabled = enabled != 0;
        let mut archived = archived != 0;
        for (key, value) in patch {
            match key.as_str() {
                "enabled" => enabled = value.as_bool().unwrap_or(enabled),
                "archived" => archived = value.as_bool().unwrap_or(archived),
                _ => {}
            }
            config.insert(key.clone(), value.clone());
        }
        let config_text = Value::Object(config).to_string();
        let rev: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(rev), 0) + 1 FROM revisions WHERE task_id = ?1",
                params![id],
                |row| row.get(0),
            )
            .map_err(db_error)?;
        let now = now_ms();
        transaction
            .execute(
                &format!(
                    "UPDATE tasks SET config = ?2, enabled = ?3, archived = ?4, {} WHERE id = ?1",
                    bump(5)
                ),
                params![id, config_text, i64::from(enabled), i64::from(archived), now],
            )
            .map_err(db_error)?;
        transaction
            .execute(
                "INSERT INTO revisions (task_id, rev, config, at) VALUES (?1, ?2, ?3, ?4)",
                params![id, rev, config_text, now],
            )
            .map_err(db_error)?;
        transaction.commit().map_err(db_error)?;
        Ok(rev)
    }

    /// Pause or resume a task. Whether that leaves a next fire to compute is the caller's call.
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let db = self.db.lock();
        let changed = db
            .execute(
                &format!("UPDATE tasks SET enabled = ?2, {} WHERE id = ?1", bump(3)),
                params![id, i64::from(enabled), now_ms()],
            )
            .map_err(db_error)?;
        if changed == 0 {
            return Err(missing("task", id));
        }
        Ok(())
    }

    pub fn archive(&self, id: &str) -> Result<()> {
        let db = self.db.lock();
        let changed = db
            .execute(
                &format!("UPDATE tasks SET archived = 1, {} WHERE id = ?1", bump(2)),
                params![id, now_ms()],
            )
            .map_err(db_error)?;
        if changed == 0 {
            return Err(missing("task", id));
        }
        Ok(())
    }

    /// Remove a task with its revisions and runs.
    pub fn delete(&self, id: &str) -> Result<()> {
        let mut db = self.db.lock();
        let transaction = db.transaction().map_err(db_error)?;
        transaction
            .execute("DELETE FROM runs WHERE task_id = ?1", params![id])
            .map_err(db_error)?;
        transaction
            .execute("DELETE FROM revisions WHERE task_id = ?1", params![id])
            .map_err(db_error)?;
        let changed = transaction
            .execute("DELETE FROM tasks WHERE id = ?1", params![id])
            .map_err(db_error)?;
        if changed == 0 {
            return Err(missing("task", id));
        }
        transaction.commit().map_err(db_error)?;
        Ok(())
    }

    /// Tasks in creation order, oldest first. Archived tasks are hidden unless asked for.
    pub fn list(&self, include_archived: bool) -> Result<Vec<Value>> {
        let db = self.db.lock();
        let filter = if include_archived { "" } else { "WHERE archived = 0" };
        let mut statement = db
            .prepare(&format!(
                "SELECT id, config, enabled, archived, created_at, updated_at,
                        (SELECT MAX(rev) FROM revisions WHERE revisions.task_id = tasks.id)
                 FROM tasks {filter}
                 ORDER BY created_at ASC, rowid ASC"
            ))
            .map_err(db_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                ))
            })
            .map_err(db_error)?;
        let mut tasks = Vec::new();
        for row in rows {
            let (id, config, enabled, archived, created_at, updated_at, rev) =
                row.map_err(db_error)?;
            let mut task = config_object(&config)?;
            task.insert("id".to_owned(), Value::String(id));
            task.insert("rev".to_owned(), Value::from(rev.unwrap_or_default()));
            task.insert("enabled".to_owned(), Value::Bool(enabled != 0));
            task.insert("archived".to_owned(), Value::Bool(archived != 0));
            task.insert("createdAt".to_owned(), Value::from(created_at));
            task.insert("updatedAt".to_owned(), Value::from(updated_at));
            tasks.push(Value::Object(task));
        }
        Ok(tasks)
    }

    /// Start a run row; returns its id, generating one when the JSON carries none.
    pub fn record_run(&self, run: &Value) -> Result<String> {
        let object = run
            .as_object()
            .ok_or_else(|| Error::msg("run must be a JSON object"))?;
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map_or_else(|| new_id("run"), str::to_owned);
        let task_id = pick(object, "taskId", "task_id")
            .and_then(Value::as_str)
            .filter(|task| !task.is_empty())
            .ok_or_else(|| Error::msg("run.taskId is required"))?;
        let status = object
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("queued");
        let started_at = pick(object, "startedAt", "started_at")
            .and_then(Value::as_i64)
            .unwrap_or_else(now_ms);
        let ended_at = pick(object, "endedAt", "ended_at").and_then(Value::as_i64);
        let session_path = pick(object, "sessionPath", "session_path").and_then(Value::as_str);
        let report = json_text(object.get("report"));
        let review = json_text(object.get("review"));
        let cost = object.get("cost").and_then(Value::as_f64);
        let db = self.db.lock();
        let changed = db
            .execute(
                "INSERT OR IGNORE INTO runs
                 (id, task_id, status, started_at, ended_at, session_path, report, review, cost)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id,
                    task_id,
                    status,
                    started_at,
                    ended_at,
                    session_path,
                    report,
                    review,
                    cost
                ],
            )
            .map_err(db_error)?;
        if changed == 0 {
            return Err(Error::msg(format!("scheduler run {id} is already recorded")));
        }
        Ok(id)
    }

    /// Close a run. Fields left as `None` keep whatever the record already held.
    pub fn finish_run(
        &self,
        id: &str,
        status: &str,
        report: Option<&Value>,
        review: Option<&Value>,
        cost: Option<f64>,
        session_path: Option<&str>,
        ended_at: i64,
    ) -> Result<()> {
        let report = json_text(report);
        let review = json_text(review);
        let db = self.db.lock();
        let changed = db
            .execute(
                "UPDATE runs SET status = ?2, ended_at = ?3,
                     session_path = COALESCE(?4, session_path),
                     report = COALESCE(?5, report),
                     review = COALESCE(?6, review),
                     cost = COALESCE(?7, cost)
                 WHERE id = ?1",
                params![id, status, ended_at, session_path, report, review, cost],
            )
            .map_err(db_error)?;
        if changed == 0 {
            return Err(missing("run", id));
        }
        Ok(())
    }

    /// Runs of one task, newest first.
    pub fn runs(&self, task_id: &str, limit: usize) -> Result<Vec<Value>> {
        let db = self.db.lock();
        let mut statement = db
            .prepare(
                "SELECT id, task_id, status, started_at, ended_at, session_path, report, review, cost
                 FROM runs WHERE task_id = ?1
                 ORDER BY started_at DESC, rowid DESC LIMIT ?2",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map(params![task_id, limit as i64], |row| {
                Ok(RunRow {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    status: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    session_path: row.get(5)?,
                    report: row.get(6)?,
                    review: row.get(7)?,
                    cost: row.get(8)?,
                })
            })
            .map_err(db_error)?;
        let mut runs = Vec::new();
        for row in rows {
            runs.push(row.map_err(db_error)?.into_value()?);
        }
        Ok(runs)
    }

    /// Frozen configs of one task, revision 1 first.
    pub fn revisions(&self, task_id: &str) -> Result<Vec<Value>> {
        let db = self.db.lock();
        let mut statement = db
            .prepare("SELECT rev, config, at FROM revisions WHERE task_id = ?1 ORDER BY rev ASC")
            .map_err(db_error)?;
        let rows = statement
            .query_map(params![task_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(db_error)?;
        let mut revisions = Vec::new();
        for row in rows {
            let (rev, config, at) = row.map_err(db_error)?;
            revisions.push(json!({
                "rev": rev,
                "at": at,
                "config": Value::Object(config_object(&config)?),
            }));
        }
        Ok(revisions)
    }
    /// Runs across every task started at or after `since_ms`, newest first. Insights and the
    /// notice bell both need cross-task history, which `runs(task_id, …)` cannot give them.
    pub fn all_runs(&self, since_ms: i64, limit: usize) -> Result<Vec<Value>> {
        let db = self.db.lock();
        let mut statement = db
            .prepare(
                "SELECT id, task_id, status, started_at, ended_at, session_path, report, review, cost
                 FROM runs WHERE started_at >= ?1
                 ORDER BY started_at DESC, rowid DESC LIMIT ?2",
            )
            .map_err(db_error)?;
        let rows = statement
            .query_map(params![since_ms, limit as i64], |row| {
                Ok(RunRow {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    status: row.get(2)?,
                    started_at: row.get(3)?,
                    ended_at: row.get(4)?,
                    session_path: row.get(5)?,
                    report: row.get(6)?,
                    review: row.get(7)?,
                    cost: row.get(8)?,
                })
            })
            .map_err(db_error)?;
        let mut runs = Vec::new();
        for row in rows {
            runs.push(row.map_err(db_error)?.into_value()?);
        }
        Ok(runs)
    }

    /// Notice ids the user already dismissed, persisted so the bell does not reopen them.
    pub fn read_notice_ids(&self) -> Result<std::collections::HashSet<String>> {
        let raw = self.get_meta("notices.read")?.unwrap_or_default();
        Ok(serde_json::from_str::<Vec<String>>(&raw)
            .unwrap_or_default()
            .into_iter()
            .collect())
    }

    /// Mark notices read. `None` marks everything currently unread, which is what "clear all" does.
    pub fn mark_notices_read(&self, ids: Option<&[String]>, unread: &[String]) -> Result<()> {
        let mut known = self.read_notice_ids()?;
        match ids {
            Some(ids) => known.extend(ids.iter().cloned()),
            None => known.extend(unread.iter().cloned()),
        }
        let mut sorted: Vec<String> = known.into_iter().collect();
        sorted.sort();
        self.set_meta("notices.read", &serde_json::to_string(&sorted)?)
    }

    /// Record a run's review decision.
    pub fn review_run(&self, run_id: &str, decision: &str, note: Option<&str>, at: i64) -> Result<()> {
        let review = serde_json::json!({ "state": decision, "note": note, "at": at });
        let db = self.db.lock();
        db.execute(
            "UPDATE runs SET review = ?2 WHERE id = ?1",
            params![run_id, review.to_string()],
        )
        .map_err(db_error)?;
        Ok(())
    }

}

struct RunRow {
    id: String,
    task_id: String,
    status: String,
    started_at: i64,
    ended_at: Option<i64>,
    session_path: Option<String>,
    report: Option<String>,
    review: Option<String>,
    cost: Option<f64>,
}

impl RunRow {
    fn into_value(self) -> Result<Value> {
        let report = parse_json(self.report)?;
        let review = parse_json(self.review)?;
        Ok(json!({
            "id": self.id,
            "taskId": self.task_id,
            "status": self.status,
            "startedAt": self.started_at,
            "endedAt": self.ended_at,
            "sessionPath": self.session_path,
            "report": report,
            "review": review,
            "cost": self.cost,
        }))
    }
}

/// Inputs are camelCase on the IPC wire and snake_case from Rust callers; accept both.
fn pick<'a>(object: &'a Map<String, Value>, camel: &str, snake: &str) -> Option<&'a Value> {
    object.get(camel).or_else(|| object.get(snake))
}

fn json_text(value: Option<&Value>) -> Option<String> {
    value.filter(|value| !value.is_null()).map(Value::to_string)
}

fn parse_json(text: Option<String>) -> Result<Value> {
    match text {
        Some(text) => Ok(serde_json::from_str(&text)?),
        None => Ok(Value::Null),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{remove_db, temp_path};

    fn scratch() -> TaskStore {
        TaskStore::open(":memory:").unwrap()
    }

    fn draft(title: &str) -> Value {
        json!({
            "title": title,
            "prompt": "do the thing",
            "cwd": "D:/work",
            "schedule": {"kind": "cron", "expr": "*/5 * * * *", "tz": "UTC"},
            "timeoutMs": 600000,
        })
    }

    #[test]
    fn create_update_and_revision_chain() {
        let store = scratch();
        let alpha = store.create(&draft("alpha")).unwrap();
        let beta = store.create(&draft("beta")).unwrap();

        let tasks = store.list(false).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0]["id"], json!(alpha));
        assert_eq!(tasks[1]["id"], json!(beta));
        assert_eq!(tasks[0]["rev"], json!(1));
        assert_eq!(tasks[0]["enabled"], json!(true));
        assert_eq!(tasks[0]["archived"], json!(false));
        assert!(tasks[0]["createdAt"].as_i64().unwrap() > 0);

        let rev = store
            .update(&alpha, &json!({"title": "alpha 2", "extra": {"keep": true}}))
            .unwrap();
        assert_eq!(rev, 2);

        let tasks = store.list(false).unwrap();
        assert_eq!(tasks[0]["title"], json!("alpha 2"));
        assert_eq!(tasks[0]["extra"]["keep"], json!(true));
        assert_eq!(tasks[0]["rev"], json!(2));
        assert_eq!(tasks[0]["prompt"], json!("do the thing"));
        assert_eq!(tasks[0]["cwd"], json!("D:/work"));
        assert!(tasks[0]["updatedAt"].as_i64() > tasks[0]["createdAt"].as_i64());

        let revisions = store.revisions(&alpha).unwrap();
        assert_eq!(revisions.len(), 2);
        assert_eq!(revisions[0]["rev"], json!(1));
        assert_eq!(revisions[0]["config"]["title"], json!("alpha"));
        assert!(revisions[0]["config"].get("extra").is_none());
        assert_eq!(revisions[1]["rev"], json!(2));
        assert_eq!(revisions[1]["config"]["title"], json!("alpha 2"));
        assert_eq!(revisions[1]["config"]["prompt"], json!("do the thing"));
        assert_eq!(revisions[1]["config"]["extra"]["keep"], json!(true));
        assert_eq!(store.revisions(&beta).unwrap().len(), 1);
    }

    #[test]
    fn archive_hides_from_the_default_list() {
        let store = scratch();
        let id = store.create(&draft("gone")).unwrap();
        store.archive(&id).unwrap();
        assert!(store.list(false).unwrap().is_empty());
        let all = store.list(true).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0]["archived"], json!(true));
        assert_eq!(all[0]["enabled"], json!(true));
    }

    #[test]
    fn disabling_persists_and_leaves_next_fire_alone() {
        let path = temp_path("disabled");
        let id = {
            let store = TaskStore::open(&path).unwrap();
            let id = store.create(&draft("quiet")).unwrap();
            store.set_enabled(&id, false).unwrap();
            let task = &store.list(false).unwrap()[0];
            assert_eq!(task["enabled"], json!(false));
            assert!(task.get("nextFireAt").is_none());
            id
        };
        let reopened = TaskStore::open(&path).unwrap();
        let task = &reopened.list(false).unwrap()[0];
        assert_eq!(task["id"], json!(id));
        assert_eq!(task["enabled"], json!(false));
        drop(reopened);
        remove_db(&path);
    }

    #[test]
    fn runs_round_trip_every_column() {
        let store = scratch();
        let task = store.create(&draft("run me")).unwrap();
        let first = store
            .record_run(&json!({
                "taskId": task,
                "status": "running",
                "startedAt": 1_700_000_000_000_i64,
                "sessionPath": "sessions/a.jsonl",
            }))
            .unwrap();
        store
            .finish_run(
                &first,
                "succeeded",
                Some(&json!({"status": "done", "summary": "all good", "findings": [], "changes": []})),
                Some(&json!({"state": "pending"})),
                Some(0.42),
                Some("sessions/a.jsonl"),
                1_700_000_600_000,
            )
            .unwrap();

        let runs = store.runs(&task, 10).unwrap();
        assert_eq!(runs.len(), 1);
        let run = &runs[0];
        assert_eq!(run["id"], json!(first));
        assert_eq!(run["taskId"], json!(task));
        assert_eq!(run["status"], json!("succeeded"));
        assert_eq!(run["startedAt"], json!(1_700_000_000_000_i64));
        assert_eq!(run["endedAt"], json!(1_700_000_600_000_i64));
        assert_eq!(run["sessionPath"], json!("sessions/a.jsonl"));
        assert_eq!(run["report"]["summary"], json!("all good"));
        assert_eq!(run["report"]["status"], json!("done"));
        assert_eq!(run["review"]["state"], json!("pending"));
        assert_eq!(run["cost"], json!(0.42));

        let second = store
            .record_run(&json!({"taskId": task, "status": "running", "startedAt": 1_700_000_700_000_i64}))
            .unwrap();
        let runs = store.runs(&task, 10).unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0]["id"], json!(second));
        assert_eq!(runs[0]["endedAt"], Value::Null);
        assert_eq!(runs[0]["report"], Value::Null);
        assert_eq!(runs[0]["review"], Value::Null);
        assert_eq!(runs[0]["cost"], Value::Null);
        assert_eq!(store.runs(&task, 1).unwrap().len(), 1);
        assert!(store.runs("other", 10).unwrap().is_empty());
    }

    #[test]
    fn finishing_keeps_fields_the_run_already_had() {
        let store = scratch();
        let task = store.create(&draft("run me")).unwrap();
        let run = store
            .record_run(&json!({"taskId": task, "status": "running", "sessionPath": "a.jsonl"}))
            .unwrap();
        store
            .finish_run(&run, "failed", None, None, None, None, 7)
            .unwrap();
        let finished = &store.runs(&task, 1).unwrap()[0];
        assert_eq!(finished["status"], json!("failed"));
        assert_eq!(finished["sessionPath"], json!("a.jsonl"));
        assert_eq!(finished["endedAt"], json!(7));
    }

    #[test]
    fn delete_removes_revisions_and_runs() {
        let store = scratch();
        let id = store.create(&draft("short lived")).unwrap();
        store.update(&id, &json!({"title": "renamed"})).unwrap();
        store
            .record_run(&json!({"taskId": id, "status": "queued"}))
            .unwrap();
        store.delete(&id).unwrap();
        assert!(store.list(true).unwrap().is_empty());
        assert!(store.revisions(&id).unwrap().is_empty());
        assert!(store.runs(&id, 10).unwrap().is_empty());
    }

    #[test]
    fn unknown_ids_are_errors() {
        let store = scratch();
        assert!(store.update("nope", &json!({"title": "x"})).is_err());
        assert!(store.set_enabled("nope", false).is_err());
        assert!(store.archive("nope").is_err());
        assert!(store.delete("nope").is_err());
        assert!(store.finish_run("nope", "succeeded", None, None, None, None, 0).is_err());
        assert!(store.create(&json!(["not", "an", "object"])).is_err());
        assert!(store.update("nope", &json!("not an object")).is_err());
    }

    #[test]
    fn runs_need_a_task_and_unique_ids() {
        let store = scratch();
        assert!(store.record_run(&json!({"status": "queued"})).is_err());
        store
            .record_run(&json!({"id": "run_1", "taskId": "task_1", "status": "queued"}))
            .unwrap();
        assert!(
            store
                .record_run(&json!({"id": "run_1", "taskId": "task_1", "status": "queued"}))
                .is_err()
        );
    }

    #[test]
    fn enabling_through_a_patch_updates_the_column() {
        let store = scratch();
        let id = store.create(&draft("toggle")).unwrap();
        assert_eq!(store.list(false).unwrap()[0]["enabled"], json!(true));
        store.update(&id, &json!({"enabled": false})).unwrap();
        assert_eq!(store.list(false).unwrap()[0]["enabled"], json!(false));
    }

    #[test]
    fn meta_round_trips() {
        let store = scratch();
        assert_eq!(store.get_meta("paused").unwrap(), None);
        store.set_meta("paused", "1").unwrap();
        assert_eq!(store.get_meta("paused").unwrap().as_deref(), Some("1"));
        store.set_meta("paused", "0").unwrap();
        assert_eq!(store.get_meta("paused").unwrap().as_deref(), Some("0"));
    }
}
