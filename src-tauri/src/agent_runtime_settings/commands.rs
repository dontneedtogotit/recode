use crate::agent_runtime_settings::{
    AgentRuntimeSettings, AgentRuntimeSettingsRepository, SqliteAgentRuntimeSettingsRepository,
};
use rusqlite::Connection;

#[tauri::command]
pub fn save_agent_runtime_settings(
    settings: AgentRuntimeSettings,
) -> Result<AgentRuntimeSettings, String> {
    save_agent_runtime_settings_with_path(settings, "recode.db")
}

#[tauri::command]
pub fn load_agent_runtime_settings() -> Result<AgentRuntimeSettings, String> {
    load_agent_runtime_settings_with_path("recode.db")
}

pub fn save_agent_runtime_settings_with_path(
    settings: AgentRuntimeSettings,
    path: &str,
) -> Result<AgentRuntimeSettings, String> {
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    SqliteAgentRuntimeSettingsRepository::save_settings(&conn, &settings)
        .map_err(|e| e.to_string())?;
    Ok(settings)
}

pub fn load_agent_runtime_settings_with_path(
    path: &str,
) -> Result<AgentRuntimeSettings, String> {
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    SqliteAgentRuntimeSettingsRepository::load_settings(&conn).map_err(|e| e.to_string())
}
