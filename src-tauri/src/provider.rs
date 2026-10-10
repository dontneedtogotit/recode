use super::domain::DomainError;
use super::gateway::{CircuitBreaker, CircuitBreakerConfig, GatewayRequest, GatewayResponse};
use std::sync::Arc;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderCapabilities {
    pub supports_streaming: bool,
    pub supports_tools: bool,
}

pub trait ProviderAdapter {
    fn capabilities(&self) -> ProviderCapabilities;
    fn send(&self, request: GatewayRequest) -> Result<GatewayResponse, DomainError>;
}

pub struct FailingProviderAdapter;

impl ProviderAdapter for FailingProviderAdapter {
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_streaming: false,
            supports_tools: false,
        }
    }

    fn send(&self, _request: GatewayRequest) -> Result<GatewayResponse, DomainError> {
        Err(DomainError::internal("provider failure"))
    }
}

pub struct StubProviderAdapter {
    pub capabilities: ProviderCapabilities,
}

impl ProviderAdapter for StubProviderAdapter {
    fn capabilities(&self) -> ProviderCapabilities {
        self.capabilities.clone()
    }

    fn send(&self, _request: GatewayRequest) -> Result<GatewayResponse, DomainError> {
        Ok(GatewayResponse {
            status: 200,
            body: serde_json::json!({"provider": "stub"}),
        })
    }
}

pub struct DefaultGatewayService {
    adapter: Arc<dyn ProviderAdapter + Send + Sync>,
    circuit_breaker: std::sync::Mutex<CircuitBreaker>,
}

impl DefaultGatewayService {
    pub fn new(adapter: Arc<dyn ProviderAdapter + Send + Sync>, config: CircuitBreakerConfig) -> Self {
        Self {
            adapter,
            circuit_breaker: std::sync::Mutex::new(CircuitBreaker::new(config)),
        }
    }
}

impl super::gateway::GatewayService for DefaultGatewayService {
    fn send(&self, request: GatewayRequest) -> Result<GatewayResponse, DomainError> {
        let mut breaker = self.circuit_breaker.lock().unwrap();
        if !breaker.allow_request() {
            return Err(DomainError::internal("circuit breaker open"));
        }
        drop(breaker);

        let result = self.adapter.send(request.clone());

        match result {
            Ok(_) => {
                self.circuit_breaker.lock().unwrap().record_success();
            }
            Err(_) => {
                self.circuit_breaker.lock().unwrap().record_failure();
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::super::gateway::GatewayService;
    use super::*;

    #[test]
    fn provider_adapter_reports_capabilities() {
        let adapter = StubProviderAdapter {
            capabilities: ProviderCapabilities {
                supports_streaming: true,
                supports_tools: false,
            },
        };
        let caps = adapter.capabilities();
        assert!(caps.supports_streaming);
        assert!(!caps.supports_tools);
    }

    #[test]
    fn gateway_uses_adapter_response() {
        let adapter = Arc::new(StubProviderAdapter {
            capabilities: ProviderCapabilities {
                supports_streaming: false,
                supports_tools: false,
            },
        });
        let service = DefaultGatewayService::new(adapter, CircuitBreakerConfig::default());
        let response = service
            .send(GatewayRequest {
                provider: "stub".into(),
                payload: serde_json::json!({}),
                task_type: None,
            })
            .unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, serde_json::json!({"provider": "stub"}));
    }

    #[test]
    fn gateway_respects_circuit_breaker() {
        let failing_adapter = Arc::new(FailingProviderAdapter);
        let mut config = CircuitBreakerConfig::default();
        config.failure_threshold = 1;
        let service = DefaultGatewayService::new(failing_adapter, config);

        let _ = service.send(GatewayRequest {
            provider: "stub".into(),
            payload: serde_json::json!({}),
            task_type: None,
        });

        let result = service.send(GatewayRequest {
            provider: "stub".into(),
            payload: serde_json::json!({}),
            task_type: None,
        });
        assert!(result.is_err());
    }
}
