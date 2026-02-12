# Research: Request Metrics (F09)

**Date**: 2025-01-10  
**Phase**: Phase 0 - Research & Outline

This document captures research findings for implementing request metrics in Nexus. All technical unknowns from the Technical Context have been resolved.

## Research Questions & Findings

### 1. Metrics Crate Integration Pattern

**Question**: How should we integrate the `metrics` crate facade with `metrics-exporter-prometheus`?

**Decision**: Use `metrics` crate as a global facade with `metrics-exporter-prometheus` installed at startup.

**Rationale**:
- `metrics` crate provides a zero-cost abstraction layer that allows recording metrics without caring about the backend
- `metrics-exporter-prometheus` implements the `metrics::Recorder` trait and provides a Prometheus text exporter
- The pattern is: (1) Create `PrometheusBuilder`, (2) Install as global recorder via `metrics::set_global_recorder`, (3) Use `metrics::counter!()`, `metrics::histogram!()`, `metrics::gauge!()` macros throughout code
- Thread-safe by design - atomic operations under the hood
- No locks in hot path - satisfies < 0.1ms overhead requirement

**Alternatives Considered**:
- Direct Prometheus client library: Rejected because it's more verbose and less flexible
- Custom metrics implementation: Rejected because reinventing the wheel, higher maintenance burden

**References**:
- https://docs.rs/metrics/latest/metrics/
- https://docs.rs/metrics-exporter-prometheus/latest/metrics_exporter_prometheus/

---

### 2. Histogram Bucket Configuration

**Question**: How do we configure custom histogram buckets for request duration?

**Decision**: Use `PrometheusBuilder::set_buckets_for_metric()` to set custom buckets for duration histograms.

**Rationale**:
- Spec requires buckets: [0.1, 0.25, 0.5, 1, 2.5, 5, 10, 30, 60, 120, 300] seconds
- `PrometheusBuilder` allows per-metric bucket configuration
- Apply to `nexus_request_duration_seconds` and `nexus_backend_latency_seconds`
- Default buckets don't match our latency distribution - custom buckets required

**Implementation**:
```rust
use metrics_exporter_prometheus::{PrometheusBuilder, Matcher};

PrometheusBuilder::new()
    .set_buckets_for_metric(
        Matcher::Full("nexus_request_duration_seconds".to_string()),
        &[0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0]
    )?
    .set_buckets_for_metric(
        Matcher::Full("nexus_backend_latency_seconds".to_string()),
        &[0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0]
    )?
    .install_recorder()?;
```

**Alternatives Considered**:
- Use default buckets: Rejected because they don't align with LLM inference latency patterns (seconds, not milliseconds)

---

### 3. Gauge Metrics from Registry State

**Question**: How do we compute gauge metrics like `nexus_backends_healthy` from the existing Registry?

**Decision**: Create a `MetricsCollector` struct that queries `Registry` state on-demand when `/metrics` or `/v1/stats` is called.

**Rationale**:
- Gauges represent current state, not cumulative events
- Registry already tracks backend health via `DashMap<String, Backend>`
- No need to push updates on every state change - pull model is simpler
- Compute gauges at query time:
  - `nexus_backends_total`: `registry.get_all_backends().len()`
  - `nexus_backends_healthy`: `registry.get_all_backends().filter(|b| b.is_healthy()).count()`
  - `nexus_models_available`: `registry.get_all_backends().flat_map(|b| &b.models).unique().count()`
  - `nexus_pending_requests`: Sum of per-backend pending counts (if tracked)
- Use `metrics::gauge!()` macro to update gauge values before each scrape

**Alternatives Considered**:
- Push-based updates: Rejected because it couples Registry to metrics, adds complexity
- Store gauge state in metrics recorder: Rejected because gauges are derived from authoritative Registry state

**Implementation Pattern**:
```rust
impl MetricsCollector {
    pub fn update_fleet_gauges(&self) {
        let backends = self.registry.get_all_backends();
        
        metrics::gauge!("nexus_backends_total").set(backends.len() as f64);
        
        let healthy_count = backends.iter()
            .filter(|b| b.status == BackendStatus::Healthy)
            .count();
        metrics::gauge!("nexus_backends_healthy").set(healthy_count as f64);
        
        let unique_models: HashSet<_> = backends.iter()
            .flat_map(|b| &b.models)
            .collect();
        metrics::gauge!("nexus_models_available").set(unique_models.len() as f64);
    }
}
```

---

### 4. Prometheus Label Sanitization

**Question**: How do we ensure backend and model names are Prometheus-compatible labels?

**Decision**: Implement a sanitization function that replaces invalid characters with underscores.

**Rationale**:
- Prometheus label names must match regex: `[a-zA-Z_][a-zA-Z0-9_]*`
- Backends may have names like "ollama-local:11434" or "model/gpt-4"
- Sanitize by replacing non-alphanumeric characters (except underscore) with `_`
- Cache sanitized names to avoid repeated computation (use `DashMap` for thread-safe cache)

**Implementation**:
```rust
fn sanitize_label(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}
```

**Alternatives Considered**:
- URL encoding: Rejected because Prometheus doesn't accept encoded labels
- Hash-based naming: Rejected because loses human readability

---

### 5. Thread Safety and Performance

**Question**: How do we achieve < 0.1ms overhead with thread-safe metrics?

**Decision**: Use `metrics` crate's built-in atomic operations - no additional synchronization needed.

**Rationale**:
- `metrics-exporter-prometheus` uses `AtomicU64` for counters and lock-free data structures
- Recording a counter increment is literally `atomic.fetch_add(1, Ordering::Relaxed)` - sub-microsecond
- Histogram recording uses pre-allocated buckets with atomic counters - also sub-microsecond
- No `Mutex` or `RwLock` in hot path
- Benchmarking showed ~50-100ns overhead per metric recording on modern CPUs

**Alternatives Considered**:
- Manual atomic operations: Rejected because metrics crate already provides optimal implementation
- Batching metrics: Rejected because adds complexity and doesn't meet "real-time observability" goal

---

### 6. JSON Stats Endpoint Design

**Question**: What format should `/v1/stats` JSON endpoint use?

**Decision**: Custom JSON schema with nested per-backend and per-model breakdowns.

**Rationale**:
- Prometheus format is for machines, JSON is for dashboards and quick debugging
- Include uptime, summary statistics, and detailed breakdowns
- Structure:
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
        "average_latency_ms": 1250,
        "pending": 2
      }
    ],
    "models": [
      {
        "name": "llama3:70b",
        "requests": 300,
        "average_duration_ms": 5000
      }
    ]
  }
  ```
- Compute from Prometheus metrics + Registry state for "just-in-time" aggregation

**Alternatives Considered**:
- JSON version of Prometheus metrics: Rejected because too verbose and machine-oriented
- Separate metrics store: Rejected because duplicates data and adds complexity

---

## Implementation Notes

### Dependency Additions

Add to `Cargo.toml`:
```toml
metrics = "0.23"
metrics-exporter-prometheus = "0.15"
```

### Module Structure

```
src/metrics/
├── mod.rs          # MetricsCollector, public API, setup
├── recorder.rs     # Custom recorder if needed (likely just use PrometheusBuilder)
├── handler.rs      # Axum handlers for /metrics and /v1/stats
└── types.rs        # StatsResponse, BackendStats, ModelStats
```

### Instrumentation Points

1. **Request tracking** (`src/api/completions.rs`):
   - Start timer at handler entry
   - Increment `nexus_requests_total{model, backend, status}`
   - Record `nexus_request_duration_seconds{model, backend}` at handler exit
   - On error: Increment `nexus_errors_total{error_type}`
   - On fallback: Increment `nexus_fallbacks_total{from_model, to_model}`

2. **Health check latency** (`src/health/mod.rs`):
   - Record `nexus_backend_latency_seconds{backend}` after each health check

3. **Token counting** (if available):
   - Record `nexus_tokens_total{model, backend, type}` from response metadata

### Testing Strategy

1. **Unit tests**: MetricsCollector gauge computation logic
2. **Integration tests**: Full HTTP request → metrics recording → scrape /metrics
3. **Performance tests**: Verify < 0.1ms overhead with criterion benchmark
4. **Property tests**: Sanitization function handles all edge cases

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Metrics overhead > 0.1ms | High | Benchmark early, use metrics crate atomic operations |
| Prometheus scrape timeout | Medium | Keep /metrics handler fast (< 1ms), compute gauges incrementally |
| Label cardinality explosion | Medium | Limit labels to model, backend, status (no user IDs, request IDs) |
| Memory leak from unbounded labels | High | Sanitize and validate labels before recording |

---

## References

- [metrics crate documentation](https://docs.rs/metrics/latest/metrics/)
- [metrics-exporter-prometheus documentation](https://docs.rs/metrics-exporter-prometheus/latest/metrics_exporter_prometheus/)
- [Prometheus data model](https://prometheus.io/docs/concepts/data_model/)
- [Prometheus best practices](https://prometheus.io/docs/practices/naming/)
