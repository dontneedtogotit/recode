use super::{GatewayRequest, GatewayResponse, ProviderAdapter, ProviderCapabilities, RetryPolicy};
use crate::domain::DomainError;
use reqwest::blocking::Client;
use reqwest::Url;
use std::io::Read;
use std::net::IpAddr;
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// A single non-streaming OpenAI-compatible endpoint. Secrets are process-local,
/// never serialized to the webview or persisted to the provider-settings table.
pub struct OpenAICompatibleAdapter {
    endpoint: Url,
    model: String,
    api_key: Option<String>,
    client: Client,
    retry: RetryPolicy,
}

/// One failed HTTP attempt, classified so the retry loop in `send` can decide
/// whether the failure is transient.
struct AttemptFailure {
    error: DomainError,
    /// Set when the failure came from an HTTP status code.
    status: Option<u16>,
    /// True when the transport reported a timeout; timeouts are never retried.
    timed_out: bool,
}

impl OpenAICompatibleAdapter {
    pub fn from_env() -> Result<Self, DomainError> {
        let base = std::env::var("RECODE_OPENAI_BASE_URL")
            .map_err(|_| DomainError::unavailable("RECODE_OPENAI_BASE_URL is not configured"))?;
        let model = std::env::var("RECODE_OPENAI_MODEL")
            .map_err(|_| DomainError::unavailable("RECODE_OPENAI_MODEL is not configured"))?;
        let api_key = std::env::var("RECODE_OPENAI_API_KEY").ok();
        Self::new(&base, &model, api_key)
    }

    pub fn new(base: &str, model: &str, api_key: Option<String>) -> Result<Self, DomainError> {
        Self::with_policy(base, model, api_key, RetryPolicy::default())
    }

    /// `new` with an explicit retry policy. Tests use this to pin attempt
    /// counts so the suite stays fast.
    pub fn with_policy(
        base: &str,
        model: &str,
        api_key: Option<String>,
        retry: RetryPolicy,
    ) -> Result<Self, DomainError> {
        if model.trim().is_empty() || model.len() > 200 {
            return Err(DomainError::invalid_input(
                "provider model must be non-empty and bounded",
            ));
        }
        let mut url = Url::parse(base)
            .map_err(|_| DomainError::invalid_input("invalid provider base URL"))?;
        let loopback = url.host_str().is_some_and(|host| {
            host == "localhost" || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
        });
        if (url.scheme() != "https" && !(url.scheme() == "http" && loopback))
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(DomainError::invalid_input(
                "provider URL must be HTTPS or loopback HTTP without credentials or query",
            ));
        }
        if !api_key.as_deref().is_some_and(|key| !key.is_empty()) && !loopback {
            return Err(DomainError::Unauthorized);
        }
        let path = format!("{}/", url.path().trim_end_matches('/'));
        url.set_path(&path);
        let endpoint = url
            .join("chat/completions")
            .map_err(|_| DomainError::invalid_input("invalid provider completion URL"))?;
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| DomainError::unavailable("provider HTTP client unavailable"))?;
        Ok(Self {
            endpoint,
            model: model.into(),
            api_key,
            client,
            retry,
        })
    }
}

impl OpenAICompatibleAdapter {
    /// A single HTTP attempt: send, read, parse, validate the response shape.
    /// Failures are classified for the retry loop in `send`.
    fn execute_once(
        &self,
        payload: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<GatewayResponse, AttemptFailure> {
        let transport = |error: DomainError, timed_out: bool| AttemptFailure {
            error,
            status: None,
            timed_out,
        };
        let mut builder = self.client.post(self.endpoint.clone()).json(payload);
        if let Some(key) = &self.api_key {
            builder = builder.bearer_auth(key);
        }
        let response = builder.send().map_err(|error| {
            if error.is_timeout() {
                transport(
                    DomainError::unavailable("provider request timed out"),
                    true,
                )
            } else {
                transport(DomainError::unavailable("provider request failed"), false)
            }
        })?;
        let status = response.status().as_u16();
        match status {
            200..=299 => {}
            401 | 403 => {
                return Err(AttemptFailure {
                    error: DomainError::Unauthorized,
                    status: Some(status),
                    timed_out: false,
                })
            }
            429 => {
                return Err(AttemptFailure {
                    error: DomainError::unavailable("provider rate limited the request"),
                    status: Some(status),
                    timed_out: false,
                })
            }
            _ => {
                return Err(AttemptFailure {
                    error: DomainError::unavailable(format!("provider returned HTTP {status}")),
                    status: Some(status),
                    timed_out: false,
                })
            }
        }
        let unusable = |error: DomainError| AttemptFailure {
            error,
            status: Some(status),
            timed_out: false,
        };
        let mut body = Vec::new();
        response
            .take(MAX_RESPONSE_BYTES as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|_| {
                unusable(DomainError::unavailable("provider response could not be read"))
            })?;
        if body.len() > MAX_RESPONSE_BYTES {
            return Err(unusable(DomainError::unavailable(
                "provider response exceeded the size limit",
            )));
        }
        let body: serde_json::Value = serde_json::from_slice(&body)
            .map_err(|_| unusable(DomainError::unavailable("provider returned invalid JSON")))?;
        let choice = body
            .get("choices")
            .and_then(|v| v.as_array())
            .and_then(|choices| choices.first())
            .ok_or_else(|| {
                unusable(DomainError::unavailable("provider response has no choices"))
            })?;
        if !choice
            .get("message")
            .and_then(|v| v.get("content"))
            .and_then(|v| v.as_str())
            .is_some_and(|text| !text.is_empty())
            && choice.get("finish_reason").and_then(|v| v.as_str()) != Some("length")
        {
            return Err(unusable(DomainError::unavailable(
                "provider response has no text content",
            )));
        }
        Ok(GatewayResponse { status, body })
    }
}

impl ProviderAdapter for OpenAICompatibleAdapter {
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            id: "openai-compatible".into(),
            supports_streaming: false,
            supports_tools: false,
            preferred_task_types: vec!["generation".into()],
        }
    }

    fn send(&self, request: GatewayRequest) -> Result<GatewayResponse, DomainError> {
        if !matches!(request.provider.as_str(), "openai" | "openai-compatible") {
            return Err(DomainError::invalid_input(
                "provider is not the configured OpenAI-compatible endpoint",
            ));
        }
        let mut payload =
            request.payload.as_object().cloned().ok_or_else(|| {
                DomainError::invalid_input("provider payload must be a JSON object")
            })?;
        if !payload
            .get("messages")
            .and_then(|v| v.as_array())
            .is_some_and(|v| !v.is_empty())
        {
            return Err(DomainError::invalid_input(
                "provider messages must be non-empty",
            ));
        }
        if payload.get("stream").and_then(|v| v.as_bool()) == Some(true) {
            return Err(DomainError::invalid_input(
                "streaming is not supported by this adapter",
            ));
        }
        payload.insert(
            "model".into(),
            serde_json::Value::String(self.model.clone()),
        );
        payload.insert("stream".into(), serde_json::Value::Bool(false));
        if serde_json::to_vec(&payload)
            .map_err(|_| DomainError::invalid_input("invalid provider payload"))?
            .len()
            > 1024 * 1024
        {
            return Err(DomainError::invalid_input(
                "provider payload exceeded the size limit",
            ));
        }
        let mut last_failure: Option<AttemptFailure> = None;
        for attempt in 0..self.retry.max_attempts {
            match self.execute_once(&payload) {
                Ok(response) => return Ok(response),
                Err(failure) => {
                    let retriable = attempt + 1 < self.retry.max_attempts
                        && super::is_retryable_failure(failure.status, failure.timed_out);
                    if !retriable {
                        return Err(failure.error);
                    }
                    last_failure = Some(failure);
                    let seed = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| u64::from(d.subsec_nanos()));
                    std::thread::sleep(super::jittered_delay(&self.retry, attempt, seed));
                }
            }
        }
        Err(last_failure.map_or_else(
            || DomainError::unavailable("provider request failed"),
            |failure| failure.error,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Serves one connection per status in `statuses`; the i-th connection is
    /// answered with `statuses[i]` and `body`. Returns the URL and a handle
    /// that yields every request it received.
    fn mock_statuses(
        statuses: Vec<u16>,
        body: &'static str,
    ) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut received = String::new();
            for status in statuses {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut chunk = [0u8; 4096];
                    let count = socket.read(&mut chunk).unwrap();
                    assert!(count > 0, "request ended before headers");
                    bytes.extend_from_slice(&chunk[..count]);
                    if let Some(position) = bytes.windows(4).position(|p| p == b"\r\n\r\n") {
                        break position + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&bytes[..header_end]).to_string();
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")?
                            .trim()
                            .parse()
                            .ok()
                    })
                    .unwrap();
                while bytes.len() - header_end < length {
                    let mut chunk = [0u8; 4096];
                    let count = socket.read(&mut chunk).unwrap();
                    assert!(count > 0, "request ended before body");
                    bytes.extend_from_slice(&chunk[..count]);
                }
                let response = format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                socket.write_all(response.as_bytes()).unwrap();
                received.push_str(&format!(
                    "{headers}{}",
                    String::from_utf8_lossy(&bytes[header_end..header_end + length])
                ));
            }
            received
        });
        (url, handle)
    }

    fn mock_response(status: u16, body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        mock_statuses(vec![status], body)
    }

    fn request() -> GatewayRequest {
        GatewayRequest {
            provider: "openai".into(),
            payload: serde_json::json!({"model":"forged","messages":[{"role":"user","content":"build app"}]}),
            task_type: Some("generation".into()),
        }
    }

    fn single_attempt_policy() -> RetryPolicy {
        RetryPolicy {
            max_attempts: 1,
            ..RetryPolicy::default()
        }
    }

    fn success_body() -> &'static str {
        r#"{"choices":[{"message":{"content":"export {};"},"finish_reason":"stop"}]}"#
    }

    #[test]
    fn sends_real_request_to_configured_loopback_and_keeps_model_authoritative() {
        let (url, server) = mock_response(200, success_body());
        let adapter = OpenAICompatibleAdapter::new(&url, "selected-model", None).unwrap();
        let result = adapter.send(request()).unwrap();
        assert_eq!(
            result.body["choices"][0]["message"]["content"],
            "export {};"
        );
        let received = server.join().unwrap();
        assert!(received.contains("POST /v1/chat/completions"));
        assert!(received.contains("\"model\":\"selected-model\""));
        assert!(!received.contains("forged"));
    }

    #[test]
    fn rejects_remote_plain_http_and_reports_rate_limit_without_body() {
        assert!(
            OpenAICompatibleAdapter::new("http://example.com/v1", "test", Some("test".into()))
                .is_err()
        );
        let (url, server) = mock_response(429, "sensitive body");
        let adapter =
            OpenAICompatibleAdapter::with_policy(&url, "test", None, single_attempt_policy())
                .unwrap();
        let error = adapter.send(request()).unwrap_err().to_string();
        assert!(error.contains("rate limited"));
        assert!(!error.contains("sensitive body"));
        server.join().unwrap();
    }

    #[test]
    fn retries_server_errors_until_success_within_attempt_budget() {
        let (url, server) = mock_statuses(vec![503, 503, 200], success_body());
        let adapter = OpenAICompatibleAdapter::new(&url, "test", None).unwrap();
        let result = adapter.send(request()).unwrap();
        assert_eq!(result.status, 200);
        let received = server.join().unwrap();
        assert_eq!(received.matches("POST /v1/chat/completions").count(), 3);
    }

    #[test]
    fn stops_retrying_when_attempt_budget_is_exhausted() {
        let (url, server) = mock_statuses(vec![429, 429], success_body());
        let adapter = OpenAICompatibleAdapter::with_policy(
            &url,
            "test",
            None,
            RetryPolicy {
                max_attempts: 2,
                ..RetryPolicy::default()
            },
        )
        .unwrap();
        let error = adapter.send(request()).unwrap_err().to_string();
        assert!(error.contains("rate limited"));
        let received = server.join().unwrap();
        assert_eq!(received.matches("POST /v1/chat/completions").count(), 2);
    }

    #[test]
    fn never_retries_authentication_failures() {
        // Only one connection is served: a second adapter attempt would hit a
        // refused connection and surface a different error, failing this test.
        let (url, server) = mock_response(401, "sensitive body");
        let adapter = OpenAICompatibleAdapter::new(&url, "test", None).unwrap();
        let error = adapter.send(request()).unwrap_err().to_string();
        assert!(error.contains("unauthorized"));
        assert!(!error.contains("sensitive body"));
        server.join().unwrap();
    }
}
