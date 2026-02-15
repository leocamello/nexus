//! Unit tests for health module.

use super::*;
use crate::registry::Model;

// ============================================================================
// T02: HealthCheckConfig Tests
// ============================================================================

#[test]
fn test_config_default_values() {
    let config = HealthCheckConfig::default();
    assert!(config.enabled);
    assert_eq!(config.interval_seconds, 30);
    assert_eq!(config.timeout_seconds, 5);
    assert_eq!(config.failure_threshold, 3);
    assert_eq!(config.recovery_threshold, 2);
}

#[test]
fn test_config_serde_roundtrip() {
    let config = HealthCheckConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    let parsed: HealthCheckConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config.interval_seconds, parsed.interval_seconds);
    assert_eq!(config.timeout_seconds, parsed.timeout_seconds);
    assert_eq!(config.failure_threshold, parsed.failure_threshold);
    assert_eq!(config.recovery_threshold, parsed.recovery_threshold);
}

#[test]
fn test_config_toml_parsing() {
    let toml = r#"
        enabled = true
        interval_seconds = 60
        timeout_seconds = 10
        failure_threshold = 5
        recovery_threshold = 3
    "#;
    let config: HealthCheckConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.interval_seconds, 60);
    assert_eq!(config.timeout_seconds, 10);
    assert_eq!(config.failure_threshold, 5);
    assert_eq!(config.recovery_threshold, 3);
}

#[test]
fn test_config_partial_toml() {
    // Using defaults for missing fields
    let toml = r#"
        enabled = false
    "#;
    let config: HealthCheckConfig = toml::from_str(toml).unwrap();
    assert!(!config.enabled);
    assert_eq!(config.interval_seconds, 30); // default
    assert_eq!(config.timeout_seconds, 5); // default
}

// ============================================================================
// T03: HealthCheckError Tests
// ============================================================================

#[test]
fn test_error_timeout_display() {
    let err = HealthCheckError::Timeout(5);
    assert_eq!(err.to_string(), "request timeout after 5s");
}

#[test]
fn test_error_connection_display() {
    let err = HealthCheckError::ConnectionFailed("refused".to_string());
    assert_eq!(err.to_string(), "connection failed: refused");
}

#[test]
fn test_error_dns_display() {
    let err = HealthCheckError::DnsError("unknown host".to_string());
    assert_eq!(err.to_string(), "DNS resolution failed: unknown host");
}

#[test]
fn test_error_tls_display() {
    let err = HealthCheckError::TlsError("certificate expired".to_string());
    assert_eq!(
        err.to_string(),
        "TLS certificate error: certificate expired"
    );
}

#[test]
fn test_error_http_display() {
    let err = HealthCheckError::HttpError(503);
    assert_eq!(err.to_string(), "HTTP error: 503");
}

#[test]
fn test_error_parse_display() {
    let err = HealthCheckError::ParseError("invalid JSON".to_string());
    assert_eq!(err.to_string(), "invalid response: invalid JSON");
}

// ============================================================================
// T04: BackendHealthState Tests
// ============================================================================

#[test]
fn test_state_default() {
    let state = BackendHealthState::default();
    assert_eq!(state.consecutive_failures, 0);
    assert_eq!(state.consecutive_successes, 0);
    assert!(state.last_check_time.is_none());
    assert_eq!(state.last_status, crate::registry::BackendStatus::Unknown);
    assert!(state.last_models.is_empty());
}

#[test]
fn test_state_clone() {
    let state = BackendHealthState {
        consecutive_failures: 2,
        ..Default::default()
    };
    let cloned = state.clone();
    assert_eq!(cloned.consecutive_failures, 2);
}

// ============================================================================
// T05: Response Parsing (Ollama) Tests
// ============================================================================

#[test]
fn test_parse_ollama_single_model() {
    let body = r#"{"models": [{"name": "llama3:70b"}]}"#;
    let models = parser::parse_ollama_response(body).unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "llama3:70b");
    assert_eq!(models[0].name, "llama3:70b");
}

#[test]
fn test_parse_ollama_multiple_models() {
    let body = r#"{"models": [{"name": "llama3:70b"}, {"name": "mistral:7b"}]}"#;
    let models = parser::parse_ollama_response(body).unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "llama3:70b");
    assert_eq!(models[1].id, "mistral:7b");
}

#[test]
fn test_parse_ollama_empty_list() {
    let body = r#"{"models": []}"#;
    let models = parser::parse_ollama_response(body).unwrap();
    assert!(models.is_empty());
}

#[test]
fn test_parse_ollama_invalid_json() {
    let body = "not json";
    let result = parser::parse_ollama_response(body);
    assert!(matches!(result, Err(HealthCheckError::ParseError(_))));
}

#[test]
fn test_parse_ollama_vision_detection() {
    // parse_ollama_response returns defaults; enrichment happens via /api/show
    // Test name heuristics via apply_name_heuristics as fallback
    let body = r#"{"models": [{"name": "llava:13b"}]}"#;
    let mut models = parser::parse_ollama_response(body).unwrap();
    parser::apply_name_heuristics(&mut models[0]);
    assert!(models[0].supports_vision);
}

#[test]
fn test_parse_ollama_tool_detection() {
    let body = r#"{"models": [{"name": "mistral:7b"}]}"#;
    let mut models = parser::parse_ollama_response(body).unwrap();
    parser::apply_name_heuristics(&mut models[0]);
    assert!(models[0].supports_tools);
}

#[test]
fn test_parse_ollama_default_context_length() {
    let body = r#"{"models": [{"name": "llama3:70b"}]}"#;
    let models = parser::parse_ollama_response(body).unwrap();
    assert_eq!(models[0].context_length, 4096);
}

// ============================================================================
// T06: Response Parsing (OpenAI/LlamaCpp) Tests
// ============================================================================

#[test]
fn test_parse_openai_single_model() {
    let body = r#"{"data": [{"id": "mistral-7b"}]}"#;
    let models = parser::parse_openai_response(body).unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "mistral-7b");
    assert_eq!(models[0].name, "mistral-7b");
    // Name heuristics applied: mistral supports tools
    assert!(models[0].supports_tools);
}

#[test]
fn test_parse_openai_multiple_models() {
    let body = r#"{"data": [{"id": "gpt-4"}, {"id": "gpt-3.5-turbo"}]}"#;
    let models = parser::parse_openai_response(body).unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, "gpt-4");
    assert_eq!(models[1].id, "gpt-3.5-turbo");
}

#[test]
fn test_parse_openai_vision_model_heuristics() {
    // LM Studio serving gemma-3-4b should be detected as vision-capable
    let body = r#"{"data": [{"id": "google/gemma-3-4b"}]}"#;
    let models = parser::parse_openai_response(body).unwrap();
    assert!(
        models[0].supports_vision,
        "gemma-3-4b should be vision-capable"
    );
}

#[test]
fn test_parse_openai_no_false_positive_vision() {
    let body = r#"{"data": [{"id": "text-embedding-nomic-embed-text-v1.5"}]}"#;
    let models = parser::parse_openai_response(body).unwrap();
    assert!(
        !models[0].supports_vision,
        "embedding model should not be vision-capable"
    );
    assert!(
        !models[0].supports_tools,
        "embedding model should not be tool-capable"
    );
}

// ============================================================================
// T06a: Name-Based Heuristics Tests
// ============================================================================

#[test]
fn test_heuristics_vision_models() {
    let vision_models = [
        "llava:13b",
        "llava:34b",
        "bakllava:7b",
        "llama4:latest",
        "google/gemma-3-4b",
        "google/gemma-3-12b",
        "google/gemma-3-27b",
        "pixtral:12b",
        "moondream:latest",
        "minicpm-v:latest",
    ];
    for name in vision_models {
        let mut model = Model {
            id: name.to_string(),
            name: name.to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        };
        parser::apply_name_heuristics(&mut model);
        assert!(model.supports_vision, "{name} should be vision-capable");
    }
}

#[test]
fn test_heuristics_tool_models() {
    let tool_models = [
        "mistral:7b",
        "llama3.1:8b",
        "llama3.2:3b",
        "llama3.3:70b",
        "llama4:latest",
        "qwen2.5:7b",
        "qwen3:8b",
        "command-r:latest",
        "firefunction:latest",
    ];
    for name in tool_models {
        let mut model = Model {
            id: name.to_string(),
            name: name.to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        };
        parser::apply_name_heuristics(&mut model);
        assert!(model.supports_tools, "{name} should be tool-capable");
    }
}

#[test]
fn test_heuristics_no_false_positives() {
    let basic_models = ["phi3:mini", "tinyllama:latest", "codellama:7b"];
    for name in basic_models {
        let mut model = Model {
            id: name.to_string(),
            name: name.to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        };
        parser::apply_name_heuristics(&mut model);
        assert!(
            !model.supports_vision,
            "{name} should not be vision-capable"
        );
    }
}

#[test]
fn test_heuristics_preserve_existing_true() {
    // If already set to true (e.g., from /api/show), heuristics should not revert
    let mut model = Model {
        id: "custom-model".to_string(),
        name: "custom-model".to_string(),
        context_length: 4096,
        supports_vision: true,
        supports_tools: true,
        supports_json_mode: false,
        max_output_tokens: None,
    };
    parser::apply_name_heuristics(&mut model);
    assert!(model.supports_vision, "should preserve existing true");
    assert!(model.supports_tools, "should preserve existing true");
}

#[test]
fn test_parse_openai_empty_data() {
    let body = r#"{"data": []}"#;
    let models = parser::parse_openai_response(body).unwrap();
    assert!(models.is_empty());
}

#[test]
fn test_parse_openai_invalid_json() {
    let body = "not json";
    let result = parser::parse_openai_response(body);
    assert!(matches!(result, Err(HealthCheckError::ParseError(_))));
}

#[test]
fn test_parse_llamacpp_healthy() {
    let body = r#"{"status": "ok"}"#;
    let result = parser::parse_llamacpp_response(body);
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[test]
fn test_parse_llamacpp_unhealthy() {
    let body = r#"{"status": "error"}"#;
    let result = parser::parse_llamacpp_response(body);
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[test]
fn test_parse_llamacpp_invalid_json() {
    let body = "not json";
    let result = parser::parse_llamacpp_response(body);
    assert!(matches!(result, Err(HealthCheckError::ParseError(_))));
}

// ============================================================================
// T07: Status Transition Logic Tests
// ============================================================================

use crate::registry::BackendStatus;

/// Helper for creating success results
fn make_success() -> HealthCheckResult {
    HealthCheckResult::Success {
        latency_ms: 100,
        models: vec![],
    }
}

/// Helper for creating failure results
fn make_failure() -> HealthCheckResult {
    HealthCheckResult::Failure {
        error: HealthCheckError::Timeout(5),
    }
}

#[test]
fn test_unknown_to_healthy_on_success() {
    let mut state = BackendHealthState::default();
    let config = HealthCheckConfig::default();
    let result = make_success();

    let new_status = state.apply_result(&result, &config);
    assert_eq!(new_status, Some(BackendStatus::Healthy));
    assert_eq!(state.consecutive_successes, 1);
    assert_eq!(state.consecutive_failures, 0);
}

#[test]
fn test_unknown_to_unhealthy_on_failure() {
    let mut state = BackendHealthState::default();
    let config = HealthCheckConfig::default();
    let result = make_failure();

    let new_status = state.apply_result(&result, &config);
    assert_eq!(new_status, Some(BackendStatus::Unhealthy));
    assert_eq!(state.consecutive_failures, 1);
    assert_eq!(state.consecutive_successes, 0);
}

#[test]
fn test_healthy_stays_healthy_under_threshold() {
    let mut state = BackendHealthState {
        last_status: BackendStatus::Healthy,
        ..Default::default()
    };
    let config = HealthCheckConfig::default();

    // First failure
    state.apply_result(&make_failure(), &config);
    assert_eq!(state.consecutive_failures, 1);

    // Second failure - still under threshold (3)
    let new_status = state.apply_result(&make_failure(), &config);
    assert_eq!(new_status, None); // No transition
    assert_eq!(state.consecutive_failures, 2);
}

#[test]
fn test_healthy_to_unhealthy_at_threshold() {
    let mut state = BackendHealthState {
        last_status: BackendStatus::Healthy,
        ..Default::default()
    };
    let config = HealthCheckConfig::default();

    // Apply 3 consecutive failures
    state.apply_result(&make_failure(), &config);
    state.apply_result(&make_failure(), &config);
    let new_status = state.apply_result(&make_failure(), &config);

    assert_eq!(new_status, Some(BackendStatus::Unhealthy));
    assert_eq!(state.consecutive_failures, 3);
}

#[test]
fn test_unhealthy_stays_unhealthy_under_threshold() {
    let mut state = BackendHealthState {
        last_status: BackendStatus::Unhealthy,
        ..Default::default()
    };
    let config = HealthCheckConfig::default();

    // First success - under recovery threshold (2)
    let new_status = state.apply_result(&make_success(), &config);
    assert_eq!(new_status, None); // No transition
    assert_eq!(state.consecutive_successes, 1);
}

#[test]
fn test_unhealthy_to_healthy_at_threshold() {
    let mut state = BackendHealthState {
        last_status: BackendStatus::Unhealthy,
        ..Default::default()
    };
    let config = HealthCheckConfig::default();

    // Apply 2 consecutive successes
    state.apply_result(&make_success(), &config);
    let new_status = state.apply_result(&make_success(), &config);

    assert_eq!(new_status, Some(BackendStatus::Healthy));
    assert_eq!(state.consecutive_successes, 2);
}

#[test]
fn test_success_resets_failure_counter() {
    let mut state = BackendHealthState {
        last_status: BackendStatus::Healthy,
        ..Default::default()
    };
    let config = HealthCheckConfig::default();

    // Apply 2 failures
    state.apply_result(&make_failure(), &config);
    state.apply_result(&make_failure(), &config);
    assert_eq!(state.consecutive_failures, 2);

    // Success should reset counter
    state.apply_result(&make_success(), &config);
    assert_eq!(state.consecutive_failures, 0);
    assert_eq!(state.consecutive_successes, 1);
}

#[test]
fn test_failure_resets_success_counter() {
    let mut state = BackendHealthState {
        last_status: BackendStatus::Unhealthy,
        ..Default::default()
    };
    let config = HealthCheckConfig::default();

    // Apply 1 success
    state.apply_result(&make_success(), &config);
    assert_eq!(state.consecutive_successes, 1);

    // Failure should reset counter
    state.apply_result(&make_failure(), &config);
    assert_eq!(state.consecutive_successes, 0);
    assert_eq!(state.consecutive_failures, 1);
}

// ============================================================================
// T08: Endpoint Selection Tests
// ============================================================================

use crate::registry::BackendType;

#[test]
fn test_endpoint_selection_ollama() {
    let endpoint = crate::health::HealthChecker::get_health_endpoint(BackendType::Ollama);
    assert_eq!(endpoint, "/api/tags");
}

#[test]
fn test_endpoint_selection_vllm() {
    let endpoint = crate::health::HealthChecker::get_health_endpoint(BackendType::VLLM);
    assert_eq!(endpoint, "/v1/models");
}

#[test]
fn test_endpoint_selection_llamacpp() {
    let endpoint = crate::health::HealthChecker::get_health_endpoint(BackendType::LlamaCpp);
    assert_eq!(endpoint, "/health");
}

#[test]
fn test_endpoint_selection_exo() {
    let endpoint = crate::health::HealthChecker::get_health_endpoint(BackendType::Exo);
    assert_eq!(endpoint, "/v1/models");
}

#[test]
fn test_endpoint_selection_openai() {
    let endpoint = crate::health::HealthChecker::get_health_endpoint(BackendType::OpenAI);
    assert_eq!(endpoint, "/v1/models");
}

#[test]
fn test_endpoint_selection_lmstudio() {
    let endpoint = crate::health::HealthChecker::get_health_endpoint(BackendType::LMStudio);
    assert_eq!(endpoint, "/v1/models");
}

#[test]
fn test_endpoint_selection_generic() {
    let endpoint = crate::health::HealthChecker::get_health_endpoint(BackendType::Generic);
    assert_eq!(endpoint, "/v1/models");
}

// ============================================================================
// SuccessWithParseError Tests (F02 Critical Fix)
// ============================================================================

/// Helper for creating SuccessWithParseError results
fn make_success_with_parse_error() -> HealthCheckResult {
    HealthCheckResult::SuccessWithParseError {
        latency_ms: 100,
        parse_error: "invalid JSON".to_string(),
    }
}

#[test]
fn test_success_with_parse_error_counts_as_success() {
    // Backend returning 200 with invalid JSON should be treated as healthy
    let mut state = BackendHealthState::default();
    let config = HealthCheckConfig::default();
    let result = make_success_with_parse_error();

    let new_status = state.apply_result(&result, &config);
    assert_eq!(new_status, Some(BackendStatus::Healthy));
    assert_eq!(state.consecutive_successes, 1);
    assert_eq!(state.consecutive_failures, 0);
}

#[test]
fn test_success_with_parse_error_resets_failure_counter() {
    // Parse error should reset failure counter like a normal success
    let mut state = BackendHealthState {
        last_status: BackendStatus::Healthy,
        consecutive_failures: 2,
        ..Default::default()
    };
    let config = HealthCheckConfig::default();

    state.apply_result(&make_success_with_parse_error(), &config);
    assert_eq!(state.consecutive_failures, 0);
    assert_eq!(state.consecutive_successes, 1);
}

#[test]
fn test_success_with_parse_error_recovers_unhealthy_backend() {
    // Parse error should allow recovery from unhealthy state
    let mut state = BackendHealthState {
        last_status: BackendStatus::Unhealthy,
        ..Default::default()
    };
    let config = HealthCheckConfig::default();

    // Apply 2 parse error successes (recovery threshold)
    state.apply_result(&make_success_with_parse_error(), &config);
    let new_status = state.apply_result(&make_success_with_parse_error(), &config);

    assert_eq!(new_status, Some(BackendStatus::Healthy));
    assert_eq!(state.consecutive_successes, 2);
}

#[test]
fn test_success_with_parse_error_preserves_healthy_status() {
    // Parse error on healthy backend should keep it healthy
    let mut state = BackendHealthState {
        last_status: BackendStatus::Healthy,
        ..Default::default()
    };
    let config = HealthCheckConfig::default();

    let new_status = state.apply_result(&make_success_with_parse_error(), &config);
    // No transition (already healthy)
    assert_eq!(new_status, None);
    assert_eq!(state.consecutive_successes, 1);
}

// ============================================================================
// T10: from_agent_error Conversion Tests
// ============================================================================

#[test]
fn test_from_agent_error_network() {
    let agent_err = crate::agent::AgentError::Network("connection refused".to_string());
    let health_err = HealthCheckError::from_agent_error(agent_err);
    assert!(matches!(
        health_err,
        HealthCheckError::ConnectionFailed(msg) if msg == "connection refused"
    ));
}

#[test]
fn test_from_agent_error_timeout_ms_to_seconds() {
    // 5000ms → 5s
    let agent_err = crate::agent::AgentError::Timeout(5000);
    let health_err = HealthCheckError::from_agent_error(agent_err);
    assert!(matches!(health_err, HealthCheckError::Timeout(5)));
}

#[test]
fn test_from_agent_error_timeout_rounds_up() {
    // 5001ms → 6s (ceil division)
    let agent_err = crate::agent::AgentError::Timeout(5001);
    let health_err = HealthCheckError::from_agent_error(agent_err);
    assert!(matches!(health_err, HealthCheckError::Timeout(6)));
}

#[test]
fn test_from_agent_error_timeout_sub_second() {
    // 500ms → 1s
    let agent_err = crate::agent::AgentError::Timeout(500);
    let health_err = HealthCheckError::from_agent_error(agent_err);
    assert!(matches!(health_err, HealthCheckError::Timeout(1)));
}

#[test]
fn test_from_agent_error_upstream() {
    let agent_err = crate::agent::AgentError::Upstream {
        status: 503,
        message: "Service Unavailable".to_string(),
    };
    let health_err = HealthCheckError::from_agent_error(agent_err);
    assert!(matches!(health_err, HealthCheckError::HttpError(503)));
}

#[test]
fn test_from_agent_error_invalid_response() {
    let agent_err = crate::agent::AgentError::InvalidResponse("bad json".to_string());
    let health_err = HealthCheckError::from_agent_error(agent_err);
    assert!(matches!(
        health_err,
        HealthCheckError::ParseError(msg) if msg == "bad json"
    ));
}

#[test]
fn test_from_agent_error_unsupported() {
    let agent_err = crate::agent::AgentError::Unsupported("embeddings");
    let health_err = HealthCheckError::from_agent_error(agent_err);
    assert!(matches!(health_err, HealthCheckError::AgentError(_)));
}

#[test]
fn test_from_agent_error_configuration() {
    let agent_err = crate::agent::AgentError::Configuration("missing API key".to_string());
    let health_err = HealthCheckError::from_agent_error(agent_err);
    assert!(matches!(health_err, HealthCheckError::AgentError(_)));
}

// ============================================================================
// T11: check_backend with Agent Tests (async)
// ============================================================================

use crate::agent::{
    AgentCapabilities, AgentProfile, HealthStatus, ModelCapability, PrivacyZone, StreamChunk,
};
use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
use async_trait::async_trait;
use axum::http::HeaderMap;
use futures_util::stream::BoxStream;
use std::sync::Arc;

/// Mock health response for testing (avoids AgentError not being Clone).
#[derive(Clone)]
enum MockHealthResponse {
    Ok(HealthStatus),
    NetworkError(String),
    TimeoutError(u64),
}

/// Mock models response for testing.
#[derive(Clone)]
enum MockModelsResponse {
    Ok(Vec<ModelCapability>),
    NetworkError(String),
}

/// Mock agent for testing health check paths.
struct MockAgent {
    id: String,
    health_response: MockHealthResponse,
    models_response: MockModelsResponse,
}

#[async_trait]
impl crate::agent::InferenceAgent for MockAgent {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        "MockAgent"
    }
    fn profile(&self) -> AgentProfile {
        AgentProfile {
            backend_type: "generic".to_string(),
            version: None,
            privacy_zone: PrivacyZone::Open,
            capabilities: AgentCapabilities::default(),
        }
    }
    async fn health_check(&self) -> Result<HealthStatus, crate::agent::AgentError> {
        match &self.health_response {
            MockHealthResponse::Ok(status) => Ok(status.clone()),
            MockHealthResponse::NetworkError(msg) => {
                Err(crate::agent::AgentError::Network(msg.clone()))
            }
            MockHealthResponse::TimeoutError(ms) => Err(crate::agent::AgentError::Timeout(*ms)),
        }
    }
    async fn list_models(&self) -> Result<Vec<ModelCapability>, crate::agent::AgentError> {
        match &self.models_response {
            MockModelsResponse::Ok(models) => Ok(models.clone()),
            MockModelsResponse::NetworkError(msg) => {
                Err(crate::agent::AgentError::Network(msg.clone()))
            }
        }
    }
    async fn chat_completion(
        &self,
        _request: ChatCompletionRequest,
        _headers: Option<&HeaderMap>,
    ) -> Result<ChatCompletionResponse, crate::agent::AgentError> {
        Err(crate::agent::AgentError::Unsupported("chat_completion"))
    }
    async fn chat_completion_stream(
        &self,
        _request: ChatCompletionRequest,
        _headers: Option<&HeaderMap>,
    ) -> Result<
        BoxStream<'static, Result<StreamChunk, crate::agent::AgentError>>,
        crate::agent::AgentError,
    > {
        Err(crate::agent::AgentError::Unsupported(
            "chat_completion_stream",
        ))
    }
}

fn make_test_backend(id: &str) -> Backend {
    Backend::new(
        id.to_string(),
        "test-backend".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    )
}

#[tokio::test]
async fn test_check_backend_agent_healthy_with_models() {
    let registry = Arc::new(crate::registry::Registry::new());
    let backend = make_test_backend("test-1");

    let agent: Arc<dyn crate::agent::InferenceAgent> = Arc::new(MockAgent {
        id: "test-1".to_string(),
        health_response: MockHealthResponse::Ok(HealthStatus::Healthy { model_count: 2 }),
        models_response: MockModelsResponse::Ok(vec![
            ModelCapability {
                id: "llama3:8b".to_string(),
                name: "llama3:8b".to_string(),
                context_length: 8192,
                supports_vision: false,
                supports_tools: true,
                supports_json_mode: false,
                max_output_tokens: None,
                capability_tier: None,
            },
            ModelCapability {
                id: "gpt-4".to_string(),
                name: "gpt-4".to_string(),
                context_length: 128000,
                supports_vision: true,
                supports_tools: true,
                supports_json_mode: true,
                max_output_tokens: Some(4096),
                capability_tier: None,
            },
        ]),
    });

    registry.add_backend_with_agent(backend, agent).unwrap();
    let backend = registry.get_backend("test-1").unwrap();

    let checker = HealthChecker::with_client(
        registry,
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );
    let result = checker.check_backend(&backend).await;

    match result {
        HealthCheckResult::Success { models, .. } => {
            assert_eq!(models.len(), 2);
            assert_eq!(models[0].id, "llama3:8b");
            assert!(models[1].supports_vision);
        }
        other => panic!("Expected Success, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_check_backend_agent_healthy_list_models_fails() {
    let registry = Arc::new(crate::registry::Registry::new());
    let backend = make_test_backend("test-2");

    let agent: Arc<dyn crate::agent::InferenceAgent> = Arc::new(MockAgent {
        id: "test-2".to_string(),
        health_response: MockHealthResponse::Ok(HealthStatus::Healthy { model_count: 0 }),
        models_response: MockModelsResponse::NetworkError("timeout".to_string()),
    });

    registry.add_backend_with_agent(backend, agent).unwrap();
    let backend = registry.get_backend("test-2").unwrap();

    let checker = HealthChecker::with_client(
        registry,
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );
    let result = checker.check_backend(&backend).await;

    assert!(matches!(
        result,
        HealthCheckResult::SuccessWithParseError { .. }
    ));
}

#[tokio::test]
async fn test_check_backend_agent_unhealthy() {
    let registry = Arc::new(crate::registry::Registry::new());
    let backend = make_test_backend("test-3");

    let agent: Arc<dyn crate::agent::InferenceAgent> = Arc::new(MockAgent {
        id: "test-3".to_string(),
        health_response: MockHealthResponse::Ok(HealthStatus::Unhealthy),
        models_response: MockModelsResponse::Ok(vec![]),
    });

    registry.add_backend_with_agent(backend, agent).unwrap();
    let backend = registry.get_backend("test-3").unwrap();

    let checker = HealthChecker::with_client(
        registry,
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );
    let result = checker.check_backend(&backend).await;

    assert!(matches!(result, HealthCheckResult::Failure { .. }));
}

#[tokio::test]
async fn test_check_backend_agent_loading() {
    let registry = Arc::new(crate::registry::Registry::new());
    let backend = make_test_backend("test-4");

    let agent: Arc<dyn crate::agent::InferenceAgent> = Arc::new(MockAgent {
        id: "test-4".to_string(),
        health_response: MockHealthResponse::Ok(HealthStatus::Loading {
            model_id: "llama3:70b".to_string(),
            percent: 42,
            eta_ms: Some(5000),
        }),
        models_response: MockModelsResponse::Ok(vec![]),
    });

    registry.add_backend_with_agent(backend, agent).unwrap();
    let backend = registry.get_backend("test-4").unwrap();

    let checker = HealthChecker::with_client(
        registry,
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );
    let result = checker.check_backend(&backend).await;

    match result {
        HealthCheckResult::Failure { error } => {
            let msg = error.to_string();
            assert!(
                msg.contains("loading"),
                "Expected loading message, got: {msg}"
            );
        }
        other => panic!("Expected Failure, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_check_backend_agent_draining() {
    let registry = Arc::new(crate::registry::Registry::new());
    let backend = make_test_backend("test-5");

    let agent: Arc<dyn crate::agent::InferenceAgent> = Arc::new(MockAgent {
        id: "test-5".to_string(),
        health_response: MockHealthResponse::Ok(HealthStatus::Draining),
        models_response: MockModelsResponse::Ok(vec![]),
    });

    registry.add_backend_with_agent(backend, agent).unwrap();
    let backend = registry.get_backend("test-5").unwrap();

    let checker = HealthChecker::with_client(
        registry,
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );
    let result = checker.check_backend(&backend).await;

    match result {
        HealthCheckResult::Failure { error } => {
            let msg = error.to_string();
            assert!(
                msg.contains("draining"),
                "Expected draining message, got: {msg}"
            );
        }
        other => panic!("Expected Failure, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_check_backend_agent_network_error() {
    let registry = Arc::new(crate::registry::Registry::new());
    let backend = make_test_backend("test-6");

    let agent: Arc<dyn crate::agent::InferenceAgent> = Arc::new(MockAgent {
        id: "test-6".to_string(),
        health_response: MockHealthResponse::NetworkError("connection refused".to_string()),
        models_response: MockModelsResponse::Ok(vec![]),
    });

    registry.add_backend_with_agent(backend, agent).unwrap();
    let backend = registry.get_backend("test-6").unwrap();

    let checker = HealthChecker::with_client(
        registry,
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );
    let result = checker.check_backend(&backend).await;

    match result {
        HealthCheckResult::Failure { error } => {
            assert!(matches!(error, HealthCheckError::ConnectionFailed(_)));
        }
        other => panic!("Expected Failure, got: {other:?}"),
    }
}

#[tokio::test]
async fn test_check_backend_agent_timeout_error() {
    let registry = Arc::new(crate::registry::Registry::new());
    let backend = make_test_backend("test-7");

    let agent: Arc<dyn crate::agent::InferenceAgent> = Arc::new(MockAgent {
        id: "test-7".to_string(),
        health_response: MockHealthResponse::TimeoutError(5000),
        models_response: MockModelsResponse::Ok(vec![]),
    });

    registry.add_backend_with_agent(backend, agent).unwrap();
    let backend = registry.get_backend("test-7").unwrap();

    let checker = HealthChecker::with_client(
        registry,
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );
    let result = checker.check_backend(&backend).await;

    match result {
        HealthCheckResult::Failure { error } => {
            assert!(matches!(error, HealthCheckError::Timeout(5)));
        }
        other => panic!("Expected Failure, got: {other:?}"),
    }
}

// ============================================================================
// T12: apply_result Registry Integration Tests
// ============================================================================

#[tokio::test]
async fn test_apply_result_success_updates_registry() {
    let registry = Arc::new(crate::registry::Registry::new());
    let backend = make_test_backend("apply-1");
    registry.add_backend(backend).unwrap();

    let checker = HealthChecker::with_client(
        registry.clone(),
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );

    let models = vec![Model {
        id: "llama3:8b".to_string(),
        name: "llama3:8b".to_string(),
        context_length: 8192,
        supports_vision: false,
        supports_tools: true,
        supports_json_mode: false,
        max_output_tokens: None,
    }];

    let result = HealthCheckResult::Success {
        latency_ms: 42,
        models: models.clone(),
    };

    checker.apply_result("apply-1", result);

    // Registry should have models
    let b = registry.get_backend("apply-1").unwrap();
    assert_eq!(b.models.len(), 1);
    assert_eq!(b.models[0].id, "llama3:8b");

    // Status should transition Unknown → Healthy
    assert_eq!(b.status, BackendStatus::Healthy);
}

#[tokio::test]
async fn test_apply_result_failure_updates_status() {
    let registry = Arc::new(crate::registry::Registry::new());
    let backend = make_test_backend("apply-2");
    registry.add_backend(backend).unwrap();

    let checker = HealthChecker::with_client(
        registry.clone(),
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );

    let result = HealthCheckResult::Failure {
        error: HealthCheckError::ConnectionFailed("refused".to_string()),
    };

    checker.apply_result("apply-2", result);

    // Status should transition Unknown → Unhealthy
    let backends = registry.get_all_backends();
    let b = backends.iter().find(|b| b.id == "apply-2").unwrap();
    assert_eq!(b.status, BackendStatus::Unhealthy);
}

#[tokio::test]
async fn test_apply_result_parse_error_preserves_models() {
    let registry = Arc::new(crate::registry::Registry::new());
    let backend = make_test_backend("apply-3");
    registry.add_backend(backend).unwrap();

    let checker = HealthChecker::with_client(
        registry.clone(),
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );

    // First: successful check with models
    let models = vec![Model {
        id: "model-a".to_string(),
        name: "model-a".to_string(),
        context_length: 4096,
        supports_vision: false,
        supports_tools: false,
        supports_json_mode: false,
        max_output_tokens: None,
    }];
    checker.apply_result(
        "apply-3",
        HealthCheckResult::Success {
            latency_ms: 10,
            models,
        },
    );

    // Second: parse error should preserve the models
    checker.apply_result(
        "apply-3",
        HealthCheckResult::SuccessWithParseError {
            latency_ms: 15,
            parse_error: "bad json".to_string(),
        },
    );

    let b = registry.get_backend("apply-3").unwrap();
    assert_eq!(b.models.len(), 1);
    assert_eq!(b.models[0].id, "model-a");
}

// ============================================================================
// T13: check_all_backends Tests
// ============================================================================

#[tokio::test]
async fn test_check_all_backends_with_agents() {
    let registry = Arc::new(crate::registry::Registry::new());

    // Add two backends with mock agents
    let backend1 = make_test_backend("all-1");
    let agent1: Arc<dyn crate::agent::InferenceAgent> = Arc::new(MockAgent {
        id: "all-1".to_string(),
        health_response: MockHealthResponse::Ok(HealthStatus::Healthy { model_count: 1 }),
        models_response: MockModelsResponse::Ok(vec![ModelCapability {
            id: "model-x".to_string(),
            name: "model-x".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
            capability_tier: None,
        }]),
    });
    registry.add_backend_with_agent(backend1, agent1).unwrap();

    let backend2 = make_test_backend("all-2");
    let agent2: Arc<dyn crate::agent::InferenceAgent> = Arc::new(MockAgent {
        id: "all-2".to_string(),
        health_response: MockHealthResponse::NetworkError("refused".to_string()),
        models_response: MockModelsResponse::Ok(vec![]),
    });
    registry.add_backend_with_agent(backend2, agent2).unwrap();

    let checker = HealthChecker::with_client(
        registry.clone(),
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );

    let results = checker.check_all_backends().await;
    assert_eq!(results.len(), 2);

    // One should succeed, one should fail
    let successes = results
        .iter()
        .filter(|(_, r)| matches!(r, HealthCheckResult::Success { .. }))
        .count();
    let failures = results
        .iter()
        .filter(|(_, r)| matches!(r, HealthCheckResult::Failure { .. }))
        .count();
    assert_eq!(successes, 1);
    assert_eq!(failures, 1);
}

#[tokio::test]
async fn test_check_all_backends_empty_registry() {
    let registry = Arc::new(crate::registry::Registry::new());
    let checker = HealthChecker::with_client(
        registry,
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );

    let results = checker.check_all_backends().await;
    assert!(results.is_empty());
}
