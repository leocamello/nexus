# Implementation Plan: Health Checker

**Spec**: [spec.md](./spec.md)  
**Status**: Ready for Implementation  
**Estimated Complexity**: Medium-High  
**Depends On**: F02 (Backend Registry) ✅ Implemented

## Approach

Implement the Health Checker as a background Tokio task that periodically polls all registered backends. Follow strict TDD: write failing tests first, then implement to make them pass.

### Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Background task | `tokio::spawn` + `CancellationToken` | Standard Tokio pattern for graceful shutdown |
| HTTP client | `reqwest::Client` with pooling | Reuse connections, configurable timeout |
| Per-backend state | `DashMap<String, BackendHealthState>` | Lock-free concurrent access, matches Registry pattern |
| Response parsing | `serde_json::from_str` | Direct JSON parsing, no extra dependencies |
| Endpoint dispatch | Match on `BackendType` enum | Compile-time exhaustive checking |
| Error classification | Pattern match on `reqwest::Error` | Distinguish timeout vs DNS vs connection errors |

### File Structure

```
src/
├── lib.rs                  # Add `pub mod health;`
└── health/
    ├── mod.rs              # HealthChecker struct and main loop
    ├── config.rs           # HealthCheckConfig with defaults
    ├── state.rs            # BackendHealthState tracking
    ├── error.rs            # HealthCheckError enum
    ├── parser.rs           # Ollama/OpenAI response parsing
    └── tests.rs            # Unit tests (#[cfg(test)])
```

### Dependencies

**Already in `Cargo.toml`**:
- `reqwest = { features = ["json"] }` ✓
- `tokio = { features = ["full"] }` ✓
- `serde = { features = ["derive"] }` ✓
- `serde_json` ✓
- `tracing` ✓
- `chrono` ✓
- `dashmap` ✓

**New dependency needed**:
```toml
tokio-util = { version = "0.7", features = ["rt"] }  # For CancellationToken
```

## Implementation Phases

### Phase 1: Configuration & Error Types (Tests First)

**Goal**: Define HealthCheckConfig, HealthCheckError, and BackendHealthState.

**Tests to write first**:
1. `test_config_default_values` - Default config has expected values
2. `test_config_serde_roundtrip` - Config serializes to/from TOML
3. `test_error_display_messages` - Each error variant has correct message
4. `test_state_default` - BackendHealthState::default() has zeroed counters

**Implementation**:
```rust
// src/health/config.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
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

**Acceptance**: All 4 tests pass.

---

### Phase 2: Response Parsing (Tests First)

**Goal**: Parse Ollama and OpenAI response formats into `Vec<Model>`.

**Tests to write first**:
1. `test_parse_ollama_single_model` - Parse single model from Ollama response
2. `test_parse_ollama_multiple_models` - Parse multiple models
3. `test_parse_ollama_empty_list` - Empty models array returns empty Vec
4. `test_parse_ollama_invalid_json` - Invalid JSON returns ParseError
5. `test_parse_openai_single_model` - Parse single model from OpenAI response
6. `test_parse_openai_multiple_models` - Parse multiple models
7. `test_parse_openai_empty_data` - Empty data array returns empty Vec
8. `test_parse_llamacpp_healthy` - Parse llama.cpp health response
9. `test_vision_model_detection` - "llava" in name sets supports_vision=true
10. `test_tool_model_detection` - "mistral" in name sets supports_tools=true

**Implementation**:
```rust
// src/health/parser.rs

/// Ollama /api/tags response format
#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
    #[serde(default)]
    details: Option<OllamaModelDetails>,
}

/// Parse Ollama /api/tags response into Model structs
pub fn parse_ollama_response(body: &str) -> Result<Vec<Model>, HealthCheckError> {
    let response: OllamaTagsResponse = serde_json::from_str(body)
        .map_err(|e| HealthCheckError::ParseError(e.to_string()))?;
    
    Ok(response.models.into_iter().map(|m| {
        let supports_vision = m.name.to_lowercase().contains("llava") 
            || m.name.to_lowercase().contains("vision");
        let supports_tools = m.name.to_lowercase().contains("mistral");
        
        Model {
            id: m.name.clone(),
            name: m.name,
            context_length: 4096,  // Ollama doesn't expose this
            supports_vision,
            supports_tools,
            supports_json_mode: false,
            max_output_tokens: None,
        }
    }).collect())
}
```

**Acceptance**: All 10 tests pass.

---

### Phase 3: Status Transitions (Tests First)

**Goal**: Implement threshold-based status transition logic.

**Tests to write first**:
1. `test_unknown_to_healthy_on_success` - 1 success transitions Unknown → Healthy
2. `test_unknown_to_unhealthy_on_failure` - 1 failure transitions Unknown → Unhealthy
3. `test_healthy_stays_healthy_under_threshold` - 2 failures keeps Healthy
4. `test_healthy_to_unhealthy_at_threshold` - 3 consecutive failures → Unhealthy
5. `test_unhealthy_stays_unhealthy_under_threshold` - 1 success keeps Unhealthy
6. `test_unhealthy_to_healthy_at_threshold` - 2 consecutive successes → Healthy
7. `test_success_resets_failure_counter` - Success after 2 failures resets to 0
8. `test_failure_resets_success_counter` - Failure after 1 success resets to 0

**Implementation**:
```rust
// src/health/state.rs

impl BackendHealthState {
    /// Apply a health check result and determine if status should transition.
    /// Returns Some(new_status) if transition should occur, None otherwise.
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
                    BackendStatus::Unhealthy if 
                        self.consecutive_successes >= config.recovery_threshold => {
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
                    BackendStatus::Healthy if 
                        self.consecutive_failures >= config.failure_threshold => {
                        Some(BackendStatus::Unhealthy)
                    }
                    _ => None,
                }
            }
        }
    }
}
```

**Acceptance**: All 8 tests pass.

---

### Phase 4: Single Backend Check (Tests First)

**Goal**: Implement `check_backend()` that sends HTTP request and returns result.

**Tests to write first**:
1. `test_check_backend_success` - Mock 200 response → Success with latency
2. `test_check_backend_timeout` - Mock timeout → Failure with Timeout error
3. `test_check_backend_connection_refused` - Mock refused → ConnectionFailed
4. `test_check_backend_http_500` - Mock 500 → Failure with HttpError(500)
5. `test_endpoint_selection_ollama` - Ollama backend uses /api/tags
6. `test_endpoint_selection_vllm` - vLLM backend uses /v1/models
7. `test_endpoint_selection_llamacpp` - LlamaCpp uses /health
8. `test_latency_measurement` - Latency reflects actual request time

**Implementation**:
```rust
// src/health/mod.rs

impl HealthChecker {
    /// Get the health check endpoint for a backend type
    pub fn get_health_endpoint(backend_type: BackendType) -> &'static str {
        match backend_type {
            BackendType::Ollama => "/api/tags",
            BackendType::LlamaCpp => "/health",
            BackendType::Vllm | BackendType::Exo | 
            BackendType::OpenAi | BackendType::Generic => "/v1/models",
        }
    }
    
    /// Check a single backend's health
    pub async fn check_backend(&self, backend: &Backend) -> HealthCheckResult {
        let endpoint = Self::get_health_endpoint(backend.backend_type);
        let url = format!("{}{}", backend.url, endpoint);
        
        let start = Instant::now();
        
        match self.client
            .get(&url)
            .timeout(Duration::from_secs(self.config.timeout_seconds))
            .send()
            .await
        {
            Ok(response) => {
                let latency_ms = start.elapsed().as_millis() as u32;
                
                if !response.status().is_success() {
                    return HealthCheckResult::Failure {
                        error: HealthCheckError::HttpError(response.status().as_u16()),
                    };
                }
                
                // Parse response based on backend type
                match response.text().await {
                    Ok(body) => self.parse_response(backend.backend_type, &body, latency_ms),
                    Err(e) => HealthCheckResult::Failure {
                        error: HealthCheckError::ParseError(e.to_string()),
                    },
                }
            }
            Err(e) => HealthCheckResult::Failure {
                error: Self::classify_error(e),
            },
        }
    }
    
    fn classify_error(e: reqwest::Error) -> HealthCheckError {
        if e.is_timeout() {
            HealthCheckError::Timeout(5)  // Use config value
        } else if e.is_connect() {
            HealthCheckError::ConnectionFailed(e.to_string())
        } else {
            HealthCheckError::ConnectionFailed(e.to_string())
        }
    }
}
```

**Acceptance**: All 8 tests pass with mock HTTP server.

---

### Phase 5: Registry Integration (Tests First)

**Goal**: Implement `apply_result()` that updates Registry based on check result.

**Tests to write first**:
1. `test_apply_success_updates_status` - Success updates registry status to Healthy
2. `test_apply_success_updates_models` - Success updates registry models
3. `test_apply_success_updates_latency` - Success calls registry.update_latency()
4. `test_apply_failure_updates_status` - Failure at threshold updates status
5. `test_apply_preserves_models_on_parse_error` - Parse error keeps last models
6. `test_apply_logs_transition` - Status transition logs at INFO level
7. `test_apply_skips_removed_backend` - Removed backend is handled gracefully

**Implementation**:
```rust
// src/health/mod.rs

impl HealthChecker {
    /// Apply health check result to registry
    pub fn apply_result(&self, backend_id: &str, result: HealthCheckResult) {
        // Get or create backend state
        let mut state = self.state
            .entry(backend_id.to_string())
            .or_insert_with(BackendHealthState::default);
        
        // Determine if status should transition
        let new_status = state.apply_result(&result, &self.config);
        state.last_check_time = Some(Utc::now());
        
        // Update registry
        match &result {
            HealthCheckResult::Success { latency_ms, models } => {
                // Update latency
                self.registry.update_latency(backend_id, *latency_ms);
                
                // Update models (or preserve on empty for parse error fallback)
                if !models.is_empty() || state.last_models.is_empty() {
                    if self.registry.update_models(backend_id, models.clone()).is_ok() {
                        state.last_models = models.clone();
                    }
                }
            }
            HealthCheckResult::Failure { .. } => {
                // Models preserved in state.last_models
            }
        }
        
        // Update status if transition occurred
        if let Some(status) = new_status {
            let error = match &result {
                HealthCheckResult::Failure { error } => Some(error.to_string()),
                _ => None,
            };
            
            if self.registry.update_status(backend_id, status, error).is_ok() {
                tracing::info!(
                    backend_id = backend_id,
                    old_status = ?state.last_status,
                    new_status = ?status,
                    "Backend status changed"
                );
                state.last_status = status;
            }
        }
    }
}
```

**Acceptance**: All 7 tests pass.

---

### Phase 6: Main Loop & Graceful Shutdown (Tests First)

**Goal**: Implement the background check loop with cancellation support.

**Tests to write first**:
1. `test_start_returns_join_handle` - start() returns JoinHandle
2. `test_loop_respects_interval` - Checks happen at configured interval
3. `test_cancellation_stops_loop` - CancellationToken stops the loop
4. `test_graceful_shutdown_completes_check` - In-progress check completes on shutdown
5. `test_loop_handles_empty_registry` - No panic with zero backends
6. `test_loop_checks_all_backends` - All registered backends are checked

**Implementation**:
```rust
// src/health/mod.rs

impl HealthChecker {
    /// Start the health checker background task.
    /// Returns a JoinHandle that resolves when the checker stops.
    pub fn start(self, cancel_token: CancellationToken) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                Duration::from_secs(self.config.interval_seconds)
            );
            
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
    
    /// Check all registered backends once.
    pub async fn check_all_backends(&self) -> Vec<(String, HealthCheckResult)> {
        let backends: Vec<_> = self.registry.get_all_backends()
            .into_iter()
            .map(|b| (b.id.clone(), b))
            .collect();
        
        let mut results = Vec::with_capacity(backends.len());
        
        for (id, backend) in backends {
            let result = self.check_backend(&backend).await;
            self.apply_result(&id, result.clone());
            results.push((id, result));
        }
        
        results
    }
}
```

**Acceptance**: All 6 tests pass.

---

### Phase 7: Integration Tests

**Goal**: End-to-end tests with mock HTTP servers.

**Tests in `tests/health_integration.rs`**:
1. `test_full_health_check_cycle` - Start checker, mock backends, verify registry updates
2. `test_status_transition_thresholds` - Verify 3 failures → Unhealthy, 2 successes → Healthy
3. `test_model_discovery_ollama` - Ollama response updates registry models
4. `test_model_discovery_openai` - OpenAI response updates registry models
5. `test_graceful_shutdown` - Verify no leaks, clean shutdown

**Mock Server Setup**:
```rust
use axum::{Router, routing::get, Json};

async fn mock_ollama_tags() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "models": [{"name": "llama3:70b"}]
    }))
}

async fn setup_mock_backend() -> (String, tokio::task::JoinHandle<()>) {
    let app = Router::new()
        .route("/api/tags", get(mock_ollama_tags));
    
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    
    (format!("http://{}", addr), handle)
}
```

**Acceptance**: All 5 integration tests pass.

---

### Phase 8: Documentation & Cleanup

**Goal**: Add documentation and ensure code quality.

**Tasks**:
1. Add doc comments to all public types and functions
2. Add module-level documentation with examples
3. Run `cargo clippy --all-features -- -D warnings`
4. Run `cargo fmt --all -- --check`
5. Verify all tests pass: `cargo test`
6. Update `src/lib.rs` to export health module

**Acceptance**: 
- Zero clippy warnings
- All tests pass
- Doc examples compile

---

## Test Summary

| Phase | Unit Tests | Integration Tests | Total |
|-------|------------|-------------------|-------|
| Phase 1 | 4 | 0 | 4 |
| Phase 2 | 10 | 0 | 10 |
| Phase 3 | 8 | 0 | 8 |
| Phase 4 | 8 | 0 | 8 |
| Phase 5 | 7 | 0 | 7 |
| Phase 6 | 6 | 0 | 6 |
| Phase 7 | 0 | 5 | 5 |
| **Total** | **43** | **5** | **48** |

**Note**: Tasks (tasks.md) define 59 tests total due to additional edge case coverage added during task breakdown. The 48 count above represents the plan's initial estimate; actual implementation may include more tests as detailed in tasks.md.

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| reqwest error classification incomplete | Extensive testing with real error scenarios |
| Mock server flakiness | Use deterministic ports, proper cleanup |
| Timing-sensitive tests | Use tokio::time::pause() for deterministic timing |
| Race conditions in integration tests | Sequential test execution with proper isolation |

---

## Constitution Gate Checklist

Before implementation, verify plan meets constitution requirements:

- [x] **Simplicity**: Single module with clear responsibility
- [x] **Anti-Abstraction**: Uses reqwest directly, no wrapper
- [x] **Integration-First**: API contract with Registry defined first
- [x] **Performance**: Non-blocking background task, < 5KB per backend
- [x] **Test-First**: 48 tests defined before implementation

---

## Definition of Done

- [ ] All 48 tests pass
- [ ] Zero clippy warnings
- [ ] Code formatted with rustfmt
- [ ] Public API documented with examples
- [ ] Integration with Registry verified
- [ ] Graceful shutdown works correctly
- [ ] Memory usage < 5KB per backend verified
