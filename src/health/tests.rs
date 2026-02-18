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
            capability_tier: None,
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

// ============================================================================
// T14: classify_error Tests
// ============================================================================

#[tokio::test]
async fn test_classify_error_timeout() {
    // Create a client with very short timeout and try connecting to unreachable addr
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(1))
        .build()
        .unwrap();

    // Try to connect to a non-routable address to trigger timeout
    let result = client
        .get("http://192.0.2.1:1/test") // RFC 5737 TEST-NET, should timeout
        .timeout(std::time::Duration::from_millis(1))
        .send()
        .await;

    if let Err(e) = result {
        let health_err = HealthChecker::classify_error(e, 5);
        // Should be either Timeout or ConnectionFailed depending on OS
        match health_err {
            HealthCheckError::Timeout(secs) => assert_eq!(secs, 5),
            HealthCheckError::ConnectionFailed(_) => {
                // Connection refused is also acceptable
            }
            other => panic!("Expected Timeout or ConnectionFailed, got: {other}"),
        }
    }
}

#[tokio::test]
async fn test_classify_error_connection() {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap();

    // Connect to a port that's almost certainly not listening
    let result = client
        .get("http://127.0.0.1:1/test")
        .timeout(std::time::Duration::from_secs(1))
        .send()
        .await;

    if let Err(e) = result {
        let health_err = HealthChecker::classify_error(e, 5);
        match health_err {
            HealthCheckError::ConnectionFailed(msg) => {
                assert!(!msg.is_empty(), "Error message should not be empty");
            }
            HealthCheckError::Timeout(_) => {
                // Timeout is also acceptable in some environments
            }
            other => panic!("Expected ConnectionFailed or Timeout, got: {other}"),
        }
    }
}

// ============================================================================
// T15: Legacy fallback (no agent) health check test
// ============================================================================

#[tokio::test]
async fn test_check_backend_legacy_fallback() {
    // Create a backend WITHOUT an agent to trigger legacy HTTP path
    let registry = Arc::new(crate::registry::Registry::new());
    let backend = make_test_backend("legacy-1");
    // Add backend without agent (no add_backend_with_agent)
    registry.add_backend(backend).unwrap();

    let backend = registry.get_backend("legacy-1").unwrap();

    let checker = HealthChecker::with_client(
        registry,
        HealthCheckConfig {
            timeout_seconds: 1,
            ..HealthCheckConfig::default()
        },
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(1))
            .build()
            .unwrap(),
    );

    let result = checker.check_backend(&backend).await;
    // Without an agent and with an unreachable backend, this should fail
    // via the legacy HTTP path
    match result {
        HealthCheckResult::Failure { error } => {
            // Should be a connection error since localhost:11434 is not running
            match error {
                HealthCheckError::ConnectionFailed(_) | HealthCheckError::Timeout(_) => {}
                other => panic!("Expected connection error, got: {other}"),
            }
        }
        HealthCheckResult::Success { .. } => {
            // If an Ollama instance happens to be running, that's also OK
        }
        HealthCheckResult::SuccessWithParseError { .. } => {
            // Also acceptable if backend responds with non-JSON
        }
    }
}

// ============================================================================
// Agent-based health check with Loading/Draining/Unhealthy/Healthy statuses
// ============================================================================

#[tokio::test]
async fn test_check_backend_agent_returns_models() {
    use crate::agent::{
        AgentCapabilities, AgentError, AgentProfile, HealthStatus, InferenceAgent, ModelCapability,
        PrivacyZone, StreamChunk,
    };
    use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
    use async_trait::async_trait;
    use axum::http::HeaderMap;
    use futures_util::stream::BoxStream;

    struct MockAgent;

    #[async_trait]
    impl InferenceAgent for MockAgent {
        fn id(&self) -> &str {
            "mock-agent"
        }
        fn name(&self) -> &str {
            "Mock Agent"
        }
        fn profile(&self) -> AgentProfile {
            AgentProfile {
                backend_type: "ollama".to_string(),
                version: None,
                privacy_zone: PrivacyZone::Restricted,
                capabilities: AgentCapabilities {
                    embeddings: false,
                    model_lifecycle: false,
                    token_counting: false,
                    resource_monitoring: false,
                },
                capability_tier: None,
            }
        }
        async fn health_check(&self) -> Result<HealthStatus, AgentError> {
            Ok(HealthStatus::Healthy { model_count: 1 })
        }
        async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
            Ok(vec![ModelCapability {
                id: "model-1".to_string(),
                name: "model-1".to_string(),
                context_length: 4096,
                supports_vision: false,
                supports_tools: false,
                supports_json_mode: false,
                max_output_tokens: None,
                capability_tier: None,
            }])
        }
        async fn chat_completion(
            &self,
            _: ChatCompletionRequest,
            _: Option<&HeaderMap>,
        ) -> Result<ChatCompletionResponse, AgentError> {
            unimplemented!()
        }
        async fn chat_completion_stream(
            &self,
            _: ChatCompletionRequest,
            _: Option<&HeaderMap>,
        ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
            unimplemented!()
        }
    }

    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "b1".to_string(),
        "Backend1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    registry
        .add_backend_with_agent(backend, Arc::new(MockAgent))
        .unwrap();

    let checker = HealthChecker::new(registry, HealthCheckConfig::default());
    let b = Backend::new(
        "b1".to_string(),
        "Backend1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    let result = checker.check_backend(&b).await;
    match result {
        HealthCheckResult::Success { models, .. } => {
            assert_eq!(models.len(), 1);
            assert_eq!(models[0].id, "model-1");
        }
        other => panic!("Expected Success, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_check_backend_agent_loading_status() {
    use crate::agent::{
        AgentCapabilities, AgentError, AgentProfile, HealthStatus, InferenceAgent, ModelCapability,
        PrivacyZone, StreamChunk,
    };
    use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
    use async_trait::async_trait;
    use axum::http::HeaderMap;
    use futures_util::stream::BoxStream;

    struct LoadingAgent;

    #[async_trait]
    impl InferenceAgent for LoadingAgent {
        fn id(&self) -> &str {
            "loading-agent"
        }
        fn name(&self) -> &str {
            "Loading Agent"
        }
        fn profile(&self) -> AgentProfile {
            AgentProfile {
                backend_type: "ollama".to_string(),
                version: None,
                privacy_zone: PrivacyZone::Restricted,
                capabilities: AgentCapabilities {
                    embeddings: false,
                    model_lifecycle: false,
                    token_counting: false,
                    resource_monitoring: false,
                },
                capability_tier: None,
            }
        }
        async fn health_check(&self) -> Result<HealthStatus, AgentError> {
            Ok(HealthStatus::Loading {
                model_id: "big-model".to_string(),
                percent: 42,
                eta_ms: None,
            })
        }
        async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
            Ok(vec![])
        }
        async fn chat_completion(
            &self,
            _: ChatCompletionRequest,
            _: Option<&HeaderMap>,
        ) -> Result<ChatCompletionResponse, AgentError> {
            unimplemented!()
        }
        async fn chat_completion_stream(
            &self,
            _: ChatCompletionRequest,
            _: Option<&HeaderMap>,
        ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
            unimplemented!()
        }
    }

    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "b2".to_string(),
        "Backend2".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    registry
        .add_backend_with_agent(backend, Arc::new(LoadingAgent))
        .unwrap();

    let checker = HealthChecker::new(registry, HealthCheckConfig::default());
    let b = Backend::new(
        "b2".to_string(),
        "Backend2".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    let result = checker.check_backend(&b).await;
    match result {
        HealthCheckResult::Failure { error } => {
            assert!(error.to_string().contains("loading"));
        }
        other => panic!("Expected Failure with loading, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_check_backend_agent_draining_status() {
    use crate::agent::{
        AgentCapabilities, AgentError, AgentProfile, HealthStatus, InferenceAgent, ModelCapability,
        PrivacyZone, StreamChunk,
    };
    use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
    use async_trait::async_trait;
    use axum::http::HeaderMap;
    use futures_util::stream::BoxStream;

    struct DrainingAgent;

    #[async_trait]
    impl InferenceAgent for DrainingAgent {
        fn id(&self) -> &str {
            "draining-agent"
        }
        fn name(&self) -> &str {
            "Draining Agent"
        }
        fn profile(&self) -> AgentProfile {
            AgentProfile {
                backend_type: "ollama".to_string(),
                version: None,
                privacy_zone: PrivacyZone::Restricted,
                capabilities: AgentCapabilities {
                    embeddings: false,
                    model_lifecycle: false,
                    token_counting: false,
                    resource_monitoring: false,
                },
                capability_tier: None,
            }
        }
        async fn health_check(&self) -> Result<HealthStatus, AgentError> {
            Ok(HealthStatus::Draining)
        }
        async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
            Ok(vec![])
        }
        async fn chat_completion(
            &self,
            _: ChatCompletionRequest,
            _: Option<&HeaderMap>,
        ) -> Result<ChatCompletionResponse, AgentError> {
            unimplemented!()
        }
        async fn chat_completion_stream(
            &self,
            _: ChatCompletionRequest,
            _: Option<&HeaderMap>,
        ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
            unimplemented!()
        }
    }

    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "b3".to_string(),
        "Backend3".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    registry
        .add_backend_with_agent(backend, Arc::new(DrainingAgent))
        .unwrap();

    let checker = HealthChecker::new(registry, HealthCheckConfig::default());
    let b = Backend::new(
        "b3".to_string(),
        "Backend3".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    let result = checker.check_backend(&b).await;
    match result {
        HealthCheckResult::Failure { error } => {
            assert!(error.to_string().contains("draining"));
        }
        other => panic!("Expected Failure with draining, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_check_backend_agent_unhealthy_status() {
    use crate::agent::{
        AgentCapabilities, AgentError, AgentProfile, HealthStatus, InferenceAgent, ModelCapability,
        PrivacyZone, StreamChunk,
    };
    use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
    use async_trait::async_trait;
    use axum::http::HeaderMap;
    use futures_util::stream::BoxStream;

    struct UnhealthyAgent;

    #[async_trait]
    impl InferenceAgent for UnhealthyAgent {
        fn id(&self) -> &str {
            "unhealthy-agent"
        }
        fn name(&self) -> &str {
            "Unhealthy Agent"
        }
        fn profile(&self) -> AgentProfile {
            AgentProfile {
                backend_type: "ollama".to_string(),
                version: None,
                privacy_zone: PrivacyZone::Restricted,
                capabilities: AgentCapabilities {
                    embeddings: false,
                    model_lifecycle: false,
                    token_counting: false,
                    resource_monitoring: false,
                },
                capability_tier: None,
            }
        }
        async fn health_check(&self) -> Result<HealthStatus, AgentError> {
            Ok(HealthStatus::Unhealthy)
        }
        async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
            Ok(vec![])
        }
        async fn chat_completion(
            &self,
            _: ChatCompletionRequest,
            _: Option<&HeaderMap>,
        ) -> Result<ChatCompletionResponse, AgentError> {
            unimplemented!()
        }
        async fn chat_completion_stream(
            &self,
            _: ChatCompletionRequest,
            _: Option<&HeaderMap>,
        ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
            unimplemented!()
        }
    }

    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "b4".to_string(),
        "Backend4".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    registry
        .add_backend_with_agent(backend, Arc::new(UnhealthyAgent))
        .unwrap();

    let checker = HealthChecker::new(registry, HealthCheckConfig::default());
    let b = Backend::new(
        "b4".to_string(),
        "Backend4".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    let result = checker.check_backend(&b).await;
    match result {
        HealthCheckResult::Failure { error } => {
            assert!(error.to_string().contains("unhealthy"));
        }
        other => panic!("Expected Failure with unhealthy, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_check_backend_agent_list_models_error() {
    use crate::agent::{
        AgentCapabilities, AgentError, AgentProfile, HealthStatus, InferenceAgent, ModelCapability,
        PrivacyZone, StreamChunk,
    };
    use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
    use async_trait::async_trait;
    use axum::http::HeaderMap;
    use futures_util::stream::BoxStream;

    struct ListModelsErrorAgent;

    #[async_trait]
    impl InferenceAgent for ListModelsErrorAgent {
        fn id(&self) -> &str {
            "list-err-agent"
        }
        fn name(&self) -> &str {
            "List Error Agent"
        }
        fn profile(&self) -> AgentProfile {
            AgentProfile {
                backend_type: "ollama".to_string(),
                version: None,
                privacy_zone: PrivacyZone::Restricted,
                capabilities: AgentCapabilities {
                    embeddings: false,
                    model_lifecycle: false,
                    token_counting: false,
                    resource_monitoring: false,
                },
                capability_tier: None,
            }
        }
        async fn health_check(&self) -> Result<HealthStatus, AgentError> {
            Ok(HealthStatus::Healthy { model_count: 1 })
        }
        async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
            Err(AgentError::Network("connection reset".to_string()))
        }
        async fn chat_completion(
            &self,
            _: ChatCompletionRequest,
            _: Option<&HeaderMap>,
        ) -> Result<ChatCompletionResponse, AgentError> {
            unimplemented!()
        }
        async fn chat_completion_stream(
            &self,
            _: ChatCompletionRequest,
            _: Option<&HeaderMap>,
        ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
            unimplemented!()
        }
    }

    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "b5".to_string(),
        "Backend5".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    registry
        .add_backend_with_agent(backend, Arc::new(ListModelsErrorAgent))
        .unwrap();

    let checker = HealthChecker::new(registry, HealthCheckConfig::default());
    let b = Backend::new(
        "b5".to_string(),
        "Backend5".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    let result = checker.check_backend(&b).await;
    match result {
        HealthCheckResult::SuccessWithParseError { parse_error, .. } => {
            assert!(parse_error.contains("connection reset"));
        }
        other => panic!("Expected SuccessWithParseError, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_check_backend_agent_health_check_error() {
    use crate::agent::{
        AgentCapabilities, AgentError, AgentProfile, HealthStatus, InferenceAgent, ModelCapability,
        PrivacyZone, StreamChunk,
    };
    use crate::api::types::{ChatCompletionRequest, ChatCompletionResponse};
    use async_trait::async_trait;
    use axum::http::HeaderMap;
    use futures_util::stream::BoxStream;

    struct ErrorAgent;

    #[async_trait]
    impl InferenceAgent for ErrorAgent {
        fn id(&self) -> &str {
            "err-agent"
        }
        fn name(&self) -> &str {
            "Error Agent"
        }
        fn profile(&self) -> AgentProfile {
            AgentProfile {
                backend_type: "ollama".to_string(),
                version: None,
                privacy_zone: PrivacyZone::Restricted,
                capabilities: AgentCapabilities {
                    embeddings: false,
                    model_lifecycle: false,
                    token_counting: false,
                    resource_monitoring: false,
                },
                capability_tier: None,
            }
        }
        async fn health_check(&self) -> Result<HealthStatus, AgentError> {
            Err(AgentError::Timeout(5000))
        }
        async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError> {
            Ok(vec![])
        }
        async fn chat_completion(
            &self,
            _: ChatCompletionRequest,
            _: Option<&HeaderMap>,
        ) -> Result<ChatCompletionResponse, AgentError> {
            unimplemented!()
        }
        async fn chat_completion_stream(
            &self,
            _: ChatCompletionRequest,
            _: Option<&HeaderMap>,
        ) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError> {
            unimplemented!()
        }
    }

    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "b6".to_string(),
        "Backend6".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    registry
        .add_backend_with_agent(backend, Arc::new(ErrorAgent))
        .unwrap();

    let checker = HealthChecker::new(registry, HealthCheckConfig::default());
    let b = Backend::new(
        "b6".to_string(),
        "Backend6".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    let result = checker.check_backend(&b).await;
    assert!(matches!(result, HealthCheckResult::Failure { .. }));
}

// ============================================================================
// parse_and_enrich for different backend types via legacy HTTP path
// ============================================================================

#[tokio::test]
async fn test_parse_and_enrich_llamacpp_healthy() {
    let registry = Arc::new(Registry::new());
    let checker = HealthChecker::new(registry, HealthCheckConfig::default());

    let backend = Backend::new(
        "b-llcpp".to_string(),
        "LlamaCpp".to_string(),
        "http://localhost:8080".to_string(),
        BackendType::LlamaCpp,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );

    let result = checker
        .parse_and_enrich(&backend, r#"{"status":"ok"}"#, 10)
        .await;
    match result {
        HealthCheckResult::Success { models, latency_ms } => {
            assert_eq!(latency_ms, 10);
            assert!(models.is_empty()); // LlamaCpp doesn't return models
        }
        other => panic!("Expected Success, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_parse_and_enrich_llamacpp_unhealthy() {
    let registry = Arc::new(Registry::new());
    let checker = HealthChecker::new(registry, HealthCheckConfig::default());

    let backend = Backend::new(
        "b-llcpp".to_string(),
        "LlamaCpp".to_string(),
        "http://localhost:8080".to_string(),
        BackendType::LlamaCpp,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );

    let result = checker
        .parse_and_enrich(&backend, r#"{"status":"loading"}"#, 10)
        .await;
    assert!(matches!(result, HealthCheckResult::Failure { .. }));
}

#[tokio::test]
async fn test_parse_and_enrich_llamacpp_invalid_json() {
    let registry = Arc::new(Registry::new());
    let checker = HealthChecker::new(registry, HealthCheckConfig::default());

    let backend = Backend::new(
        "b-llcpp".to_string(),
        "LlamaCpp".to_string(),
        "http://localhost:8080".to_string(),
        BackendType::LlamaCpp,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );

    let result = checker.parse_and_enrich(&backend, "not json", 10).await;
    assert!(matches!(
        result,
        HealthCheckResult::SuccessWithParseError { .. }
    ));
}

#[tokio::test]
async fn test_parse_and_enrich_vllm() {
    let registry = Arc::new(Registry::new());
    let checker = HealthChecker::new(registry, HealthCheckConfig::default());

    let backend = Backend::new(
        "b-vllm".to_string(),
        "VLLM".to_string(),
        "http://localhost:8000".to_string(),
        BackendType::VLLM,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );

    let result = checker
        .parse_and_enrich(&backend, r#"{"data":[{"id":"llama-2-7b"}]}"#, 15)
        .await;
    match result {
        HealthCheckResult::Success { models, latency_ms } => {
            assert_eq!(latency_ms, 15);
            assert_eq!(models.len(), 1);
            assert_eq!(models[0].id, "llama-2-7b");
        }
        other => panic!("Expected Success, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_parse_and_enrich_generic_invalid_json() {
    let registry = Arc::new(Registry::new());
    let checker = HealthChecker::new(registry, HealthCheckConfig::default());

    let backend = Backend::new(
        "b-gen".to_string(),
        "Generic".to_string(),
        "http://localhost:9000".to_string(),
        BackendType::Generic,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );

    let result = checker.parse_and_enrich(&backend, "bad json", 5).await;
    assert!(matches!(
        result,
        HealthCheckResult::SuccessWithParseError { .. }
    ));
}

// ============================================================================
// broadcast_model_change tests
// ============================================================================

#[test]
fn test_broadcast_model_change_with_sender() {
    let registry = Arc::new(Registry::new());
    let (tx, mut rx) = tokio::sync::broadcast::channel(16);
    let checker = HealthChecker::new(registry, HealthCheckConfig::default()).with_broadcast(tx);

    let models = vec![Model {
        id: "test-model".to_string(),
        name: "test-model".to_string(),
        context_length: 4096,
        supports_vision: true,
        supports_tools: false,
        supports_json_mode: false,
        max_output_tokens: None,
    }];

    checker.broadcast_model_change("b1", &models);

    let update = rx.try_recv();
    assert!(update.is_ok());
}

#[test]
fn test_broadcast_model_change_without_sender() {
    let registry = Arc::new(Registry::new());
    let checker = HealthChecker::new(registry, HealthCheckConfig::default());

    // Should not panic even without a broadcast sender
    let models = vec![Model {
        id: "test-model".to_string(),
        name: "test-model".to_string(),
        context_length: 4096,
        supports_vision: false,
        supports_tools: false,
        supports_json_mode: false,
        max_output_tokens: None,
    }];

    checker.broadcast_model_change("b1", &models);
}

// ============================================================================
// apply_result with broadcast paths
// ============================================================================

#[tokio::test]
async fn test_apply_result_failure_broadcasts_empty_models() {
    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "b-fail".to_string(),
        "FailBackend".to_string(),
        "http://localhost:9000".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    registry.add_backend(backend).unwrap();

    let (tx, mut rx) = tokio::sync::broadcast::channel(16);
    let checker =
        HealthChecker::new(registry.clone(), HealthCheckConfig::default()).with_broadcast(tx);

    // First apply success to set last_models
    checker.apply_result(
        "b-fail",
        HealthCheckResult::Success {
            latency_ms: 10,
            models: vec![Model {
                id: "m1".to_string(),
                name: "m1".to_string(),
                context_length: 4096,
                supports_vision: false,
                supports_tools: false,
                supports_json_mode: false,
                max_output_tokens: None,
            }],
        },
    );
    // Drain the model_change broadcast
    let _ = rx.try_recv();

    // Now apply failure - should broadcast empty models
    checker.apply_result(
        "b-fail",
        HealthCheckResult::Failure {
            error: HealthCheckError::ConnectionFailed("refused".to_string()),
        },
    );
    let update = rx.try_recv();
    assert!(update.is_ok());
}

#[tokio::test]
async fn test_apply_result_success_with_parse_error_preserves_models() {
    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "b-parse".to_string(),
        "ParseBackend".to_string(),
        "http://localhost:9000".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    registry.add_backend(backend).unwrap();

    let checker = HealthChecker::new(registry.clone(), HealthCheckConfig::default());

    // Apply success first
    checker.apply_result(
        "b-parse",
        HealthCheckResult::Success {
            latency_ms: 10,
            models: vec![Model {
                id: "m1".to_string(),
                name: "m1".to_string(),
                context_length: 4096,
                supports_vision: false,
                supports_tools: false,
                supports_json_mode: false,
                max_output_tokens: None,
            }],
        },
    );

    // Apply parse error - should preserve previous models
    checker.apply_result(
        "b-parse",
        HealthCheckResult::SuccessWithParseError {
            latency_ms: 15,
            parse_error: "bad json".to_string(),
        },
    );

    // Backend should still have models
    let b = registry.get_backend("b-parse").unwrap();
    assert!(!b.models.is_empty());
}

// ============================================================================
// check_all_backends with ws_broadcast
// ============================================================================

#[tokio::test]
async fn test_check_all_backends_broadcasts_status() {
    use mockito::Server;

    let mut server = Server::new_async().await;
    let mock = server
        .mock("GET", "/api/tags")
        .with_status(200)
        .with_body(r#"{"models":[{"name":"llama3:8b"}]}"#)
        .create_async()
        .await;

    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "b-ws".to_string(),
        "WSBackend".to_string(),
        server.url(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    registry.add_backend(backend).unwrap();

    let (tx, mut rx) = tokio::sync::broadcast::channel(16);
    let checker = HealthChecker::with_client(
        registry,
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    )
    .with_broadcast(tx);

    let results = checker.check_all_backends().await;
    assert_eq!(results.len(), 1);
    mock.assert_async().await;

    // Should have received at least one broadcast (backend status update)
    let update = rx.try_recv();
    assert!(update.is_ok());
}

#[tokio::test]
async fn test_parse_and_enrich_llamacpp_not_ok() {
    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "llamacpp-not-ok".to_string(),
        "LlamaCpp".to_string(),
        "http://localhost:8080".to_string(),
        BackendType::LlamaCpp,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    registry.add_backend(backend).unwrap();

    let checker = HealthChecker::with_client(
        registry.clone(),
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );

    let backend_ref = registry.get_backend("llamacpp-not-ok").unwrap();
    let result = checker
        .parse_and_enrich(&backend_ref, r#"{"status":"loading"}"#, 50)
        .await;
    assert!(matches!(result, HealthCheckResult::Failure { .. }));
}

#[tokio::test]
async fn test_parse_and_enrich_llamacpp_status_ok() {
    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "llamacpp-healthy".to_string(),
        "LlamaCpp".to_string(),
        "http://localhost:8080".to_string(),
        BackendType::LlamaCpp,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    registry.add_backend(backend).unwrap();

    let checker = HealthChecker::with_client(
        registry.clone(),
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );

    let backend_ref = registry.get_backend("llamacpp-healthy").unwrap();
    let result = checker
        .parse_and_enrich(&backend_ref, r#"{"status":"ok"}"#, 50)
        .await;
    match result {
        HealthCheckResult::Success { latency_ms, models } => {
            assert_eq!(latency_ms, 50);
            assert!(models.is_empty());
        }
        other => panic!("Expected Success, got {:?}", other),
    }
}

#[tokio::test]
async fn test_parse_and_enrich_llamacpp_bad_json() {
    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "llamacpp-bad-json".to_string(),
        "LlamaCpp".to_string(),
        "http://localhost:8080".to_string(),
        BackendType::LlamaCpp,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    registry.add_backend(backend).unwrap();

    let checker = HealthChecker::with_client(
        registry.clone(),
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );

    let backend_ref = registry.get_backend("llamacpp-bad-json").unwrap();
    let result = checker.parse_and_enrich(&backend_ref, "not json", 50).await;
    assert!(matches!(
        result,
        HealthCheckResult::SuccessWithParseError { .. }
    ));
}

#[tokio::test]
async fn test_parse_and_enrich_vllm_bad_json() {
    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "vllm-bad-json".to_string(),
        "VLLM".to_string(),
        "http://localhost:8000".to_string(),
        BackendType::VLLM,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    registry.add_backend(backend).unwrap();

    let checker = HealthChecker::with_client(
        registry.clone(),
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );

    let backend_ref = registry.get_backend("vllm-bad-json").unwrap();
    let result = checker.parse_and_enrich(&backend_ref, "not json", 50).await;
    assert!(matches!(
        result,
        HealthCheckResult::SuccessWithParseError { .. }
    ));
}

#[tokio::test]
async fn test_parse_and_enrich_ollama_bad_json() {
    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "ollama-bad-json".to_string(),
        "Ollama".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    registry.add_backend(backend).unwrap();

    let checker = HealthChecker::with_client(
        registry.clone(),
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );

    let backend_ref = registry.get_backend("ollama-bad-json").unwrap();
    let result = checker.parse_and_enrich(&backend_ref, "not json", 50).await;
    assert!(matches!(
        result,
        HealthCheckResult::SuccessWithParseError { .. }
    ));
}

#[tokio::test]
async fn test_broadcast_model_change_with_models() {
    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "b-model-change".to_string(),
        "ModelChange".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    registry.add_backend(backend).unwrap();

    let (tx, mut rx) = tokio::sync::broadcast::channel(16);
    let checker = HealthChecker::with_client(
        registry.clone(),
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    )
    .with_broadcast(tx);

    let models = vec![Model {
        id: "llama3:8b".to_string(),
        name: "llama3:8b".to_string(),
        context_length: 4096,
        supports_vision: false,
        supports_tools: false,
        supports_json_mode: false,
        max_output_tokens: None,
    }];

    checker.broadcast_model_change("b-model-change", &models);

    let update = rx.try_recv();
    assert!(update.is_ok());
}

#[test]
fn test_broadcast_model_change_no_sender() {
    let registry = Arc::new(Registry::new());
    let checker = HealthChecker::with_client(
        registry,
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );
    // Should not panic when no broadcast sender
    checker.broadcast_model_change("nonexistent", &[]);
}

#[test]
fn test_apply_result_success_with_parse_error() {
    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "parse-err-1".to_string(),
        "ParseErr".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    registry.add_backend(backend).unwrap();

    let checker = HealthChecker::with_client(
        registry.clone(),
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    );

    let result = HealthCheckResult::SuccessWithParseError {
        latency_ms: 50,
        parse_error: "bad json".to_string(),
    };
    // Should not panic
    checker.apply_result("parse-err-1", result);
}

#[test]
fn test_apply_result_failure_broadcasts_empty_model_list() {
    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "fail-bc2".to_string(),
        "FailBC2".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        vec![],
        crate::registry::DiscoverySource::Static,
        std::collections::HashMap::new(),
    );
    registry.add_backend(backend).unwrap();

    let (tx, mut rx) = tokio::sync::broadcast::channel(16);
    let checker = HealthChecker::with_client(
        registry.clone(),
        HealthCheckConfig::default(),
        reqwest::Client::new(),
    )
    .with_broadcast(tx);

    // First apply success to populate last_models
    let success = HealthCheckResult::Success {
        latency_ms: 50,
        models: vec![Model {
            id: "test".to_string(),
            name: "test".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }],
    };
    checker.apply_result("fail-bc2", success);
    // Drain the success broadcasts
    while rx.try_recv().is_ok() {}

    // Now apply failure - should broadcast empty models
    let failure = HealthCheckResult::Failure {
        error: HealthCheckError::Timeout(5),
    };
    checker.apply_result("fail-bc2", failure);

    let update = rx.try_recv();
    assert!(update.is_ok());
}
