use super::domain::DomainError;
use super::tauri_commands::capture_diagnostics;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod openai;
pub use openai::OpenAICompatibleAdapter;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayRequest {
    pub provider: String,
    pub payload: serde_json::Value,
    pub task_type: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayResponse {
    pub status: u16,
    pub body: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub recovery_timeout: Duration,
}

/// Bounded retry policy for transient provider failures. Exponential backoff
/// with full jitter: delay is uniform in `[0, min(max_delay, base · 2^attempt)]`.
/// Timeouts are deliberately not retried: a 60 s generation timeout retried
/// three times would triple wall-clock latency without improving the outcome.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(250),
            max_delay: Duration::from_secs(4),
        }
    }
}

/// Entropy source for jitter; overridable so tests can assert exact delays.
pub type JitterSeed = u64;

/// Full-jitter delay for `attempt` (0-based). Deterministic for a fixed seed.
pub fn jittered_delay(policy: &RetryPolicy, attempt: u32, seed: JitterSeed) -> Duration {
    let exponential = policy
        .base_delay
        .saturating_mul(1u32.checked_shl(attempt.min(16)).unwrap_or(0));
    let cap = exponential.min(policy.max_delay);
    if cap.is_zero() {
        return cap;
    }
    let millis = cap.as_millis() as u64;
    // xorshift64* seeded by (seed, attempt) so retries diverge without a rand dependency.
    let mut state = seed ^ (u64::from(attempt) << 32) ^ 0x9E37_79B9_7F4A_7C15;
    state ^= state >> 12;
    state ^= state << 25;
    state ^= state >> 27;
    let random = state.wrapping_mul(0x2545_F491_4F6C_DD1D);
    Duration::from_millis(random % (millis + 1))
}

/// Whether a failure is transient and worth retrying. HTTP status based
/// failures pass the status; transport errors are retried except timeouts
/// (see type-level docs). Authentication and input errors never retry.
pub fn is_retryable_failure(status: Option<u16>, timed_out: bool) -> bool {
    if timed_out {
        return false;
    }
    match status {
        Some(429) => true,
        Some(code) if (500..600).contains(&code) => true,
        Some(_) => false,
        None => true, // transport-level failure without an HTTP status
    }
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    state: CircuitBreakerState,
    failures: u32,
    last_failure: Option<Instant>,
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failures: 0,
            last_failure: None,
            config,
        }
    }

    pub fn state(&self) -> &CircuitBreakerState {
        &self.state
    }

    pub fn record_success(&mut self) {
        self.failures = 0;
        self.state = CircuitBreakerState::Closed;
        self.last_failure = None;
    }

    pub fn record_failure(&mut self) {
        self.failures += 1;
        self.last_failure = Some(Instant::now());
        if self.failures >= self.config.failure_threshold {
            self.state = CircuitBreakerState::Open;
        }
    }

    pub fn allow_request(&mut self) -> bool {
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                if let Some(last) = self.last_failure {
                    if last.elapsed() >= self.config.recovery_timeout {
                        self.state = CircuitBreakerState::HalfOpen;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderCapabilities {
    pub id: String,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub preferred_task_types: Vec<String>,
}

pub trait ProviderAdapter: Send + Sync {
    fn capabilities(&self) -> ProviderCapabilities;
    fn send(&self, request: GatewayRequest) -> Result<GatewayResponse, DomainError>;
}

pub struct DefaultGatewayService<A: ProviderAdapter> {
    adapter: Arc<A>,
    circuit_breaker: Mutex<CircuitBreaker>,
    routing: Mutex<HashMap<String, Arc<dyn ProviderAdapter>>>,
    default_provider: Option<String>,
}

impl<A: ProviderAdapter + 'static> DefaultGatewayService<A> {
    pub fn new(adapter: Arc<A>, config: CircuitBreakerConfig) -> Self {
        Self {
            adapter,
            circuit_breaker: Mutex::new(CircuitBreaker::new(config)),
            routing: Mutex::new(HashMap::new()),
            default_provider: None,
        }
    }

    pub fn with_router(
        adapter: Arc<A>,
        config: CircuitBreakerConfig,
        routes: HashMap<String, Arc<dyn ProviderAdapter>>,
        default_provider: Option<String>,
    ) -> Self {
        Self {
            adapter,
            circuit_breaker: Mutex::new(CircuitBreaker::new(config)),
            routing: Mutex::new(routes),
            default_provider,
        }
    }

    fn resolve_adapter(&self, request: &GatewayRequest) -> Arc<dyn ProviderAdapter> {
        let task_type = request
            .task_type
            .as_ref()
            .map(|value| value.to_lowercase());

        if let Some(task_type) = task_type {
            let routes = self.routing.lock().expect("lock routing");
            if let Some(adapter) = routes.get(&task_type) {
                return adapter.clone();
            }
        }

        if let Some(provider) = request.provider.strip_prefix("router:") {
            let routes = self.routing.lock().expect("lock routing");
            if let Some(adapter) = routes.get(provider) {
                return adapter.clone();
            }
        }

        if let Some(default) = &self.default_provider {
            let routes = self.routing.lock().expect("lock routing");
            if let Some(adapter) = routes.get(default.as_str()) {
                return adapter.clone();
            }
        }

        self.adapter.clone() as Arc<dyn ProviderAdapter>
    }
}

impl<A: ProviderAdapter + 'static> GatewayService for DefaultGatewayService<A> {
    fn send(&self, request: GatewayRequest) -> Result<GatewayResponse, DomainError> {
        let adapter = self.resolve_adapter(&request);

        let mut breaker = self.circuit_breaker.lock().expect("lock circuit breaker");
        if !breaker.allow_request() {
            let _ = capture_diagnostics(super::tauri_commands::CaptureDiagnosticsInput {
                kind: "provider".into(),
                message: format!(
                    "gateway blocked by circuit breaker for provider {}",
                    request.provider
                ),
                detail: None,
            });
            return Err(DomainError::unavailable("gateway blocked by circuit breaker"));
        }
        drop(breaker);

        let result = adapter.send(request.clone());

        match &result {
            Ok(_) => {
                self.circuit_breaker
                    .lock()
                    .expect("lock circuit breaker")
                    .record_success();
            }
            Err(_) => {
                self.circuit_breaker
                    .lock()
                    .expect("lock circuit breaker")
                    .record_failure();
            }
        }

        result
    }

    fn circuit_breaker_state(&self) -> Option<CircuitBreakerState> {
        Some(self._circuit_breaker().lock().expect("lock breaker").state().clone())
    }

    fn is_configured(&self) -> bool {
        self.adapter.capabilities().id != "stub"
    }
}

pub trait GatewayService: Send + Sync {
    fn send(&self, request: GatewayRequest) -> Result<GatewayResponse, DomainError>;
    fn is_configured(&self) -> bool { true }
    fn circuit_breaker_state(&self) -> Option<CircuitBreakerState> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_monitor::{RuntimeMonitorService, tests_helpers::service as monitor_service};
    #[derive(Debug, Clone)]
    struct TestAdapter {
        capabilities: ProviderCapabilities,
    }

    impl ProviderAdapter for TestAdapter {
        fn capabilities(&self) -> ProviderCapabilities {
            self.capabilities.clone()
        }

        fn send(&self, _request: GatewayRequest) -> Result<GatewayResponse, DomainError> {
            Ok(GatewayResponse {
                status: 200,
                body: serde_json::json!({"provider": self.capabilities.id}),
            })
        }
    }

    #[test]
    fn circuit_breaker_closes_after_failures() {
        let mut breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 3,
            recovery_timeout: Duration::from_secs(1),
        });
        assert_eq!(breaker.state(), &CircuitBreakerState::Closed);
        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.state(), &CircuitBreakerState::Closed);
        breaker.record_failure();
        assert_eq!(breaker.state(), &CircuitBreakerState::Open);
    }

    #[test]
    fn circuit_breaker_blocks_requests_when_open() {
        let mut breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            recovery_timeout: Duration::from_secs(60),
        });
        breaker.record_failure();
        assert!(!breaker.allow_request());
    }

    #[test]
    fn circuit_breaker_recovers_after_timeout() {
        let mut breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            recovery_timeout: Duration::from_secs(0),
        });
        breaker.record_failure();
        assert!(breaker.allow_request());
        breaker.record_success();
        assert_eq!(breaker.state(), &CircuitBreakerState::Closed);
    }

    #[test]
    fn gateway_returns_stub_response() {
        let adapter = Arc::new(TestAdapter {
            capabilities: ProviderCapabilities {
                id: "stub".into(),
                supports_streaming: false,
                supports_tools: false,
                preferred_task_types: vec![],
            },
        });
        let service = DefaultGatewayService::new(adapter, CircuitBreakerConfig::default());
        let response = service
            .send(GatewayRequest {
                provider: "test".into(),
                payload: serde_json::json!({}),
                task_type: None,
            })
            .unwrap();
        assert_eq!(response.status, 200);
    }

    #[test]
    fn default_gateway_rejects_unconfigured_provider_instead_of_fabricating_success() {
        if default_gateway().is_configured() {
            // A developer may run tests with a real provider configured; never call it here.
            return;
        }
        let result = default_gateway().send(GatewayRequest {
            provider: "openai".into(),
            payload: serde_json::json!({"prompt": "build app"}),
            task_type: Some("generation".into()),
        });
        assert!(matches!(result, Err(DomainError::Unavailable { .. })));
    }

    #[test]
    fn gateway_captures_diagnostics_when_circuit_open() {
        let adapter = Arc::new(TestAdapter {
            capabilities: ProviderCapabilities {
                id: "test".into(),
                supports_streaming: false,
                supports_tools: false,
                preferred_task_types: vec![],
            },
        });
        let service = DefaultGatewayService::new(adapter, CircuitBreakerConfig {
            failure_threshold: 1,
            recovery_timeout: Duration::from_secs(60),
        });
        {
            let mut breaker = service
                ._circuit_breaker()
                .lock()
                .expect("lock breaker");
            breaker.record_failure();
        }

        let response = service.send(GatewayRequest {
            provider: "test".into(),
            payload: serde_json::json!({}),
            task_type: None,
        });
        assert!(response.is_err());
        let events = monitor_service().recent_events();
        assert!(
            events
                .iter()
                .any(|event| event.message.contains("gateway blocked by circuit breaker")),
            "expected gateway blocked diagnostics, got: {events:?}"
        );
    }

    #[test]
    fn gateway_routes_by_task_type() {
        let primary = Arc::new(TestAdapter {
            capabilities: ProviderCapabilities {
                id: "primary".into(),
                supports_streaming: true,
                supports_tools: false,
                preferred_task_types: vec!["chat".into()],
            },
        });
        let fallback = Arc::new(TestAdapter {
            capabilities: ProviderCapabilities {
                id: "fallback".into(),
                supports_streaming: false,
                supports_tools: true,
                preferred_task_types: vec!["tool".into()],
            },
        });

        let mut routes = HashMap::new();
        routes.insert("chat".into(), primary.clone() as Arc<dyn ProviderAdapter>);
        routes.insert("tool".into(), fallback.clone() as Arc<dyn ProviderAdapter>);

        let service = DefaultGatewayService::with_router(
            primary,
            CircuitBreakerConfig::default(),
            routes,
            Some("chat".into()),
        );

        let response = service
            .send(GatewayRequest {
                provider: "router:tool".into(),
                payload: serde_json::json!({}),
                task_type: Some("tool".into()),
            })
            .unwrap();
        assert_eq!(response.body, serde_json::json!({"provider": "fallback"}));
    }
}

impl<A: ProviderAdapter> DefaultGatewayService<A> {
    fn _circuit_breaker(&self) -> &Mutex<CircuitBreaker> {
        &self.circuit_breaker
    }
}

static DEFAULT_GATEWAY: std::sync::OnceLock<std::sync::Arc<dyn GatewayService>> = std::sync::OnceLock::new();

pub fn default_gateway() -> std::sync::Arc<dyn GatewayService> {
    DEFAULT_GATEWAY
        .get_or_init(|| {
            if let Ok(adapter) = OpenAICompatibleAdapter::from_env() {
                return std::sync::Arc::new(DefaultGatewayService::new(
                    std::sync::Arc::new(adapter),
                    CircuitBreakerConfig::default(),
                ));
            }
            struct StubAdapter;
            impl ProviderAdapter for StubAdapter {
                fn capabilities(&self) -> ProviderCapabilities {
                    ProviderCapabilities {
                        id: "stub".into(),
                        supports_streaming: false,
                        supports_tools: false,
                        preferred_task_types: vec![],
                    }
                }

                fn send(&self, _request: GatewayRequest) -> Result<GatewayResponse, DomainError> {
                    Err(DomainError::unavailable("no real provider configured"))
                }
            }

            std::sync::Arc::new(DefaultGatewayService::new(
                std::sync::Arc::new(StubAdapter),
                CircuitBreakerConfig::default(),
            ))
        })
        .clone()
}
