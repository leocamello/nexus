# Code Walkthrough: Health Checker

**Feature**: F03 - Health Checker (002-health-checker)  
**Author**: Nexus Development Team  
**Date**: 2026-02-01  
**Audience**: Developers new to the Nexus codebase

## Overview

The Health Checker is a background service that periodically checks the health of all registered backends and updates the Registry with their status and available models. It's the primary mechanism for keeping backend health information accurate.

### What You'll Learn

1. How the health check loop works
2. How different backend types are handled
3. How status transitions use thresholds to prevent flapping
4. How model discovery works automatically
5. How to test async background services

## File Structure

```
src/health/
├── mod.rs      (270 lines) - Main HealthChecker struct and background loop
├── config.rs   (31 lines)  - Configuration with defaults
├── error.rs    (31 lines)  - Error types for health checks
├── state.rs    (85 lines)  - Per-backend tracking state
├── parser.rs   (91 lines)  - Response parsers for Ollama/OpenAI/LlamaCpp
└── tests.rs    (414 lines) - Comprehensive unit tests

tests/
└── health_integration.rs (279 lines) - Integration tests with mock servers
```

## Core Concepts

### 1. The HealthChecker Struct

```rust
// src/health/mod.rs

pub struct HealthChecker {
    /// Reference to the backend registry (shared, read-only)
    registry: Arc<Registry>,
    
    /// HTTP client with connection pooling
    client: reqwest::Client,
    
    /// Health check configuration
    config: HealthCheckConfig,
    
    /// Per-backend health tracking state
    state: DashMap<String, BackendHealthState>,
}
```

**Key Design Decisions**:

1. **`Arc<Registry>`**: The checker doesn't own the registry—it shares it. This allows multiple components (API, Router, Health Checker) to access the same registry concurrently.

2. **`reqwest::Client`**: HTTP client with connection pooling. Creating one client and reusing it for all requests is more efficient than creating a new client per request.

3. **`DashMap<String, BackendHealthState>`**: Tracks per-backend state (consecutive failures, last models). Uses DashMap for thread-safe access without external locking.

### 2. Configuration

```rust
// src/health/config.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]  // Use defaults for missing fields
pub struct HealthCheckConfig {
    pub enabled: bool,           // Default: true
    pub interval_seconds: u64,   // Default: 30
    pub timeout_seconds: u64,    // Default: 5
    pub failure_threshold: u32,  // Default: 3
    pub recovery_threshold: u32, // Default: 2
}
```

**Why These Defaults?**

- **30 seconds**: Often enough to detect failures, rare enough to not overwhelm backends
- **5 second timeout**: Long enough for slow backends, short enough to fail fast
- **3 failures before unhealthy**: Prevents marking healthy backends as failed due to single network glitches
- **2 successes before healthy**: Confirms recovery isn't a fluke

### 3. Error Classification

```rust
// src/health/error.rs

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

**Why Classify Errors?**

Different errors may require different responses:
- **Timeout**: Backend is slow or overloaded
- **ConnectionFailed**: Network issue or backend down
- **HttpError(503)**: Backend is explicitly unhealthy
- **ParseError**: Backend responded but format is wrong

## How It Works

### The Health Check Loop

```rust
// src/health/mod.rs

pub fn start(self, cancel_token: CancellationToken) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(
            Duration::from_secs(self.config.interval_seconds)
        );
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

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
```

**Understanding `tokio::select!`**:

This is Tokio's way of waiting for multiple async events. Whichever happens first wins:
- If `cancel_token.cancelled()` completes → break the loop (graceful shutdown)
- If `interval.tick()` completes → run health checks

**Why `set_missed_tick_behavior(Skip)`?**

If a health check takes longer than the interval, we don't want to "catch up" by running multiple checks back-to-back. Skip means: if we miss a tick, just wait for the next one.

### Backend-Specific Endpoints

```rust
// src/health/mod.rs

pub fn get_health_endpoint(backend_type: BackendType) -> &'static str {
    match backend_type {
        BackendType::Ollama => "/api/tags",
        BackendType::LlamaCpp => "/health",
        BackendType::VLLM | BackendType::Exo | 
        BackendType::OpenAI | BackendType::Generic => "/v1/models",
    }
}
```

**Why Different Endpoints?**

Each backend type has its own API:
- **Ollama**: Returns models at `/api/tags` in its own format
- **LlamaCpp**: Simple health check at `/health`, no model list
- **OpenAI-compatible**: Standard `/v1/models` endpoint

### Response Parsing

```rust
// src/health/parser.rs

pub fn parse_ollama_response(body: &str) -> Result<Vec<Model>, HealthCheckError> {
    let response: OllamaTagsResponse = serde_json::from_str(body)
        .map_err(|e| HealthCheckError::ParseError(e.to_string()))?;

    Ok(response.models.into_iter().map(|m| {
        let name_lower = m.name.to_lowercase();
        
        // Auto-detect capabilities from model name
        let supports_vision = name_lower.contains("llava") 
            || name_lower.contains("vision");
        let supports_tools = name_lower.contains("mistral");

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

**Capability Detection**:

Since Ollama doesn't explicitly report model capabilities, we infer them from the model name:
- `llava` or `vision` → `supports_vision = true`
- `mistral` → `supports_tools = true`

This is a heuristic—not perfect, but works for common models.

### Status Transitions with Thresholds

```rust
// src/health/state.rs

pub fn apply_result(
    &mut self,
    result: &HealthCheckResult,
    config: &HealthCheckConfig,
) -> Option<BackendStatus> {
    match result {
        HealthCheckResult::Success { .. } => {
            self.consecutive_failures = 0;  // Reset failure counter
            self.consecutive_successes += 1;

            match self.last_status {
                // First check: immediately healthy
                BackendStatus::Unknown => Some(BackendStatus::Healthy),
                
                // Recovery: need 2 consecutive successes
                BackendStatus::Unhealthy
                    if self.consecutive_successes >= config.recovery_threshold =>
                {
                    Some(BackendStatus::Healthy)
                }
                
                // Already healthy or not enough successes yet
                _ => None,
            }
        }
        HealthCheckResult::Failure { .. } => {
            self.consecutive_successes = 0;  // Reset success counter
            self.consecutive_failures += 1;

            match self.last_status {
                // First check: immediately unhealthy
                BackendStatus::Unknown => Some(BackendStatus::Unhealthy),
                
                // Degradation: need 3 consecutive failures
                BackendStatus::Healthy
                    if self.consecutive_failures >= config.failure_threshold =>
                {
                    Some(BackendStatus::Unhealthy)
                }
                
                // Already unhealthy or not enough failures yet
                _ => None,
            }
        }
    }
}
```

**Status Transition State Machine**:

```
          ┌─────────┐
          │ Unknown │
          └────┬────┘
               │
    1 success  │  1 failure
        ┌──────┴───────┐
        ▼              ▼
   ┌─────────┐    ┌───────────┐
   │ Healthy │    │ Unhealthy │
   └────┬────┘    └─────┬─────┘
        │               │
        │ 3 failures    │ 2 successes
        │               │
        ▼               ▼
   ┌───────────┐   ┌─────────┐
   │ Unhealthy │   │ Healthy │
   └───────────┘   └─────────┘
```

**Why Thresholds?**

Without thresholds, a single network hiccup would mark a backend unhealthy, potentially causing unnecessary failovers. Thresholds ensure:
- A backend must fail **consistently** before being marked unhealthy
- A backend must recover **consistently** before being trusted again

### Registry Integration

```rust
// src/health/mod.rs

pub fn apply_result(&self, backend_id: &str, result: HealthCheckResult) {
    let mut state = self.state.entry(backend_id.to_string()).or_default();
    let new_status = state.apply_result(&result, &self.config);
    
    match &result {
        HealthCheckResult::Success { latency_ms, models } => {
            // Always update latency
            self.registry.update_latency(backend_id, *latency_ms);
            
            // Update models if we got any
            if !models.is_empty() {
                self.registry.update_models(backend_id, models.clone());
                state.last_models = models.clone();
            }
        }
        HealthCheckResult::Failure { .. } => {
            // Preserve last_models for recovery
        }
    }
    
    // Only update status if threshold was crossed
    if let Some(status) = new_status {
        self.registry.update_status(backend_id, status, error_msg);
        tracing::info!(backend_id, ?status, "Backend status changed");
    }
}
```

**Key Insight**: We always update latency on success, but only update status when a threshold is crossed.

## Testing Strategy

### Unit Tests

The health module has 40 unit tests covering:

1. **Configuration**: Default values, TOML parsing, validation
2. **Error types**: Display messages, classification
3. **State transitions**: All threshold scenarios
4. **Parsing**: Ollama, OpenAI, LlamaCpp formats
5. **Endpoint selection**: All backend types

Example test for status transitions:

```rust
#[test]
fn test_healthy_to_unhealthy_at_threshold() {
    let mut state = BackendHealthState {
        last_status: BackendStatus::Healthy,
        ..Default::default()
    };
    let config = HealthCheckConfig::default(); // failure_threshold = 3
    let failure = HealthCheckResult::Failure { 
        error: HealthCheckError::Timeout(5) 
    };

    // First two failures: no transition
    assert_eq!(state.apply_result(&failure, &config), None);
    assert_eq!(state.apply_result(&failure, &config), None);
    
    // Third failure: transition to Unhealthy
    assert_eq!(
        state.apply_result(&failure, &config),
        Some(BackendStatus::Unhealthy)
    );
}
```

### Integration Tests

Integration tests use mock HTTP servers to test end-to-end behavior:

```rust
// tests/health_integration.rs

#[tokio::test]
async fn test_full_health_check_cycle_ollama() {
    // Start mock Ollama server
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": [{"name": "llama3:70b"}, {"name": "mistral:7b"}]
        })))
        .mount(&mock_server)
        .await;

    // Create registry and add backend
    let registry = Arc::new(Registry::new());
    let backend = Backend::new("test", &mock_server.uri(), BackendType::Ollama);
    registry.add_backend(backend.clone()).unwrap();

    // Run health checker
    let config = HealthCheckConfig { interval_seconds: 1, ..Default::default() };
    let checker = HealthChecker::new(registry.clone(), config);
    let cancel = CancellationToken::new();
    let handle = checker.start(cancel.clone());

    // Wait for check cycle
    tokio::time::sleep(Duration::from_millis(1500)).await;
    cancel.cancel();
    handle.await.unwrap();

    // Verify registry was updated
    let updated = registry.get_backend(&backend.id).unwrap();
    assert_eq!(updated.status, BackendStatus::Healthy);
    assert_eq!(updated.models.len(), 2);
}
```

**What This Tests**:
1. Mock server returns Ollama-format response
2. Health checker parses response correctly
3. Registry is updated with status and models
4. Graceful shutdown works

## Common Patterns

### Pattern 1: DashMap Entry API

```rust
let mut state = self.state.entry(backend_id.to_string()).or_default();
```

This is the "get or create" pattern:
- If `backend_id` exists → get mutable reference
- If not → create default entry and get mutable reference

### Pattern 2: CancellationToken for Graceful Shutdown

```rust
pub fn start(self, cancel_token: CancellationToken) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => break,
                _ = do_work() => {}
            }
        }
    })
}
```

This pattern is standard for background services in Tokio:
1. Caller keeps a `CancellationToken`
2. Task checks for cancellation in its loop
3. Caller calls `cancel.cancel()` to trigger shutdown
4. Caller awaits `handle` to ensure clean exit

### Pattern 3: Connection Pooling

```rust
let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(config.timeout_seconds))
    .build()
    .expect("Failed to build HTTP client");
```

One client is shared for all requests. Benefits:
- Connection reuse (no TCP handshake per request)
- Automatic keep-alive
- Built-in connection pooling

## Exercises

### Exercise 1: Add a New Backend Type

Add support for a hypothetical "LocalAI" backend:
1. Add `LocalAI` to `BackendType` enum in `src/registry/backend.rs`
2. Add endpoint mapping in `get_health_endpoint()`
3. If it has a unique response format, add a parser function
4. Write tests for the new backend type

### Exercise 2: Add Jitter to Prevent Thundering Herd

Currently all backends are checked simultaneously. Add random jitter:
1. Calculate `jitter = random(0..interval/backends.len())`
2. Sleep for jitter before each backend check
3. Write a test that verifies checks are staggered

### Exercise 3: Add Exponential Backoff for Unhealthy Backends

Don't check unhealthy backends as frequently:
1. Track consecutive failures per backend
2. Increase check interval: `base_interval * 2^failures` (capped at 5 minutes)
3. Reset to normal interval on success

## Summary

The Health Checker demonstrates several key patterns:

1. **Background Services**: Using `tokio::spawn` with `CancellationToken`
2. **Thread-Safe State**: Using `DashMap` for concurrent access
3. **Shared Ownership**: Using `Arc<Registry>` for shared state
4. **Threshold Logic**: Preventing flapping with consecutive counters
5. **Backend Abstraction**: Handling different APIs uniformly
6. **Integration Testing**: Using mock servers for reliable tests

Understanding this module gives you a solid foundation for:
- The Router (uses health status for backend selection)
- The API layer (starts health checker on startup)
- Future features like circuit breakers or adaptive health checking
