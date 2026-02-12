# Code Walkthrough: Request Metrics (F09)

**Feature**: F09 Request Metrics  
**PR**: #107  
**Branch**: `009-request-metrics`  
**Date**: 2026-02-12

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    HTTP Request Flow                         │
│                                                             │
│  Client → POST /v1/chat/completions                         │
│           │                                                 │
│           ├─ start_time = Instant::now()                     │
│           │                                                 │
│           ├─ Router selects backend                          │
│           │   └─ On routing error → nexus_errors_total++     │
│           │                                                 │
│           ├─ proxy_request() to backend                      │
│           │   ├─ Success:                                    │
│           │   │   ├─ nexus_requests_total{status=200}++      │
│           │   │   ├─ nexus_request_duration_seconds.record() │
│           │   │   ├─ nexus_fallbacks_total++ (if fallback)   │
│           │   │   └─ nexus_tokens_total.record() (if usage)  │
│           │   └─ Error:                                      │
│           │       └─ nexus_errors_total{type}++              │
│           │                                                 │
│           └─ Return response                                 │
│                                                             │
│  Prometheus → GET /metrics                                   │
│               ├─ update_fleet_gauges() ← Registry            │
│               └─ PrometheusHandle::render()                  │
│                                                             │
│  Dashboard → GET /v1/stats                                   │
│              ├─ update_fleet_gauges() ← Registry             │
│              ├─ compute_request_stats()                      │
│              ├─ compute_backend_stats() ← Registry atomics   │
│              └─ compute_model_stats()                        │
└─────────────────────────────────────────────────────────────┘
```

## New Module: `src/metrics/`

### `src/metrics/mod.rs` — MetricsCollector

**Purpose**: Central coordinator for metrics collection and gauge computation.

**Key type**: `MetricsCollector`
- Holds `Arc<Registry>` for computing gauge values
- Holds `PrometheusHandle` for rendering Prometheus text format
- Caches sanitized labels in `DashMap<String, String>` for performance

**Key functions**:
- `setup_metrics()` — Initializes Prometheus recorder with custom histogram buckets [0.1s - 300s]
- `sanitize_label()` — Converts model/backend names to valid Prometheus labels (cached)
- `update_fleet_gauges()` — Computes backends_total, backends_healthy, models_available from Registry
- `render_metrics()` — Delegates to PrometheusHandle for Prometheus text output

**Design decisions**:
- Labels are cached because the same model/backend names are used repeatedly
- Fleet gauges are "pull-based" — computed at scrape time from Registry state, not pushed
- `setup_metrics()` returns a handle; the global recorder is installed once

### `src/metrics/types.rs` — Response Types

**Types**: `StatsResponse`, `RequestStats`, `BackendStats`, `ModelStats`
- Simple Serde `Serialize` structs for JSON output
- No business logic — pure data containers

### `src/metrics/handler.rs` — HTTP Handlers

**Endpoints**:
- `metrics_handler` (GET /metrics) — Updates gauges, renders Prometheus text
- `stats_handler` (GET /v1/stats) — Updates gauges, computes stats JSON

**Known limitation**: `compute_request_stats()` and `compute_model_stats()` return placeholders.
The `metrics` crate doesn't provide a query API for recorded values. Accurate per-request
stats are available via the Prometheus scrape endpoint (GET /metrics). The JSON endpoint
sources backend stats from Registry atomics which are accurate.

## Modified Files

### `src/api/completions.rs` — Request Instrumentation

**Changes**: Added metrics recording to the `handle()` function:
- Timer start at function entry
- Counter increment on success (nexus_requests_total)
- Histogram record on success (nexus_request_duration_seconds)
- Error counter on routing and backend errors (nexus_errors_total)
- Fallback counter when fallback chain used (nexus_fallbacks_total)
- Token histogram when usage data available (nexus_tokens_total)

**Pattern**: All recording uses `state.metrics_collector.sanitize_label()` for label safety.

### `src/health/mod.rs` — Health Check Latency

**Change**: Added `nexus_backend_latency_seconds` histogram recording after successful health checks.
Latency converted from milliseconds to seconds for Prometheus convention.

### `src/api/mod.rs` — AppState & Router

**Changes**:
- Added `metrics_collector: Arc<MetricsCollector>` to `AppState`
- `setup_metrics()` called in `AppState::new()` with fallback for test environments
- Two new routes: `/metrics` and `/v1/stats`

### `Cargo.toml` — Dependencies

**Added**: `metrics = "0.24"`, `metrics-exporter-prometheus = "0.16"`

### `src/lib.rs` — Module Registration

**Added**: `pub mod metrics;`

## Metrics Reference

| Metric | Type | Labels | Source |
|--------|------|--------|--------|
| nexus_requests_total | Counter | model, backend, status | completions.rs |
| nexus_errors_total | Counter | error_type, model | completions.rs |
| nexus_fallbacks_total | Counter | from_model, to_model | completions.rs |
| nexus_request_duration_seconds | Histogram | model, backend | completions.rs |
| nexus_backend_latency_seconds | Histogram | backend | health/mod.rs |
| nexus_tokens_total | Histogram | model, backend, type | completions.rs |
| nexus_backends_total | Gauge | — | MetricsCollector |
| nexus_backends_healthy | Gauge | — | MetricsCollector |
| nexus_models_available | Gauge | — | MetricsCollector |

## Test Coverage

| File | Tests | Location |
|------|-------|----------|
| mod.rs | 5 | MetricsCollector construction, label sanitization (4 cases) |
| types.rs | 1 | StatsResponse serialization |
| handler.rs | 3 | compute_request_stats, compute_backend_stats, compute_model_stats |

All 9 new tests pass. Total suite: 365 tests.
