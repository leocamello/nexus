# Research: Quality Tracking & Backend Profiling

**Feature**: F16 Quality Tracking & Backend Profiling  
**Date**: 2025-01-24  
**Status**: Complete

## Overview

This document consolidates research findings for implementing rolling-window quality metrics that feed into Nexus's intelligent routing system. The research covers data structures, metric patterns, and async reconciliation loop best practices.

---

## 1. Rolling Window Statistics Implementation

### Decision: VecDeque with Periodic Pruning

**Rationale**: The existing Nexus implementation uses `VecDeque<RequestOutcome>` with periodic pruning, which provides optimal balance of:
- **Memory efficiency**: O(n) where n = requests in 24-hour window
- **Computation efficiency**: O(n) scan per reconciliation interval (30s), vs. O(n) per request
- **Simplicity**: Leverages stdlib without external dependencies

**Data Structure**:
```rust
pub struct QualityMetricsStore {
    outcomes: DashMap<String, RwLock<VecDeque<RequestOutcome>>>,
    metrics: DashMap<String, AgentQualityMetrics>,
}

struct RequestOutcome {
    timestamp: Instant,
    success: bool,
    ttft_ms: u32,
}
```

**Alternatives Considered**:
- **Ring buffer (fixed-size)**: Rejected because request volume varies; would waste memory or lose data
- **Timestamp-based filtering without VecDeque**: Rejected because O(n) scans on every read are inefficient
- **Incremental aggregation**: Considered for future optimization if reconciliation overhead exceeds 1ms

**Memory Budget**:
- **Per backend**: ~500KB for 24-hour history at 1000 req/hour (24K entries × 20 bytes)
- **Total for 10 backends**: ~5MB (acceptable within 50MB baseline target)

---

## 2. Thread-Safe Concurrent Access

### Decision: DashMap + RwLock Pattern

**Rationale**: 
- **DashMap**: Lock-free concurrent map provides per-agent isolation, minimizing contention
- **RwLock<VecDeque>**: Multiple readers can compute metrics concurrently; single writer (reconciliation loop) holds exclusive lock
- **parking_lot::RwLock**: Could improve performance (~10% faster) but stdlib is sufficient for current load

**Concurrency Model**:
1. **Routing path** (read-only): Reads computed metrics from `DashMap<String, AgentQualityMetrics>`
2. **Reconciliation loop** (write): Scans raw outcomes, computes new metrics, updates cache
3. **Request completion** (write): Appends new `RequestOutcome` to VecDeque

**Lock Granularity**: Per-agent locks prevent global contention across backends.

**Alternatives Considered**:
- **Arc<Mutex<HashMap>>**: Rejected because single global lock would block all concurrent routing
- **Lockless data structures (crossbeam)**: Rejected as premature optimization; current approach meets <1ms routing target

---

## 3. Prometheus Metrics Patterns

### Decision: Gauges for Quality Ratios, Histograms for Latency

**Metric Types**:
- **Gauge**: `nexus_agent_error_rate` (0.0–1.0), `nexus_agent_success_rate_24h` (0.0–1.0)
- **Histogram**: `nexus_agent_ttft_seconds` with buckets `[0.05, 0.1, 0.5, 1.0, 5.0]`
- **Counter**: Not used for quality (ratios fluctuate)

**Label Strategy**:
- **agent_id**: Uniquely identifies backend+model combination
- **Cardinality budget**: 10 backends × 20 models = 200 time series (well within Prometheus limits)
- **Label sanitization**: Replace non-alphanumeric chars with underscore (`backend-prod:8080` → `backend_prod_8080`)

**Update Pattern**:
- Gauges updated in `quality_reconciliation_loop` every 30 seconds
- Prometheus scrapes at 15-second interval (metrics may be up to 30s stale)

**Alternatives Considered**:
- **Summary metrics**: Rejected because histograms provide percentile flexibility in PromQL
- **Per-request metric updates**: Rejected due to overhead (30x more updates than batch approach)

---

## 4. Async Reconciliation Loop Patterns

### Decision: Tokio Background Task with Interval Timer

**Pattern**:
```rust
pub async fn quality_reconciliation_loop(
    store: Arc<QualityMetricsStore>,
    config: QualityConfig,
) {
    let mut interval = tokio::time::interval(
        Duration::from_secs(config.metrics_interval_seconds)
    );
    
    loop {
        interval.tick().await;
        store.recompute_all();
        // Update Prometheus gauges
    }
}
```

**Rationale**:
- **Non-blocking**: Tokio async allows concurrent routing during computation
- **Fixed interval**: Simplifies reasoning vs. dynamic scheduling
- **Graceful failure**: Loop continues even if single recompute fails (error logged, metrics retain last known values)

**Error Handling**:
- Request history unavailable → Use last known metrics + log warning
- Metric computation error → Skip update, retain previous values
- Prometheus export failure → Non-fatal (routing continues)

**Alternatives Considered**:
- **Cron-based scheduling**: Rejected as overkill for in-process task
- **Event-driven recomputation**: Rejected because periodic batch is more efficient
- **Thread-based loop**: Rejected in favor of async (lower overhead, better integration with Tokio runtime)

---

## 5. Handling Incomplete Data Windows

### Decision: Safe Defaults with Confidence Indicators

**Strategy**:
```rust
// Error rate: default to 0.0 (optimistic) when no requests
let error_rate_1h = if count_1h > 0 {
    errors_1h as f32 / count_1h as f32
} else {
    0.0
};

// JSON API: omit metrics if insufficient data
if m.request_count_1h > 0 {
    Some(m.error_rate_1h)
} else {
    None  // Skip in response
}
```

**Rationale**:
- **New backends**: Should not be penalized for lack of history
- **Low-traffic backends**: Default assumes healthy until proven otherwise
- **Minimum sample threshold**: Could add (e.g., require 10 requests before computing), but current approach is simpler

**Edge Cases Handled**:
- **System startup (no history)**: All backends start with neutral scores
- **All backends degraded**: Routing includes all with penalties (never fails all requests)
- **Clock skew**: Uses `Instant` (monotonic) for relative measurements, not `SystemTime`

**Alternatives Considered**:
- **Require minimum samples**: Rejected as too restrictive for low-traffic scenarios
- **Exponential backoff**: Considered for gradual re-inclusion of degraded backends (future enhancement)

---

## 6. Integration with Existing Nexus Architecture

### Request History System

**Current Implementation** (`src/dashboard/history.rs`):
- Ring buffer: 100 entries max (FIFO eviction)
- Per-entry: timestamp, model, backend_id, duration_ms, status, error_message

**Quality Tracking Integration**:
- Request history provides raw data for quality metrics
- Quality store maintains separate 24-hour rolling window (larger capacity)
- Dashboard history is for UI; quality outcomes are for routing

**Data Flow**:
```
Request Completion
    ↓
RequestHistory.add() [100 entries, UI only]
    ↓
QualityMetricsStore.record_outcome() [24h window, routing]
    ↓
quality_reconciliation_loop [periodic]
    ↓
QualityReconciler.run() [routing decisions]
```

### Reconciler Pipeline

**Pipeline Order**:
1. RequestAnalyzer → Parse requirements
2. PrivacyReconciler → Enforce privacy zones
3. BudgetReconciler → Apply cost constraints
4. TierReconciler → Filter by capability tier
5. **QualityReconciler** → **Exclude degraded backends (error_rate_1h)**
6. **SchedulerReconciler** → **Penalize slow backends (avg_ttft_ms)**

**Quality Integration Points**:
- **QualityReconciler**: Binary exclusion (error_rate_1h ≥ threshold)
- **SchedulerReconciler**: Scoring penalty (TTFT-based weighting)

---

## 7. Configuration System

### TOML Schema

```toml
[quality]
error_rate_threshold = 0.5              # 50% error rate triggers exclusion
ttft_penalty_threshold_ms = 3000        # 3-second TTFT applies penalty
metrics_interval_seconds = 30           # Recompute every 30 seconds
```

**Rationale**:
- **error_rate_threshold**: 50% is conservative (excludes only severely degraded backends)
- **ttft_penalty_threshold_ms**: 3 seconds captures "slow" backends without false positives
- **metrics_interval_seconds**: 30 seconds balances freshness vs. CPU overhead

**Dynamic Configuration**: Config changes require restart (acceptable for operational tuning).

---

## 8. Performance Characteristics

### Routing Latency Budget

**Target**: <1ms per routing decision (constitution requirement)

**Quality Overhead**:
- **Metric read**: O(1) DashMap lookup (~100ns)
- **Total quality overhead**: <10μs per request

**Reconciliation Overhead** (per 30-second interval):
- **Per backend**: ~50μs to scan 1000 outcomes
- **10 backends**: ~500μs total
- **Prometheus updates**: ~100μs per gauge

**Memory Baseline**: ~5MB for 10 backends × 24h history (within 50MB target)

### Scale Estimates

**Supported Load**:
- **Request rate**: 10,000 req/sec across all backends
- **Backends**: 100 concurrent backends
- **Window retention**: 24-hour rolling window
- **Reconciliation interval**: 10-30 seconds (configurable)

---

## 9. Testing Strategy

### Test Coverage

**Unit Tests** (`src/agent/quality.rs`):
- `recompute_all()` with various time windows
- Edge cases: empty history, all failures, clock skew

**Property-Based Tests** (proptest):
- Verify error_rate_1h ∈ [0.0, 1.0]
- Verify success_rate_24h + error_rate_24h ≈ 1.0 (accounting for timeouts)

**Integration Tests**:
- Mock backends with controlled error rates
- Verify QualityReconciler excludes degraded backends
- Verify SchedulerReconciler penalizes high-TTFT backends

**Contract Tests**:
- Prometheus `/metrics` endpoint returns valid format
- `/v1/stats` JSON schema includes quality metrics

---

## 10. Open Questions (Resolved)

All critical questions from the spec have been resolved:

✅ **Insufficient data handling**: Use safe defaults (neutral scores)  
✅ **All backends degraded**: Include all with penalties (never fail all)  
✅ **Initial window (no metrics)**: Neutral scores until data collected  
✅ **Request history unavailable**: Log warning, use last known metrics  
✅ **Reconciliation loop crash**: Automatically restarted by Tokio runtime  
✅ **Clock skew**: Use monotonic time (Instant) for relative measurements

---

## Implementation Priorities

### Phase 0: Foundation (Complete via Research)
- ✅ Data structure decisions
- ✅ Metric patterns defined
- ✅ Concurrency model established

### Phase 1: Core Implementation (Next)
- Data model specification (`data-model.md`)
- API contracts for `/v1/stats` endpoint (`contracts/`)
- Developer quickstart guide (`quickstart.md`)

### Phase 2: Task Breakdown (Separate Command)
- Implemented via `/speckit.tasks` command
- Not part of `/speckit.plan` scope

---

## References

- **Rust VecDeque**: https://doc.rust-lang.org/std/collections/struct.VecDeque.html
- **DashMap**: https://docs.rs/dashmap/latest/dashmap/
- **Prometheus Best Practices**: https://prometheus.io/docs/practices/naming/
- **Tokio Interval**: https://docs.rs/tokio/latest/tokio/time/struct.Interval.html
- **Nexus Constitution**: `.specify/memory/constitution.md`
- **Existing Implementation**: `src/agent/quality.rs`, `src/routing/reconciler/quality.rs`
