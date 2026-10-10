use super::domain::{DomainError, Result};
use rusqlite::Connection;
use std::path::Path;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentTaskRunRecord {
    pub id: String,
    pub agent_id: String,
    pub input: String,
    pub success: bool,
    pub output: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanRecord {
    pub id: String,
    pub goal: String,
    pub steps: Vec<PlanStepRecord>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanStepRecord {
    pub id: String,
    pub description: String,
    pub status: String,
}

pub trait ProjectRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError>;
    fn insert_project(conn: &Connection, record: &ProjectRecord) -> Result<(), DomainError>;
    fn list_projects(conn: &Connection) -> Result<Vec<ProjectRecord>, DomainError>;
}

pub trait AgentTaskRunRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError>;
    fn insert_run(conn: &Connection, record: &AgentTaskRunRecord) -> Result<(), DomainError>;
    fn list_runs(conn: &Connection, agent_id: &str) -> Result<Vec<AgentTaskRunRecord>, DomainError>;
}

pub trait PlanRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError>;
    fn save_plan(conn: &Connection, record: &PlanRecord) -> Result<(), DomainError>;
    fn list_plans(conn: &Connection) -> Result<Vec<PlanRecord>, DomainError>;
}

pub struct SqliteProjectRepository;

impl ProjectRepository for SqliteProjectRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS projects (id TEXT PRIMARY KEY, name TEXT NOT NULL, path TEXT NOT NULL)",
            [],
        )
        .map_err(|e| DomainError::internal(format!("schema creation failed: {e}")))
        .map(|_| ())
    }

    fn insert_project(conn: &Connection, record: &ProjectRecord) -> Result<(), DomainError> {
        conn.execute(
            "INSERT INTO projects (id, name, path) VALUES (?1, ?2, ?3)",
            [&record.id, &record.name, &record.path],
        )
        .map_err(|e| DomainError::internal(format!("insert failed: {e}")))
        .map(|_| ())
    }

    fn list_projects(conn: &Connection) -> Result<Vec<ProjectRecord>, DomainError> {
        let mut stmt = conn
            .prepare("SELECT id, name, path FROM projects")
            .map_err(|e| DomainError::internal(format!("query prepare failed: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(ProjectRecord {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    path: row.get(2)?,
                })
            })
            .map_err(|e| DomainError::internal(format!("query failed: {e}")))?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| DomainError::internal(format!("row read failed: {e}")))?);
        }
        Ok(out)
    }
}

#[derive(Debug, Clone)]
pub struct SqliteAgentTaskRunRepository;

impl AgentTaskRunRepository for SqliteAgentTaskRunRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS agent_task_runs (id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, input TEXT NOT NULL, success INTEGER NOT NULL, output TEXT, error TEXT, duration_ms INTEGER NOT NULL, timestamp INTEGER NOT NULL)",
            [],
        )
        .map_err(|e| DomainError::internal(format!("agent task run schema creation failed: {e}")))
        .map(|_| ())
    }

    fn insert_run(conn: &Connection, record: &AgentTaskRunRecord) -> Result<(), DomainError> {
        conn.execute(
            "INSERT INTO agent_task_runs (id, agent_id, input, success, output, error, duration_ms, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params!(
                record.id.as_str(),
                record.agent_id.as_str(),
                record.input.as_str(),
                record.success,
                record.output.as_deref().unwrap_or(""),
                record.error.as_deref().unwrap_or(""),
                record.duration_ms,
                record.timestamp
            ),
        )
        .map_err(|e| DomainError::internal(format!("agent task run insert failed: {e}")))
        .map(|_| ())
    }

    fn list_runs(conn: &Connection, agent_id: &str) -> Result<Vec<AgentTaskRunRecord>, DomainError> {
        let mut stmt = conn
            .prepare("SELECT id, agent_id, input, success, output, error, duration_ms, timestamp FROM agent_task_runs WHERE agent_id = ?1 ORDER BY timestamp DESC")
            .map_err(|e| DomainError::internal(format!("agent task run query prepare failed: {e}")))?;

        let rows = stmt
            .query_map([agent_id], |row| {
                Ok(AgentTaskRunRecord {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    input: row.get(2)?,
                    success: row.get(3)?,
                    output: row.get(4)?,
                    error: row.get(5)?,
                    duration_ms: row.get(6)?,
                    timestamp: row.get(7)?,
                })
            })
            .map_err(|e| DomainError::internal(format!("agent task run query failed: {e}")))?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| DomainError::internal(format!("agent task run row read failed: {e}")))?);
        }
        Ok(out)
    }
}

#[derive(Debug, Clone)]
pub struct SqlitePlanRepository;

impl PlanRepository for SqlitePlanRepository {
    fn ensure_schema(conn: &Connection) -> Result<(), DomainError> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS plans (id TEXT PRIMARY KEY, goal TEXT NOT NULL)",
            [],
        )
        .map_err(|e| DomainError::internal(format!("plan schema creation failed: {e}")))
        .map(|_| ())
    }

    fn save_plan(conn: &Connection, record: &PlanRecord) -> Result<(), DomainError> {
        conn.execute(
            "INSERT OR REPLACE INTO plans (id, goal) VALUES (?1, ?2)",
            [&record.id, &record.goal],
        )
        .map_err(|e| DomainError::internal(format!("plan save failed: {e}")))
        .map(|_| ())
    }

    fn list_plans(conn: &Connection) -> Result<Vec<PlanRecord>, DomainError> {
        let mut stmt = conn
            .prepare("SELECT id, goal FROM plans")
            .map_err(|e| DomainError::internal(format!("plan query prepare failed: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(PlanRecord {
                    id: row.get(0)?,
                    goal: row.get(1)?,
                    steps: vec![],
                })
            })
            .map_err(|e| DomainError::internal(format!("plan query failed: {e}")))?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| DomainError::internal(format!("plan row read failed: {e}")))?);
        }
        Ok(out)
    }
}

pub fn open_recode_db(path: &Path) -> Result<Connection, DomainError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| DomainError::internal(format!("mkdir failed: {e}")))?;
    }
    let conn =
        Connection::open(path).map_err(|e| DomainError::internal(format!("open failed: {e}")))?;
    SqliteProjectRepository::ensure_schema(&conn)?;
    SqliteAgentTaskRunRepository::ensure_schema(&conn)?;
    SqlitePlanRepository::ensure_schema(&conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_conn() -> Result<Connection, DomainError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| DomainError::internal(format!("open in-memory failed: {e}")))?;
        SqliteProjectRepository::ensure_schema(&conn)?;
        SqliteAgentTaskRunRepository::ensure_schema(&conn)?;
        SqlitePlanRepository::ensure_schema(&conn)?;
        Ok(conn)
    }

    #[test]
    fn insert_and_list_projects() {
        let conn = in_memory_conn().expect("in-memory db");
        let record = ProjectRecord {
            id: "p1".into(),
            name: "Demo".into(),
            path: "/tmp/demo".into(),
        };
        SqliteProjectRepository::insert_project(&conn, &record).unwrap();
        let list = SqliteProjectRepository::list_projects(&conn).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "p1");
    }

    #[test]
    fn insert_and_list_agent_task_runs() {
        let conn = in_memory_conn().expect("in-memory db");
        let record = AgentTaskRunRecord {
            id: "r1".into(),
            agent_id: "planner".into(),
            input: "hello".into(),
            success: true,
            output: Some("ok".into()),
            error: None,
            duration_ms: 10,
            timestamp: 1,
        };
        SqliteAgentTaskRunRepository::insert_run(&conn, &record).unwrap();
        let list = SqliteAgentTaskRunRepository::list_runs(&conn, "planner").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "r1");
        assert_eq!(list[0].output.as_deref(), Some("ok"));
    }

    #[test]
    fn save_and_list_plans() {
        let conn = in_memory_conn().expect("in-memory db");
        let record = PlanRecord {
            id: "plan-1".into(),
            goal: "build landing page".into(),
            steps: vec![],
        };
        SqlitePlanRepository::save_plan(&conn, &record).unwrap();
        let list = SqlitePlanRepository::list_plans(&conn).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "plan-1");
        assert_eq!(list[0].goal, "build landing page");
    }
}
