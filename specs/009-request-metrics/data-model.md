# Data Model: Request Metrics (F09)

**Date**: 2025-01-10  
**Phase**: Phase 1 - Design & Contracts

This document defines the data entities and their relationships for the Request Metrics feature.

## Core Entities

### 1. MetricsCollector

**Purpose**: Central coordinator for metrics collection, gauge computation, and state management.

**Attributes**:
- `registry: Arc<Registry>` - Reference to backend registry for computing gauges
- `start_time: Instant` - Gateway startup time for uptime calculation
- `label_cache: DashMap<String, String>` - Thread-safe cache for sanitized Prometheus labels

**Responsibilities**:
- Initialize Prometheus exporter at gateway startup
- Compute gauge metrics from Registry state (backends_total, backends_healthy, models_available)
- Sanitize backend and model names for Prometheus label compatibility
- Provide snapshots for JSON stats endpoint

**Lifecycle**: Created once at gateway startup, shared via `Arc<MetricsCollector>` in `AppState`.

**Thread Safety**: Uses `Arc` for shared ownership, `DashMap` for concurrent label cache access.

---

### 2. Request Metric

**Purpose**: Represents a single HTTP request event for metrics tracking.

**Attributes**:
- `model: String` - Requested model name (from request body)
- `backend_id: String` - Backend that served the request
- `status_code: u16` - HTTP status code (200, 500, etc.)
- `duration_seconds: f64` - Total request duration from handler entry to exit
- `timestamp: Instant` - Request start time

**Recorded Metrics**:
- Counter: `nexus_requests_total{model, backend, status}`
- Histogram: `nexus_request_duration_seconds{model, backend}`

**Source**: Instrumented in `src/api/completions.rs` handler.

**Validation Rules**:
- Model and backend names must be sanitized before use as labels
- Duration must be non-negative
- Status code must be valid HTTP code (100-599)

---

### 3. Error Metric

**Purpose**: Represents an error event with classification for debugging.

**Attributes**:
- `error_type: ErrorType` - Enum: `Timeout`, `BackendError`, `NoBackend`, `NoHealthyBackend`, `ParseError`, `Other`
- `model: String` - Model associated with the error
- `backend_id: Option<String>` - Backend if error occurred at backend level
- `timestamp: Instant` - Error occurrence time

**Recorded Metrics**:
- Counter: `nexus_errors_total{error_type, model}`

**Source**: Instrumented at error paths in `src/api/completions.rs`.

**Classification Rules**:
- Map `RoutingError::NoHealthyBackend` → `ErrorType::NoHealthyBackend`
- Map HTTP 503 from backend → `ErrorType::BackendError`
- Map request timeout → `ErrorType::Timeout`

---

### 4. Fallback Metric

**Purpose**: Tracks routing fallback events when primary model is unavailable.

**Attributes**:
- `from_model: String` - Original requested model
- `to_model: String` - Fallback model used
- `reason: String` - Why fallback occurred (e.g., "primary_unhealthy")
- `timestamp: Instant` - Fallback occurrence time

**Recorded Metrics**:
- Counter: `nexus_fallbacks_total{from_model, to_model}`

**Source**: Instrumented in routing layer when fallback chain is traversed.

**Validation Rules**:
- Both model names must be sanitized
- Fallback should only increment if actual model substitution occurred

---

### 5. Backend Health Metric

**Purpose**: Tracks backend health check latency for performance monitoring.

**Attributes**:
- `backend_id: String` - Backend identifier
- `latency_ms: u32` - Health check round-trip time in milliseconds
- `timestamp: Instant` - Health check time

**Recorded Metrics**:
- Histogram: `nexus_backend_latency_seconds{backend}`

**Source**: Instrumented in `src/health/mod.rs` after each health check.

**Conversion**: Convert milliseconds to seconds for histogram (Prometheus convention).

---

### 6. Token Count Metric

**Purpose**: Tracks token usage for cost estimation and monitoring.

**Attributes**:
- `model: String` - Model used
- `backend_id: String` - Backend that served the request
- `token_type: TokenType` - Enum: `Prompt`, `Completion`
- `count: u32` - Number of tokens

**Recorded Metrics**:
- Histogram: `nexus_tokens_total{model, backend, type}`

**Source**: Extracted from backend response (if available in OpenAI-compatible format).

**Note**: Optional - not all backends provide token counts.

---

### 7. Fleet State Gauges

**Purpose**: Represents current state of the backend fleet (point-in-time snapshot).

**Computed Attributes** (derived from Registry):
- `backends_total: usize` - Total registered backends
- `backends_healthy: usize` - Backends with status = Healthy
- `models_available: usize` - Distinct models across all healthy backends
- `pending_requests: usize` - Sum of pending requests across all backends (future)

**Recorded Metrics**:
- Gauge: `nexus_backends_total`
- Gauge: `nexus_backends_healthy`
- Gauge: `nexus_models_available`
- Gauge: `nexus_pending_requests{backend}`

**Update Strategy**: Computed on-demand when `/metrics` or `/v1/stats` is called.

---

### 8. Stats Response (JSON)

**Purpose**: JSON representation of metrics for `/v1/stats` endpoint.

**Schema**:
```json
{
  "uptime_seconds": 3600,
  "requests": {
    "total": 1000,
    "success": 950,
    "errors": 50
  },
  "backends": [
    {
      "id": "ollama-local",
      "requests": 500,
      "average_latency_ms": 1250.5,
      "pending": 2
    }
  ],
  "models": [
    {
      "name": "llama3:70b",
      "requests": 300,
      "average_duration_ms": 5000.0
    }
  ]
}
```

**Rust Types**:
```rust
#[derive(Serialize)]
pub struct StatsResponse {
    pub uptime_seconds: u64,
    pub requests: RequestStats,
    pub backends: Vec<BackendStats>,
    pub models: Vec<ModelStats>,
}

#[derive(Serialize)]
pub struct RequestStats {
    pub total: u64,
    pub success: u64,
    pub errors: u64,
}

#[derive(Serialize)]
pub struct BackendStats {
    pub id: String,
    pub requests: u64,
    pub average_latency_ms: f64,
    pub pending: usize,
}

#[derive(Serialize)]
pub struct ModelStats {
    pub name: String,
    pub requests: u64,
    pub average_duration_ms: f64,
}
```

**Computation**:
- Uptime: `Instant::now() - start_time`
- Request stats: Query Prometheus metrics handle for counter values
- Per-backend/model stats: Aggregate from Prometheus histogram data

---

## Entity Relationships

```
┌─────────────────────┐
│  MetricsCollector   │
│                     │
│  - registry         │◄────┐
│  - start_time       │     │
│  - label_cache      │     │
└─────────────────────┘     │
           │                │
           │ queries        │
           ▼                │
    ┌──────────────┐        │
    │   Registry   │────────┘
    │              │
    │ - backends   │
    │ - models     │
    └──────────────┘
           ▲
           │
           │ updated by
           │
    ┌──────────────┐
    │ HealthChecker│
    │              │
    │ Records:     │
    │ - backend    │
    │   latency    │
    └──────────────┘

┌─────────────────────┐
│  Request Handler    │
│  (completions.rs)   │
│                     │
│  Records:           │
│  - request count    │
│  - duration         │
│  - errors           │
│  - fallbacks        │
└─────────────────────┘
           │
           │ writes to
           ▼
    ┌──────────────┐
    │   Prometheus │
    │   Recorder   │
    │              │
    │ (global)     │
    └──────────────┘
           │
           │ scraped by
           ▼
    ┌──────────────┐
    │ /metrics     │
    │ handler      │
    └──────────────┘
```

---

## State Transitions

### Backend Health Status → Metrics

When `Registry.update_status(backend_id, status)` is called:
1. No direct metric update
2. Next `/metrics` or `/v1/stats` query computes gauges from current Registry state
3. `nexus_backends_healthy` gauge reflects new count

### Request Lifecycle → Metrics

```
Request arrives
    ↓
Start timer
    ↓
Route to backend (record fallback if occurs)
    ↓
Proxy request
    ↓
Response received OR error
    ↓
Stop timer
    ↓
Record metrics:
    - nexus_requests_total (counter)
    - nexus_request_duration_seconds (histogram)
    - nexus_errors_total (counter, if error)
    - nexus_tokens_total (histogram, if available)
```

---

## Validation & Constraints

### Label Sanitization

**Rule**: Prometheus labels must match regex `[a-zA-Z_][a-zA-Z0-9_]*`

**Implementation**:
```rust
fn sanitize_label(s: &str) -> String {
    // Replace invalid chars with underscore
    let mut result = s.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect::<String>();
    
    // Ensure first char is not a digit
    if result.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        result.insert(0, '_');
    }
    
    result
}
```

**Cached**: Use `DashMap<String, String>` to cache sanitized labels for performance.

---

### Cardinality Limits

**Risk**: Unbounded label values cause memory exhaustion.

**Mitigation**:
- Only use known dimensions: `model`, `backend`, `status`, `error_type`, `type`
- Never use request IDs, timestamps, or user data as labels
- Backend and model names come from finite Registry
- Monitor Prometheus memory usage in production

---

### Thread Safety

**Requirement**: All metrics recording must be thread-safe without locks in hot path.

**Implementation**:
- `metrics` crate uses `AtomicU64` for counters
- Histograms use pre-allocated atomic buckets
- `MetricsCollector.label_cache` uses `DashMap` (concurrent HashMap)
- No `Mutex` or `RwLock` in request path

---

## Performance Characteristics

| Operation | Target Latency | Implementation |
|-----------|----------------|----------------|
| Record counter | < 50ns | `atomic.fetch_add(1, Ordering::Relaxed)` |
| Record histogram | < 100ns | Find bucket + atomic increment |
| Sanitize label (cached) | < 10ns | DashMap lookup |
| Sanitize label (uncached) | < 500ns | String iteration + DashMap insert |
| Compute gauges | < 500µs | Iterate Registry backends (typically < 100) |
| Render /metrics | < 1ms | Prometheus exporter serialization |
| Compute /v1/stats | < 2ms | Aggregate histograms + compute averages |

**Total Request Overhead**: < 0.1ms (counter + histogram recording only)

---

## Testing Strategy

### Unit Tests

1. **Label sanitization**:
   - Test valid names pass through unchanged
   - Test invalid chars replaced with `_`
   - Test first char digit handling
   - Property test: Output always matches Prometheus regex

2. **Gauge computation**:
   - Mock Registry with known backends
   - Verify `backends_total` = backend count
   - Verify `backends_healthy` counts only healthy
   - Verify `models_available` counts unique models

3. **Stats aggregation**:
   - Mock Prometheus data
   - Verify JSON output matches expected schema
   - Verify averages computed correctly

### Integration Tests

1. **End-to-end request tracking**:
   - Send request to `/v1/chat/completions`
   - Query `/metrics`
   - Assert `nexus_requests_total` incremented
   - Assert `nexus_request_duration_seconds` has sample

2. **Error tracking**:
   - Trigger error condition (no backend)
   - Query `/metrics`
   - Assert `nexus_errors_total{error_type="no_backend"}` incremented

3. **Fallback tracking**:
   - Configure fallback chain
   - Trigger fallback
   - Query `/metrics`
   - Assert `nexus_fallbacks_total` incremented

### Performance Tests

1. **Benchmark metric recording overhead**:
   - Use `criterion` crate
   - Measure request handler latency with/without metrics
   - Assert overhead < 0.1ms (100µs)

2. **Benchmark /metrics endpoint**:
   - Pre-populate with 10k requests
   - Measure GET /metrics latency
   - Assert < 1ms p95

---

## Future Extensions

### Phase 2 Considerations (Not in Scope)

1. **Request queue depth**: Track pending requests per backend
2. **Circuit breaker state**: Expose circuit breaker status as gauge
3. **Cache hit rate**: If request caching is added
4. **Streaming metrics**: Track SSE stream duration separately
5. **Cost tracking**: If cost-per-request becomes available

These are mentioned for awareness but are NOT part of F09 implementation.
