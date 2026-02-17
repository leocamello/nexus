# Data Model: Quality Tracking & Backend Profiling

**Feature**: F16 Quality Tracking & Backend Profiling  
**Date**: 2025-01-24  
**Status**: Complete

## Overview

This document defines the data structures, state management, and entity relationships for the Quality Tracking feature. All entities are in-memory only (no persistence required per constitution).

---

## Core Entities

### 1. AgentQualityMetrics

**Purpose**: Represents the computed quality profile for a single backend+model agent.

**Location**: `src/agent/quality.rs`

**Structure**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentQualityMetrics {
    /// Error rate over the last 1 hour (0.0 = no errors, 1.0 = all failed)
    pub error_rate_1h: f32,
    
    /// Average time-to-first-token in milliseconds
    pub avg_ttft_ms: u32,
    
    /// Success rate over the last 24 hours (0.0 = all failed, 1.0 = all succeeded)
    pub success_rate_24h: f32,
    
    /// Timestamp of the most recent request failure (None if no failures recorded)
    pub last_failure_ts: Option<Instant>,
    
    /// Total number of requests processed in the last 1 hour
    pub request_count_1h: u32,
}
```

**Invariants**:
- `error_rate_1h` ∈ [0.0, 1.0]
- `success_rate_24h` ∈ [0.0, 1.0]
- `avg_ttft_ms` ≥ 0
- `request_count_1h` ≥ 0

**Default Values** (when no history exists):
```rust
impl Default for AgentQualityMetrics {
    fn default() -> Self {
        Self {
            error_rate_1h: 0.0,        // Optimistic default (assume healthy)
            avg_ttft_ms: 0,             // No TTFT penalty
            success_rate_24h: 1.0,      // Optimistic default
            last_failure_ts: None,      // No failures yet
            request_count_1h: 0,        // No requests yet
        }
    }
}
```

**State Transitions**:
- **New backend discovered** → Default metrics (neutral score)
- **Request completed** → Raw outcome recorded to store
- **Reconciliation interval** → Metrics recomputed from rolling window
- **Backend removed** → Metrics deleted from store

---

### 2. RequestOutcome (Internal)

**Purpose**: Individual request record used for computing rolling window statistics.

**Location**: `src/agent/quality.rs` (private to quality module)

**Structure**:
```rust
struct RequestOutcome {
    /// Monotonic timestamp (not wall-clock time)
    timestamp: Instant,
    
    /// True if request succeeded (2xx status), false if failed (4xx/5xx/timeout)
    success: bool,
    
    /// Time-to-first-token in milliseconds (0 if request failed before streaming)
    ttft_ms: u32,
}
```

**Retention Policy**:
- Stored in `VecDeque` per agent
- Pruned during `recompute_all()` when `timestamp` exceeds 24 hours ago
- Maximum per agent: ~24,000 entries (1 req/sec × 24h × 60min × 60sec)

**Memory Budget**: ~20 bytes per outcome × 24K = ~480KB per backend

---

### 3. QualityMetricsStore

**Purpose**: Central thread-safe storage for all agent quality data. Provides concurrent read/write access.

**Location**: `src/agent/quality.rs`

**Structure**:
```rust
pub struct QualityMetricsStore {
    /// Raw request outcomes per agent (keyed by agent_id)
    outcomes: DashMap<String, RwLock<VecDeque<RequestOutcome>>>,
    
    /// Computed quality metrics per agent (keyed by agent_id)
    metrics: DashMap<String, AgentQualityMetrics>,
}
```

**Key Methods**:
```rust
impl QualityMetricsStore {
    /// Creates new empty store
    pub fn new() -> Self;
    
    /// Records a completed request outcome
    pub fn record_outcome(&self, agent_id: &str, outcome: RequestOutcome);
    
    /// Recomputes all metrics from raw outcomes (called by reconciliation loop)
    pub fn recompute_all(&self);
    
    /// Retrieves current quality metrics for an agent
    pub fn get_metrics(&self, agent_id: &str) -> Option<AgentQualityMetrics>;
    
    /// Retrieves all metrics (for Prometheus export and /v1/stats)
    pub fn get_all_metrics(&self) -> Vec<(String, AgentQualityMetrics)>;
    
    /// Removes all data for an agent (when backend is deregistered)
    pub fn remove_agent(&self, agent_id: &str);
}
```

**Concurrency Model**:
- **Record outcome**: Acquires write lock on specific agent's VecDeque (O(1) DashMap lookup)
- **Recompute all**: Iterates all agents, acquires write lock per agent sequentially
- **Get metrics**: Reads from computed metrics DashMap (lock-free read)

**Thread Safety**:
- `DashMap`: Lock-free concurrent map (per-shard locking)
- `RwLock<VecDeque>`: Multiple readers OR single writer per agent

---

### 4. QualityConfig

**Purpose**: Configuration parameters for quality tracking behavior.

**Location**: `src/config/quality.rs`

**Structure**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityConfig {
    /// Error rate threshold for excluding agents from routing (0.0-1.0)
    /// Default: 0.5 (50% error rate)
    #[serde(default = "default_error_rate_threshold")]
    pub error_rate_threshold: f32,
    
    /// TTFT threshold in milliseconds for applying routing penalty
    /// Default: 3000 (3 seconds)
    #[serde(default = "default_ttft_penalty_threshold_ms")]
    pub ttft_penalty_threshold_ms: u32,
    
    /// How often to recompute quality metrics (in seconds)
    /// Default: 30 seconds
    #[serde(default = "default_metrics_interval_seconds")]
    pub metrics_interval_seconds: u64,
}

fn default_error_rate_threshold() -> f32 { 0.5 }
fn default_ttft_penalty_threshold_ms() -> u32 { 3000 }
fn default_metrics_interval_seconds() -> u64 { 30 }
```

**TOML Configuration**:
```toml
[quality]
error_rate_threshold = 0.5
ttft_penalty_threshold_ms = 3000
metrics_interval_seconds = 30
```

**Validation**:
- `error_rate_threshold` must be ∈ (0.0, 1.0] (0.0 would exclude all backends)
- `ttft_penalty_threshold_ms` must be > 0
- `metrics_interval_seconds` must be ≥ 10 (avoid excessive CPU usage)

---

## Entity Relationships

### Lifecycle Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                     Backend Discovery                        │
│                     (Registry.add_backend)                   │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
            ┌────────────────────────┐
            │ QualityMetricsStore    │
            │ (empty metrics cache)  │
            └────────────────────────┘
                         │
        ┌────────────────┼────────────────┐
        │                │                │
        ▼                ▼                ▼
┌─────────────┐  ┌──────────────┐  ┌────────────┐
│  Request    │  │ Reconcile    │  │  Routing   │
│ Completion  │  │    Loop      │  │  Decision  │
└─────────────┘  └──────────────┘  └────────────┘
        │                │                │
        │                │                │
        ▼                ▼                │
record_outcome() → recompute_all()        │
        │                │                │
        │                ▼                │
        │      AgentQualityMetrics        │
        │      (computed & cached)        │
        │                │                │
        │                └────────────────┼──────► QualityReconciler
        │                                 │         (excludes degraded)
        └─────────────────────────────────┼──────► SchedulerReconciler
                                          │         (penalizes slow)
                                          │
                                          ▼
                                     Route Request
```

### Data Flow

**1. Request Completion → Quality Store**
```rust
// In router completion handler
let outcome = RequestOutcome {
    timestamp: Instant::now(),
    success: response.status().is_success(),
    ttft_ms: ttft_duration.as_millis() as u32,
};
quality_store.record_outcome(&agent_id, outcome);
```

**2. Background Reconciliation → Prometheus**
```rust
// In quality_reconciliation_loop (runs every 30s)
quality_store.recompute_all();

for (agent_id, metrics) in quality_store.get_all_metrics() {
    metrics::gauge!("nexus_agent_error_rate", "agent_id" => &agent_id)
        .set(metrics.error_rate_1h as f64);
    metrics::gauge!("nexus_agent_success_rate_24h", "agent_id" => &agent_id)
        .set(metrics.success_rate_24h as f64);
    metrics::histogram!("nexus_agent_ttft_seconds", "agent_id" => &agent_id)
        .record(metrics.avg_ttft_ms as f64 / 1000.0);
}
```

**3. Routing Decision → Quality Metrics**
```rust
// In QualityReconciler::run()
if let Some(metrics) = quality_store.get_metrics(&agent.id) {
    if metrics.error_rate_1h >= config.error_rate_threshold {
        intent.exclude_agent(&agent.id, ExclusionReason::HighErrorRate);
    }
}

// In SchedulerReconciler::run() (scoring phase)
let ttft_penalty = if metrics.avg_ttft_ms > config.ttft_penalty_threshold_ms {
    let excess_ms = metrics.avg_ttft_ms - config.ttft_penalty_threshold_ms;
    -0.01 * (excess_ms as f32 / 1000.0)  // -0.01 per second over threshold
} else {
    0.0
};
score += ttft_penalty;
```

---

## State Management

### In-Memory Only (Constitution Principle VII)

**Rationale**: Nexus is stateless by design. Quality metrics are operational state, not user data.

**Implications**:
- Metrics reset on system restart → Acceptable (backends start with neutral scores)
- No database required → Simplifies deployment
- Prometheus provides long-term persistence → Operators use Prometheus for trends

**Memory Budget** (for 10 backends × 24h history):
- Raw outcomes: 10 × 480KB = 4.8MB
- Computed metrics: 10 × 64 bytes = 640 bytes
- **Total**: ~5MB (within 50MB baseline target)

### Cleanup Policy

**Agent Removal**:
```rust
// When backend is deregistered
quality_store.remove_agent(&agent_id);
```

**Automatic Pruning**:
- Outcomes older than 24 hours pruned during `recompute_all()`
- No explicit cleanup task required

---

## API Exposure

### Prometheus Metrics (Read-Only)

**Endpoint**: `GET /metrics`

**Format**: Prometheus text format

**Example**:
```
# HELP nexus_agent_error_rate Error rate over the last 1 hour
# TYPE nexus_agent_error_rate gauge
nexus_agent_error_rate{agent_id="backend1_llama3"} 0.15

# HELP nexus_agent_success_rate_24h Success rate over the last 24 hours
# TYPE nexus_agent_success_rate_24h gauge
nexus_agent_success_rate_24h{agent_id="backend1_llama3"} 0.98

# HELP nexus_agent_ttft_seconds Time-to-first-token histogram
# TYPE nexus_agent_ttft_seconds histogram
nexus_agent_ttft_seconds_bucket{agent_id="backend1_llama3",le="0.05"} 10
nexus_agent_ttft_seconds_bucket{agent_id="backend1_llama3",le="0.1"} 50
nexus_agent_ttft_seconds_bucket{agent_id="backend1_llama3",le="0.5"} 200
nexus_agent_ttft_seconds_bucket{agent_id="backend1_llama3",le="+Inf"} 250
nexus_agent_ttft_seconds_sum{agent_id="backend1_llama3"} 125.5
nexus_agent_ttft_seconds_count{agent_id="backend1_llama3"} 250
```

### JSON Stats API (Read-Only)

**Endpoint**: `GET /v1/stats`

**Format**: JSON (extends existing stats response)

**Schema**: See `contracts/stats-response.json`

---

## Validation Rules

### Input Validation (on `record_outcome`)

```rust
pub fn record_outcome(&self, agent_id: &str, outcome: RequestOutcome) {
    // Validate agent_id is not empty
    if agent_id.is_empty() {
        tracing::warn!("Attempted to record outcome with empty agent_id");
        return;
    }
    
    // Validate timestamp is not in the future (clock skew)
    let now = Instant::now();
    if outcome.timestamp > now {
        tracing::warn!(
            agent_id = agent_id,
            "Timestamp is in the future (clock skew?), using current time"
        );
        let corrected = RequestOutcome {
            timestamp: now,
            ..outcome
        };
        self.record_outcome_unchecked(agent_id, corrected);
        return;
    }
    
    // Validate TTFT is reasonable (< 5 minutes)
    if outcome.ttft_ms > 300_000 {
        tracing::warn!(
            agent_id = agent_id,
            ttft_ms = outcome.ttft_ms,
            "TTFT exceeds 5 minutes, capping at 300000ms"
        );
        let capped = RequestOutcome {
            ttft_ms: 300_000,
            ..outcome
        };
        self.record_outcome_unchecked(agent_id, capped);
        return;
    }
    
    self.record_outcome_unchecked(agent_id, outcome);
}
```

### Metric Computation Validation

```rust
fn compute_metrics(outcomes: &VecDeque<RequestOutcome>) -> AgentQualityMetrics {
    let now = Instant::now();
    let one_hour = Duration::from_secs(3600);
    let twenty_four_hours = Duration::from_secs(86400);
    
    // Compute 1-hour window metrics
    let (count_1h, errors_1h, ttft_sum_1h) = outcomes
        .iter()
        .filter(|o| now.duration_since(o.timestamp) <= one_hour)
        .fold((0u32, 0u32, 0u64), |(cnt, err, ttft), o| {
            (
                cnt + 1,
                err + (!o.success as u32),
                ttft + o.ttft_ms as u64,
            )
        });
    
    let error_rate_1h = if count_1h > 0 {
        (errors_1h as f32 / count_1h as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };
    
    let avg_ttft_ms = if count_1h > 0 {
        (ttft_sum_1h / count_1h as u64) as u32
    } else {
        0
    };
    
    // Compute 24-hour window metrics
    let (count_24h, successes_24h) = outcomes
        .iter()
        .filter(|o| now.duration_since(o.timestamp) <= twenty_four_hours)
        .fold((0u32, 0u32), |(cnt, succ), o| {
            (cnt + 1, succ + (o.success as u32))
        });
    
    let success_rate_24h = if count_24h > 0 {
        (successes_24h as f32 / count_24h as f32).clamp(0.0, 1.0)
    } else {
        1.0  // Optimistic default
    };
    
    // Find last failure timestamp
    let last_failure_ts = outcomes
        .iter()
        .filter(|o| !o.success)
        .map(|o| o.timestamp)
        .max();
    
    AgentQualityMetrics {
        error_rate_1h,
        avg_ttft_ms,
        success_rate_24h,
        last_failure_ts,
        request_count_1h: count_1h,
    }
}
```

---

## Edge Cases

### 1. Insufficient Data (< 10 requests in 1 hour)

**Behavior**: Compute metrics from available data, use defaults if empty.

**Rationale**: Avoids excluding new or low-traffic backends unnecessarily.

### 2. All Backends Degraded (error_rate_1h > threshold)

**Behavior**: QualityReconciler includes all backends with penalty scores.

**Implementation**:
```rust
// In QualityReconciler::run()
let degraded_count = intent.candidates.iter()
    .filter(|a| quality_store.get_metrics(&a.id)
        .map_or(false, |m| m.error_rate_1h >= config.error_rate_threshold))
    .count();

if degraded_count == intent.candidates.len() {
    // All backends degraded: include all with warnings
    tracing::warn!("All backends degraded, including all with penalties");
} else {
    // Exclude only degraded backends
    for agent in intent.candidates.iter() {
        if let Some(m) = quality_store.get_metrics(&agent.id) {
            if m.error_rate_1h >= config.error_rate_threshold {
                intent.exclude_agent(&agent.id, ExclusionReason::HighErrorRate);
            }
        }
    }
}
```

### 3. Clock Skew / Time Synchronization

**Mitigation**: Use `Instant` (monotonic) for relative time measurements, not `SystemTime`.

**Behavior**: Timestamps in the future are corrected to current time with warning.

### 4. Reconciliation Loop Failure

**Behavior**: Last computed metrics retained; routing continues with stale data.

**Recovery**: Tokio runtime automatically restarts background tasks on panic.

---

## Testing Considerations

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_metrics_optimistic() {
        let metrics = AgentQualityMetrics::default();
        assert_eq!(metrics.error_rate_1h, 0.0);
        assert_eq!(metrics.success_rate_24h, 1.0);
    }
    
    #[test]
    fn test_empty_window_returns_defaults() {
        let store = QualityMetricsStore::new();
        let metrics = store.get_metrics("nonexistent");
        assert!(metrics.is_none());
    }
    
    #[test]
    fn test_error_rate_clamps_to_one() {
        // All requests failed in 1-hour window
        let outcomes = vec![
            RequestOutcome { success: false, ... },
            RequestOutcome { success: false, ... },
        ];
        let metrics = compute_metrics(&outcomes);
        assert_eq!(metrics.error_rate_1h, 1.0);
    }
}
```

### Property-Based Tests (proptest)

```rust
proptest! {
    #[test]
    fn test_error_rate_bounded(
        successes in 0u32..1000,
        failures in 0u32..1000
    ) {
        let outcomes = generate_outcomes(successes, failures);
        let metrics = compute_metrics(&outcomes);
        assert!(metrics.error_rate_1h >= 0.0);
        assert!(metrics.error_rate_1h <= 1.0);
    }
}
```

---

## Performance Characteristics

### Time Complexity

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| `record_outcome` | O(1) | DashMap lookup + VecDeque push_back |
| `get_metrics` | O(1) | DashMap lookup (cached metrics) |
| `recompute_all` | O(n × m) | n = backends, m = outcomes per backend |

### Space Complexity

| Structure | Size per Backend | Total (10 Backends) |
|-----------|------------------|---------------------|
| Raw outcomes (24h) | 480KB | 4.8MB |
| Computed metrics | 64 bytes | 640 bytes |
| DashMap overhead | ~1KB | 10KB |

**Total**: ~5MB for 10 backends (acceptable within 50MB target)

---

## Dependencies

### Internal Dependencies
- `src/routing/reconciler/quality.rs` → Reads from QualityMetricsStore
- `src/routing/reconciler/scheduler.rs` → Uses avg_ttft_ms for scoring
- `src/metrics/handler.rs` → Exposes metrics via /v1/stats
- `src/config/quality.rs` → Provides configuration parameters

### External Dependencies (Cargo.toml)
- `dashmap = "6"` → Concurrent map
- `tokio = { version = "1", features = ["full"] }` → Async runtime
- `serde = { version = "1", features = ["derive"] }` → Serialization
- `metrics = "0.24"` → Prometheus metrics facade
- `tracing = "0.1"` → Structured logging

---

## Summary

This data model provides:
- **Thread-safe** concurrent access to quality metrics
- **Memory-efficient** rolling window implementation
- **Simple** entity relationships with clear ownership
- **Testable** validation rules and edge case handling
- **Scalable** to 100+ backends without performance degradation

All design decisions align with Nexus constitution principles: stateless, in-memory, no external dependencies, and performance-first.
