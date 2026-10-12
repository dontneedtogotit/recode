use recode::provider_settings::{ProviderSettings, ProviderSettingsRepository, SqliteProviderSettingsRepository};
use rusqlite::Connection;

#[test]
fn settings_command_persists_provider_settings() {
    let conn = Connection::open_in_memory().expect("open in-memory connection");

    let settings = ProviderSettings {
        id: "cmd-1".into(),
        provider: "openai".into(),
        model: None,
        api_key: String::new(),
        enabled: true,
    };

    SqliteProviderSettingsRepository::save_provider(&conn, &settings)
        .expect("save provider settings");

    let loaded = SqliteProviderSettingsRepository::load_providers(&conn)
        .expect("load providers");

    assert_eq!(loaded.len(), 1);
    assert!(loaded[0].api_key.is_empty());
}
