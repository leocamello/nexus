# Implementation Plan: Request Metrics (F09)

**Branch**: `009-request-metrics` | **Date**: 2025-01-10 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/009-request-metrics/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Track request statistics for observability and debugging. Expose metrics in both Prometheus-compatible format (GET /metrics) and JSON format (GET /v1/stats). This is the first feature of v0.2 (Observability) and lays the foundation for F10 (Web Dashboard) and F11 (Structured Logging).

**Technical Approach**: Use the `metrics` crate facade with `metrics-exporter-prometheus` for Prometheus export. Create a new `src/metrics/` module with a `MetricsCollector` struct that will be added to the existing `AppState`. Instrument existing handlers in `src/api/completions.rs` and `src/health/mod.rs` to track requests and latencies. Expose two new routes: `GET /metrics` (Prometheus text format) and `GET /v1/stats` (JSON format). Use lock-free atomic operations for thread safety with < 0.1ms overhead per request.

## Technical Context

**Language/Version**: Rust 1.75 (stable)  
**Primary Dependencies**: 
- `axum` 0.7 (HTTP framework)
- `tokio` 1.x (async runtime)
- `reqwest` 0.12 (HTTP client)
- `metrics` crate (facade for metrics collection)
- `metrics-exporter-prometheus` (Prometheus export)
- `dashmap` 6.x (concurrent HashMap for registry state)

**Storage**: In-memory only - metrics reset on restart (no persistence required)  
**Testing**: `cargo test` with unit tests, integration tests, and property-based tests (proptest)  
**Target Platform**: Linux server (cross-platform via Rust)  
**Project Type**: Single Rust binary (web server)  
**Performance Goals**: 
- < 0.1ms overhead per request recording
- < 1ms for metrics computation at /metrics or /v1/stats endpoints
- Support 10,000+ requests per second without degradation

**Constraints**: 
- Thread-safe via atomic operations (no locks in hot path)
- Must not interfere with existing OpenAI-compatible API endpoints
- Histogram buckets: [0.1, 0.25, 0.5, 1, 2.5, 5, 10, 30, 60, 120, 300] seconds
- Prometheus label sanitization for backend and model names

**Scale/Scope**: 
- Tracks metrics for unlimited backends and models
- Metrics reset to zero on gateway restart
- Designed for continuous Prometheus scraping (15-30 second intervals)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Simplicity Gate
- [x] Using ≤3 main modules for initial implementation? **YES** - Single module `src/metrics/` with collector, recorder, and handler components
- [x] No speculative "might need" features? **YES** - Only implementing metrics defined in spec (counters, histograms, gauges)
- [x] No premature optimization? **YES** - Using standard `metrics` crate patterns, atomic operations are required by spec
- [x] Start with simplest approach that could work? **YES** - Direct instrumentation at handler level

### Anti-Abstraction Gate
- [x] Using Axum/Tokio/reqwest directly (no wrapper layers)? **YES** - Direct axum handlers for /metrics and /v1/stats
- [x] Single representation for each data type? **YES** - metrics crate types for recording, Prometheus text/JSON for output
- [x] No "framework on top of framework" patterns? **YES** - metrics crate is a thin facade, not a framework
- [x] Abstractions justified by actual (not theoretical) needs? **YES** - MetricsCollector needed to compute gauges from Registry state

### Integration-First Gate
- [x] API contracts defined before implementation? **YES** - Prometheus text format and JSON schema will be in /contracts/
- [x] Integration tests planned with real/mock backends? **YES** - Test request tracking with actual axum handlers
- [x] End-to-end flow testable? **YES** - Send request → verify counter increments → query /metrics endpoint

### Performance Gate
- [x] Routing decision target: < 1ms? **N/A** - Not a routing feature
- [x] Total overhead target: < 5ms? **YES** - Target is < 0.1ms for recording, < 1ms for metrics computation
- [x] Memory baseline target: < 50MB? **YES** - Metrics use atomic counters and histograms, minimal overhead

**All gates PASS** - No complexity tracking needed.

## Project Structure

### Documentation (this feature)

```text
specs/009-request-metrics/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
│   ├── prometheus.txt   # Example Prometheus metrics output
│   └── stats-api.json   # JSON schema for /v1/stats endpoint
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
src/
├── metrics/             # NEW: Metrics collection module
│   ├── mod.rs          # Public API, MetricsCollector
│   ├── recorder.rs     # Custom metrics recorder implementation
│   ├── handler.rs      # /metrics and /v1/stats handlers
│   └── types.rs        # Shared types (StatsResponse, etc.)
├── api/
│   ├── mod.rs          # AppState gains metrics_collector: Arc<MetricsCollector>
│   ├── completions.rs  # MODIFIED: Instrument request tracking
│   └── types.rs        # Possibly add metrics-related types
├── health/
│   └── mod.rs          # MODIFIED: Instrument health check latency
└── lib.rs              # Export metrics module

tests/
├── integration/
│   └── metrics_test.rs # NEW: End-to-end metrics tests
└── unit/
    └── metrics/        # NEW: Unit tests for MetricsCollector
```

**Structure Decision**: Single project (Rust binary). New `src/metrics/` module for metrics collection and export. Existing modules (`src/api/`, `src/health/`) instrumented with metrics recording calls. Two new HTTP routes registered in `src/api/mod.rs` router.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

No violations - all gates passed.

---

## Phase 0: Research Summary

**Status**: ✅ Complete

**Key Decisions**:
1. Use `metrics` crate facade with `metrics-exporter-prometheus` for Prometheus export
2. Custom histogram buckets: [0.1, 0.25, 0.5, 1, 2.5, 5, 10, 30, 60, 120, 300] seconds
3. Pull-based gauge computation: Query Registry state at scrape time (no push-based updates)
4. Label sanitization: Replace non-alphanumeric chars (except `_`) with underscore
5. Thread safety: Built-in atomic operations in `metrics` crate (no manual synchronization)
6. JSON stats format: Custom schema with per-backend and per-model breakdowns

**Output**: [research.md](./research.md)

---

## Phase 1: Design Summary

**Status**: ✅ Complete

**Core Entities**:
1. **MetricsCollector**: Central coordinator for metrics collection and gauge computation
2. **Request Metric**: Tracks request count, duration, status per model/backend
3. **Error Metric**: Tracks error count by type (timeout, backend_error, etc.)
4. **Fallback Metric**: Tracks fallback routing events
5. **Backend Health Metric**: Tracks health check latency
6. **Token Count Metric**: Tracks token usage (optional)
7. **Fleet State Gauges**: Backends total/healthy, models available, pending requests
8. **Stats Response**: JSON representation for `/v1/stats` endpoint

**API Contracts**:
- `/metrics`: Prometheus text format with counters, histograms, gauges
- `/v1/stats`: JSON format with uptime, aggregate stats, per-backend/model breakdowns

**Outputs**:
- [data-model.md](./data-model.md)
- [contracts/prometheus.txt](./contracts/prometheus.txt)
- [contracts/stats-api.md](./contracts/stats-api.md)
- [contracts/stats-api-schema.json](./contracts/stats-api-schema.json)
- [quickstart.md](./quickstart.md)

**Agent Context**: Updated GitHub Copilot instructions with metrics technology.

---

## Phase 2: Implementation Readiness

**Status**: ✅ Complete - Ready for `/speckit.tasks` command

**Next Steps**:
1. Run `/speckit.tasks` to generate `tasks.md` with implementation tasks
2. Implement tasks following TDD approach (tests first)
3. Benchmark to verify < 0.1ms overhead requirement
4. Integration test with real backends

**Performance Targets**:
- Metric recording: < 0.1ms (100µs) per request
- `/metrics` endpoint: < 1ms response time
- `/v1/stats` endpoint: < 2ms response time
- Support 10,000+ requests/second

**Constitution Re-check**: All gates still PASS after Phase 1 design.

---

## Implementation Notes

### Critical Path

1. Add dependencies (`metrics`, `metrics-exporter-prometheus`) to `Cargo.toml`
2. Create `src/metrics/` module with collector, handlers, types
3. Initialize Prometheus exporter at gateway startup
4. Instrument `src/api/completions.rs` for request tracking
5. Instrument `src/health/mod.rs` for health check latency
6. Register `/metrics` and `/v1/stats` routes in `src/api/mod.rs`
7. Add `metrics_collector: Arc<MetricsCollector>` to `AppState`

### Testing Strategy

- **Unit tests**: Label sanitization, gauge computation, stats aggregation
- **Integration tests**: End-to-end request → metrics recording → scrape
- **Performance tests**: Benchmark with `criterion` to verify overhead
- **Contract tests**: Validate Prometheus and JSON output formats

### Risk Mitigation

- **Performance**: Benchmark early (Step 1 of implementation)
- **Cardinality**: Limit labels to model, backend, status only
- **Thread safety**: Use built-in `metrics` crate atomics (no manual locks)

---

## Artifacts Generated

| File | Purpose | Status |
|------|---------|--------|
| plan.md | This file - implementation plan | ✅ Complete |
| research.md | Phase 0 research findings | ✅ Complete |
| data-model.md | Entity definitions and relationships | ✅ Complete |
| contracts/prometheus.txt | Prometheus metrics specification | ✅ Complete |
| contracts/stats-api.md | JSON stats API documentation | ✅ Complete |
| contracts/stats-api-schema.json | JSON schema for /v1/stats | ✅ Complete |
| quickstart.md | Developer implementation guide | ✅ Complete |
| tasks.md | Implementation tasks (Phase 2) | ⏳ Pending `/speckit.tasks` |

---

## References

- [Feature Spec](./spec.md)
- [Constitution](./.specify/memory/constitution.md)
- [Metrics Crate Docs](https://docs.rs/metrics/)
- [Prometheus Best Practices](https://prometheus.io/docs/practices/naming/)
