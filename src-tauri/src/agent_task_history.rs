use super::domain::{DomainError, Result};
use rusqlite::Connection;
use std::time::SystemTime;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentTaskHistoryRecord {
    pub id: String,
    pub agent_id: String,
    pub task_id: String,
    pub status: String,
    pub started_at: SystemTime,
    pub finished_at: Option<SystemTime>,
    pub output: Option<String>,
    pub error: Option<String>,
}

pub trait AgentTaskHistoryRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError>;
    fn insert_record(conn: &Connection, record: &AgentTaskHistoryRecord) -> Result<(), DomainError>;
    fn list_by_agent(conn: &Connection, agent_id: &str) -> Result<Vec<AgentTaskHistoryRecord>, DomainError>;
}

#[derive(Debug, Clone, Default)]
pub struct SqliteAgentTaskHistoryRepository;

impl AgentTaskHistoryRepository for SqliteAgentTaskHistoryRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS agent_task_history (id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, task_id TEXT NOT NULL, status TEXT NOT NULL, started_at INTEGER NOT NULL, finished_at INTEGER, output TEXT, error TEXT)",
            [],
        )
        .map_err(|e| DomainError::internal(format!("agent task history schema creation failed: {e}")))
        .map(|_| ())
    }

    fn insert_record(conn: &Connection, record: &AgentTaskHistoryRecord) -> Result<(), DomainError> {
        conn.execute(
            "INSERT OR REPLACE INTO agent_task_history (id, agent_id, task_id, status, started_at, finished_at, output, error) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params!(
                record.id.as_str(),
                record.agent_id.as_str(),
                record.task_id.as_str(),
                record.status.as_str(),
                record.started_at.duration_since(SystemTime::UNIX_EPOCH).expect("time went backwards").as_secs().to_string(),
                record.finished_at.map(|t| t.duration_since(SystemTime::UNIX_EPOCH).expect("time went backwards").as_secs().to_string()),
                record.output.as_deref().unwrap_or(""),
                record.error.as_deref().unwrap_or(""),
            ),
        )
        .map_err(|e| DomainError::internal(format!("agent task history insert failed: {e}")))
        .map(|_| ())
    }

    fn list_by_agent(conn: &Connection, agent_id: &str) -> Result<Vec<AgentTaskHistoryRecord>, DomainError> {
        let mut stmt = conn
            .prepare("SELECT id, agent_id, task_id, status, started_at, finished_at, output, error FROM agent_task_history WHERE agent_id = ?1 ORDER BY started_at DESC")
            .map_err(|e| DomainError::internal(format!("agent task history query prepare failed: {e}")))?;

        let rows = stmt
            .query_map([agent_id], |row| {
                Ok(AgentTaskHistoryRecord {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    task_id: row.get(2)?,
                    status: row.get(3)?,
                    started_at: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(row.get::<_, u64>(4)?),
                    finished_at: row.get::<_, Option<u64>>(5)?.map(|secs| SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs)),
                    output: row.get(6)?,
                    error: if row.get::<_, String>(7)?.is_empty() { None } else { row.get(7)? },
                })
            })
            .map_err(|e| DomainError::internal(format!("agent task history query failed: {e}")))?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| DomainError::internal(format!("agent task history row read failed: {e}")))?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DomainError;

    fn in_memory_conn() -> Result<Connection, DomainError> {
        let conn = Connection::open_in_memory().map_err(|e| DomainError::internal(format!("open in-memory failed: {e}")))?;
        SqliteAgentTaskHistoryRepository::ensure_schema(&conn)?;
        Ok(conn)
    }

    #[test]
    fn insert_and_list_history() {
        let conn = in_memory_conn().expect("in-memory db");
        let record = AgentTaskHistoryRecord {
            id: "h1".into(),
            agent_id: "planner".into(),
            task_id: "task-1".into(),
            status: "completed".into(),
            started_at: SystemTime::UNIX_EPOCH,
            finished_at: Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1)),
            output: Some("ok".into()),
            error: None,
        };

        SqliteAgentTaskHistoryRepository::insert_record(&conn, &record).expect("insert history");
        let list = SqliteAgentTaskHistoryRepository::list_by_agent(&conn, "planner").expect("list history");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "h1");
        assert_eq!(list[0].status, "completed");
    }

    #[test]
    fn history_schema_is_idempotent() {
        let conn = in_memory_conn().expect("in-memory db");
        SqliteAgentTaskHistoryRepository::ensure_schema(&conn).expect("schema");
        SqliteAgentTaskHistoryRepository::ensure_schema(&conn).expect("schema again");
    }
}
