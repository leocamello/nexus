# Feature Specification: Health Checker

**Feature Branch**: `002-health-checker`  
**Created**: 2026-02-01  
**Status**: Draft  
**Priority**: P0 (MVP)  
**Depends On**: F02 (Backend Registry)

## Overview

Background service that periodically checks backend health and updates the registry. Runs continuously without blocking request routing. This is the primary mechanism for keeping the Registry's health status accurate.

## User Scenarios & Testing

### User Story 1 - Periodic Health Checks (Priority: P1)

As a system operator, I want Nexus to automatically check backend health so that unhealthy backends are excluded from routing without manual intervention.

**Why this priority**: Core functionality - without health checks, Nexus can't know which backends are available.

**Independent Test**: Can be tested with a mock backend that returns healthy/unhealthy responses.

**Acceptance Scenarios**:

1. **Given** a registered backend, **When** health checker runs, **Then** it sends a health check request to the backend's health endpoint
2. **Given** a backend returns 200 OK, **When** response is parsed, **Then** the registry status is updated to Healthy
3. **Given** a backend returns 500 or times out, **When** health check completes, **Then** the registry status is updated to Unhealthy
4. **Given** health check interval is 30s, **When** service runs for 90s, **Then** each backend is checked approximately 3 times

---

### User Story 2 - Backend-Specific Endpoints (Priority: P1)

As a developer, I want the health checker to use the correct endpoint for each backend type so that health checks work with Ollama, vLLM, llama.cpp, and OpenAI-compatible backends.

**Why this priority**: Different backends have different health/model endpoints.

**Independent Test**: Can be tested by mocking different backend types and verifying correct endpoints are called.

**Acceptance Scenarios**:

1. **Given** an Ollama backend, **When** health check runs, **Then** it calls `GET /api/tags`
2. **Given** a vLLM backend, **When** health check runs, **Then** it calls `GET /v1/models`
3. **Given** a llama.cpp backend, **When** health check runs, **Then** it calls `GET /health`
4. **Given** a Generic backend, **When** health check runs, **Then** it calls `GET /v1/models`

---

### User Story 3 - Model Discovery (Priority: P1)

As a user, I want Nexus to automatically discover which models are available on each backend so that I don't have to configure them manually.

**Why this priority**: Zero-config philosophy - models should be auto-discovered.

**Independent Test**: Can be tested by mocking backend responses with different model formats.

**Acceptance Scenarios**:

1. **Given** Ollama returns `{"models": [{"name": "llama3:70b"}]}`, **When** parsed, **Then** registry contains model "llama3:70b"
2. **Given** vLLM returns `{"data": [{"id": "mistral-7b"}]}`, **When** parsed, **Then** registry contains model "mistral-7b"
3. **Given** backend adds a new model, **When** next health check runs, **Then** registry is updated with the new model
4. **Given** backend removes a model, **When** next health check runs, **Then** registry no longer lists that model
5. **Given** backend serves models A, B, C, **When** health check sees A, B, D, **Then** registry contains exactly A, B, D (atomic replacement)
6. **Given** backend returns 200 but invalid JSON, **When** parsed, **Then** status is Healthy but models list is preserved (last known models)
7. **Given** backend returns 200 with empty model list, **When** parsed, **Then** status is Healthy and models list is cleared

---

### User Story 4 - Status Transitions with Thresholds (Priority: P1)

As a system operator, I want backends to require multiple failures before being marked unhealthy so that temporary network issues don't cause unnecessary failovers.

**Why this priority**: Resilience - prevents flapping on transient failures.

**Independent Test**: Can be tested by simulating sequences of success/failure responses.

**Acceptance Scenarios**:

1. **Given** an Unknown backend, **When** 1 health check succeeds, **Then** status becomes Healthy
2. **Given** an Unknown backend, **When** 1 health check fails, **Then** status becomes Unhealthy
3. **Given** a Healthy backend, **When** 2 checks fail, **Then** status remains Healthy (threshold is 3)
4. **Given** a Healthy backend, **When** 3 consecutive checks fail, **Then** status becomes Unhealthy
5. **Given** an Unhealthy backend, **When** 1 check succeeds, **Then** status remains Unhealthy (threshold is 2)
6. **Given** an Unhealthy backend, **When** 2 consecutive checks succeed, **Then** status becomes Healthy
7. **Given** a Healthy backend with 2 consecutive failures, **When** 1 success occurs, **Then** failure counter resets to 0
8. **Given** an Unhealthy backend with 1 consecutive success, **When** 1 failure occurs, **Then** success counter resets to 0

---

### User Story 5 - Timeout Handling (Priority: P1)

As a system operator, I want health checks to timeout so that slow backends don't block the health checker.

**Why this priority**: Prevents health checker from getting stuck on unresponsive backends.

**Independent Test**: Can be tested with a mock backend that delays responses.

**Acceptance Scenarios**:

1. **Given** timeout is 5s, **When** backend responds in 3s, **Then** check succeeds and latency is recorded
2. **Given** timeout is 5s, **When** backend takes 10s to respond, **Then** check fails with timeout error
3. **Given** a timeout failure, **When** counter checked, **Then** it counts toward failure threshold

---

### User Story 6 - Staggered Checks (Priority: P2)

As a system operator, I want health checks to be staggered so that all backends aren't checked simultaneously (thundering herd prevention).

**Why this priority**: Improves stability but basic health checking works without it. **Not required for MVP.**

**Independent Test**: Can be tested by measuring check timing with multiple backends.

**Acceptance Scenarios**:

1. **Given** 10 backends with 30s interval, **When** checker runs, **Then** checks are spread across the interval (~3s apart)
2. **Given** staggered checks, **When** observing network, **Then** no burst of simultaneous requests

---

### User Story 7 - Graceful Shutdown (Priority: P2)

As a system operator, I want the health checker to finish current checks during shutdown so that registry state is consistent.

**Why this priority**: Clean shutdown behavior, but not required for basic operation.

**Independent Test**: Can be tested by triggering shutdown during a check cycle.

**Acceptance Scenarios**:

1. **Given** shutdown signal received, **When** check is in progress, **Then** current check completes
2. **Given** shutdown signal received, **When** waiting for next interval, **Then** service stops immediately
3. **Given** shutdown complete, **When** service state checked, **Then** no background tasks remain

---

### User Story 8 - Logging (Priority: P2)

As a system operator, I want health transitions logged so that I can debug connectivity issues.

**Why this priority**: Observability, but not required for core functionality.

**Independent Test**: Can be tested by capturing log output during status transitions.

**Acceptance Scenarios**:

1. **Given** backend transitions Healthy → Unhealthy, **When** logged, **Then** INFO level log with backend ID and error
2. **Given** backend transitions Unhealthy → Healthy, **When** logged, **Then** INFO level log with backend ID
3. **Given** routine successful check, **When** logged, **Then** DEBUG level (not INFO)

---

### User Story 9 - Latency Tracking (Priority: P2)

As a system operator, I want health checks to record backend response latency so that the router can make informed decisions.

**Why this priority**: Enables smart routing by load/latency, not just health status.

**Independent Test**: Can be tested by verifying registry latency updates after health checks.

**Acceptance Scenarios**:

1. **Given** health check succeeds in 150ms, **When** registry checked, **Then** `avg_latency_ms` reflects new value (EMA)
2. **Given** health check times out, **When** registry checked, **Then** `avg_latency_ms` is not updated
3. **Given** first health check for backend, **When** succeeds in 100ms, **Then** `avg_latency_ms` is set to 100

---

## Edge Cases (Summary)

_See detailed edge case table in the Edge Cases section below._

- Backend returns 200 but invalid JSON → Mark healthy (responding), preserve last model list
- Backend returns 200 but empty model list → Healthy, clear models from registry
- DNS resolution fails → Unhealthy with DnsError
- TLS certificate invalid → Unhealthy with TlsError
- Backend added while checker running → Pick up on next interval
- Backend removed while being checked → Skip gracefully, clean up state
- Registry update fails → Log error, continue with next backend

## Requirements

### Functional Requirements

- **FR-001**: Health checker MUST run as a background task (tokio::spawn)
- **FR-002**: Health checker MUST check all registered backends periodically
- **FR-003**: Health checker MUST use backend-specific endpoints (Ollama: /api/tags, vLLM: /v1/models, etc.)
- **FR-004**: Health checker MUST parse model lists from backend responses
- **FR-005**: Health checker MUST update registry status based on check results
- **FR-006**: Health checker MUST apply failure threshold before marking Healthy → Unhealthy
- **FR-007**: Health checker MUST apply recovery threshold before marking Unhealthy → Healthy
- **FR-008**: Health checker MUST timeout requests that exceed timeout_seconds
- **FR-009**: Health checker MUST update latency metrics for successful checks
- **FR-010**: Health checker MUST support graceful shutdown via cancellation token
- **FR-011**: Health checker MUST log status transitions at INFO level
- **FR-012**: Health checker SHOULD stagger checks across the interval (P2 enhancement, post-MVP phase)

### Non-Functional Requirements

- **NFR-001**: Health checks MUST NOT block request routing
- **NFR-002**: Health checker MUST be configurable via HealthCheckConfig
- **NFR-003**: Health checker MUST handle network errors gracefully (no panics)
- **NFR-004**: Memory overhead MUST be < 5KB per backend for tracking state (aligned with constitution)
- **NFR-005**: Only one HealthChecker instance MUST run per Registry instance

### Integration with Backend Registry

The Registry uses `DashMap` for thread-safe storage with interior mutability. Health checker
only needs `Arc<Registry>` (not `Arc<RwLock<Registry>>`) because:

- `Registry::update_status()` uses `DashMap::get_mut()` internally
- `Registry::update_models()` uses `DashMap::get_mut()` internally  
- `Registry::update_latency()` uses atomic operations with `SeqCst` ordering

All Registry operations are thread-safe without external synchronization.

### Key Entities

- **HealthChecker**: The background service that runs health checks
  - Owns reference to Registry (Arc<Registry>)
  - Owns HTTP client (reqwest::Client with connection pooling)
  - Runs check loop until cancelled

- **HealthCheckConfig**: Configuration for health checking
  - enabled, interval_seconds, timeout_seconds
  - failure_threshold, recovery_threshold

- **BackendHealthState**: Per-backend tracking state
  - consecutive_failures, consecutive_successes
  - last_check_time, last_status

- **HealthCheckResult**: Result of a single health check
  - success/failure, latency_ms, models (if successful), error (if failed)

## Data Structures

### HealthChecker

```rust
/// Background service that periodically checks backend health.
///
/// Uses DashMap for thread-safe per-backend state tracking.
pub struct HealthChecker {
    /// Reference to the backend registry
    registry: Arc<Registry>,
    /// HTTP client with connection pooling
    client: reqwest::Client,
    /// Health check configuration
    config: HealthCheckConfig,
    /// Per-backend health tracking state
    state: DashMap<String, BackendHealthState>,
}

impl HealthChecker {
    /// Create a new health checker with default HTTP client.
    pub fn new(registry: Arc<Registry>, config: HealthCheckConfig) -> Self;
    
    /// Create a health checker with custom HTTP client (for testing).
    pub fn with_client(
        registry: Arc<Registry>, 
        config: HealthCheckConfig, 
        client: reqwest::Client
    ) -> Self;
}
```

### HealthCheckConfig

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Whether health checking is enabled
    pub enabled: bool,
    /// Seconds between health check cycles
    pub interval_seconds: u64,
    /// Timeout for each health check request
    pub timeout_seconds: u64,
    /// Consecutive failures before marking unhealthy
    pub failure_threshold: u32,
    /// Consecutive successes before marking healthy
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

### BackendHealthState

```rust
/// Tracks health check state for a single backend
#[derive(Debug, Clone)]
pub struct BackendHealthState {
    /// Count of consecutive failed checks
    pub consecutive_failures: u32,
    /// Count of consecutive successful checks
    pub consecutive_successes: u32,
    /// When last check completed
    pub last_check_time: Option<DateTime<Utc>>,
    /// Last known status (for detecting transitions)
    pub last_status: BackendStatus,
    /// Last known model list (preserved on parse errors)
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

### HealthCheckResult

```rust
/// Result of a single health check
#[derive(Debug)]
pub enum HealthCheckResult {
    Success {
        latency_ms: u32,
        models: Vec<Model>,
    },
    Failure {
        error: HealthCheckError,
    },
}
```

### HealthCheckError

```rust
#[derive(Debug, thiserror::Error)]
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

## Operations

| Operation | Signature | Description |
|-----------|-----------|-------------|
| new | `(registry: Arc<Registry>, config: HealthCheckConfig) -> Self` | Create health checker |
| start | `(self, cancel_token: CancellationToken) -> JoinHandle<()>` | Start background task |
| check_all_backends | `(&self) -> Vec<(String, HealthCheckResult)>` | Check all backends once |
| check_backend | `(&self, backend: &Backend) -> HealthCheckResult` | Check single backend |
| get_health_endpoint | `(backend_type: BackendType) -> &'static str` | Get endpoint for backend type |
| parse_response | `(&self, backend_type: BackendType, body: &str) -> Result<Vec<Model>, HealthCheckError>` | Parse response based on backend type |
| parse_ollama_response | `(body: &str) -> Result<Vec<Model>, HealthCheckError>` | Parse Ollama /api/tags format |
| parse_openai_response | `(body: &str) -> Result<Vec<Model>, HealthCheckError>` | Parse OpenAI /v1/models format (used by vLLM, Exo, Generic) |
| parse_llamacpp_response | `(body: &str) -> Result<bool, HealthCheckError>` | Parse llama.cpp /health (returns health status, no models) |
| apply_result | `(&self, id: &str, result: HealthCheckResult)` | Apply result to registry |
| should_transition | `(&self, id: &str, result: &HealthCheckResult) -> Option<BackendStatus>` | Determine status transition |

## Health Check Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                    Health Checker Loop                          │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ 1. Sleep until next check (staggered per backend)       │   │
│  └────────────────────────┬────────────────────────────────┘   │
│                           ▼                                     │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ 2. Get all backends from registry                       │   │
│  └────────────────────────┬────────────────────────────────┘   │
│                           ▼                                     │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ 3. For each backend (staggered):                        │   │
│  │    a. Send HTTP request to health endpoint              │   │
│  │    b. Apply timeout (5s default)                        │   │
│  │    c. Parse response (models, latency)                  │   │
│  │    d. Update consecutive success/failure count          │   │
│  │    e. If threshold crossed, update registry status      │   │
│  │    f. Update registry models if successful              │   │
│  │    g. Log transition at INFO if status changed          │   │
│  └────────────────────────┬────────────────────────────────┘   │
│                           ▼                                     │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ 4. Loop back to step 1 (unless cancelled)               │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## Backend-Specific Endpoints

| BackendType | Endpoint | Response Format |
|-------------|----------|-----------------|
| Ollama | GET /api/tags | `{"models": [{"name": "...", "details": {...}}]}` |
| VLLM | GET /v1/models | `{"data": [{"id": "...", "object": "model"}]}` |
| LlamaCpp | GET /health | `{"status": "ok"}` (no models) |
| Exo | GET /v1/models | OpenAI format |
| OpenAI | GET /v1/models | `{"data": [{"id": "...", "object": "model"}]}` |
| Generic | GET /v1/models | OpenAI format (assumed) |

**Note**: For LlamaCpp, which doesn't return model info, and for Generic backends where response parsing fails, use fallback behavior: assume backend supports whatever model was configured or previously detected.

## Model Parsing Details

The Health Checker parses backend responses into `Model` structs (defined in `src/registry/backend.rs`):

```rust
// Already exists in registry - Health Checker uses this
pub struct Model {
    pub id: String,              // Required: model identifier
    pub name: String,            // Required: display name  
    pub context_length: u32,     // Default: 4096 if not provided
    pub supports_vision: bool,   // Default: false
    pub supports_tools: bool,    // Default: false
    pub supports_json_mode: bool,// Default: false
    pub max_output_tokens: Option<u32>, // Default: None
}
```

### Parsing Ollama Response

```json
{
  "models": [{
    "name": "llama3:70b",
    "details": {
      "parameter_size": "70B",
      "quantization_level": "Q4_0"
    }
  }]
}
```

**Field Mapping**:
- `id` ← `name`
- `name` ← `name` 
- `context_length` ← Use default (4096); Ollama doesn't expose this
- `supports_vision` ← Check if `name` contains "llava" or "vision"
- `supports_tools` ← Check if `name` contains "mistral" or known tool models
- Other fields ← Use defaults

### Parsing OpenAI-Format Response

```json
{
  "data": [{
    "id": "mistral-7b",
    "object": "model",
    "created": 1234567890,
    "owned_by": "vllm"
  }]
}
```

**Field Mapping**:
- `id` ← `id`
- `name` ← `id`
- All capability fields ← Use defaults (no standard way to detect)

**Future Enhancement**: Consider adding a model capabilities config file for known models.

## Status Transition State Machine

```
                    ┌─────────┐
                    │ Unknown │
                    └────┬────┘
                         │
          ┌──────────────┴──────────────┐
          │ 1 success                   │ 1 failure
          ▼                             ▼
     ┌─────────┐                   ┌───────────┐
     │ Healthy │                   │ Unhealthy │
     └────┬────┘                   └─────┬─────┘
          │                              │
          │ 3 consecutive failures       │ 2 consecutive successes
          └──────────────┬───────────────┘
                         │
          ┌──────────────┴──────────────┐
          ▼                             ▼
     ┌───────────┐                 ┌─────────┐
     │ Unhealthy │                 │ Healthy │
     └───────────┘                 └─────────┘
```

## Configuration Example

```toml
[health_check]
# Enable/disable health checking
enabled = true

# Seconds between health check cycles
interval_seconds = 30

# Timeout for each health check request
timeout_seconds = 5

# Consecutive failures before marking unhealthy (Healthy → Unhealthy)
failure_threshold = 3

# Consecutive successes before marking healthy (Unhealthy → Healthy)
recovery_threshold = 2
```

## Edge Cases

| Scenario | Behavior |
|----------|----------|
| Backend removed during health check | Skip gracefully - if `registry.get_backend(id)` returns `None`, discard result and clean up state |
| Backend added while check in progress | Will be checked in next cycle |
| Backend returns 200 but invalid JSON | Mark as healthy (backend is responding), preserve last known model list, log warning |
| Backend very slow but responds | Mark healthy, record high latency (caller can use for routing) |
| DNS resolution fails | Mark unhealthy with `DnsError` |
| TLS certificate error | Mark unhealthy with `TlsError` |
| Connection refused | Mark unhealthy with `ConnectionFailed` |
| Empty model list response | Valid success - update registry with empty models |
| Network partition (all backends fail) | Each backend tracked independently |

## Staggered Timing Specification

To prevent thundering herd (all backends checked simultaneously), health checks are staggered:

**Algorithm**: For `N` backends and interval `I` seconds:
- Delay between backends: `I / N` seconds (minimum 100ms)
- Example: 10 backends, 30s interval → 3s between each check

**Implementation**:
```rust
let stagger_delay = Duration::from_secs(config.interval_seconds) / backends.len() as u32;
let stagger_delay = stagger_delay.max(Duration::from_millis(100));

// First cycle: check all immediately (skip stagger)
// Subsequent cycles: apply stagger delay
for (index, backend) in backends.iter().enumerate() {
    if !is_first_cycle && index > 0 {
        tokio::time::sleep(stagger_delay).await;
    }
    // check backend...
}
```

**First cycle behavior**: On startup, check all backends immediately (no stagger delay) to establish initial health status quickly. Subsequent cycles apply staggering.

## Success Criteria

### Measurable Outcomes

- **SC-001**: Health checks run at configured interval (±1 second tolerance)
- **SC-002**: Status transitions respect configured thresholds
- **SC-003**: Model lists are updated on each successful check
- **SC-004**: Timeouts trigger after configured seconds (±100ms tolerance)
- **SC-005**: Graceful shutdown completes within 2x timeout period
- **SC-006**: No memory leaks after 1000 check cycles

### Definition of Done

- [ ] HealthChecker struct implemented with all operations
- [ ] Backend-specific endpoint selection works for all types
- [ ] Ollama response parsing extracts models correctly
- [ ] OpenAI response parsing extracts models correctly
- [ ] Status transition thresholds work correctly
- [ ] Timeout handling works correctly
- [ ] Staggered checks implemented
- [ ] Graceful shutdown implemented
- [ ] Logging at appropriate levels
- [ ] Unit tests for parsing and transitions
- [ ] Integration tests with mock backends
- [ ] Code passes clippy and fmt checks
