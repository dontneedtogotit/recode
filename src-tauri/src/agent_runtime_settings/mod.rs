use crate::domain::DomainError;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

pub mod commands;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRuntimeSettings {
    pub max_concurrency: i32,
    pub default_timeout_ms: i32,
    pub retry_limit: i32,
}

pub trait AgentRuntimeSettingsRepository {
    fn save_settings(
        conn: &Connection,
        settings: &AgentRuntimeSettings,
    ) -> Result<(), DomainError>;
    fn load_settings(conn: &Connection) -> Result<AgentRuntimeSettings, DomainError>;
}

pub struct SqliteAgentRuntimeSettingsRepository;

impl AgentRuntimeSettingsRepository for SqliteAgentRuntimeSettingsRepository {
    fn save_settings(
        conn: &Connection,
        settings: &AgentRuntimeSettings,
    ) -> Result<(), DomainError> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS agent_runtime_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )
        .map_err(|e| DomainError::internal(format!("schema failed: {e}")))?;

        let value = serde_json::to_string(settings)
            .map_err(|e| DomainError::internal(format!("serialize failed: {e}")))?;

        conn.execute(
            "INSERT OR REPLACE INTO agent_runtime_settings (key, value) VALUES (?1, ?2)",
            rusqlite::params!["default", value],
        )
        .map_err(|e| DomainError::internal(format!("save failed: {e}")))?;

        Ok(())
    }

    fn load_settings(conn: &Connection) -> Result<AgentRuntimeSettings, DomainError> {
        let mut stmt = conn
            .prepare("SELECT value FROM agent_runtime_settings WHERE key = ?1")
            .map_err(|e| DomainError::internal(format!("prepare failed: {e}")))?;

        let value: String = stmt
            .query_row(["default"], |row| row.get(0))
            .map_err(|e| DomainError::internal(format!("query failed: {e}")))?;

        let settings = serde_json::from_str(&value)
            .map_err(|e| DomainError::internal(format!("deserialize failed: {e}")))?;

        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn in_memory_conn() -> Connection {
        Connection::open_in_memory().expect("open in-memory")
    }

    #[test]
    fn saves_and_loads_agent_runtime_settings() {
        let conn = in_memory_conn();
        let settings = AgentRuntimeSettings {
            max_concurrency: 4,
            default_timeout_ms: 120000,
            retry_limit: 5,
        };

        SqliteAgentRuntimeSettingsRepository::save_settings(&conn, &settings)
            .expect("save settings");

        let loaded = SqliteAgentRuntimeSettingsRepository::load_settings(&conn)
            .expect("load settings");

        assert_eq!(loaded.max_concurrency, 4);
        assert_eq!(loaded.default_timeout_ms, 120000);
        assert_eq!(loaded.retry_limit, 5);
    }
}
