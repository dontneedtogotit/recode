use crate::domain::DomainError;

pub mod commands;
#[cfg(test)]
pub mod tests;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderSettings {
    pub id: String,
    pub provider: String,
    pub api_key: String,
    pub enabled: bool,
}

pub trait ProviderSettingsRepository {
    fn save_provider(
        conn: &rusqlite::Connection,
        settings: &ProviderSettings,
    ) -> Result<(), DomainError>;
    fn load_providers(
        conn: &rusqlite::Connection,
    ) -> Result<Vec<ProviderSettings>, DomainError>;
}

#[derive(Debug, Clone)]
pub struct SqliteProviderSettingsRepository;

impl ProviderSettingsRepository for SqliteProviderSettingsRepository {
    fn save_provider(
        conn: &rusqlite::Connection,
        settings: &ProviderSettings,
    ) -> Result<(), DomainError> {
        if !settings.api_key.is_empty() {
            return Err(DomainError::invalid_input("API keys cannot be stored in SQLite; use RECODE_OPENAI_API_KEY in the app process environment"));
        }
        conn.execute(
            "CREATE TABLE IF NOT EXISTS provider_settings (id TEXT PRIMARY KEY, provider TEXT NOT NULL, api_key TEXT NOT NULL, enabled INTEGER NOT NULL)",
            [],
        )
        .map_err(|e| DomainError::internal(format!("schema failed: {e}")))?;

        conn.execute(
            "INSERT OR REPLACE INTO provider_settings (id, provider, api_key, enabled) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![&settings.id, &settings.provider, &settings.api_key, settings.enabled as i32],
        )
        .map_err(|e| DomainError::internal(format!("save provider settings failed: {e}")))?;

        Ok(())
    }

    fn load_providers(
        conn: &rusqlite::Connection,
    ) -> Result<Vec<ProviderSettings>, DomainError> {
        let mut stmt = conn
            .prepare("SELECT id, provider, enabled FROM provider_settings")
            .map_err(|e| DomainError::internal(format!("prepare failed: {e}")))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(ProviderSettings {
                    id: row.get(0)?,
                    provider: row.get(1)?,
                    api_key: String::new(),
                    enabled: row.get::<_, i32>(2)? != 0,
                })
            })
            .map_err(|e| DomainError::internal(format!("query failed: {e}")))?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}
