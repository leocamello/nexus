//! Unit tests for health module.

use super::*;

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
    let body = r#"{"models": [{"name": "llava:13b"}]}"#;
    let models = parser::parse_ollama_response(body).unwrap();
    assert!(models[0].supports_vision);
}

#[test]
fn test_parse_ollama_tool_detection() {
    let body = r#"{"models": [{"name": "mistral:7b"}]}"#;
    let models = parser::parse_ollama_response(body).unwrap();
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
fn test_endpoint_selection_generic() {
    let endpoint = crate::health::HealthChecker::get_health_endpoint(BackendType::Generic);
    assert_eq!(endpoint, "/v1/models");
}
