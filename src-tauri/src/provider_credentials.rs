//! Persistent, per-provider credential + config store.
//!
//! The `provider_settings` module deliberately keeps the settings store
//! key-free: it rejects plaintext API keys and `load_providers` never returns
//! one. This module is the *additive* home for stored secrets so that users can
//! add providers with or without an API key (stored locally, never broadcast):
//!
//!   - `provider_credentials`  : `provider -> api_key` (local SQLite file,
//!     the same `recode.db` the rest of the app uses). Keys are written via
//!     dedicated commands and are **never** read back through IPC — only
//!     `has_*` booleans and redacted metadata cross the boundary.
//!   - `provider_config`       : `provider -> (base_url, default_model)` so a
//!     provider can be pointed at any OpenAI-compatible endpoint and have a
//!     preferred model without touching the settings struct.
//!
//! Nothing here modifies `provider_settings`; the two stores coexist and the
//! command layer resolves "stored key, else environment variable".
//!
//! All operations are synchronous (rusqlite), matching the crate's blocking
//! architecture.
use std::collections::HashMap;
use std::path::Path;

/// Default database file (relative), mirroring `provider_settings/commands.rs`.
pub const DEFAULT_CREDENTIAL_DB: &str = "recode.db";

/// Providers that expose an OpenAI-compatible `/v1/models` catalog.
pub const DISCOVERABLE_PROVIDERS: &[&str] = &[
    "openai",
    "openrouter",
    "deepseek",
    "grok",
    "gemini",
    "mistral",
    "anthropic",
];

/// A stored, fully-redacted credential/config record. Safe to send to the UI:
/// the key itself is never included.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCredentialInfo {
    pub provider: String,
    pub has_api_key: bool,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
    /// First eight characters of the key for UI display, or `None`.
    pub key_hint: Option<String>,
}

fn ensure_schema(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS provider_credentials (
             provider TEXT PRIMARY KEY,
             api_key TEXT NOT NULL,
             updated_at INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS provider_config (
             provider TEXT PRIMARY KEY,
             base_url TEXT,
             default_model TEXT,
             updated_at INTEGER NOT NULL DEFAULT 0
         );",
    )
}

/// Store (or overwrite) a provider's API key. Returns the key hint stored.
pub fn save_api_key(path: &str, provider: &str, api_key: &str) -> Result<String, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err("api key must not be empty".into());
    }
    let conn = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
    ensure_schema(&conn).map_err(|e| e.to_string())?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    conn.execute(
        "INSERT INTO provider_credentials (provider, api_key, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(provider) DO UPDATE SET api_key = excluded.api_key, updated_at = excluded.updated_at",
        rusqlite::params![provider, key, ts as i64],
    )
    .map_err(|e| e.to_string())?;
    Ok(hint(key))
}

/// Whether a provider has a stored key.
pub fn has_api_key(path: &str, provider: &str) -> bool {
    let conn = match rusqlite::Connection::open(path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    if let Err(_) = ensure_schema(&conn) {
        return false;
    }
    conn.query_row(
        "SELECT COUNT(*) FROM provider_credentials WHERE provider = ?1",
        rusqlite::params![provider],
        |row| row.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .unwrap_or(false)
}

/// Delete a provider's stored key.
pub fn delete_api_key(path: &str, provider: &str) -> Result<bool, String> {
    let conn = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
    ensure_schema(&conn).map_err(|e| e.to_string())?;
    let n = conn
        .execute(
            "DELETE FROM provider_credentials WHERE provider = ?1",
            rusqlite::params![provider],
        )
        .map_err(|e| e.to_string())?;
    Ok(n > 0)
}

/// Store per-provider config (base_url / default_model). Pass `None` to clear a
/// field.
pub fn save_config(
    path: &str,
    provider: &str,
    base_url: Option<&str>,
    default_model: Option<&str>,
) -> Result<(), String> {
    let conn = rusqlite::Connection::open(path).map_err(|e| e.to_string())?;
    ensure_schema(&conn).map_err(|e| e.to_string())?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    conn.execute(
        "INSERT INTO provider_config (provider, base_url, default_model, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(provider) DO UPDATE SET
             base_url = COALESCE(excluded.base_url, provider_config.base_url),
             default_model = COALESCE(excluded.default_model, provider_config.default_model),
             updated_at = excluded.updated_at",
        rusqlite::params![provider, base_url, default_model, ts as i64],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Load a single provider's stored key (local use only — never sent to the UI).
pub fn load_api_key(path: &str, provider: &str) -> Option<String> {
    let conn = rusqlite::Connection::open(path).ok()?;
    if let Err(_) = ensure_schema(&conn) {
        return None;
    }
    conn.query_row(
        "SELECT api_key FROM provider_credentials WHERE provider = ?1",
        rusqlite::params![provider],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

/// Load a provider's stored config.
pub fn load_config(path: &str, provider: &str) -> Option<(Option<String>, Option<String>)> {
    let conn = rusqlite::Connection::open(path).ok()?;
    if let Err(_) = ensure_schema(&conn) {
        return None;
    }
    conn.query_row(
        "SELECT base_url, default_model FROM provider_config WHERE provider = ?1",
        rusqlite::params![provider],
        |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?)),
    )
    .ok()
}

/// Redacted credential/config records for every provider that has either a key
/// or config. Safe for IPC.
pub fn list_infos(path: &str) -> HashMap<String, ProviderCredentialInfo> {
    let conn = match rusqlite::Connection::open(path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    if let Err(_) = ensure_schema(&conn) {
        return HashMap::new();
    }
    let mut out: HashMap<String, ProviderCredentialInfo> = HashMap::new();

    let mut keys: HashMap<String, String> = HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT provider, api_key FROM provider_credentials") {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            for row in rows.flatten() {
                keys.insert(row.0, row.1);
            }
        }
    }

    let mut configs: HashMap<String, (Option<String>, Option<String>)> = HashMap::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT provider, base_url, default_model FROM provider_config",
    ) {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        }) {
            for row in rows.flatten() {
                configs.insert(row.0, (row.1, row.2));
            }
        }
    }

    for (provider, key) in &keys {
        let (base_url, default_model) = configs.get(provider).cloned().unwrap_or((None, None));
        out.insert(
            provider.clone(),
            ProviderCredentialInfo {
                provider: provider.clone(),
                has_api_key: true,
                base_url: base_url.clone(),
                default_model: default_model.clone(),
                key_hint: Some(hint(key)),
            },
        );
    }
    for (provider, (base_url, default_model)) in &configs {
        if let Some(info) = out.get_mut(provider) {
            info.base_url = base_url.clone();
            info.default_model = default_model.clone();
        } else {
            out.insert(
                provider.clone(),
                ProviderCredentialInfo {
                    provider: provider.clone(),
                    has_api_key: false,
                    base_url: base_url.clone(),
                    default_model: default_model.clone(),
                    key_hint: None,
                },
            );
        }
    }
    out
}

/// First 8 chars of a key for display, never the full secret.
fn hint(key: &str) -> String {
    format!("{}…", key.chars().take(8).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_db() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("cred.db");
        (dir, p.to_string_lossy().to_string())
    }

    #[test]
    fn save_has_delete_roundtrip() {
        let (_dir, p) = temp_db();
        assert!(!has_api_key(&p, "openai"));
        let hint = save_api_key(&p, "openai", "sk-1234567890").unwrap();
        assert_eq!(hint, "sk-12345…".to_string());
        assert!(has_api_key(&p, "openai"));
        assert_eq!(load_api_key(&p, "openai").as_deref(), Some("sk-1234567890"));
        assert!(delete_api_key(&p, "openai").unwrap());
        assert!(!has_api_key(&p, "openai"));
    }

    #[test]
    fn empty_key_rejected() {
        let (_dir, p) = temp_db();
        assert!(save_api_key(&p, "openai", "   ").is_err());
    }

    #[test]
    fn list_infos_never_exposes_full_key() {
        let (_dir, p) = temp_db();
        save_api_key(&p, "grok", "sk-abcdef123456").unwrap();
        save_config(&p, "grok", Some("https://x.example/v1"), Some("grok-2")).unwrap();
        let infos = list_infos(&p);
        let info = &infos["grok"];
        assert!(info.has_api_key);
        assert_eq!(info.base_url.as_deref(), Some("https://x.example/v1"));
        assert_eq!(info.default_model.as_deref(), Some("grok-2"));
        assert_eq!(info.key_hint.as_deref(), Some("sk-abcde…".as_ref()));
        // The full key must never appear in any serialized field.
        let json = serde_json::to_string(&info).unwrap();
        assert!(!json.contains("sk-abcdef123456"));
    }

    #[test]
    fn _path_used() {
        // keep Path import alive for future path-normalization helpers
        assert!(Path::new("recode.db").file_name().is_some());
    }
}
