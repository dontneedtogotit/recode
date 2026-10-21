use super::domain::{DomainError, Result};
use rusqlite::Connection;
use std::time::SystemTime;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TeamTask {
    pub id: String,
    pub team_id: String,
    pub assignee_id: String,
    pub description: String,
    pub status: TeamTaskStatus,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum TeamTaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TeamTaskRecord {
    pub id: String,
    pub team_id: String,
    pub assignee_id: String,
    pub description: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

pub trait TeamTaskRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError>;
    fn insert_task(conn: &Connection, record: &TeamTaskRecord) -> Result<(), DomainError>;
    fn list_tasks(conn: &Connection, team_id: &str) -> Result<Vec<TeamTaskRecord>, DomainError>;
    fn update_status(conn: &Connection, task_id: &str, status: &str) -> Result<(), DomainError>;
}

#[derive(Debug, Clone, Default)]
pub struct SqliteTeamTaskRepository;

impl TeamTaskRepository for SqliteTeamTaskRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS team_tasks (id TEXT PRIMARY KEY, team_id TEXT NOT NULL, assignee_id TEXT NOT NULL, description TEXT NOT NULL, status TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
            [],
        )
        .map_err(|e| DomainError::internal(format!("team task schema creation failed: {e}")))
        .map(|_| ())
    }

    fn insert_task(conn: &Connection, record: &TeamTaskRecord) -> Result<(), DomainError> {
        conn.execute(
            "INSERT OR REPLACE INTO team_tasks (id, team_id, assignee_id, description, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params!(
                record.id.as_str(),
                record.team_id.as_str(),
                record.assignee_id.as_str(),
                record.description.as_str(),
                record.status.as_str(),
                record.created_at.as_str(),
                record.updated_at.as_str(),
            ),
        )
        .map_err(|e| DomainError::internal(format!("team task insert failed: {e}")))
        .map(|_| ())
    }

    fn list_tasks(conn: &Connection, team_id: &str) -> Result<Vec<TeamTaskRecord>, DomainError> {
        let mut stmt = conn
            .prepare("SELECT id, team_id, assignee_id, description, status, created_at, updated_at FROM team_tasks WHERE team_id = ?1 ORDER BY created_at DESC")
            .map_err(|e| DomainError::internal(format!("team task query prepare failed: {e}")))?;

        let rows = stmt
            .query_map([team_id], |row| {
                Ok(TeamTaskRecord {
                    id: row.get(0)?,
                    team_id: row.get(1)?,
                    assignee_id: row.get(2)?,
                    description: row.get(3)?,
                    status: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })
            .map_err(|e| DomainError::internal(format!("team task query failed: {e}")))?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| DomainError::internal(format!("team task row read failed: {e}")))?);
        }
        Ok(out)
    }

    fn update_status(conn: &Connection, task_id: &str, status: &str) -> Result<(), DomainError> {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default()
            .to_string();
        conn.execute(
            "UPDATE team_tasks SET status = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params!(status, now, task_id),
        )
        .map_err(|e| DomainError::internal(format!("team task update failed: {e}")))
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DomainError;

    fn in_memory_conn() -> Result<Connection, DomainError> {
        let conn = Connection::open_in_memory().map_err(|e| DomainError::internal(format!("open in-memory failed: {e}")))?;
        SqliteTeamTaskRepository::ensure_schema(&conn)?;
        Ok(conn)
    }

    #[test]
    fn insert_and_list_team_tasks() {
        let conn = in_memory_conn().expect("in-memory db");
        let record = TeamTaskRecord {
            id: "t1".into(),
            team_id: "team-1".into(),
            assignee_id: "agent-1".into(),
            description: "Build feature".into(),
            status: "pending".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        };

        SqliteTeamTaskRepository::insert_task(&conn, &record).expect("insert task");
        let list = SqliteTeamTaskRepository::list_tasks(&conn, "team-1").expect("list tasks");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "t1");
    }

    #[test]
    fn team_task_schema_is_idempotent() {
        let conn = in_memory_conn().expect("in-memory db");
        SqliteTeamTaskRepository::ensure_schema(&conn).expect("schema");
        SqliteTeamTaskRepository::ensure_schema(&conn).expect("schema again");
    }
}
