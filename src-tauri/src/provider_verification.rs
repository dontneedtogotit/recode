//! Provider key verification and model concurrency calibration.
//!
//! Verification issues a cheap authenticated GET against the provider's
//! models endpoint (the same probe `model_discovery` uses) and classifies the
//! outcome. Calibration runs a ladder of concurrent chat probes and stops as
//! soon as the provider rate-limits, deriving the highest cleanly-sustained
//! concurrency for the model.
//!
//! All calls are synchronous (`std::thread` + `reqwest::blocking`) — this
//! crate has no tokio runtime. API keys are resolved server-side from the
//! local credential store (or env) and never appear in results or errors.

use crate::model_discovery::{endpoint_for, models_url, resolve_api_key};
use crate::provider_credentials::load_config;
use serde::{Deserialize, Serialize};

/// Timeout for verification GET probes (seconds).
const VERIFY_TIMEOUT_SECS: u64 = 20;
/// Per-request timeout for calibration chat probes (seconds).
const CALIBRATION_TIMEOUT_SECS: u64 = 30;
/// Concurrency ladder: fire `level` probes in parallel per rung.
const LADDER: &[usize] = &[2, 4, 8, 12, 16, 20, 24, 32];
/// Abort a rung when more than this fraction of its probes fail.
const MAX_FAILURE_RATE: f64 = 0.30;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn is_loopback(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1" || host == "::1" || host.starts_with("127.")
}

/// Enforce the same transport rule as the discovery probe and OpenAI adapter:
/// HTTPS for remote hosts, HTTP only for loopback.
fn ensure_https_or_loopback(url: &str) -> Result<(), String> {
    let u = url.parse::<reqwest::Url>().map_err(|e| e.to_string())?;
    let loopback = is_loopback(u.host_str().unwrap_or(""));
    if u.scheme() != "https" && !(u.scheme() == "http" && loopback) {
        return Err("remote endpoints must use https (http only allowed for loopback)".into());
    }
    Ok(())
}

/// Configured base URL for a provider, else the discovery table default.
fn resolve_base_url(provider: &str, db_path: &str) -> Option<String> {
    load_config(db_path, provider).and_then(|(base, _)| base)
}

// ---------------------------------------------------------------------------
// Key verification
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyVerificationResult {
    pub provider: String,
    /// `valid` | `invalid` | `transport_error`
    pub status: String,
    pub message: String,
    pub model_count: usize,
    pub checked_at_ms: u64,
}

/// Issue an authenticated GET against the provider's models endpoint.
/// Mirrors the discovery probe: bearer auth, x-api-key for anthropic.
fn get_models(url: &str, provider: &str, key: &str) -> Result<(u16, serde_json::Value), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(VERIFY_TIMEOUT_SECS))
        .build()
        .map_err(|e| e.to_string())?;
    let mut request = client.get(url);
    if provider == "anthropic" {
        request = request
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01");
    } else {
        request = request.header("Authorization", format!("Bearer {key}"));
    }
    let response = request.send().map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let body: serde_json::Value = response.json().unwrap_or(serde_json::Value::Null);
    Ok((status, body))
}

/// Verify a provider's stored API key by probing its models endpoint.
/// The key is resolved server-side and never embedded in the result.
pub fn verify_provider_key(provider: &str, db_path: &str) -> KeyVerificationResult {
    let provider = provider.trim();
    let checked_at_ms = now_ms();
    let fail = |status: &str, message: String, model_count: usize| KeyVerificationResult {
        provider: provider.to_string(),
        status: status.into(),
        message,
        model_count,
        checked_at_ms,
    };
    let ep = match endpoint_for(provider) {
        Some(ep) => ep,
        None => {
            return fail(
                "invalid",
                format!("no known endpoint for provider '{provider}'"),
                0,
            )
        }
    };
    let key = match resolve_api_key(provider, None, db_path) {
        Some(k) => k,
        None => {
            return fail(
                "invalid",
                "no API key stored for this provider (and none in the environment)".into(),
                0,
            )
        }
    };
    let base = resolve_base_url(provider, db_path).unwrap_or_else(|| ep.default_base.to_string());
    let url = models_url(&base, ep.models_path);
    if let Err(e) = ensure_https_or_loopback(&url) {
        return fail("transport_error", e, 0);
    }
    match get_models(&url, provider, &key) {
        Ok((status, body)) => {
            let (classification, message) = match status {
                200..=299 => ("valid", format!("key accepted (HTTP {status})")),
                401 | 403 => ("invalid", format!("key rejected (HTTP {status})")),
                429 => (
                    "transport_error",
                    "provider is rate limiting (HTTP 429)".into(),
                ),
                _ => (
                    "transport_error",
                    format!("provider responded with HTTP {status}"),
                ),
            };
            let model_count = if classification == "valid" {
                crate::model_discovery::parse_model_ids(&body).len()
            } else {
                0
            };
            fail(classification, message, model_count)
        }
        Err(e) => fail("transport_error", format!("network error: {e}"), 0),
    }
}

// ---------------------------------------------------------------------------
// Concurrency calibration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConcurrencyCalibrationResult {
    pub provider: String,
    pub model_id: String,
    pub ok: bool,
    /// Highest rung that completed with <= 30% failures and no rate limit.
    /// 0 means even the first rung was rate-limited or fully failed.
    pub limit: usize,
    pub levels_tested: usize,
    pub stopped_reason: String,
    pub checked_at_ms: u64,
}

/// Chat-endpoint URL for a provider: OpenAI-compatible providers use
/// `/v1/chat/completions`; anthropic uses `/v1/messages`.
fn chat_url(
    provider: &str,
    db_path: &str,
    ep: &crate::model_discovery::ProviderEndpoint,
) -> String {
    let base = resolve_base_url(provider, db_path).unwrap_or_else(|| ep.default_base.to_string());
    if ep.openai_compatible {
        models_url(&base, "/v1/chat/completions")
    } else {
        models_url(&base, "/v1/messages")
    }
}

/// One probe outcome.
enum ProbeOutcome {
    /// HTTP response received (any status).
    Status(u16),
    /// Request failed to complete (timeout, connection error).
    Failed,
}

/// A single minimal chat probe. `max_tokens: 5`, distinct `ping {i}` payload.
fn probe_chat(
    url: &str,
    provider: &str,
    key: &str,
    model_id: &str,
    probe_index: usize,
) -> ProbeOutcome {
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(CALIBRATION_TIMEOUT_SECS))
        .build()
    {
        Ok(c) => c,
        Err(_) => return ProbeOutcome::Failed,
    };
    let payload = serde_json::json!({
        "model": model_id,
        "messages": [
            {"role": "user", "content": format!("ping {probe_index} — reply with the single word: ok")}
        ],
        "max_tokens": 5,
    });
    let mut request = client.post(url).json(&payload);
    if provider == "anthropic" {
        request = request
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01");
    } else {
        request = request.header("Authorization", format!("Bearer {key}"));
    }
    match request.send() {
        Ok(resp) => ProbeOutcome::Status(resp.status().as_u16()),
        Err(_) => ProbeOutcome::Failed,
    }
}

/// True if the HTTP status signals the provider's rate/concurrency limit.
fn is_rate_limit_status(status: u16) -> bool {
    status == 429
}

/// Calibrate the maximum concurrency for a provider+model.
///
/// Walks the ladder [2, 4, 8, 12, 16, 20, 24, 32]. At each rung, fire `level`
/// minimal chat probes in parallel via `std::thread` and join them. A rung
/// passes when no probe hit a rate limit and at most 30% of probes failed.
/// The first rung that fails ends the walk; the calibrated limit is the
/// highest passing rung (0 if even rung 2 failed).
pub fn calibrate_model_concurrency(
    provider: &str,
    model_id: &str,
    db_path: &str,
) -> ConcurrencyCalibrationResult {
    let provider = provider.trim();
    let model_id = model_id.trim();
    let checked_at_ms = now_ms();
    let done = |ok: bool, limit: usize, levels_tested: usize, stopped_reason: String| {
        ConcurrencyCalibrationResult {
            provider: provider.to_string(),
            model_id: model_id.to_string(),
            ok,
            limit,
            levels_tested,
            stopped_reason,
            checked_at_ms,
        }
    };
    if provider.is_empty() || model_id.is_empty() {
        return done(false, 0, 0, "provider or model id is empty".into());
    }
    let ep = match endpoint_for(provider) {
        Some(ep) => ep,
        None => {
            return done(
                false,
                0,
                0,
                format!("no known endpoint for provider '{provider}'"),
            );
        }
    };
    let key = match resolve_api_key(provider, None, db_path) {
        Some(k) => k,
        None => {
            return done(false, 0, 0, "no API key available for this provider".into());
        }
    };
    let url = chat_url(provider, db_path, &ep);
    if let Err(e) = ensure_https_or_loopback(&url) {
        return done(false, 0, 0, e);
    }

    let mut best = 0usize;
    let mut levels_tested = 0usize;
    for &level in LADDER {
        levels_tested += 1;
        let mut handles = Vec::with_capacity(level);
        for probe_index in 0..level {
            let url = url.clone();
            let provider = provider.to_string();
            let key = key.clone();
            let model_id = model_id.to_string();
            handles.push(std::thread::spawn(move || {
                probe_chat(&url, &provider, &key, &model_id, probe_index)
            }));
        }
        let mut rate_limited = false;
        let mut failures = 0usize;
        for handle in handles {
            match handle.join() {
                Ok(ProbeOutcome::Status(status)) if is_rate_limit_status(status) => {
                    rate_limited = true
                }
                Ok(ProbeOutcome::Status(_)) => {}
                _ => failures += 1,
            }
        }
        if rate_limited {
            return done(
                true,
                best,
                levels_tested,
                format!("rate limited at concurrency {level}"),
            );
        }
        let failure_rate = failures as f64 / level as f64;
        if failure_rate > MAX_FAILURE_RATE {
            return done(
                best > 0,
                best,
                levels_tested,
                format!(
                    "failure rate {:.0}% at concurrency {}",
                    failure_rate * 100.0,
                    level
                ),
            );
        }
        best = level;
    }
    done(true, best, levels_tested, "ladder completed".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladder_starts_at_two_and_ends_at_thirty_two() {
        assert_eq!(LADDER.first(), Some(&2));
        assert_eq!(LADDER.last(), Some(&32));
        assert!(LADDER.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn rate_limit_classification() {
        assert!(is_rate_limit_status(429));
        assert!(!is_rate_limit_status(200));
        assert!(!is_rate_limit_status(401));
    }

    #[test]
    fn https_enforced_for_remote() {
        assert!(ensure_https_or_loopback("https://api.openai.com/v1/models").is_ok());
        assert!(ensure_https_or_loopback("http://127.0.0.1:8080/v1/models").is_ok());
        assert!(ensure_https_or_loopback("http://api.openai.com/v1/models").is_err());
    }

    #[test]
    fn verification_result_serializes_camel_case() {
        let result = KeyVerificationResult {
            provider: "openai".into(),
            status: "valid".into(),
            message: "key accepted (HTTP 200)".into(),
            model_count: 12,
            checked_at_ms: 1_700_000_000_000,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["provider"], "openai");
        assert_eq!(json["status"], "valid");
        assert_eq!(json["modelCount"], 12);
        assert_eq!(json["checkedAtMs"], 1_700_000_000_000i64);
    }

    #[test]
    fn calibration_result_serializes_camel_case() {
        let result = ConcurrencyCalibrationResult {
            provider: "openai".into(),
            model_id: "gpt-4o-mini".into(),
            ok: true,
            limit: 8,
            levels_tested: 4,
            stopped_reason: "rate limited at concurrency 12".into(),
            checked_at_ms: 1_700_000_000_000,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["modelId"], "gpt-4o-mini");
        assert_eq!(json["stoppedReason"], "rate limited at concurrency 12");
        assert_eq!(json["levelsTested"], 4);
    }
}
