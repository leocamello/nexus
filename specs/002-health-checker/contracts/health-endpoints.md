# Health Checker Endpoints Contract

This document defines the backend-specific health check protocol and the system health endpoint.

**Source**: `src/health/mod.rs`, `src/health/parser.rs`, `src/health/config.rs`, `src/health/state.rs`, `src/api/health.rs`

---

## Overview

The Health Checker is a background service that periodically probes registered backends using backend-type-specific endpoints. Results update the Registry's status, model list, and latency metrics. A system-level `GET /health` endpoint exposes aggregate health status.

---

## Backend Health Check Endpoints

Each backend type has a specific health check endpoint and response format:

| BackendType | Endpoint | Expected Response |
|-------------|----------|-------------------|
| `Ollama` | `GET /api/tags` | JSON with `models` array |
| `VLLM` | `GET /v1/models` | OpenAI-format JSON with `data` array |
| `LlamaCpp` | `GET /health` | JSON with `status` field |
| `Exo` | `GET /v1/models` | OpenAI-format JSON with `data` array |
| `OpenAI` | `GET /v1/models` | OpenAI-format JSON with `data` array |
| `LMStudio` | `GET /v1/models` | OpenAI-format JSON with `data` array |
| `Generic` | `GET /v1/models` | OpenAI-format JSON with `data` array |

### Endpoint Selection

```rust
pub fn get_health_endpoint(backend_type: BackendType) -> &'static str {
    match backend_type {
        BackendType::Ollama => "/api/tags",
        BackendType::LlamaCpp => "/health",
        BackendType::VLLM
        | BackendType::Exo
        | BackendType::OpenAI
        | BackendType::LMStudio
        | BackendType::Generic => "/v1/models",
    }
}
```

---

## Response Parsing

### Ollama (`/api/tags`)

**Expected Response**:
```json
{
  "models": [
    { "name": "llama3:70b" },
    { "name": "llava:13b" },
    { "name": "mistral:7b" }
  ]
}
```

**Parsing Rules**:
- Extracts `name` from each model in the `models` array
- `context_length` defaults to `4096` (Ollama doesn't expose this)
- `supports_vision` inferred from name: `true` if name contains `"llava"` or `"vision"` (case-insensitive)
- `supports_tools` inferred from name: `true` if name contains `"mistral"` (case-insensitive)
- `supports_json_mode` always `false`
- `max_output_tokens` always `null`

**Resulting Model**:
```json
{
  "id": "llama3:70b",
  "name": "llama3:70b",
  "context_length": 4096,
  "supports_vision": false,
  "supports_tools": false,
  "supports_json_mode": false,
  "max_output_tokens": null
}
```

### OpenAI-Compatible (`/v1/models`)

Used by: VLLM, Exo, OpenAI, LMStudio, Generic.

**Expected Response**:
```json
{
  "data": [
    { "id": "gpt-4" },
    { "id": "gpt-3.5-turbo" }
  ]
}
```

**Parsing Rules**:
- Extracts `id` from each model in the `data` array
- Both `id` and `name` are set to the model's `id` field
- `context_length` defaults to `4096`
- All capability flags default to `false`
- `max_output_tokens` always `null`

### LlamaCpp (`/health`)

**Expected Response**:
```json
{
  "status": "ok"
}
```

**Parsing Rules**:
- Healthy if `status == "ok"`
- Unhealthy for any other status value
- **Does not return models** — model list is empty on success
- Existing models are preserved from previous successful checks via `BackendHealthState.last_models`

---

## Health Check Result Types

```rust
pub enum HealthCheckResult {
    Success {
        latency_ms: u32,
        models: Vec<Model>,
    },
    SuccessWithParseError {
        latency_ms: u32,
        parse_error: String,
    },
    Failure {
        error: HealthCheckError,
    },
}
```

### Success
Backend responded with HTTP 200 and valid, parseable JSON. Models and latency are updated in the Registry.

### SuccessWithParseError
Backend responded with HTTP 200 but JSON was invalid or unparseable. Treated as **healthy** (backend is responding). Latency is updated but models are preserved from the last successful check.

### Failure
Backend did not respond, timed out, or returned a non-2xx status code.

---

## Health Check Error Types

```rust
pub enum HealthCheckError {
    Timeout(u64),              // Request timed out after N seconds
    ConnectionFailed(String),  // TCP connection failure
    DnsError(String),          // DNS resolution failure
    TlsError(String),          // TLS certificate error
    HttpError(u16),            // Non-2xx HTTP status code
    ParseError(String),        // Invalid response body
}
```

**Error Classification**: `reqwest` errors are classified as `Timeout` (if `is_timeout()`) or `ConnectionFailed` (all other errors).

---

## Status Transition Logic

State transitions use configurable thresholds with consecutive check counting:

| Current Status | Event | Threshold | New Status |
|---------------|-------|-----------|------------|
| `Unknown` | Success | 1 (immediate) | `Healthy` |
| `Unknown` | Failure | 1 (immediate) | `Unhealthy` |
| `Healthy` | Failure | `failure_threshold` consecutive | `Unhealthy` |
| `Unhealthy` | Success | `recovery_threshold` consecutive | `Healthy` |
| `Healthy` | Success | — | No change |
| `Unhealthy` | Failure | — | No change |

**Default Thresholds**:
- `failure_threshold`: 3 consecutive failures
- `recovery_threshold`: 2 consecutive successes

### Per-Backend State Tracking

```rust
pub struct BackendHealthState {
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub last_check_time: Option<DateTime<Utc>>,
    pub last_status: BackendStatus,
    pub last_models: Vec<Model>,
}
```

State is stored in a `DashMap<String, BackendHealthState>` keyed by backend ID.

---

## Health Check Configuration

```rust
pub struct HealthCheckConfig {
    pub enabled: bool,              // Default: true
    pub interval_seconds: u64,      // Default: 30
    pub timeout_seconds: u64,       // Default: 5
    pub failure_threshold: u32,     // Default: 3
    pub recovery_threshold: u32,    // Default: 2
}
```

**TOML Configuration**:
```toml
[health_check]
enabled = true
interval_seconds = 30
timeout_seconds = 5
```

Note: `failure_threshold` and `recovery_threshold` are not exposed in the TOML config (use defaults).

---

## Background Service Lifecycle

```rust
pub fn start(self, cancel_token: CancellationToken) -> JoinHandle<()>
```

- Runs in a `tokio::spawn` task
- Checks all backends on each tick using `tokio::time::interval`
- Missed ticks are skipped (`MissedTickBehavior::Skip`)
- Graceful shutdown via `CancellationToken`
- Logs cycle completion with `tracing::debug!`

### Check Cycle

1. Get all backends from Registry via `get_all_backends()`
2. For each backend, call `check_backend()` sequentially
3. Apply results to Registry via `apply_result()`
4. Broadcast backend status updates via WebSocket (if configured)

---

## System Health Endpoint

### `GET /health`

**Content-Type**: `application/json`
**Authentication**: None (trusted network)

### Response Format

**Status**: `200 OK` (always — even when unhealthy)

```json
{
  "status": "healthy",
  "uptime_seconds": 3600,
  "backends": {
    "total": 3,
    "healthy": 2,
    "unhealthy": 1
  },
  "models": 5
}
```

### Field Definitions

#### `status`
- **Type**: string
- **Values**: `"healthy"` | `"degraded"` | `"unhealthy"`
- **Logic**:
  - `"healthy"`: All backends healthy AND at least 1 backend registered
  - `"degraded"`: Some backends healthy, some not
  - `"unhealthy"`: No healthy backends (or no backends registered)

#### `uptime_seconds`
- **Type**: integer
- **Description**: Seconds since server startup (`Instant::now() - start_time`)

#### `backends`
- **Type**: object
- **Fields**:
  - `total` (integer): Total registered backends
  - `healthy` (integer): Backends with `status == Healthy`
  - `unhealthy` (integer): `total - healthy`

#### `models`
- **Type**: integer
- **Description**: Number of unique models across all backends (from Registry model index)

### Example Responses

**All Healthy**:
```json
{
  "status": "healthy",
  "uptime_seconds": 7200,
  "backends": {
    "total": 2,
    "healthy": 2,
    "unhealthy": 0
  },
  "models": 8
}
```

**Degraded**:
```json
{
  "status": "degraded",
  "uptime_seconds": 120,
  "backends": {
    "total": 3,
    "healthy": 1,
    "unhealthy": 2
  },
  "models": 3
}
```

**No Backends**:
```json
{
  "status": "unhealthy",
  "uptime_seconds": 5,
  "backends": {
    "total": 0,
    "healthy": 0,
    "unhealthy": 0
  },
  "models": 0
}
```

---

## Apply Result Side Effects

When a health check result is applied:

| Result Type | Registry Updates |
|-------------|-----------------|
| `Success` | Update latency, update models (if non-empty), update status |
| `SuccessWithParseError` | Update latency, preserve last known models, update status |
| `Failure` | Preserve models in state, update status, broadcast empty models |

### Model Preservation Logic

- **Success with models**: Models are updated in Registry and saved in `last_models`
- **Success with empty models** (e.g., LlamaCpp): `last_models` are re-applied to Registry
- **SuccessWithParseError**: `last_models` are re-applied to Registry
- **Failure**: Models preserved in `BackendHealthState` only (not re-applied)

---

## Implementation Notes

### HTTP Client

- Built with `reqwest::Client::builder()` with configurable timeout
- Connection pooling enabled by default
- Custom client injection supported via `with_client()` for testing

### WebSocket Broadcasting

When a broadcast sender is configured via `with_broadcast()`:
- Backend status updates are broadcast after each full check cycle
- Model change updates are broadcast when models are updated
- Errors on send are silently ignored (no receivers listening)

### Latency Recording

Backend health check latency is recorded to Prometheus as a histogram:
```
nexus_backend_latency_seconds{backend="backend-id"}
```
Latency is measured from request start to response receipt, in seconds.

---

## Testing Strategy

### Unit Tests
1. Parse Ollama `/api/tags` response (valid, empty, malformed)
2. Parse OpenAI `/v1/models` response (valid, empty, malformed)
3. Parse LlamaCpp `/health` response (`"ok"`, other status, malformed)
4. Status transition logic with failure/recovery thresholds
5. Model preservation on parse errors and failures
6. Health check config defaults

### Integration Tests
1. Mock HTTP server returning valid responses → verify Registry updates
2. Mock HTTP server returning errors → verify status transitions
3. Timeout handling with slow mock server
4. Full check cycle with multiple backends
5. `GET /health` endpoint with various backend states
