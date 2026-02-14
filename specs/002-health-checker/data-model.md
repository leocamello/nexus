# Data Model: Health Checker (F02)

**Date**: 2025-01-10  
**Phase**: Phase 1 - Foundation

This document defines the data entities and their relationships for the Health Checker feature.

## Core Entities

### 1. HealthChecker

**Purpose**: Background service that periodically probes registered backends, updates their status, refreshes model lists, and records latency metrics.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `registry` | `Arc<Registry>` | Shared reference to backend registry |
| `client` | `reqwest::Client` | HTTP client with connection pooling and timeout |
| `config` | `HealthCheckConfig` | Interval, timeout, and threshold settings |
| `state` | `DashMap<String, BackendHealthState>` | Per-backend health tracking state |
| `ws_broadcast` | `Option<broadcast::Sender<WebSocketUpdate>>` | Optional dashboard notification channel |

**Responsibilities**:
- Periodically call `check_all_backends()` on a configurable interval
- Determine backend-specific health endpoint via `get_health_endpoint()`
- Parse backend responses into model lists (Ollama, OpenAI, LlamaCpp formats)
- Apply threshold-based status transitions via `BackendHealthState`
- Update registry with new status, latency, and models
- Record `nexus_backend_latency_seconds` Prometheus histogram
- Broadcast status/model changes to dashboard via WebSocket

**Lifecycle**: Created at server startup via `new()` or `with_client()`. Started as a tokio task via `start(cancel_token)`. Runs until `CancellationToken` is cancelled during graceful shutdown.

**Thread Safety**: `DashMap` for per-backend state. `Arc<Registry>` for shared registry access. Runs on a single tokio task (no internal parallelism across backends).

---

### 2. HealthCheckConfig

**Purpose**: Configuration parameters controlling health check behavior.

**Attributes**:

| Attribute | Type | Default | Constraints |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `true` | Can be disabled via config or CLI `--no-health-check` |
| `interval_seconds` | `u64` | `30` | Seconds between health check cycles |
| `timeout_seconds` | `u64` | `5` | Per-request timeout; also sets reqwest client timeout |
| `failure_threshold` | `u32` | `3` | Consecutive failures before marking Unhealthy |
| `recovery_threshold` | `u32` | `2` | Consecutive successes before marking Healthy |

**Responsibilities**:
- Provide threshold values for `BackendHealthState::apply_result()`
- Configure timing for the background check loop

**Lifecycle**: Deserialized from TOML `[health_check]` section. Immutable after creation.

**Thread Safety**: Plain `Clone` type with no interior mutability.

---

### 3. BackendHealthState

**Purpose**: Tracks per-backend health check history for threshold-based status transitions.

**Attributes**:

| Attribute | Type | Default | Constraints |
|-----------|------|---------|-------------|
| `consecutive_failures` | `u32` | `0` | Reset to 0 on any success |
| `consecutive_successes` | `u32` | `0` | Reset to 0 on any failure |
| `last_check_time` | `Option<DateTime<Utc>>` | `None` | Set after each check |
| `last_status` | `BackendStatus` | `Unknown` | Last status for transition detection |
| `last_models` | `Vec<Model>` | `[]` | Preserved on parse errors; updated on successful parse |

**Responsibilities**:
- Count consecutive successes/failures for threshold comparison
- Determine if a status transition should occur via `apply_result()`
- Preserve last known model list when parse errors occur

**Lifecycle**: Created lazily via `DashMap::entry().or_default()` on first health check for a backend. Persists for the lifetime of the HealthChecker.

**Thread Safety**: Accessed through `DashMap` guards. Not shared directly.

---

### 4. HealthCheckResult (Enum)

**Purpose**: Represents the outcome of a single health check probe.

**Variants**:

| Variant | Fields | Description |
|---------|--------|-------------|
| `Success` | `latency_ms: u32, models: Vec<Model>` | Backend responded with valid model list |
| `SuccessWithParseError` | `latency_ms: u32, parse_error: String` | Backend responded HTTP 200 but invalid JSON; treated as healthy |
| `Failure` | `error: HealthCheckError` | Backend unreachable or returned error status |

**Responsibilities**:
- Carry structured result data for `apply_result()` and registry updates
- Distinguish between "backend alive but malformed response" and "backend dead"

**Lifecycle**: Created per health check call, consumed by `apply_result()`, then dropped.

---

### 5. HealthCheckError (Enum)

**Purpose**: Classifies health check failures for diagnostics and logging.

**Variants**:

| Variant | Fields | Trigger |
|---------|--------|---------|
| `Timeout` | `u64` (seconds) | Request exceeded `timeout_seconds` |
| `ConnectionFailed` | `String` | TCP connection refused, network error |
| `DnsError` | `String` | DNS resolution failure |
| `TlsError` | `String` | TLS/SSL certificate error |
| `HttpError` | `u16` (status code) | Non-2xx HTTP response |
| `ParseError` | `String` | Invalid JSON in response body |

**Classification Logic**: `classify_error()` maps `reqwest::Error`:
- `e.is_timeout()` → `Timeout`
- All other errors → `ConnectionFailed`

---

### 6. Response Parsers

**Purpose**: Backend-specific response parsing that extracts model lists from health endpoints.

#### Ollama Parser (`parse_ollama_response`)

**Input**: JSON from `/api/tags`  
**Schema**: `{ "models": [{ "name": "llama3:70b" }] }`  
**Heuristics**:
- Vision detection: model name contains `"llava"` or `"vision"`
- Tools detection: model name contains `"mistral"`
- Default `context_length`: 4096

#### OpenAI Parser (`parse_openai_response`)

**Input**: JSON from `/v1/models`  
**Schema**: `{ "data": [{ "id": "model-name" }] }`  
**Used by**: VLLM, Exo, OpenAI, LMStudio, Generic  
**Defaults**: All capability flags `false`, `context_length` 4096

#### LlamaCpp Parser (`parse_llamacpp_response`)

**Input**: JSON from `/health`  
**Schema**: `{ "status": "ok" }`  
**Returns**: Boolean healthy/unhealthy. Does not return models (preserved from previous check).

---

## Entity Relationships

```
┌──────────────────────┐
│    HealthChecker     │
│                      │
│  registry ───────────┼──────► Arc<Registry>
│  client              │
│  config ─────────────┼──────► HealthCheckConfig
│  state ──────────────┼──┐
│  ws_broadcast        │  │
└──────────────────────┘  │
                          │
            ┌─────────────┘
            ▼
  DashMap<String, BackendHealthState>
            │
            │ per backend ID
            ▼
  ┌──────────────────────┐
  │  BackendHealthState  │
  │                      │
  │  consecutive_failures│
  │  consecutive_successes│
  │  last_status         │
  │  last_models ────────┼──► Vec<Model>
  └──────────────────────┘
            ▲
            │ apply_result()
            │
  ┌──────────────────────┐
  │  HealthCheckResult   │
  │                      │
  │  Success { latency,  │
  │            models }  │
  │  SuccessWithParseErr │
  │  Failure { error }   │
  └──────────────────────┘
            ▲
            │ check_backend()
            │
  ┌──────────────────────┐
  │  Response Parsers    │
  │                      │
  │  parse_ollama_*      │
  │  parse_openai_*      │
  │  parse_llamacpp_*    │
  └──────────────────────┘
```

---

## State Transitions

### Backend Status via Health Checks

```
┌───────────┐
│  Unknown  │ ← initial state for all backends
└─────┬─────┘
      │
      ├── first Success/SuccessWithParseError ──► Healthy
      │
      └── first Failure ──────────────────────► Unhealthy


┌───────────┐                              ┌───────────┐
│  Healthy  │── failures ≥ failure_threshold ──►│ Unhealthy │
└───────────┘                              └─────┬─────┘
      ▲                                          │
      │                                          │
      └── successes ≥ recovery_threshold ────────┘
```

**Key Rules**:
- `Unknown → Healthy`: First successful check (threshold = 1)
- `Unknown → Unhealthy`: First failed check (threshold = 1)
- `Healthy → Unhealthy`: Requires `failure_threshold` (default 3) consecutive failures
- `Unhealthy → Healthy`: Requires `recovery_threshold` (default 2) consecutive successes
- Thresholds prevent flapping on transient network issues

### Health Check Cycle Flow

```
Timer tick (every interval_seconds)
    ↓
get_all_backends() from Registry
    ↓
For each backend:
    ↓
    GET {backend.url}{health_endpoint}
    ↓
    Parse response → HealthCheckResult
    ↓
    apply_result():
      - Update consecutive counters
      - Determine status transition
    ↓
    Update Registry:
      - update_latency(backend_id, latency_ms)
      - update_models(backend_id, models)  [if non-empty]
      - update_status(backend_id, status)  [if transition]
    ↓
    Record Prometheus histogram:
      nexus_backend_latency_seconds{backend}
    ↓
Broadcast backend_status WebSocket update
```

### Model Preservation Logic

```
HealthCheckResult::Success { models: [...] }
    → models non-empty: update registry + save to last_models
    → models empty: preserve last_models in registry (LlamaCpp case)

HealthCheckResult::SuccessWithParseError
    → preserve last_models in registry (backend alive but bad JSON)

HealthCheckResult::Failure
    → models stay in state.last_models (not cleared)
    → broadcast empty model list to dashboard
```

---

## Validation & Constraints

### Health Endpoint Selection

**Rule**: Each `BackendType` has a fixed health endpoint.

| BackendType | Endpoint | Rationale |
|-------------|----------|-----------|
| Ollama | `/api/tags` | Returns model list natively |
| LlamaCpp | `/health` | Dedicated health endpoint |
| VLLM, Exo, OpenAI, LMStudio, Generic | `/v1/models` | OpenAI-compatible model listing |

---

### Timeout Configuration

**Rule**: `reqwest::Client` timeout matches `config.timeout_seconds`. Additionally, each request sets an explicit `.timeout()` for defense-in-depth.

---

### Parse Error Handling

**Rule**: HTTP 200 with unparseable JSON is treated as healthy (`SuccessWithParseError`). This prevents marking a responsive backend as unhealthy due to API format changes.

**Logged**: Warning with backend_type and error details via `tracing::warn!`.

---

### Sequential Backend Checking

**Rule**: Backends are checked sequentially within each cycle (not in parallel). This prevents thundering herd effects on the network and simplifies error handling.

---

## Performance Characteristics

| Operation | Target Latency | Implementation |
|-----------|----------------|----------------|
| Single health check | < `timeout_seconds` | HTTP GET with timeout |
| Full cycle (N backends) | < N × `timeout_seconds` | Sequential iteration |
| `apply_result` | < 1µs | Counter increment + comparison |
| Registry update | < 10µs | DashMap write + model index update |
| Prometheus recording | < 100ns | Atomic histogram bucket increment |
| WebSocket broadcast | < 1µs | Channel send (non-blocking) |

**Interval behavior**: `MissedTickBehavior::Skip` — if a cycle takes longer than `interval_seconds`, the next tick is skipped rather than queued.

**Memory**: `BackendHealthState` ~200 bytes per backend + last_models. For 100 backends with 10 models each: ~120 KB.

---

## Future Extensions

### Not in Current Scope

1. **Parallel health checks**: Check multiple backends concurrently with a semaphore
2. **Adaptive intervals**: Increase frequency for recently-failed backends
3. **Deep health checks**: Verify model inference capability, not just endpoint reachability
4. **Custom health endpoints**: Per-backend configurable health URLs
5. **Circuit breaker**: Automatic request blocking after sustained failures

These are mentioned for awareness but are NOT part of F02 implementation.
