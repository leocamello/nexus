# Implementation Tasks: Health Checker

**Spec**: [spec.md](./spec.md)  
**Plan**: [plan.md](./plan.md)  
**Status**: Ready for Implementation

## Task Overview

| Task | Description | Est. Time | Dependencies |
|------|-------------|-----------|--------------|
| T01 | Module scaffolding & dependencies | 1h | None |
| T02 | HealthCheckConfig & defaults | 1h | T01 |
| T03 | HealthCheckError enum | 1h | T01 |
| T04 | BackendHealthState struct | 1.5h | T01 |
| T05 | Response parsing (Ollama) | 2h | T03 |
| T06 | Response parsing (OpenAI/LlamaCpp) | 1.5h | T05 |
| T07 | Status transition logic | 2h | T04 |
| T08 | Endpoint selection & HTTP request | 2h | T03, T06 |
| T09 | Single backend check | 2.5h | T07, T08 |
| T10 | Registry integration (apply_result) | 2h | T09 |
| T11 | Main loop & graceful shutdown | 2.5h | T10 |
| T12 | Integration tests with mock server | 2.5h | T11 |
| T13 | Documentation & cleanup | 1.5h | All |

**Total Estimated Time**: ~23 hours
**Total Tests**: 59 (unit + integration)

### MVP Scope Note

These tasks implement **P0/P1 features only**. The following P2 features are **excluded from MVP**:
- **Staggered checks (FR-012, US6)**: Post-MVP enhancement to prevent thundering herd
- **Graceful shutdown optimization (US7)**: Basic cancellation is included; advanced completion guarantees are post-MVP

---

## T01: Module Scaffolding & Dependencies

**Goal**: Create module structure and add tokio-util dependency.

**Files to create/modify**:
- `src/lib.rs` (modify - add health module)
- `src/health/mod.rs` (create)
- `src/health/config.rs` (create, placeholder)
- `src/health/error.rs` (create, placeholder)
- `src/health/state.rs` (create, placeholder)
- `src/health/parser.rs` (create, placeholder)
- `src/health/tests.rs` (create, placeholder)
- `Cargo.toml` (add tokio-util)

**Implementation Steps**:
1. Add to `Cargo.toml`:
   ```toml
   tokio-util = { version = "0.7", features = ["rt"] }
   ```
2. Update `src/lib.rs`:
   ```rust
   pub mod registry;
   pub mod health;
   ```
3. Create `src/health/mod.rs`:
   ```rust
   mod config;
   mod error;
   mod state;
   mod parser;
   #[cfg(test)]
   mod tests;
   
   pub use config::*;
   pub use error::*;
   pub use state::*;
   ```
4. Create placeholder files with minimal content
5. Run `cargo check` to verify structure compiles

**Acceptance Criteria**:
- [ ] `cargo check` passes with no errors
- [ ] Module structure matches plan's file layout
- [ ] tokio-util is available in dependencies

**Test Command**: `cargo check`

---

## T02: HealthCheckConfig & Defaults

**Goal**: Implement configuration struct with serialization and default values.

**Files to modify**:
- `src/health/config.rs`
- `src/health/tests.rs`

**Tests to Write First**:
```rust
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
}
```

**Implementation**:
```rust
// src/health/config.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HealthCheckConfig {
    pub enabled: bool,
    pub interval_seconds: u64,
    pub timeout_seconds: u64,
    pub failure_threshold: u32,
    pub recovery_threshold: u32,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: 30,
            timeout_seconds: 5,
            failure_threshold: 3,
            recovery_threshold: 2,
        }
    }
}
```

**Acceptance Criteria**:
- [ ] All 4 tests pass
- [ ] Config can be parsed from TOML
- [ ] Default values match spec

**Test Command**: `cargo test health::tests::test_config`

---

## T03: HealthCheckError Enum

**Goal**: Define error types with thiserror for display messages.

**Files to modify**:
- `src/health/error.rs`
- `src/health/tests.rs`

**Tests to Write First**:
```rust
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
    assert_eq!(err.to_string(), "TLS certificate error: certificate expired");
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
```

**Implementation**:
```rust
// src/health/error.rs
use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum HealthCheckError {
    #[error("request timeout after {0}s")]
    Timeout(u64),
    
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    
    #[error("DNS resolution failed: {0}")]
    DnsError(String),
    
    #[error("TLS certificate error: {0}")]
    TlsError(String),
    
    #[error("HTTP error: {0}")]
    HttpError(u16),
    
    #[error("invalid response: {0}")]
    ParseError(String),
}
```

**Acceptance Criteria**:
- [ ] All 6 tests pass
- [ ] Error implements std::error::Error
- [ ] Display messages match spec

**Test Command**: `cargo test health::tests::test_error`

---

## T04: BackendHealthState Struct

**Goal**: Implement per-backend tracking state with default values.

**Files to modify**:
- `src/health/state.rs`
- `src/health/tests.rs`

**Tests to Write First**:
```rust
#[test]
fn test_state_default() {
    let state = BackendHealthState::default();
    assert_eq!(state.consecutive_failures, 0);
    assert_eq!(state.consecutive_successes, 0);
    assert!(state.last_check_time.is_none());
    assert_eq!(state.last_status, BackendStatus::Unknown);
    assert!(state.last_models.is_empty());
}

#[test]
fn test_state_clone() {
    let mut state = BackendHealthState::default();
    state.consecutive_failures = 2;
    let cloned = state.clone();
    assert_eq!(cloned.consecutive_failures, 2);
}
```

**Implementation**:
```rust
// src/health/state.rs
use chrono::{DateTime, Utc};
use crate::registry::{BackendStatus, Model};

#[derive(Debug, Clone)]
pub struct BackendHealthState {
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub last_check_time: Option<DateTime<Utc>>,
    pub last_status: BackendStatus,
    pub last_models: Vec<Model>,
}

impl Default for BackendHealthState {
    fn default() -> Self {
        Self {
            consecutive_failures: 0,
            consecutive_successes: 0,
            last_check_time: None,
            last_status: BackendStatus::Unknown,
            last_models: Vec::new(),
        }
    }
}
```

**Acceptance Criteria**:
- [ ] All 2 tests pass
- [ ] State references existing registry types
- [ ] Default values match spec

**Test Command**: `cargo test health::tests::test_state`

---

## T05: Response Parsing (Ollama)

**Goal**: Parse Ollama /api/tags response format into Vec<Model>.

**Files to modify**:
- `src/health/parser.rs`
- `src/health/tests.rs`

**Tests to Write First**:
```rust
#[test]
fn test_parse_ollama_single_model() {
    let body = r#"{"models": [{"name": "llama3:70b"}]}"#;
    let models = parse_ollama_response(body).unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "llama3:70b");
    assert_eq!(models[0].name, "llama3:70b");
}

#[test]
fn test_parse_ollama_multiple_models() {
    let body = r#"{"models": [{"name": "llama3:70b"}, {"name": "mistral:7b"}]}"#;
    let models = parse_ollama_response(body).unwrap();
    assert_eq!(models.len(), 2);
}

#[test]
fn test_parse_ollama_empty_list() {
    let body = r#"{"models": []}"#;
    let models = parse_ollama_response(body).unwrap();
    assert!(models.is_empty());
}

#[test]
fn test_parse_ollama_invalid_json() {
    let body = "not json";
    let result = parse_ollama_response(body);
    assert!(matches!(result, Err(HealthCheckError::ParseError(_))));
}

#[test]
fn test_parse_ollama_vision_detection() {
    let body = r#"{"models": [{"name": "llava:13b"}]}"#;
    let models = parse_ollama_response(body).unwrap();
    assert!(models[0].supports_vision);
}

#[test]
fn test_parse_ollama_tool_detection() {
    let body = r#"{"models": [{"name": "mistral:7b"}]}"#;
    let models = parse_ollama_response(body).unwrap();
    assert!(models[0].supports_tools);
}

#[test]
fn test_parse_ollama_default_context_length() {
    let body = r#"{"models": [{"name": "llama3:70b"}]}"#;
    let models = parse_ollama_response(body).unwrap();
    assert_eq!(models[0].context_length, 4096);
}
```

**Implementation**:
```rust
// src/health/parser.rs
use serde::Deserialize;
use crate::registry::Model;
use super::error::HealthCheckError;

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

pub fn parse_ollama_response(body: &str) -> Result<Vec<Model>, HealthCheckError> {
    let response: OllamaTagsResponse = serde_json::from_str(body)
        .map_err(|e| HealthCheckError::ParseError(e.to_string()))?;
    
    Ok(response.models.into_iter().map(|m| {
        let name_lower = m.name.to_lowercase();
        let supports_vision = name_lower.contains("llava") || name_lower.contains("vision");
        let supports_tools = name_lower.contains("mistral");
        
        Model {
            id: m.name.clone(),
            name: m.name,
            context_length: 4096,
            supports_vision,
            supports_tools,
            supports_json_mode: false,
            max_output_tokens: None,
        }
    }).collect())
}
```

**Acceptance Criteria**:
- [ ] All 7 tests pass
- [ ] Vision model detection works (llava, vision)
- [ ] Tool model detection works (mistral)

**Test Command**: `cargo test health::tests::test_parse_ollama`

---

## T06: Response Parsing (OpenAI/LlamaCpp)

**Goal**: Parse OpenAI /v1/models and LlamaCpp /health responses.

**Files to modify**:
- `src/health/parser.rs`
- `src/health/tests.rs`

**Tests to Write First**:
```rust
#[test]
fn test_parse_openai_single_model() {
    let body = r#"{"data": [{"id": "gpt-4", "object": "model"}]}"#;
    let models = parse_openai_response(body).unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "gpt-4");
}

#[test]
fn test_parse_openai_multiple_models() {
    let body = r#"{"data": [{"id": "gpt-4"}, {"id": "gpt-3.5-turbo"}]}"#;
    let models = parse_openai_response(body).unwrap();
    assert_eq!(models.len(), 2);
}

#[test]
fn test_parse_openai_empty_data() {
    let body = r#"{"data": []}"#;
    let models = parse_openai_response(body).unwrap();
    assert!(models.is_empty());
}

#[test]
fn test_parse_openai_invalid_json() {
    let body = "not json";
    let result = parse_openai_response(body);
    assert!(matches!(result, Err(HealthCheckError::ParseError(_))));
}

#[test]
fn test_parse_llamacpp_healthy() {
    let body = r#"{"status": "ok"}"#;
    let is_healthy = parse_llamacpp_response(body).unwrap();
    assert!(is_healthy);
}

#[test]
fn test_parse_llamacpp_error_status() {
    let body = r#"{"status": "error"}"#;
    let is_healthy = parse_llamacpp_response(body).unwrap();
    assert!(!is_healthy);
}

#[test]
fn test_parse_llamacpp_invalid_json() {
    let body = "not json";
    let result = parse_llamacpp_response(body);
    assert!(matches!(result, Err(HealthCheckError::ParseError(_))));
}
```

**Implementation**:
```rust
// Add to src/health/parser.rs

#[derive(Deserialize)]
struct OpenAIModelsResponse {
    data: Vec<OpenAIModel>,
}

#[derive(Deserialize)]
struct OpenAIModel {
    id: String,
}

pub fn parse_openai_response(body: &str) -> Result<Vec<Model>, HealthCheckError> {
    let response: OpenAIModelsResponse = serde_json::from_str(body)
        .map_err(|e| HealthCheckError::ParseError(e.to_string()))?;
    
    Ok(response.data.into_iter().map(|m| {
        Model {
            id: m.id.clone(),
            name: m.id,
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }
    }).collect())
}

#[derive(Deserialize)]
struct LlamaCppHealthResponse {
    status: String,
}

pub fn parse_llamacpp_response(body: &str) -> Result<bool, HealthCheckError> {
    let response: LlamaCppHealthResponse = serde_json::from_str(body)
        .map_err(|e| HealthCheckError::ParseError(e.to_string()))?;
    
    Ok(response.status == "ok")
}
```

**Acceptance Criteria**:
- [ ] All 7 tests pass
- [ ] OpenAI format parsing works for vLLM, Exo, Generic
- [ ] LlamaCpp health check returns boolean

**Test Command**: `cargo test health::tests::test_parse_openai && cargo test health::tests::test_parse_llamacpp`

---

## T07: Status Transition Logic

**Goal**: Implement threshold-based status transition in BackendHealthState.

**Files to modify**:
- `src/health/state.rs`
- `src/health/tests.rs`

**Tests to Write First**:
```rust
#[test]
fn test_unknown_to_healthy_on_success() {
    let mut state = BackendHealthState::default();
    let config = HealthCheckConfig::default();
    let result = HealthCheckResult::Success { latency_ms: 100, models: vec![] };
    
    let new_status = state.apply_result(&result, &config);
    assert_eq!(new_status, Some(BackendStatus::Healthy));
}

#[test]
fn test_unknown_to_unhealthy_on_failure() {
    let mut state = BackendHealthState::default();
    let config = HealthCheckConfig::default();
    let result = HealthCheckResult::Failure { 
        error: HealthCheckError::Timeout(5) 
    };
    
    let new_status = state.apply_result(&result, &config);
    assert_eq!(new_status, Some(BackendStatus::Unhealthy));
}

#[test]
fn test_healthy_stays_healthy_under_threshold() {
    let mut state = BackendHealthState {
        last_status: BackendStatus::Healthy,
        ..Default::default()
    };
    let config = HealthCheckConfig::default(); // failure_threshold = 3
    let result = HealthCheckResult::Failure { 
        error: HealthCheckError::Timeout(5) 
    };
    
    // 2 failures should not transition
    state.apply_result(&result, &config);
    let new_status = state.apply_result(&result, &config);
    assert_eq!(new_status, None);
    assert_eq!(state.consecutive_failures, 2);
}

#[test]
fn test_healthy_to_unhealthy_at_threshold() {
    let mut state = BackendHealthState {
        last_status: BackendStatus::Healthy,
        ..Default::default()
    };
    let config = HealthCheckConfig::default(); // failure_threshold = 3
    let result = HealthCheckResult::Failure { 
        error: HealthCheckError::Timeout(5) 
    };
    
    // 3 failures should transition
    state.apply_result(&result, &config);
    state.apply_result(&result, &config);
    let new_status = state.apply_result(&result, &config);
    assert_eq!(new_status, Some(BackendStatus::Unhealthy));
}

#[test]
fn test_unhealthy_stays_unhealthy_under_threshold() {
    let mut state = BackendHealthState {
        last_status: BackendStatus::Unhealthy,
        ..Default::default()
    };
    let config = HealthCheckConfig::default(); // recovery_threshold = 2
    let result = HealthCheckResult::Success { latency_ms: 100, models: vec![] };
    
    // 1 success should not transition
    let new_status = state.apply_result(&result, &config);
    assert_eq!(new_status, None);
}

#[test]
fn test_unhealthy_to_healthy_at_threshold() {
    let mut state = BackendHealthState {
        last_status: BackendStatus::Unhealthy,
        ..Default::default()
    };
    let config = HealthCheckConfig::default(); // recovery_threshold = 2
    let result = HealthCheckResult::Success { latency_ms: 100, models: vec![] };
    
    // 2 successes should transition
    state.apply_result(&result, &config);
    let new_status = state.apply_result(&result, &config);
    assert_eq!(new_status, Some(BackendStatus::Healthy));
}

#[test]
fn test_success_resets_failure_counter() {
    let mut state = BackendHealthState {
        last_status: BackendStatus::Healthy,
        consecutive_failures: 2,
        ..Default::default()
    };
    let config = HealthCheckConfig::default();
    let result = HealthCheckResult::Success { latency_ms: 100, models: vec![] };
    
    state.apply_result(&result, &config);
    assert_eq!(state.consecutive_failures, 0);
}

#[test]
fn test_failure_resets_success_counter() {
    let mut state = BackendHealthState {
        last_status: BackendStatus::Unhealthy,
        consecutive_successes: 1,
        ..Default::default()
    };
    let config = HealthCheckConfig::default();
    let result = HealthCheckResult::Failure { 
        error: HealthCheckError::Timeout(5) 
    };
    
    state.apply_result(&result, &config);
    assert_eq!(state.consecutive_successes, 0);
}
```

**Implementation**:
```rust
// Add to src/health/state.rs

impl BackendHealthState {
    pub fn apply_result(
        &mut self,
        result: &HealthCheckResult,
        config: &HealthCheckConfig,
    ) -> Option<BackendStatus> {
        match result {
            HealthCheckResult::Success { .. } => {
                self.consecutive_failures = 0;
                self.consecutive_successes += 1;
                
                match self.last_status {
                    BackendStatus::Unknown => Some(BackendStatus::Healthy),
                    BackendStatus::Unhealthy 
                        if self.consecutive_successes >= config.recovery_threshold => {
                        Some(BackendStatus::Healthy)
                    }
                    _ => None,
                }
            }
            HealthCheckResult::Failure { .. } => {
                self.consecutive_successes = 0;
                self.consecutive_failures += 1;
                
                match self.last_status {
                    BackendStatus::Unknown => Some(BackendStatus::Unhealthy),
                    BackendStatus::Healthy 
                        if self.consecutive_failures >= config.failure_threshold => {
                        Some(BackendStatus::Unhealthy)
                    }
                    _ => None,
                }
            }
        }
    }
}
```

**Acceptance Criteria**:
- [ ] All 8 tests pass
- [ ] Unknown → Healthy on 1 success
- [ ] Unknown → Unhealthy on 1 failure
- [ ] Healthy → Unhealthy after 3 consecutive failures
- [ ] Unhealthy → Healthy after 2 consecutive successes
- [ ] Counters reset on opposite result

**Test Command**: `cargo test health::tests::test_transition`

---

## T08: Endpoint Selection & HTTP Request Helpers

**Goal**: Implement get_health_endpoint() and error classification.

**Files to modify**:
- `src/health/mod.rs`
- `src/health/tests.rs`

**Tests to Write First**:
```rust
#[test]
fn test_endpoint_ollama() {
    assert_eq!(
        HealthChecker::get_health_endpoint(BackendType::Ollama),
        "/api/tags"
    );
}

#[test]
fn test_endpoint_vllm() {
    assert_eq!(
        HealthChecker::get_health_endpoint(BackendType::Vllm),
        "/v1/models"
    );
}

#[test]
fn test_endpoint_llamacpp() {
    assert_eq!(
        HealthChecker::get_health_endpoint(BackendType::LlamaCpp),
        "/health"
    );
}

#[test]
fn test_endpoint_exo() {
    assert_eq!(
        HealthChecker::get_health_endpoint(BackendType::Exo),
        "/v1/models"
    );
}

#[test]
fn test_endpoint_openai() {
    assert_eq!(
        HealthChecker::get_health_endpoint(BackendType::OpenAi),
        "/v1/models"
    );
}

#[test]
fn test_endpoint_generic() {
    assert_eq!(
        HealthChecker::get_health_endpoint(BackendType::Generic),
        "/v1/models"
    );
}
```

**Implementation**:
```rust
// src/health/mod.rs
use crate::registry::BackendType;

impl HealthChecker {
    pub fn get_health_endpoint(backend_type: BackendType) -> &'static str {
        match backend_type {
            BackendType::Ollama => "/api/tags",
            BackendType::LlamaCpp => "/health",
            BackendType::Vllm 
            | BackendType::Exo 
            | BackendType::OpenAi 
            | BackendType::Generic => "/v1/models",
        }
    }
    
    pub(crate) fn classify_error(e: &reqwest::Error, timeout_secs: u64) -> HealthCheckError {
        if e.is_timeout() {
            HealthCheckError::Timeout(timeout_secs)
        } else if e.is_connect() {
            HealthCheckError::ConnectionFailed(e.to_string())
        } else if e.is_builder() {
            HealthCheckError::DnsError(e.to_string())
        } else {
            HealthCheckError::ConnectionFailed(e.to_string())
        }
    }
}
```

**Acceptance Criteria**:
- [ ] All 6 endpoint tests pass
- [ ] All backend types have defined endpoints
- [ ] Error classification distinguishes timeout from connection errors

**Test Command**: `cargo test health::tests::test_endpoint`

---

## T09: Single Backend Check

**Goal**: Implement check_backend() that sends HTTP request and parses response.

**Files to modify**:
- `src/health/mod.rs`
- `src/health/tests.rs`

**Tests to Write First** (using mock server):
```rust
#[tokio::test]
async fn test_check_backend_ollama_success() {
    let (url, _server) = start_mock_ollama_server().await;
    let checker = create_test_checker();
    let backend = create_test_backend(BackendType::Ollama, &url);
    
    let result = checker.check_backend(&backend).await;
    assert!(matches!(result, HealthCheckResult::Success { .. }));
}

#[tokio::test]
async fn test_check_backend_timeout() {
    let (url, _server) = start_slow_mock_server(10).await; // 10s delay
    let checker = create_test_checker_with_timeout(1); // 1s timeout
    let backend = create_test_backend(BackendType::Ollama, &url);
    
    let result = checker.check_backend(&backend).await;
    assert!(matches!(result, HealthCheckResult::Failure { 
        error: HealthCheckError::Timeout(_) 
    }));
}

#[tokio::test]
async fn test_check_backend_connection_refused() {
    let backend = create_test_backend(BackendType::Ollama, "http://127.0.0.1:1");
    let checker = create_test_checker();
    
    let result = checker.check_backend(&backend).await;
    assert!(matches!(result, HealthCheckResult::Failure { 
        error: HealthCheckError::ConnectionFailed(_) 
    }));
}

#[tokio::test]
async fn test_check_backend_http_500() {
    let (url, _server) = start_error_mock_server(500).await;
    let checker = create_test_checker();
    let backend = create_test_backend(BackendType::Ollama, &url);
    
    let result = checker.check_backend(&backend).await;
    assert!(matches!(result, HealthCheckResult::Failure { 
        error: HealthCheckError::HttpError(500) 
    }));
}

#[tokio::test]
async fn test_check_backend_measures_latency() {
    let (url, _server) = start_mock_ollama_server().await;
    let checker = create_test_checker();
    let backend = create_test_backend(BackendType::Ollama, &url);
    
    let result = checker.check_backend(&backend).await;
    if let HealthCheckResult::Success { latency_ms, .. } = result {
        assert!(latency_ms > 0);
    } else {
        panic!("Expected success");
    }
}
```

**Implementation**:
```rust
// src/health/mod.rs
use std::time::Instant;

impl HealthChecker {
    pub async fn check_backend(&self, backend: &Backend) -> HealthCheckResult {
        let endpoint = Self::get_health_endpoint(backend.backend_type);
        let url = format!("{}{}", backend.url, endpoint);
        
        let start = Instant::now();
        
        let response = self.client
            .get(&url)
            .timeout(Duration::from_secs(self.config.timeout_seconds))
            .send()
            .await;
            
        match response {
            Ok(resp) => {
                let latency_ms = start.elapsed().as_millis() as u32;
                
                if !resp.status().is_success() {
                    return HealthCheckResult::Failure {
                        error: HealthCheckError::HttpError(resp.status().as_u16()),
                    };
                }
                
                match resp.text().await {
                    Ok(body) => self.parse_response(backend.backend_type, &body, latency_ms),
                    Err(e) => HealthCheckResult::Failure {
                        error: HealthCheckError::ParseError(e.to_string()),
                    },
                }
            }
            Err(e) => HealthCheckResult::Failure {
                error: Self::classify_error(&e, self.config.timeout_seconds),
            },
        }
    }
    
    fn parse_response(
        &self, 
        backend_type: BackendType, 
        body: &str, 
        latency_ms: u32
    ) -> HealthCheckResult {
        let models = match backend_type {
            BackendType::Ollama => parse_ollama_response(body),
            BackendType::LlamaCpp => {
                // LlamaCpp returns health status, not models
                match parse_llamacpp_response(body) {
                    Ok(true) => Ok(vec![]),
                    Ok(false) => return HealthCheckResult::Failure {
                        error: HealthCheckError::HttpError(503),
                    },
                    Err(e) => Err(e),
                }
            }
            _ => parse_openai_response(body),
        };
        
        match models {
            Ok(models) => HealthCheckResult::Success { latency_ms, models },
            Err(e) => HealthCheckResult::Failure { error: e },
        }
    }
}
```

**Acceptance Criteria**:
- [ ] All 5 tests pass with mock HTTP server
- [ ] Latency is measured correctly
- [ ] Different backend types use correct parsers
- [ ] Timeout, connection, and HTTP errors are classified correctly

**Test Command**: `cargo test health::tests::test_check_backend`

---

## T10: Registry Integration (apply_result)

**Goal**: Implement apply_result() that updates Registry based on health check result.

**Files to modify**:
- `src/health/mod.rs`
- `src/health/tests.rs`

**Tests to Write First**:
```rust
#[test]
fn test_apply_success_updates_status() {
    let registry = Arc::new(Registry::new());
    let backend = create_test_backend_with_unknown_status();
    registry.add_backend(backend.clone()).unwrap();
    
    let checker = HealthChecker::new(registry.clone(), HealthCheckConfig::default());
    let result = HealthCheckResult::Success { 
        latency_ms: 100, 
        models: vec![] 
    };
    
    checker.apply_result(&backend.id, result);
    
    let updated = registry.get_backend(&backend.id).unwrap();
    assert_eq!(updated.status, BackendStatus::Healthy);
}

#[test]
fn test_apply_success_updates_models() {
    let registry = Arc::new(Registry::new());
    let backend = create_test_backend_with_unknown_status();
    registry.add_backend(backend.clone()).unwrap();
    
    let checker = HealthChecker::new(registry.clone(), HealthCheckConfig::default());
    let models = vec![Model { id: "test".into(), ..Default::default() }];
    let result = HealthCheckResult::Success { 
        latency_ms: 100, 
        models: models.clone() 
    };
    
    checker.apply_result(&backend.id, result);
    
    let updated = registry.get_backend(&backend.id).unwrap();
    assert_eq!(updated.models.len(), 1);
}

#[test]
fn test_apply_success_updates_latency() {
    let registry = Arc::new(Registry::new());
    let backend = create_test_backend_with_unknown_status();
    registry.add_backend(backend.clone()).unwrap();
    
    let checker = HealthChecker::new(registry.clone(), HealthCheckConfig::default());
    let result = HealthCheckResult::Success { 
        latency_ms: 150, 
        models: vec![] 
    };
    
    checker.apply_result(&backend.id, result);
    
    let updated = registry.get_backend(&backend.id).unwrap();
    assert!(updated.avg_latency_ms > 0);
}

#[test]
fn test_apply_skips_removed_backend() {
    let registry = Arc::new(Registry::new());
    let checker = HealthChecker::new(registry.clone(), HealthCheckConfig::default());
    let result = HealthCheckResult::Success { 
        latency_ms: 100, 
        models: vec![] 
    };
    
    // Should not panic for non-existent backend
    checker.apply_result("non-existent-id", result);
}

#[test]
fn test_apply_preserves_models_on_parse_error() {
    let registry = Arc::new(Registry::new());
    let models = vec![Model { id: "existing".into(), ..Default::default() }];
    let mut backend = create_test_backend_with_unknown_status();
    backend.models = models.clone();
    registry.add_backend(backend.clone()).unwrap();
    
    let checker = HealthChecker::new(registry.clone(), HealthCheckConfig::default());
    
    // First, establish state with models
    checker.apply_result(&backend.id, HealthCheckResult::Success { 
        latency_ms: 100, 
        models: models.clone() 
    });
    
    // Then fail with parse error - models should be preserved
    checker.apply_result(&backend.id, HealthCheckResult::Failure { 
        error: HealthCheckError::ParseError("bad json".into()) 
    });
    
    let state = checker.state.get(&backend.id).unwrap();
    assert_eq!(state.last_models.len(), 1);
}
```

**Implementation**:
```rust
// src/health/mod.rs

impl HealthChecker {
    pub fn apply_result(&self, backend_id: &str, result: HealthCheckResult) {
        // Get or create backend state
        let mut state = self.state
            .entry(backend_id.to_string())
            .or_insert_with(BackendHealthState::default);
        
        // Determine if status should transition
        let new_status = state.apply_result(&result, &self.config);
        state.last_check_time = Some(Utc::now());
        
        // Update registry based on result
        match &result {
            HealthCheckResult::Success { latency_ms, models } => {
                // Update latency
                let _ = self.registry.update_latency(backend_id, *latency_ms);
                
                // Update models if we got any
                if !models.is_empty() {
                    if self.registry.update_models(backend_id, models.clone()).is_ok() {
                        state.last_models = models.clone();
                    }
                }
            }
            HealthCheckResult::Failure { .. } => {
                // Keep last_models for recovery
            }
        }
        
        // Update status if transition occurred
        if let Some(status) = new_status {
            let error_msg = match &result {
                HealthCheckResult::Failure { error } => Some(error.to_string()),
                _ => None,
            };
            
            if self.registry.update_status(backend_id, status, error_msg).is_ok() {
                let old_status = state.last_status;
                state.last_status = status;
                
                tracing::info!(
                    backend_id = backend_id,
                    old_status = ?old_status,
                    new_status = ?status,
                    "Backend status changed"
                );
            }
        }
    }
}
```

**Acceptance Criteria**:
- [ ] All 5 tests pass
- [ ] Status updates go to registry
- [ ] Model updates go to registry
- [ ] Latency updates go to registry
- [ ] Removed backends don't cause panic

**Test Command**: `cargo test health::tests::test_apply`

---

## T11: Main Loop & Graceful Shutdown

**Goal**: Implement background check loop with cancellation support.

**Files to modify**:
- `src/health/mod.rs`
- `src/health/tests.rs`

**Tests to Write First**:
```rust
#[tokio::test]
async fn test_start_returns_join_handle() {
    let registry = Arc::new(Registry::new());
    let checker = HealthChecker::new(registry, HealthCheckConfig::default());
    let cancel = CancellationToken::new();
    
    let handle = checker.start(cancel.clone());
    cancel.cancel();
    
    // Should complete without error
    handle.await.unwrap();
}

#[tokio::test]
async fn test_cancellation_stops_loop() {
    let registry = Arc::new(Registry::new());
    let mut config = HealthCheckConfig::default();
    config.interval_seconds = 1; // Fast interval for testing
    
    let checker = HealthChecker::new(registry, config);
    let cancel = CancellationToken::new();
    
    let handle = checker.start(cancel.clone());
    
    // Let it run briefly
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Cancel and verify it stops
    cancel.cancel();
    
    let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
    assert!(result.is_ok(), "Should stop within timeout");
}

#[tokio::test]
async fn test_loop_checks_all_backends() {
    let registry = Arc::new(Registry::new());
    
    // Add a mock backend
    let (url, _server) = start_mock_ollama_server().await;
    let backend = create_test_backend(BackendType::Ollama, &url);
    registry.add_backend(backend.clone()).unwrap();
    
    let mut config = HealthCheckConfig::default();
    config.interval_seconds = 1;
    
    let checker = HealthChecker::new(registry.clone(), config);
    let cancel = CancellationToken::new();
    
    let handle = checker.start(cancel.clone());
    
    // Wait for at least one check cycle
    tokio::time::sleep(Duration::from_millis(1500)).await;
    cancel.cancel();
    handle.await.unwrap();
    
    // Verify backend was checked
    let updated = registry.get_backend(&backend.id).unwrap();
    assert_eq!(updated.status, BackendStatus::Healthy);
}

#[tokio::test]
async fn test_loop_handles_empty_registry() {
    let registry = Arc::new(Registry::new());
    let mut config = HealthCheckConfig::default();
    config.interval_seconds = 1;
    
    let checker = HealthChecker::new(registry, config);
    let cancel = CancellationToken::new();
    
    let handle = checker.start(cancel.clone());
    
    // Should not panic with empty registry
    tokio::time::sleep(Duration::from_millis(100)).await;
    cancel.cancel();
    handle.await.unwrap();
}

#[tokio::test]
async fn test_check_all_backends_returns_results() {
    let registry = Arc::new(Registry::new());
    let (url, _server) = start_mock_ollama_server().await;
    let backend = create_test_backend(BackendType::Ollama, &url);
    registry.add_backend(backend.clone()).unwrap();
    
    let checker = HealthChecker::new(registry, HealthCheckConfig::default());
    let results = checker.check_all_backends().await;
    
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, backend.id);
}
```

**Implementation**:
```rust
// src/health/mod.rs
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

impl HealthChecker {
    pub fn start(self, cancel_token: CancellationToken) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                Duration::from_secs(self.config.interval_seconds)
            );
            
            // Skip first tick (fires immediately)
            interval.tick().await;
            
            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        tracing::info!("Health checker shutting down");
                        break;
                    }
                    _ = interval.tick() => {
                        self.check_all_backends().await;
                    }
                }
            }
        })
    }
    
    pub async fn check_all_backends(&self) -> Vec<(String, HealthCheckResult)> {
        let backends: Vec<_> = self.registry
            .get_all_backends()
            .into_iter()
            .collect();
        
        if backends.is_empty() {
            tracing::debug!("No backends to check");
            return vec![];
        }
        
        let mut results = Vec::with_capacity(backends.len());
        
        for backend in backends {
            let id = backend.id.clone();
            let result = self.check_backend(&backend).await;
            self.apply_result(&id, result.clone());
            results.push((id, result));
        }
        
        tracing::debug!(count = results.len(), "Health check cycle complete");
        results
    }
}
```

**Acceptance Criteria**:
- [ ] All 5 tests pass
- [ ] Loop respects cancellation token
- [ ] Empty registry doesn't panic
- [ ] All backends are checked each cycle

**Test Command**: `cargo test health::tests::test_loop`

---

## T12: Integration Tests with Mock Server

**Goal**: End-to-end tests verifying full health check cycle.

**Files to create**:
- `tests/health_integration.rs`

**Tests to Write**:
```rust
// tests/health_integration.rs

use axum::{Router, routing::get, Json};
use nexus::health::*;
use nexus::registry::*;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

async fn mock_ollama_tags() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "models": [{"name": "llama3:70b"}, {"name": "mistral:7b"}]
    }))
}

async fn start_mock_backend() -> (String, tokio::task::JoinHandle<()>) {
    let app = Router::new()
        .route("/api/tags", get(mock_ollama_tags));
    
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    
    (format!("http://{}", addr), handle)
}

#[tokio::test]
async fn test_full_health_check_cycle() {
    let (url, _server) = start_mock_backend().await;
    
    let registry = Arc::new(Registry::new());
    let backend = Backend::new("test", &url, BackendType::Ollama);
    registry.add_backend(backend.clone()).unwrap();
    
    let mut config = HealthCheckConfig::default();
    config.interval_seconds = 1;
    
    let checker = HealthChecker::new(registry.clone(), config);
    let cancel = CancellationToken::new();
    let handle = checker.start(cancel.clone());
    
    // Wait for check cycle
    tokio::time::sleep(Duration::from_millis(1500)).await;
    cancel.cancel();
    handle.await.unwrap();
    
    // Verify registry updated
    let updated = registry.get_backend(&backend.id).unwrap();
    assert_eq!(updated.status, BackendStatus::Healthy);
    assert_eq!(updated.models.len(), 2);
}

#[tokio::test]
async fn test_status_transition_thresholds() {
    // Test that status doesn't flip immediately
    // Requires mock that alternates success/failure
}

#[tokio::test]
async fn test_model_discovery_updates_registry() {
    // Verify models from response appear in registry
}

#[tokio::test]
async fn test_graceful_shutdown_no_leaks() {
    // Verify clean shutdown with no hanging tasks
}

#[test]
fn test_memory_overhead_per_backend() {
    // NFR-004: Verify BackendHealthState < 5KB per backend
    let state = BackendHealthState::default();
    let size = std::mem::size_of_val(&state);
    assert!(size < 5 * 1024, "BackendHealthState should be < 5KB, was {} bytes", size);
}
```

**Acceptance Criteria**:
- [ ] Full cycle test passes
- [ ] Status transition thresholds work correctly
- [ ] Model discovery populates registry
- [ ] Graceful shutdown completes cleanly
- [ ] Memory overhead per backend < 5KB (NFR-004)

**Test Command**: `cargo test --test health_integration`

---

## T13: Documentation & Cleanup

**Goal**: Add documentation, run lints, and finalize implementation.

**Tasks**:
1. Add doc comments to all public types:
   - `HealthChecker` - struct and all public methods
   - `HealthCheckConfig` - struct and fields
   - `HealthCheckError` - enum and variants
   - `BackendHealthState` - struct and `apply_result()`
   - Parser functions

2. Add module-level documentation:
   ```rust
   //! Health checking for LLM backends.
   //!
   //! This module provides background health monitoring for registered backends.
   //! It periodically checks each backend's health endpoint and updates the
   //! registry with status and model information.
   //!
   //! # Example
   //!
   //! ```rust,no_run
   //! use nexus::health::{HealthChecker, HealthCheckConfig};
   //! use nexus::registry::Registry;
   //! use std::sync::Arc;
   //! use tokio_util::sync::CancellationToken;
   //!
   //! #[tokio::main]
   //! async fn main() {
   //!     let registry = Arc::new(Registry::new());
   //!     let checker = HealthChecker::new(registry, HealthCheckConfig::default());
   //!     let cancel = CancellationToken::new();
   //!     let handle = checker.start(cancel.clone());
   //!     
   //!     // ... later ...
   //!     cancel.cancel();
   //!     handle.await.unwrap();
   //! }
   //! ```
   ```

3. Run lints and formatting:
   ```bash
   cargo clippy --all-features -- -D warnings
   cargo fmt --all -- --check
   cargo test
   cargo doc --no-deps
   ```

4. Update `src/lib.rs` exports:
   ```rust
   pub mod registry;
   pub mod health;
   
   pub use registry::{Registry, Backend, Model, BackendType, BackendStatus};
   pub use health::{HealthChecker, HealthCheckConfig};
   ```

**Acceptance Criteria**:
- [ ] Zero clippy warnings
- [ ] Code formatted with rustfmt
- [ ] All tests pass (48+ tests)
- [ ] Doc comments on all public items
- [ ] Module docs with example compiles
- [ ] `cargo doc` generates documentation

**Test Command**: `cargo clippy && cargo fmt -- --check && cargo test && cargo doc`

---

## Summary

| Metric | Value |
|--------|-------|
| Total Tasks | 13 |
| Total Tests | ~48 |
| Estimated Time | ~23 hours |
| Critical Path | T01 → T02/T03/T04 → T05/T06 → T07 → T08 → T09 → T10 → T11 → T12 → T13 |

### Dependency Graph

```
T01 (scaffolding)
 ├── T02 (config)
 ├── T03 (error) ────────┐
 └── T04 (state)         │
      │                  │
      ▼                  ▼
     T07 (transitions)  T05 (ollama parser)
      │                  │
      │                  ▼
      │                 T06 (openai/llamacpp)
      │                  │
      └────────┬─────────┘
               ▼
              T08 (endpoints)
               │
               ▼
              T09 (check_backend)
               │
               ▼
              T10 (apply_result)
               │
               ▼
              T11 (main loop)
               │
               ▼
              T12 (integration)
               │
               ▼
              T13 (docs)
```
