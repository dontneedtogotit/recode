use recode::provider_settings::ProviderSettings;

#[test]
fn settings_command_persists_provider_settings_to_sqlite() {
    let temp = std::env::temp_dir().join(format!("recode-settings-sqlite-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp).expect("create temp root");

    let db_path = temp.join("recode.db");
    let input = ProviderSettings {
        id: "settings-1".into(),
        provider: "openai".into(),
        api_key: String::new(),
        enabled: true,
    };

    let output = recode::provider_settings::commands::save_provider_settings_with_path(
        input,
        db_path.to_str().expect("db path"),
    )
    .expect("save settings");

    assert!(output.api_key.is_empty());
    assert!(db_path.exists(), "settings database should be created");
    std::fs::remove_dir_all(&temp).expect("remove owned test root");
}
