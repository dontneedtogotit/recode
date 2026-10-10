use crate::provider_settings::{
    ProviderSettings, ProviderSettingsRepository, SqliteProviderSettingsRepository,
};
use rusqlite::Connection;

#[tauri::command]
pub fn save_provider_settings(settings: ProviderSettings) -> Result<ProviderSettings, String> {
    save_provider_settings_with_path(settings, "recode.db")
}

#[tauri::command]
pub fn load_provider_settings() -> Result<Vec<ProviderSettings>, String> {
    load_provider_settings_with_path("recode.db")
}

pub fn save_provider_settings_with_path(
    settings: ProviderSettings,
    path: &str,
) -> Result<ProviderSettings, String> {
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    SqliteProviderSettingsRepository::save_provider(&conn, &settings)
        .map_err(|e| e.to_string())?;
    Ok(settings)
}

pub fn load_provider_settings_with_path(path: &str) -> Result<Vec<ProviderSettings>, String> {
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    SqliteProviderSettingsRepository::load_providers(&conn).map_err(|e| e.to_string())
}
