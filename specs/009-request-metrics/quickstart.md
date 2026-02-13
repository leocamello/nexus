# Quickstart: Request Metrics (F09)

This guide helps developers understand and implement the Request Metrics feature for Nexus.

## Overview

**Goal**: Track and expose request statistics for observability and debugging.

**Scope**:
- Track request counts, durations, errors, and fallbacks
- Expose metrics in Prometheus format at `GET /metrics`
- Expose JSON statistics at `GET /v1/stats`
- Use `metrics` crate with Prometheus exporter
- Achieve < 0.1ms overhead per request

**Dependencies**:
```toml
metrics = "0.24"
metrics-exporter-prometheus = "0.16"
```

---

## Architecture

### Component Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        Gateway Request                       │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
                  ┌───────────────────────┐
                  │  Completions Handler  │
                  │  (src/api/completions)│
                  └───────────────────────┘
                              │
                              │ 1. Start timer
                              │ 2. Route request
                              │ 3. Proxy to backend
                              │ 4. Record metrics
                              ▼
                  ┌───────────────────────┐
                  │   Metrics Recorder    │
                  │   (global, atomic)    │
                  └───────────────────────┘
                              │
                 ┌────────────┴────────────┐
                 │                         │
                 ▼                         ▼
     ┌───────────────────┐    ┌───────────────────┐
     │  /metrics Handler │    │  /v1/stats Handler│
     │  (Prometheus)     │    │  (JSON)           │
     └───────────────────┘    └───────────────────┘
                 │                         │
                 │                         │
                 ▼                         ▼
          ┌──────────┐              ┌──────────┐
          │Prometheus│              │Dashboard │
          │ Scraper  │              │ / Curl   │
          └──────────┘              └──────────┘
```

### Module Structure

```
src/metrics/
├── mod.rs          # Public API: MetricsCollector, setup_metrics(), label sanitization
├── handler.rs      # Axum handlers for /metrics and /v1/stats
└── types.rs        # StatsResponse, BackendStats, ModelStats, RequestStats
```

---

## Implementation Steps

### Step 1: Add Dependencies

**File**: `Cargo.toml`

```toml
[dependencies]
# ... existing dependencies ...
metrics = "0.24"
metrics-exporter-prometheus = "0.16"
```

**Verify**:
```bash
cargo check
```

---

### Step 2: Create Metrics Module

**File**: `src/metrics/mod.rs`

```rust
//! Metrics collection and export module.

mod handler;
mod types;

pub use handler::{metrics_handler, stats_handler};
pub use types::{BackendStats, ModelStats, RequestStats, StatsResponse};

use crate::registry::{BackendStatus, Registry};
use dashmap::DashMap;
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use std::sync::Arc;
use std::time::Instant;

/// Initialize Prometheus exporter with custom histogram buckets.
pub fn setup_metrics() -> Result<PrometheusHandle, Box<dyn std::error::Error>> {
    let duration_buckets = vec![0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0];
    let token_buckets = vec![10.0, 50.0, 100.0, 500.0, 1000.0, 4000.0, 8000.0, 16000.0, 32000.0, 64000.0, 128000.0];

    let handle = PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("nexus_request_duration_seconds".to_string()),
            &duration_buckets,
        )?
        .set_buckets_for_metric(
            Matcher::Full("nexus_backend_latency_seconds".to_string()),
            &duration_buckets,
        )?
        .set_buckets_for_metric(
            Matcher::Full("nexus_tokens_total".to_string()),
            &token_buckets,
        )?
        .install_recorder()?;

    Ok(handle)
}
```

---

### Step 3: Implement MetricsCollector

**File**: `src/metrics/mod.rs` (same file as Step 2)

```rust
pub struct MetricsCollector {
    registry: Arc<Registry>,
    start_time: Instant,
    label_cache: DashMap<String, String>,
    prometheus_handle: PrometheusHandle,
}

impl MetricsCollector {
    pub fn new(
        registry: Arc<Registry>,
        start_time: Instant,
        prometheus_handle: PrometheusHandle,
    ) -> Self {
        Self {
            registry,
            start_time,
            label_cache: DashMap::new(),
            prometheus_handle,
        }
    }
    
    /// Sanitize label for Prometheus compatibility.
    pub fn sanitize_label(&self, s: &str) -> String {
        if let Some(cached) = self.label_cache.get(s) {
            return cached.clone();
        }
        
        let mut result: String = s.chars()
            .map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' })
            .collect();
        
        if result.chars().next().map_or(false, |c| c.is_ascii_digit()) {
            result.insert(0, '_');
        }
        
        self.label_cache.insert(s.to_string(), result.clone());
        result
    }
    
    /// Update fleet gauges from Registry state.
    pub fn update_fleet_gauges(&self) {
        let backends = self.registry.get_all_backends();
        
        metrics::gauge!("nexus_backends_total").set(backends.len() as f64);
        
        let healthy_count = backends.iter()
            .filter(|b| b.status == BackendStatus::Healthy)
            .count();
        metrics::gauge!("nexus_backends_healthy").set(healthy_count as f64);
        
        let unique_models: std::collections::HashSet<String> = backends.iter()
            .filter(|b| b.status == BackendStatus::Healthy)
            .flat_map(|b| b.models.iter().map(|m| m.id.clone()))
            .collect();
        metrics::gauge!("nexus_models_available").set(unique_models.len() as f64);
    }
    
    /// Render Prometheus metrics in text format.
    pub fn render_metrics(&self) -> String {
        self.prometheus_handle.render()
    }
    
    /// Get uptime in seconds.
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
    
    /// Get a reference to the registry.
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}
```

---

### Step 4: Instrument Request Handler

**File**: `src/api/completions.rs`

Add at the top of the `handle` function:

```rust
pub async fn handle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Response, ApiError> {
    // Start metrics timer
    let start = std::time::Instant::now();
    let model_label = state.metrics_collector.sanitize_label(&request.model);
    
    info!(model = %request.model, stream = request.stream, "Chat completion request");
    
    // ... existing routing logic ...
    
    // After successful response (before return):
    let backend_label = state.metrics_collector.sanitize_label(&backend.id);
    let duration = start.elapsed().as_secs_f64();
    
    metrics::counter!("nexus_requests_total",
        "model" => model_label.clone(),
        "backend" => backend_label.clone(),
        "status" => "200"
    ).increment(1);
    
    metrics::histogram!("nexus_request_duration_seconds",
        "model" => model_label,
        "backend" => backend_label
    ).record(duration);
    
    // On error path:
    let error_type = match error {
        ApiError::NoHealthyBackend { .. } => "no_healthy_backend",
        ApiError::Timeout { .. } => "timeout",
        // ... other error types ...
    };
    
    metrics::counter!("nexus_errors_total",
        "error_type" => error_type,
        "model" => model_label
    ).increment(1);
}
```

---

### Step 5: Instrument Health Checker

**File**: `src/health/mod.rs`

In the `check_backend` method:

```rust
pub async fn check_backend(&self, backend: &Backend) -> HealthCheckResult {
    let start = Instant::now();
    
    // ... existing health check logic ...
    
    // After successful check:
    let latency_seconds = start.elapsed().as_secs_f64();
    let backend_label = sanitize_label(&backend.id);
    
    metrics::histogram!("nexus_backend_latency_seconds",
        "backend" => backend_label
    ).record(latency_seconds);
    
    // ... rest of function ...
}
```

---

### Step 6: Implement Metrics Handlers

**File**: `src/metrics/handler.rs`

```rust
use crate::api::AppState;
use super::{BackendStats, ModelStats, RequestStats, StatsResponse};
use axum::{extract::State, response::IntoResponse, http::StatusCode, Json};
use std::sync::Arc;

/// GET /metrics - Prometheus exposition format
pub async fn metrics_handler(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    // Update gauges before scrape
    state.metrics_collector.update_fleet_gauges();
    
    // Render Prometheus metrics from the collector's handle
    let metrics = state.metrics_collector.render_metrics();
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        metrics,
    )
}

/// GET /v1/stats - JSON statistics
pub async fn stats_handler(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    // Update gauges
    state.metrics_collector.update_fleet_gauges();
    
    // Compute stats from Registry atomics
    let uptime_seconds = state.metrics_collector.uptime_seconds();
    let requests = compute_request_stats();
    let backends = compute_backend_stats(state.metrics_collector.registry());
    let models = compute_model_stats();
    
    Json(StatsResponse {
        uptime_seconds,
        requests,
        backends,
        models,
    })
}
```

---

### Step 7: Register Routes

**File**: `src/api/mod.rs`

Update `create_router`:

```rust
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(completions::handle))
        .route("/v1/models", get(models::handle))
        .route("/health", get(health::handle))
        // Metrics endpoints
        .route("/metrics", get(crate::metrics::handler::metrics_handler))
        .route("/v1/stats", get(crate::metrics::handler::stats_handler))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_SIZE))
        .with_state(state)
}
```

Update `AppState`:

```rust
pub struct AppState {
    pub registry: Arc<Registry>,
    pub config: Arc<NexusConfig>,
    pub http_client: reqwest::Client,
    pub router: Arc<routing::Router>,
    pub start_time: Instant,
    pub metrics_collector: Arc<MetricsCollector>,  // NEW
}
```

---

### Step 8: Initialize at Startup

**File**: `src/main.rs`

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ... existing setup ...
    
    // Create app state (metrics are initialized automatically inside AppState::new)
    let state = Arc::new(AppState::new(
        Arc::clone(&registry),
        Arc::clone(&config),
    ));
    
    // ... start server ...
}
```

> **Note**: `AppState::new()` calls `setup_metrics()` internally and stores the
> `PrometheusHandle` in the `MetricsCollector`. If the global recorder is already
> installed (e.g., in tests), it falls back to creating a local handle.

---

## Testing

### Unit Test: Label Sanitization

**File**: `src/metrics/mod.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    fn get_test_handle() -> PrometheusHandle {
        PrometheusBuilder::new().build_recorder().handle()
    }
    
    #[test]
    fn test_sanitize_label() {
        let registry = Arc::new(Registry::new());
        let collector = MetricsCollector::new(
            registry,
            Instant::now(),
            get_test_handle(),
        );
        
        assert_eq!(collector.sanitize_label("valid_name"), "valid_name");
        assert_eq!(collector.sanitize_label("gpt-4"), "gpt_4");
        assert_eq!(collector.sanitize_label("ollama:11434"), "ollama_11434");
        assert_eq!(collector.sanitize_label("123model"), "_123model");
    }
}
```

### Integration Test: Endpoint Contracts

**File**: `tests/metrics_integration.rs`

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::Service;

#[tokio::test]
async fn test_metrics_endpoint_returns_200() {
    let mut app = create_app_empty();
    let response = app
        .call(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_stats_endpoint_json_schema() {
    let mut app = create_app_empty();
    let response = app
        .call(
            Request::builder()
                .uri("/v1/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    let body = get_body_string(response).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    
    assert!(json.get("uptime_seconds").is_some());
    assert!(json.get("requests").is_some());
    assert!(json.get("backends").is_some());
    assert!(json.get("models").is_some());
}
```

> **Note**: Integration tests use `tower::Service` with `app.call()` pattern
> (not `tower::ServiceExt::oneshot`). The `metrics` crate's global recorder
> means tests verify behaviour through HTTP status codes and JSON schemas rather
> than asserting specific Prometheus counter values.

---

## Usage

### Query Prometheus Metrics

```bash
curl http://localhost:8000/metrics
```

**Output**:
```
# HELP nexus_requests_total Total number of requests processed by the gateway
# TYPE nexus_requests_total counter
nexus_requests_total{model="llama3_70b",backend="ollama_local",status="200"} 1234

# HELP nexus_request_duration_seconds Request duration from handler entry to response
# TYPE nexus_request_duration_seconds histogram
nexus_request_duration_seconds_bucket{model="llama3_70b",backend="ollama_local",le="0.5"} 12
nexus_request_duration_seconds_sum{model="llama3_70b",backend="ollama_local"} 1600.5
nexus_request_duration_seconds_count{model="llama3_70b",backend="ollama_local"} 320
```

---

### Query JSON Stats

```bash
curl http://localhost:8000/v1/stats | jq
```

**Output**:
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

---

## Performance Validation

### Benchmark Metrics Overhead

**File**: `benches/metrics_overhead.rs`

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_metric_recording(c: &mut Criterion) {
    // Setup metrics
    crate::metrics::setup_metrics().unwrap();
    
    c.bench_function("record_counter", |b| {
        b.iter(|| {
            metrics::counter!("nexus_requests_total",
                "model" => "test",
                "backend" => "test",
                "status" => "200"
            ).increment(1);
        });
    });
    
    c.bench_function("record_histogram", |b| {
        b.iter(|| {
            metrics::histogram!("nexus_request_duration_seconds",
                "model" => "test",
                "backend" => "test"
            ).record(black_box(1.5));
        });
    });
}

criterion_group!(benches, benchmark_metric_recording);
criterion_main!(benches);
```

**Run**:
```bash
cargo bench
```

**Expected**:
- Counter recording: < 50ns per operation
- Histogram recording: < 100ns per operation
- Total overhead: < 0.1ms (100µs) per request

---

## Troubleshooting

### Metrics not appearing in /metrics

**Cause**: Prometheus recorder not installed  
**Solution**: Verify `setup_metrics()` is called before any metric recording

### Label cardinality warning

**Cause**: Too many unique label combinations  
**Solution**: Verify only using `model`, `backend`, `status` labels (no request IDs)

### /metrics endpoint slow (> 1ms)

**Cause**: Too many time series  
**Solution**: Check Registry size, consider sampling

---

## Next Steps

After completing F09:
1. **F10: Web Dashboard** - Consume `/v1/stats` for real-time visualization
2. **F11: Structured Logging** - Correlate logs with metrics
3. **Production deployment** - Configure Prometheus scraping

---

## References

- [Feature Spec](./spec.md)
- [Data Model](./data-model.md)
- [Prometheus Contract](./contracts/prometheus.txt)
- [Stats API Contract](./contracts/stats-api.md)
- [metrics crate docs](https://docs.rs/metrics/)
- [metrics-exporter-prometheus docs](https://docs.rs/metrics-exporter-prometheus/)
