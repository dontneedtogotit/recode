//! Provider Orchestrator
//!
//! Manages active AI provider configuration and runtime key injection.

use crate::domain::DomainError;
use crate::provider_settings::{ProviderSettings, ProviderSettingsRepository, SqliteProviderSettingsRepository};
use std::sync::Arc;

pub struct ProviderConfig {
    pub provider: String,
    pub model: String,
    pub api_key: String,
}

pub struct ProviderOrchestrator;

impl ProviderOrchestrator {
    pub fn get_active_provider(
        conn: &rusqlite::Connection,
        default_model: &str,
    ) -> Result<ProviderConfig, DomainError> {
        let providers = SqliteProviderSettingsRepository::load_providers(conn)?;
        let enabled = providers.iter().find(|p| p.enabled).ok_or_else(|| {
            DomainError::invalid_input("No enabled AI provider found.")
        })?;

        // Retrieve key from environment
        let api_key = std::env::var("RECODE_OPENAI_API_KEY").map_err(|_| {
            DomainError::unavailable("RECODE_OPENAI_API_KEY environment variable not set.")
        })?;

        Ok(ProviderConfig {
            provider: enabled.provider.clone(),
            model: enabled.model.clone().unwrap_or_else(|| default_model.to_string()),
            api_key,
        })
    }
}
