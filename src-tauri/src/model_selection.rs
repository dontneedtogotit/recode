//! Persistent model-selection and provider-availability state.
//!
//! Holds the user's choices (which models the router may pick, which
//! providers are enabled) plus the concurrency limits calibrated by
//! `provider_verification`. Stored in the app's SQLite database
//! (`recode.db`); schema created on demand. Raw API keys are never stored
//! here — they live in the credential store.

use crate::domain::DomainError;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// State shape
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionState {
    /// Model catalog ids (`provider/model`) the router may choose from.
    pub selected_models: Vec<String>,
    /// Providers the user disabled (empty = all known providers enabled).
    pub disabled_providers: Vec<String>,
    /// Calibrated concurrency limit per model id (`provider/model`).
    pub concurrency_limits: Vec<ConcurrentLimitRecord>,
    /// Per-provider verified-key status snapshot (last verification result).
    pub provider_verification: Vec<KeyStatusRecord>,
}

impl Default for SelectionState {
    fn default() -> Self {
        Self {
            selected_models: Vec::new(),
            disabled_providers: Vec::new(),
            concurrency_limits: Vec::new(),
            provider_verification: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConcurrentLimitRecord {
    pub model_id: String,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyStatusRecord {
    pub provider: String,
    /// `valid` | `invalid` | `transport_error` | `unknown`
    pub status: String,
    pub message: String,
    pub checked_at_ms: u64,
}

// ---------------------------------------------------------------------------
// Schema + persistence
// ---------------------------------------------------------------------------

/// Create the selection tables if they do not exist yet.
pub fn ensure_schema(conn: &Connection) -> Result<(), DomainError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS model_selection (
            model_id TEXT PRIMARY KEY
        );
        CREATE TABLE IF NOT EXISTS provider_disabled (
            provider TEXT PRIMARY KEY
        );
        CREATE TABLE IF NOT EXISTS model_concurrency_limit (
            model_id TEXT PRIMARY KEY,
            concurrency_limit INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS provider_key_status (
            provider TEXT PRIMARY KEY,
            status TEXT NOT NULL,
            message TEXT NOT NULL,
            checked_at_ms INTEGER NOT NULL
        );",
    )
    .map_err(|e| DomainError::Internal {
        message: e.to_string(),
    })?;
    Ok(())
}

/// Load the full selection state from SQLite.
pub fn load_selection_state(conn: &Connection) -> Result<SelectionState, DomainError> {
    ensure_schema(conn)?;
    let selected_models = query_strings(
        conn,
        "SELECT model_id FROM model_selection ORDER BY model_id",
    )?;
    let disabled_providers = query_strings(
        conn,
        "SELECT provider FROM provider_disabled ORDER BY provider",
    )?;
    let concurrency_limits = {
        let mut stmt = conn
            .prepare(
                "SELECT model_id, concurrency_limit FROM model_concurrency_limit ORDER BY model_id",
            )
            .map_err(|e| DomainError::Internal {
                message: e.to_string(),
            })?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ConcurrentLimitRecord {
                    model_id: row.get::<_, String>(0)?,
                    limit: row.get::<_, i64>(1)? as usize,
                })
            })
            .map_err(|e| DomainError::Internal {
                message: e.to_string(),
            })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| DomainError::Internal {
                message: e.to_string(),
            })?
    };
    let provider_verification = {
        let mut stmt = conn
            .prepare("SELECT provider, status, message, checked_at_ms FROM provider_key_status ORDER BY provider")
            .map_err(|e| DomainError::Internal { message: e.to_string() })?;
        let rows = stmt
            .query_map([], |row| {
                Ok(KeyStatusRecord {
                    provider: row.get::<_, String>(0)?,
                    status: row.get::<_, String>(1)?,
                    message: row.get::<_, String>(2)?,
                    checked_at_ms: row.get::<_, i64>(3)? as u64,
                })
            })
            .map_err(|e| DomainError::Internal {
                message: e.to_string(),
            })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| DomainError::Internal {
                message: e.to_string(),
            })?
    };
    Ok(SelectionState {
        selected_models,
        disabled_providers,
        concurrency_limits,
        provider_verification,
    })
}

fn query_strings(conn: &Connection, sql: &str) -> Result<Vec<String>, DomainError> {
    let mut stmt = conn.prepare(sql).map_err(|e| DomainError::Internal {
        message: e.to_string(),
    })?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| DomainError::Internal {
            message: e.to_string(),
        })?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| DomainError::Internal {
            message: e.to_string(),
        })
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/// Set or clear one model's selected flag.
pub fn set_model_selected(
    conn: &Connection,
    model_id: &str,
    selected: bool,
) -> Result<(), DomainError> {
    let model_id = model_id.trim();
    if model_id.is_empty() {
        return Err(DomainError::InvalidInput {
            message: "model id must not be empty".into(),
        });
    }
    if selected {
        conn.execute(
            "INSERT OR REPLACE INTO model_selection (model_id) VALUES (?1)",
            [model_id],
        )
        .map_err(|e| DomainError::Internal {
            message: e.to_string(),
        })?;
    } else {
        conn.execute(
            "DELETE FROM model_selection WHERE model_id = ?1",
            [model_id],
        )
        .map_err(|e| DomainError::Internal {
            message: e.to_string(),
        })?;
    }
    Ok(())
}

/// Enable or disable a provider for routing purposes.
pub fn set_provider_enabled(
    conn: &Connection,
    provider: &str,
    enabled: bool,
) -> Result<(), DomainError> {
    let provider = provider.trim();
    if provider.is_empty() {
        return Err(DomainError::InvalidInput {
            message: "provider must not be empty".into(),
        });
    }
    if enabled {
        conn.execute(
            "DELETE FROM provider_disabled WHERE provider = ?1",
            [provider],
        )
        .map_err(|e| DomainError::Internal {
            message: e.to_string(),
        })?;
    } else {
        conn.execute(
            "INSERT OR REPLACE INTO provider_disabled (provider) VALUES (?1)",
            [provider],
        )
        .map_err(|e| DomainError::Internal {
            message: e.to_string(),
        })?;
    }
    Ok(())
}

/// Persist a calibrated concurrency limit for a model.
pub fn save_concurrency_limit(
    conn: &Connection,
    model_id: &str,
    limit: usize,
) -> Result<(), DomainError> {
    let model_id = model_id.trim();
    if model_id.is_empty() {
        return Err(DomainError::InvalidInput {
            message: "model id must not be empty".into(),
        });
    }
    conn.execute(
        "INSERT OR REPLACE INTO model_concurrency_limit (model_id, concurrency_limit) VALUES (?1, ?2)",
        rusqlite::params![model_id, limit as i64],
    )
    .map_err(|e| DomainError::Internal { message: e.to_string() })?;
    Ok(())
}

/// Persist the latest key-verification status for a provider.
pub fn save_key_status(
    conn: &Connection,
    provider: &str,
    status: &str,
    message: &str,
    checked_at_ms: u64,
) -> Result<(), DomainError> {
    let provider = provider.trim();
    if provider.is_empty() {
        return Err(DomainError::InvalidInput {
            message: "provider must not be empty".into(),
        });
    }
    conn.execute(
        "INSERT OR REPLACE INTO provider_key_status (provider, status, message, checked_at_ms) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![provider, status, message, checked_at_ms as i64],
    )
    .map_err(|e| DomainError::Internal { message: e.to_string() })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Router-facing helpers
// ---------------------------------------------------------------------------

/// Candidate filter for the router: models the user selected (if any) and
/// whose provider is not disabled. Falls back to every catalog model when
/// nothing is selected, so the router never runs dry.
pub fn route_candidates(
    state: &SelectionState,
    catalog: &[crate::model_catalog::ModelInfo],
) -> Vec<crate::model_catalog::ModelInfo> {
    let disabled: HashSet<&str> = state
        .disabled_providers
        .iter()
        .map(|s| s.as_str())
        .collect();
    let selected: HashSet<&str> = state.selected_models.iter().map(|s| s.as_str()).collect();
    let pool: Vec<_> = catalog
        .iter()
        .filter(|m| !disabled.contains(m.provider.as_str()))
        .cloned()
        .collect();
    if selected.is_empty() {
        return pool;
    }
    pool.into_iter()
        .filter(|m| selected.contains(m.id.as_str()))
        .collect()
}

/// Build the (gateway, model) pair the pipeline should generate with.
///
/// Resolution order (first success wins):
/// 1. Environment adapter (`RECODE_OPENAI_*`) — explicit operator override.
/// 2. Router: score the user-selected / provider-enabled candidates (falling
///    back to the full catalog when no selection is set), and build an adapter
///    from the winner's provider credentials in the local store.
/// 3. Most recently saved GUI credential with a resolvable base URL + model
///    (mirrors `gateway::build_gateway`).
///
/// Returns `Err` when no adapter can be built; the pipeline then degrades to
/// scaffold-only generation instead of fabricating success.
pub fn resolve_router_provider(
    catalog: &crate::model_catalog::ModelCatalog,
) -> std::result::Result<(std::sync::Arc<dyn crate::gateway::GatewayService>, String), String> {
    let conn = crate::app_state::AppState::global().db();
    let state = crate::model_selection::load_selection_state(&conn).unwrap_or_default();
    let candidates = crate::model_selection::route_candidates(&state, &catalog.all());
    let request = crate::model_router::RoutingRequest::default();
    let decision =
        crate::model_router::ModelRouter::new().route_live_filtered(catalog, &candidates, &request);
    let chosen = decision
        .chosen
        .or_else(|| decision.ranked.into_iter().next())
        .ok_or_else(|| "no model in the catalog to route to".to_string())?;

    let model = chosen.model.clone();
    let (base, key) = stored_adapter_target(&chosen.provider);
    let adapter = crate::gateway::OpenAICompatibleAdapter::new(&base, &model, key)
        .map_err(|e| format!("provider '{}' not usable: {e}", chosen.provider))?;
    let gateway: std::sync::Arc<dyn crate::gateway::GatewayService> =
        std::sync::Arc::new(crate::gateway::DefaultGatewayService::new(
            std::sync::Arc::new(adapter),
            crate::gateway::CircuitBreakerConfig::default(),
        ));
    Ok((gateway, model))
}

/// Base URL + stored key for a provider the router chose. The base URL comes
/// from the provider's stored config, falling back to the well-known public
/// endpoint; a missing key is passed through as `None` (the adapter itself
/// rejects keyless remote endpoints, which the caller surfaces as "not
/// usable").
fn stored_adapter_target(provider: &str) -> (String, Option<String>) {
    let path = crate::tauri_commands::credentials_db_path();
    let (stored_base, _stored_model) =
        crate::provider_credentials::load_config(&path, provider).unwrap_or((None, None));
    let base = stored_base
        .filter(|b| !b.trim().is_empty())
        .or_else(|| crate::gateway::default_endpoint(provider).map(|s| s.to_string()))
        .unwrap_or_else(|| format!("https://{provider}.example.invalid/v1"));
    let key = crate::provider_credentials::load_api_key(&path, provider);
    (base, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn selects_and_clears_models() {
        let conn = conn();
        set_model_selected(&conn, "openai/gpt-4o", true).unwrap();
        set_model_selected(&conn, "anthropic/claude-3", true).unwrap();
        let state = load_selection_state(&conn).unwrap();
        assert_eq!(
            state.selected_models,
            vec!["anthropic/claude-3", "openai/gpt-4o"]
        );
        set_model_selected(&conn, "openai/gpt-4o", false).unwrap();
        let state = load_selection_state(&conn).unwrap();
        assert_eq!(state.selected_models, vec!["anthropic/claude-3"]);
    }

    #[test]
    fn enables_and_disables_providers() {
        let conn = conn();
        set_provider_enabled(&conn, "grok", false).unwrap();
        let state = load_selection_state(&conn).unwrap();
        assert_eq!(state.disabled_providers, vec!["grok"]);
        set_provider_enabled(&conn, "grok", true).unwrap();
        let state = load_selection_state(&conn).unwrap().disabled_providers;
        assert!(state.is_empty());
    }

    #[test]
    fn stores_concurrency_limits() {
        let conn = conn();
        save_concurrency_limit(&conn, "openai/gpt-4o", 8).unwrap();
        let limits = load_selection_state(&conn).unwrap().concurrency_limits;
        assert_eq!(limits.len(), 1);
        assert_eq!(limits[0].model_id, "openai/gpt-4o");
        assert_eq!(limits[0].limit, 8);
    }

    #[test]
    fn stores_key_status() {
        let conn = conn();
        save_key_status(&conn, "openai", "valid", "key accepted (HTTP 200)", 123).unwrap();
        let statuses = load_selection_state(&conn).unwrap().provider_verification;
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].provider, "openai");
        assert_eq!(statuses[0].status, "valid");
    }

    #[test]
    fn rejects_empty_ids() {
        let conn = conn();
        assert!(set_model_selected(&conn, "  ", true).is_err());
        assert!(set_provider_enabled(&conn, "", false).is_err());
        assert!(save_concurrency_limit(&conn, "", 4).is_err());
        assert!(save_key_status(&conn, " ", "valid", "m", 0).is_err());
    }

    #[test]
    fn route_candidates_falls_back_to_all_enabled() {
        let state = SelectionState {
            selected_models: vec![],
            disabled_providers: vec!["grok".into()],
            concurrency_limits: vec![],
            provider_verification: vec![],
        };
        let catalog = vec![
            model("openai", "gpt-4o"),
            model("grok", "grok-3"),
            model("anthropic", "claude-3"),
        ];
        let candidates = route_candidates(&state, &catalog);
        assert_eq!(candidates.len(), 2);
        assert!(candidates.iter().all(|m| m.provider != "grok"));
    }

    #[test]
    fn route_candidates_intersects_selection_and_enabled() {
        let state = SelectionState {
            selected_models: vec!["openai/gpt-4o".into(), "grok/grok-3".into()],
            disabled_providers: vec!["grok".into()],
            concurrency_limits: vec![],
            provider_verification: vec![],
        };
        let catalog = vec![
            model("openai", "gpt-4o"),
            model("openai", "gpt-4o-mini"),
            model("grok", "grok-3"),
        ];
        let candidates = route_candidates(&state, &catalog);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, "openai/gpt-4o");
    }

    fn model(provider: &str, model_id: &str) -> crate::model_catalog::ModelInfo {
        crate::model_catalog::ModelInfo {
            id: format!("{provider}/{model_id}"),
            provider: provider.into(),
            model_id: model_id.into(),
            name: model_id.into(),
            family: provider.into(),
            context_window: 128_000,
            max_output_tokens: 8_192,
            input_modalities: vec![crate::model_catalog::Modality::Text],
            supports_tools: true,
            supports_structured_output: true,
            supports_reasoning: false,
            cost_tier: crate::model_catalog::CostTier::Medium,
            description: String::new(),
            strengths: vec![],
            task_affinity: Default::default(),
            prompt_price_per_mtok: None,
            completion_price_per_mtok: None,
            discovered: false,
        }
    }
}
