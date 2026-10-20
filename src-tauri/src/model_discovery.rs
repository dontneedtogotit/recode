//! Live model discovery.
//!
//! Queries a provider's model-catalog endpoint (OpenAI-compatible
//! `/v1/models` by default) and upserts every model it lists into a
//! [`ModelCatalog`], so users can pick from whatever the provider actually
//! offers — not a fixed hardcoded set.
//!
//! Security mirrors [`crate::gateway::openai::OpenAICompatibleAdapter`]:
//!
//!   - remote endpoints must use `https`;
//!   - plain `http` is only allowed for loopback (localhost) endpoints;
//!   - a key is sent as a bearer token, and is resolved from (in order): the
//!     explicit `api_key` argument, the locally-stored credential, then the
//!     provider's environment variable. The key never appears in a returned
//!     `ModelInfo`.
//!
//! Synchronous (`reqwest::blocking`) to match the crate.
use crate::model_catalog::{ModelCatalog, ModelInfo};
use std::time::Duration;

/// Per-provider connection settings: where the catalog endpoint lives and how
/// it's shaped. `models_path` is the path appended to the scheme+host of the
/// provider's configured base URL.
#[derive(Clone, Copy)]
pub struct ProviderEndpoint {
    pub default_base: &'static str,
    pub models_path: &'static str,
    pub env_var: &'static str,
    /// If the endpoint is OpenAI-compatible `/v1/models`, true; if it returns
    /// `{"models":[...]}` under a different wrapper, handled by `parse_models`.
    pub openai_compatible: bool,
}

pub const ENDPOINTS: &[ProviderEndpoint] = &[
    ProviderEndpoint {
        default_base: "https://api.openai.com/v1",
        models_path: "/v1/models",
        env_var: "RECODE_OPENAI_API_KEY",
        openai_compatible: true,
    },
    ProviderEndpoint {
        default_base: "https://openrouter.ai/api",
        // Full path from the host root: `models_url` keeps only scheme+host, so
        // the entire API prefix must live here (not in default_base).
        models_path: "/api/v1/models",
        env_var: "RECODE_OPENROUTER_API_KEY",
        openai_compatible: true,
    },
    ProviderEndpoint {
        default_base: "https://api.deepseek.com",
        models_path: "/models",
        env_var: "RECODE_DEEPSEEK_API_KEY",
        openai_compatible: true,
    },
    ProviderEndpoint {
        default_base: "https://api.x.ai/v1",
        models_path: "/v1/models",
        env_var: "RECODE_GROK_API_KEY",
        openai_compatible: true,
    },
    ProviderEndpoint {
        default_base: "https://api.mistral.ai",
        models_path: "/v1/models",
        env_var: "RECODE_MISTRAL_API_KEY",
        openai_compatible: true,
    },
    ProviderEndpoint {
        default_base: "https://api.anthropic.com",
        models_path: "/v1/models",
        env_var: "RECODE_ANTHROPIC_API_KEY",
        openai_compatible: false,
    },
];

pub fn endpoint_for(provider: &str) -> Option<ProviderEndpoint> {
    let endpoint = match provider {
        "openai" => ENDPOINTS[0],
        "openrouter" => ENDPOINTS[1],
        "deepseek" => ENDPOINTS[2],
        "grok" => ENDPOINTS[3],
        "mistral" => ENDPOINTS[4],
        "anthropic" => ENDPOINTS[5],
        _ => return None,
    };
    Some(endpoint)
}

/// Resolve the API key for a provider: explicit -> stored credential -> env.
/// Never returns a stored key across IPC (local resolution only).
pub fn resolve_api_key(provider: &str, explicit: Option<&str>, db_path: &str) -> Option<String> {
    if let Some(k) = explicit {
        let k = k.trim();
        if !k.is_empty() {
            return Some(k.to_string());
        }
    }
    if let Some(k) = crate::provider_credentials::load_api_key(db_path, provider) {
        if !k.trim().is_empty() {
            return Some(k);
        }
    }
    if let Some(env) = endpoint_for(provider).and_then(|e| std::env::var(e.env_var).ok()) {
        if !env.trim().is_empty() {
            return Some(env);
        }
    }
    None
}

/// Build the models endpoint URL from a configured base URL (e.g.
/// `https://api.openai.com/v1`) by keeping scheme+host and appending the
/// provider's `models_path`.
pub fn models_url(base: &str, models_path: &str) -> String {
    let base = base.trim_end_matches('/');
    let after_scheme = base
        .strip_prefix("https://")
        .or_else(|| base.strip_prefix("http://"))
        .unwrap_or(base);
    let host = after_scheme
        .split_once('/')
        .map(|(h, _)| h)
        .unwrap_or(after_scheme);
    let scheme = if base.starts_with("https://") {
        "https"
    } else {
        "http"
    };
    format!("{scheme}://{host}{models_path}")
}

fn is_loopback(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1" || host == "::1" || host.starts_with("127.")
}

/// Enforce the same transport security as the OpenAI adapter.
fn validate_url(url: &str) -> Result<(), String> {
    let u = url.parse::<reqwest::Url>().map_err(|e| e.to_string())?;
    let loopback = is_loopback(u.host_str().unwrap_or(""));
    if u.scheme() != "https" && !(u.scheme() == "http" && loopback) {
        return Err("remote endpoints must use https (http only allowed for loopback)".into());
    }
    Ok(())
}

/// Parse a models-listing response body into bare model ids. Handles both the
/// OpenAI `{"data":[{"id":...}]}` and the generic `{"models":[{"id":...}]}` /
/// `{"models":["..."]}` shapes.
pub fn parse_model_ids(body: &serde_json::Value) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for array_key in ["data", "models"] {
        if let Some(arr) = body.get(array_key).and_then(|v| v.as_array()) {
            for item in arr {
                if let Some(id) = item
                    .get("id")
                    .or_else(|| item.get("model"))
                    .and_then(|v| v.as_str())
                {
                    ids.push(id.to_string());
                } else if let Some(s) = item.as_str() {
                    ids.push(s.to_string());
                }
            }
            if !ids.is_empty() {
                return ids;
            }
        }
    }
    ids
}

/// The outcome of a discovery run for one provider.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryResult {
    pub provider: String,
    pub ok: bool,
    pub count: usize,
    pub model_ids: Vec<String>,
    pub error: Option<String>,
}

/// Discover models for a provider and upsert them into `catalog`.
pub fn discover_provider(
    provider: &str,
    catalog: &ModelCatalog,
    base_url: Option<&str>,
    api_key: Option<&str>,
    db_path: &str,
) -> DiscoveryResult {
    let ep = match endpoint_for(provider) {
        Some(ep) => ep,
        None => {
            return DiscoveryResult {
                provider: provider.into(),
                ok: false,
                count: 0,
                model_ids: vec![],
                error: Some(format!("no discovery endpoint known for provider '{provider}'")),
            };
        }
    };

    let base = base_url
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty())
        .or_else(|| crate::provider_credentials::load_config(db_path, provider).map(|c| c.0).flatten())
        .unwrap_or_else(|| ep.default_base.to_string());

    let url = models_url(&base, ep.models_path);
    if let Err(e) = validate_url(&url) {
        return DiscoveryResult {
            provider: provider.into(),
            ok: false,
            count: 0,
            model_ids: vec![],
            error: Some(e),
        };
    }

    let client = match reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return DiscoveryResult {
                provider: provider.into(),
                ok: false,
                count: 0,
                model_ids: vec![],
                error: Some(format!("http client: {e}")),
            };
        }
    };

    let key = resolve_api_key(provider, api_key, db_path);
    let mut request = client.get(&url);
    if let Some(k) = &key {
        request = request.bearer_auth(k);
    }
    // Anthropic-style endpoints identify via a header; harmless elsewhere.
    if provider == "anthropic" {
        if let Some(k) = &key {
            request = request.header("x-api-key", k);
        }
    }

    let response = match request.send() {
        Ok(r) => r,
        Err(e) => {
            return DiscoveryResult {
                provider: provider.into(),
                ok: false,
                count: 0,
                model_ids: vec![],
                error: Some(format!("request failed: {e}")),
            };
        }
    };

    let status = response.status();
    if !status.is_success() {
        let text = response.text().unwrap_or_default();
        let short = text.chars().take(200).collect::<String>();
        return DiscoveryResult {
            provider: provider.into(),
            ok: false,
            count: 0,
            model_ids: vec![],
            error: Some(format!("HTTP {status}: {short}")),
        };
    }

    let body: serde_json::Value = match response.json() {
        Ok(v) => v,
        Err(e) => {
            return DiscoveryResult {
                provider: provider.into(),
                ok: false,
                count: 0,
                model_ids: vec![],
                error: Some(format!("invalid JSON: {e}")),
            };
        }
    };

    let ids = parse_model_ids(&body);
    let mut upserted: Vec<String> = Vec::new();
    for id in &ids {
        let info: ModelInfo = catalog
            .get(&format!("{provider}/{id}"))
            .unwrap_or_else(|| crate::model_catalog::discovered_default(provider, id));
        catalog.upsert(info.clone());
        upserted.push(info.id);
    }

    DiscoveryResult {
        provider: provider.into(),
        ok: true,
        count: upserted.len(),
        model_ids: upserted,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_url_strips_trailing_v1() {
        assert_eq!(
            models_url("https://api.openai.com/v1", "/v1/models"),
            "https://api.openai.com/v1/models"
        );
        assert_eq!(
            models_url("https://openrouter.ai/api", "/api/v1/models"),
            "https://openrouter.ai/api/v1/models"
        );
        assert_eq!(
            models_url("http://127.0.0.1:8080/v1/", "/v1/models"),
            "http://127.0.0.1:8080/v1/models"
        );
    }

    #[test]
    fn parse_handles_openai_shape() {
        let v = serde_json::json!({"data":[{"id":"a"},{"id":"b"}]});
        assert_eq!(parse_model_ids(&v), vec!["a", "b"]);
    }

    #[test]
    fn parse_handles_models_array_shape() {
        let v = serde_json::json!({"models":[{"id":"x"},{"model":"y"}]});
        assert_eq!(parse_model_ids(&v), vec!["x", "y"]);
    }

    #[test]
    fn parse_handles_string_models() {
        let v = serde_json::json!({"models":["p","q"]});
        assert_eq!(parse_model_ids(&v), vec!["p", "q"]);
    }

    #[test]
    fn validate_rejects_remote_http() {
        assert!(validate_url("http://example.com/v1/models").is_err());
        assert!(validate_url("https://example.com/v1/models").is_ok());
        assert!(validate_url("http://127.0.0.1:8080/v1/models").is_ok());
    }

    #[test]
    fn endpoint_for_known_providers() {
        assert!(endpoint_for("openai").is_some());
        assert!(endpoint_for("openrouter").is_some());
        assert!(endpoint_for("totally-unknown").is_none());
    }

    #[test]
    fn discovery_upserts_into_catalog() {
        let catalog = ModelCatalog::new();
        // No network here: verify the id + upsert path with a manual parse.
        let v = serde_json::json!({"data":[{"id":"brand-new"}]});
        let ids = parse_model_ids(&v);
        for id in &ids {
            let info = crate::model_catalog::discovered_default("openai", id);
            catalog.upsert(info.clone());
        }
        assert!(catalog.get("openai/brand-new").is_some());
        assert!(catalog.get("openai/brand-new").unwrap().discovered);
    }
}
