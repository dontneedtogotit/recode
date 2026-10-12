use crate::provider_settings::{ProviderSettings, ProviderSettingsRepository, SqliteProviderSettingsRepository};
use rusqlite::Connection;

fn in_memory_conn() -> Connection {
    Connection::open_in_memory().expect("open in-memory")
}

#[test]
fn save_and_list_provider_settings_round_trip() {
    let conn = in_memory_conn();
    let settings = ProviderSettings {
        id: "openai-1".into(),
        provider: "openai".into(),
        model: None,
        api_key: String::new(),
        enabled: true,
    };

    SqliteProviderSettingsRepository::save_provider(&conn, &settings).expect("save provider metadata");
    let loaded = SqliteProviderSettingsRepository::load_providers(&conn).expect("list providers");

    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, "openai-1");
    assert_eq!(loaded[0].provider, "openai");
    assert!(loaded[0].api_key.is_empty());
    assert!(loaded[0].enabled);
}

#[test]
fn delete_provider_removes_existing_provider() {
    let conn = in_memory_conn();
    let settings = ProviderSettings {
        id: "openai-1".into(),
        provider: "openai".into(),
        model: None,
        api_key: String::new(),
        enabled: true,
    };
    SqliteProviderSettingsRepository::save_provider(&conn, &settings).expect("save provider metadata");
    conn.execute("DELETE FROM provider_settings WHERE id = ?1", ["openai-1"]).expect("delete provider");
    let loaded = SqliteProviderSettingsRepository::load_providers(&conn).expect("list providers");
    assert!(loaded.is_empty());
}

#[test]
fn rejects_new_plaintext_api_keys_and_never_returns_legacy_keys_to_webview() {
    let conn = in_memory_conn();
    let mut settings = ProviderSettings {
        id: "openai-1".into(),
        provider: "openai".into(),
        model: None,
        api_key: "dummy-test-value".into(),
        enabled: true,
    };
    assert!(SqliteProviderSettingsRepository::save_provider(&conn, &settings).is_err());
    settings.api_key.clear();
    SqliteProviderSettingsRepository::save_provider(&conn, &settings).unwrap();
    conn.execute("UPDATE provider_settings SET api_key = 'legacy-dummy' WHERE id = 'openai-1'", []).unwrap();
    let loaded = SqliteProviderSettingsRepository::load_providers(&conn).unwrap();
    assert_eq!(loaded.len(), 1);
    assert!(loaded[0].api_key.is_empty());
}
