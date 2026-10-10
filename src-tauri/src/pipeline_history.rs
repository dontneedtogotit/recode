use super::domain::{DomainError, Result};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineRunHistoryRecord {
    pub id: String,
    pub goal: String,
    pub status: String,
    pub step_count: u64,
    pub repair_count: u64,
    pub success: bool,
    pub error: Option<String>,
    pub finished_at: SystemTime,
}

pub trait PipelineRunHistoryRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError>;
    fn insert_run(conn: &Connection, record: &PipelineRunHistoryRecord) -> Result<(), DomainError>;
    fn finish_run(conn: &Connection, record: &PipelineRunHistoryRecord, result_json: Option<&str>) -> Result<(), DomainError>;
    fn get_run(conn: &Connection, id: &str) -> Result<Option<PipelineRunHistoryRecord>, DomainError>;
    fn get_result_json(conn: &Connection, id: &str) -> Result<Option<String>, DomainError>;
    fn list_runs(conn: &Connection, limit: u64) -> Result<Vec<PipelineRunHistoryRecord>, DomainError>;
    /// Terminal-fail every `running` row left behind by a crashed process.
    /// Called once at startup: no run can be active in a fresh process, so
    /// any visible `running` row is orphaned. Idempotent; returns the count.
    fn fail_stale_running_runs(conn: &Connection, failed_at: SystemTime) -> Result<u64, DomainError>;
}

pub struct SqlitePipelineRunHistoryRepository;

fn read_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<PipelineRunHistoryRecord> {
    let error: String = row.get(6)?;
    Ok(PipelineRunHistoryRecord {
        id: row.get(0)?,
        goal: row.get(1)?,
        status: row.get(2)?,
        step_count: row.get(3)?,
        repair_count: row.get(4)?,
        success: row.get(5)?,
        error: if error.is_empty() { None } else { Some(error) },
        finished_at: from_timestamp_millis(row.get(7)?),
    })
}

impl PipelineRunHistoryRepository for SqlitePipelineRunHistoryRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS pipeline_run_history (id TEXT PRIMARY KEY, goal TEXT NOT NULL, status TEXT NOT NULL, step_count INTEGER NOT NULL, repair_count INTEGER NOT NULL, success INTEGER NOT NULL, error TEXT, finished_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS pipeline_run_results (run_id TEXT PRIMARY KEY REFERENCES pipeline_run_history(id) ON DELETE CASCADE, result_json TEXT NOT NULL);",
        )
        .map_err(|e| DomainError::internal(format!("pipeline history schema creation failed: {e}")))
        .map(|_| ())?;
        // Migration: older schemas lack the pid column used by crash recovery
        // to distinguish a dead process's rows from a concurrent live run.
        let has_pid: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('pipeline_run_history') WHERE name = 'pid'",
                [],
                |row| row.get(0),
            )
            .map_err(|e| DomainError::internal(format!("pipeline schema introspection failed: {e}")))?;
        if has_pid == 0 {
            conn.execute_batch("ALTER TABLE pipeline_run_history ADD COLUMN pid INTEGER")
                .map_err(|e| DomainError::internal(format!("pipeline schema migration failed: {e}")))?;
        }
        Ok(())
    }

    fn insert_run(conn: &Connection, record: &PipelineRunHistoryRecord) -> Result<(), DomainError> {
        conn.execute(
            "INSERT INTO pipeline_run_history (id, goal, status, step_count, repair_count, success, error, finished_at, pid) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params!(
                record.id.as_str(),
                record.goal.as_str(),
                record.status.as_str(),
                record.step_count,
                record.repair_count,
                record.success,
                record.error.as_deref().unwrap_or(""),
                timestamp_millis(record.finished_at),
                i32::try_from(std::process::id()).ok(),
            ),
        )
        .map_err(|e| DomainError::internal(format!("pipeline history insert failed: {e}")))
        .map(|_| ())
    }

    fn finish_run(conn: &Connection, record: &PipelineRunHistoryRecord, result_json: Option<&str>) -> Result<(), DomainError> {
        if !matches!(record.status.as_str(), "completed" | "failed" | "cancelled") {
            return Err(DomainError::invalid_input("pipeline terminal status is invalid"));
        }
        if (record.status == "completed" && result_json.is_none())
            || (record.status == "cancelled" && result_json.is_some())
            || record.success != (record.status == "completed")
            || result_json.is_some_and(|json| json.len() > 1_048_576)
        {
            return Err(DomainError::invalid_input("pipeline result and terminal status disagree or exceed the size limit"));
        }
        let tx = conn.unchecked_transaction()
            .map_err(|e| DomainError::internal(format!("pipeline terminal transaction failed: {e}")))?;
        let count = tx.execute(
            "UPDATE pipeline_run_history SET goal = ?2, status = ?3, step_count = ?4, repair_count = ?5, success = ?6, error = ?7, finished_at = ?8 WHERE id = ?1 AND status = 'running'",
            rusqlite::params!(
                record.id.as_str(), record.goal.as_str(), record.status.as_str(), record.step_count,
                record.repair_count, record.success, record.error.as_deref().unwrap_or(""),
                timestamp_millis(record.finished_at),
            ),
        ).map_err(|e| DomainError::internal(format!("pipeline terminal persistence failed: {e}")))?;
        if count != 1 {
            return Err(DomainError::conflict("pipeline run is missing or already terminal"));
        }
        if let Some(json) = result_json {
            tx.execute("INSERT INTO pipeline_run_results (run_id, result_json) VALUES (?1, ?2)", (record.id.as_str(), json))
                .map_err(|e| DomainError::internal(format!("pipeline result persistence failed: {e}")))?;
        }
        tx.commit().map_err(|e| DomainError::internal(format!("pipeline terminal commit failed: {e}")))
    }

    fn fail_stale_running_runs(conn: &Connection, failed_at: SystemTime) -> Result<u64, DomainError> {
        // Crash recovery must never corrupt a concurrent process: only rows
        // whose recorded pid is missing (legacy), NULL, or no longer alive
        // are terminal-failed. A live process's running row is left alone.
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| DomainError::internal(format!("pipeline crash recovery transaction failed: {e}")))?;
        let stale_ids: Vec<String> = {
            let mut stmt = tx
                .prepare("SELECT id, pid FROM pipeline_run_history WHERE status = 'running'")
                .map_err(|e| DomainError::internal(format!("pipeline crash recovery query failed: {e}")))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<i32>>(1)?))
                })
                .map_err(|e| DomainError::internal(format!("pipeline crash recovery query failed: {e}")))?;
            let mut ids = Vec::new();
            for row in rows {
                let (id, pid) = row.map_err(|e| DomainError::internal(format!("pipeline crash recovery row failed: {e}")))?;
                let alive = pid
                    .map(|pid| pid == i32::try_from(std::process::id()).unwrap_or(-1) || process_alive(pid))
                    .unwrap_or(false);
                if !alive {
                    ids.push(id);
                }
            }
            ids
        };
        let mut count = 0u64;
        for id in &stale_ids {
            let changed = tx
                .execute(
                    "UPDATE pipeline_run_history SET status = 'failed', success = 0, error = ?2, finished_at = ?3 WHERE id = ?1 AND status = 'running'",
                    rusqlite::params!(
                        id,
                        "pipeline process exited before the run reached a terminal state",
                        timestamp_millis(failed_at),
                    ),
                )
                .map_err(|e| DomainError::internal(format!("pipeline crash recovery update failed: {e}")))?;
            count += changed as u64;
        }
        tx.commit()
            .map_err(|e| DomainError::internal(format!("pipeline crash recovery commit failed: {e}")))?;
        Ok(count)
    }

    fn get_run(conn: &Connection, id: &str) -> Result<Option<PipelineRunHistoryRecord>, DomainError> {
        conn.query_row(
            "SELECT id, goal, status, step_count, repair_count, success, error, finished_at FROM pipeline_run_history WHERE id = ?1",
            [id], read_run,
        ).optional().map_err(|e| DomainError::internal(format!("pipeline history lookup failed: {e}")))
    }

    fn get_result_json(conn: &Connection, id: &str) -> Result<Option<String>, DomainError> {
        conn.query_row(
            "SELECT result_json FROM pipeline_run_results JOIN pipeline_run_history ON pipeline_run_history.id = pipeline_run_results.run_id WHERE run_id = ?1 AND pipeline_run_history.status IN ('completed', 'failed')",
            [id], |row| row.get(0),
        ).optional().map_err(|e| DomainError::internal(format!("pipeline result lookup failed: {e}")))
    }

    fn list_runs(conn: &Connection, limit: u64) -> Result<Vec<PipelineRunHistoryRecord>, DomainError> {
        let mut stmt = conn
            .prepare("SELECT id, goal, status, step_count, repair_count, success, error, finished_at FROM pipeline_run_history ORDER BY finished_at DESC LIMIT ?1")
            .map_err(|e| DomainError::internal(format!("pipeline history query prepare failed: {e}")))?;

        let rows = stmt
            .query_map([limit], read_run)
            .map_err(|e| DomainError::internal(format!("pipeline history query failed: {e}")))?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| DomainError::internal(format!("pipeline history row read failed: {e}")))?);
        }
        Ok(out)
    }
}

fn timestamp_millis(ts: SystemTime) -> i64 {
    ts.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn from_timestamp_millis(millis: i64) -> SystemTime {
    SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(millis.max(0) as u64)
}

/// Check process liveness without killing it: `kill(pid, 0)` returns Ok for
/// a live process and ESRCH for a dead one (EPERM also means "alive").
/// Migration note: any liveness check failure is treated as "alive" so
/// recovery never fails a live run's row on a probe error.
#[cfg(unix)]
fn process_alive(pid: i32) -> bool {
    if pid <= 1 {
        return false;
    }
    match rustix::process::Pid::from_raw(pid) {
        Some(parsed) => rustix::process::test_kill_process(parsed).is_ok(),
        None => false,
    }
}

#[cfg(not(unix))]
fn process_alive(_pid: i32) -> bool {
    // Without a liveness probe, treat every recorded pid as alive: recovery
    // must not fail a possibly-live concurrent process's row.
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn in_memory_conn() -> Result<Connection, DomainError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| DomainError::internal(format!("open in-memory failed: {e}")))?;
        SqlitePipelineRunHistoryRepository::ensure_schema(&conn)?;
        Ok(conn)
    }

    #[test]
    fn insert_and_list_pipeline_run_history() {
        let conn = in_memory_conn().expect("in-memory db");
        let record = PipelineRunHistoryRecord {
            id: "run-1".into(),
            goal: "build landing page".into(),
            status: "completed".into(),
            step_count: 2,
            repair_count: 0,
            success: true,
            error: None,
            finished_at: SystemTime::now(),
        };
        SqlitePipelineRunHistoryRepository::insert_run(&conn, &record).unwrap();
        let list = SqlitePipelineRunHistoryRepository::list_runs(&conn, 10).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "run-1");
        assert!(list[0].success);
    }

    #[test]
    fn terminal_state_is_queryable_by_run_id_and_cannot_be_overwritten() {
        let conn = in_memory_conn().unwrap();
        let mut run = PipelineRunHistoryRecord {
            id: "run-id-not-plan-id".into(), goal: "build".into(), status: "running".into(),
            step_count: 0, repair_count: 0, success: false, error: None, finished_at: SystemTime::now(),
        };
        SqlitePipelineRunHistoryRepository::insert_run(&conn, &run).unwrap();
        assert_eq!(SqlitePipelineRunHistoryRepository::get_run(&conn, &run.id).unwrap().unwrap().status, "running");
        run.status = "cancelled".into();
        run.success = true;
        assert!(SqlitePipelineRunHistoryRepository::finish_run(&conn, &run, None).is_err());
        run.success = false;
        run.error = Some("pipeline cancelled by user".into());
        SqlitePipelineRunHistoryRepository::finish_run(&conn, &run, None).unwrap();
        let persisted = SqlitePipelineRunHistoryRepository::get_run(&conn, &run.id).unwrap().unwrap();
        assert_eq!(persisted.status, "cancelled");
        assert_eq!(persisted.error, run.error);
        assert!(!persisted.success);
        assert!(SqlitePipelineRunHistoryRepository::finish_run(&conn, &run, None).is_err());
        assert!(SqlitePipelineRunHistoryRepository::insert_run(&conn, &run).is_err());
        assert!(SqlitePipelineRunHistoryRepository::get_run(&conn, "missing").unwrap().is_none());
    }

    #[test]
    fn completed_result_is_saved_atomically_and_legacy_rows_have_no_manufactured_result() {
        let conn = in_memory_conn().unwrap();
        let mut run = PipelineRunHistoryRecord {
            id: "new-run".into(), goal: "build".into(), status: "running".into(),
            step_count: 0, repair_count: 0, success: false, error: None, finished_at: SystemTime::now(),
        };
        SqlitePipelineRunHistoryRepository::insert_run(&conn, &run).unwrap();
        run.status = "completed".into();
        run.success = true;
        assert!(SqlitePipelineRunHistoryRepository::finish_run(&conn, &run, None).is_err());
        assert_eq!(SqlitePipelineRunHistoryRepository::get_run(&conn, &run.id).unwrap().unwrap().status, "running");
        let response = crate::pipeline::PipelineRun {
            run_id: run.id.clone(),
            execution_context: None,
            plan: crate::planner::Plan {
                id: crate::domain::EntityId::from_string("plan-id"),
                goal: run.goal.clone(),
                status: crate::planner::PlanStatus::Completed,
                steps: Vec::new(),
            },
            repair_actions: Vec::new(),
            events: Vec::new(),
            generated_files: Vec::new(),
            failed_files: Vec::new(),
            outcome: None,
            verification_results: Some(Vec::new()),
            repair_results: None,
            verification_summary: Some(String::new()),
        };
        let json = serde_json::to_string(&response).unwrap();
        SqlitePipelineRunHistoryRepository::finish_run(&conn, &run, Some(&json)).unwrap();
        let saved = SqlitePipelineRunHistoryRepository::get_result_json(&conn, &run.id).unwrap().unwrap();
        let restored: crate::pipeline::PipelineRun = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.run_id, "new-run");
        assert_eq!(restored.plan.id.as_str(), "plan-id");
        run.id = "old-plan-id".into();
        SqlitePipelineRunHistoryRepository::insert_run(&conn, &run).unwrap();
        assert!(SqlitePipelineRunHistoryRepository::get_result_json(&conn, &run.id).unwrap().is_none());
    }

    #[test]
    fn failed_build_result_remains_inspectable_without_marking_success() {
        let conn = in_memory_conn().unwrap();
        let mut record = PipelineRunHistoryRecord {
            id: "failed-build".into(), goal: "build".into(), status: "running".into(),
            step_count: 0, repair_count: 0, success: false, error: None, finished_at: SystemTime::now(),
        };
        SqlitePipelineRunHistoryRepository::insert_run(&conn, &record).unwrap();
        record.status = "failed".into();
        record.error = Some("generated content failed persistence verification".into());
        let result = r#"{"run_id":"failed-build","outcome":{"success":false}}"#;
        SqlitePipelineRunHistoryRepository::finish_run(&conn, &record, Some(result)).unwrap();
        let saved = SqlitePipelineRunHistoryRepository::get_result_json(&conn, &record.id).unwrap();
        assert_eq!(saved.as_deref(), Some(result));
        let persisted = SqlitePipelineRunHistoryRepository::get_run(&conn, &record.id).unwrap().unwrap();
        assert_eq!(persisted.status, "failed");
        assert!(!persisted.success);
        assert_eq!(persisted.error, record.error);
    }

    #[test]
    fn failed_result_insert_does_not_commit_terminal_status() {
        let conn = in_memory_conn().unwrap();
        let mut run = PipelineRunHistoryRecord {
            id: "rollback".into(), goal: "build".into(), status: "running".into(),
            step_count: 0, repair_count: 0, success: false, error: None, finished_at: SystemTime::now(),
        };
        SqlitePipelineRunHistoryRepository::insert_run(&conn, &run).unwrap();
        conn.execute_batch("CREATE TRIGGER fail_result BEFORE INSERT ON pipeline_run_results BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        run.status = "completed".into();
        run.success = true;
        assert!(SqlitePipelineRunHistoryRepository::finish_run(&conn, &run, Some("{}" )).is_err());
        assert_eq!(SqlitePipelineRunHistoryRepository::get_run(&conn, &run.id).unwrap().unwrap().status, "running");
    }

    #[test]
    fn crash_recovery_fails_stale_running_rows_and_preserves_live_and_terminal_ones() {
        let conn = in_memory_conn().unwrap();
        // Orphan with no pid (legacy row): recovered.
        let orphan = PipelineRunHistoryRecord {
            id: "orphan".into(), goal: "build".into(), status: "running".into(),
            step_count: 0, repair_count: 0, success: false, error: None, finished_at: SystemTime::now(),
        };
        SqlitePipelineRunHistoryRepository::insert_run(&conn, &orphan).unwrap();
        conn.execute("UPDATE pipeline_run_history SET pid = NULL WHERE id = 'orphan'", []).unwrap();
        // Dead-pid row: recovered. PID 1 is init on real systems and is not
        // this process; on test systems it is never the test's own process.
        let dead = PipelineRunHistoryRecord {
            id: "dead".into(), goal: "build".into(), status: "running".into(),
            step_count: 0, repair_count: 0, success: false, error: None, finished_at: SystemTime::now(),
        };
        SqlitePipelineRunHistoryRepository::insert_run(&conn, &dead).unwrap();
        conn.execute("UPDATE pipeline_run_history SET pid = 2147483000 WHERE id = 'dead'", []).unwrap();
        // Live-pid row (this process): must NOT be recovered.
        let live = PipelineRunHistoryRecord {
            id: "live".into(), goal: "build".into(), status: "running".into(),
            step_count: 0, repair_count: 0, success: false, error: None, finished_at: SystemTime::now(),
        };
        SqlitePipelineRunHistoryRepository::insert_run(&conn, &live).unwrap();
        // Terminal row: untouched.
        let mut done = PipelineRunHistoryRecord {
            id: "done".into(), goal: "build".into(), status: "running".into(),
            step_count: 0, repair_count: 0, success: false, error: None, finished_at: SystemTime::now(),
        };
        SqlitePipelineRunHistoryRepository::insert_run(&conn, &done).unwrap();
        done.status = "cancelled".into();
        done.success = false;
        done.error = Some("pipeline cancelled by user".into());
        SqlitePipelineRunHistoryRepository::finish_run(&conn, &done, None).unwrap();
        let failed = SqlitePipelineRunHistoryRepository::fail_stale_running_runs(&conn, SystemTime::now()).unwrap();
        assert_eq!(failed, 2, "only the orphaned and dead-pid rows are recovered");
        let recovered = SqlitePipelineRunHistoryRepository::get_run(&conn, "orphan").unwrap().unwrap();
        assert_eq!(recovered.status, "failed");
        assert!(!recovered.success);
        assert_eq!(recovered.error.as_deref(), Some("pipeline process exited before the run reached a terminal state"));
        assert_eq!(SqlitePipelineRunHistoryRepository::get_run(&conn, "dead").unwrap().unwrap().status, "failed");
        let still_running = SqlitePipelineRunHistoryRepository::get_run(&conn, "live").unwrap().unwrap();
        assert_eq!(still_running.status, "running", "a live process's row is never touched");
        let terminal = SqlitePipelineRunHistoryRepository::get_run(&conn, "done").unwrap().unwrap();
        assert_eq!(terminal.status, "cancelled");
        assert!(!terminal.success);
        assert_eq!(terminal.error.as_deref(), Some("pipeline cancelled by user"));
        // Idempotent: a second pass recovers nothing new (live row survives).
        assert_eq!(SqlitePipelineRunHistoryRepository::fail_stale_running_runs(&conn, SystemTime::now()).unwrap(), 0);
        assert_eq!(SqlitePipelineRunHistoryRepository::get_run(&conn, "live").unwrap().unwrap().status, "running");
        // A recovered run is terminal, so finish_run refuses to overwrite it.
        let mut overwrite = recovered;
        overwrite.status = "completed".into();
        overwrite.success = true;
        assert!(SqlitePipelineRunHistoryRepository::finish_run(&conn, &overwrite, Some("{}")).is_err());
    }

    #[test]
    fn terminal_state_and_result_survive_database_reopen() {
        let path = std::env::temp_dir().join(format!("recode-history-{}.db", uuid::Uuid::new_v4()));
        {
            let conn = Connection::open(&path).unwrap();
            // Existing installations already have the history table but not the result table.
            conn.execute_batch("CREATE TABLE pipeline_run_history (id TEXT PRIMARY KEY, goal TEXT NOT NULL, status TEXT NOT NULL, step_count INTEGER NOT NULL, repair_count INTEGER NOT NULL, success INTEGER NOT NULL, error TEXT, finished_at INTEGER NOT NULL);").unwrap();
            SqlitePipelineRunHistoryRepository::ensure_schema(&conn).unwrap();
            let mut record = PipelineRunHistoryRecord {
                id: "restart-run".into(), goal: "build".into(), status: "running".into(),
                step_count: 0, repair_count: 0, success: false, error: None, finished_at: SystemTime::now(),
            };
            SqlitePipelineRunHistoryRepository::insert_run(&conn, &record).unwrap();
            record.status = "completed".into();
            record.success = true;
            SqlitePipelineRunHistoryRepository::finish_run(&conn, &record, Some("{\"run_id\":\"restart-run\"}")).unwrap();
        }
        {
            let conn = Connection::open(&path).unwrap();
            SqlitePipelineRunHistoryRepository::ensure_schema(&conn).unwrap();
            assert_eq!(SqlitePipelineRunHistoryRepository::get_run(&conn, "restart-run").unwrap().unwrap().status, "completed");
            assert_eq!(SqlitePipelineRunHistoryRepository::get_result_json(&conn, "restart-run").unwrap().as_deref(), Some("{\"run_id\":\"restart-run\"}"));
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn blocked_versioned_evidence_and_legacy_results_survive_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("history.db");
        let legacy_gate = serde_json::json!({
            "gate": "default", "passed": true, "degraded": false, "detail": "legacy check"
        });
        let unknown_gate = serde_json::json!({
            "gate": "typecheck", "passed": false, "degraded": false, "detail": "Not executed",
            "evidence": {
                "schema_version": 1, "status": "unknown", "required": true,
                "applicable": null, "execution": null
            }
        });
        for (id, gate, success) in [("legacy", &legacy_gate, true), ("blocked", &unknown_gate, false)] {
            let conn = Connection::open(&path).unwrap();
            SqlitePipelineRunHistoryRepository::ensure_schema(&conn).unwrap();
            let mut record = PipelineRunHistoryRecord {
                id: id.into(), goal: "build".into(), status: "running".into(),
                step_count: 0, repair_count: 0, success: false, error: None,
                finished_at: SystemTime::now(),
            };
            SqlitePipelineRunHistoryRepository::insert_run(&conn, &record).unwrap();
            let mut json = serde_json::json!({
                "run_id": id,
                "plan": { "id": "plan-id", "goal": "build", "status": "Completed", "steps": [] },
                "repair_actions": [], "events": [],
                "outcome": {
                    "success": success, "degraded": !success, "generated_file_count": 0,
                    "failed_file_count": 0, "failed_files": [], "summary": "fixture",
                    "verification_results": [gate]
                },
                "verification_results": [gate]
            });
            // Exercise the same typed deserialization used by native history detail.
            if !success {
                json["execution_context"] = serde_json::json!({
                    "schema_version": 1, "mode": "workspace", "project_id": "original-workspace"
                });
            }
            let typed: crate::pipeline::PipelineRun = serde_json::from_value(json).unwrap();
            record.status = if success { "completed" } else { "failed" }.into();
            record.success = success;
            record.error = (!success).then(|| "verification blocked".into());
            SqlitePipelineRunHistoryRepository::finish_run(
                &conn, &record, Some(&serde_json::to_string(&typed).unwrap()),
            ).unwrap();
        }
        let conn = Connection::open(&path).unwrap();
        SqlitePipelineRunHistoryRepository::ensure_schema(&conn).unwrap();
        for (id, expected_gate, success) in [("legacy", legacy_gate, true), ("blocked", unknown_gate, false)] {
            let record = SqlitePipelineRunHistoryRepository::get_run(&conn, id).unwrap().unwrap();
            assert_eq!(record.success, success);
            assert_eq!(record.status, if success { "completed" } else { "failed" });
            let saved = SqlitePipelineRunHistoryRepository::get_result_json(&conn, id).unwrap().unwrap();
            let restored: crate::pipeline::PipelineRun = serde_json::from_str(&saved).unwrap();
            assert_eq!(restored.run_id, id);
            let serialized = serde_json::to_value(&restored).unwrap();
            if success {
                assert!(restored.execution_context.is_none(), "legacy provenance must stay unknown");
            } else {
                assert_eq!(serialized["execution_context"], serde_json::json!({
                    "schema_version": 1, "mode": "workspace", "project_id": "original-workspace"
                }));
            }
            assert_eq!(serialized["verification_results"][0], expected_gate);
            assert_eq!(serialized["outcome"]["verification_results"][0], expected_gate);
            // Reopening never grants permission to turn the same attempt green.
            assert!(SqlitePipelineRunHistoryRepository::finish_run(&conn, &record, Some(&saved)).is_err());
        }
    }
}
