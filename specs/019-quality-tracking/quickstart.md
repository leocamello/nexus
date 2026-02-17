# Developer Quickstart: Quality Tracking & Backend Profiling

**Feature**: F16 Quality Tracking & Backend Profiling  
**Audience**: Developers implementing or modifying quality tracking  
**Prerequisites**: Familiarity with Rust, Tokio, and Nexus architecture

---

## Overview

This guide helps you understand, modify, and extend Nexus's quality tracking system. You'll learn how to:
- Trace request outcomes through the quality tracking pipeline
- Add new quality metrics
- Tune quality thresholds for different deployment scenarios
- Debug quality-related routing decisions

---

## Architecture Quick Reference

```
Request → Router → Reconciler Pipeline → Backend Selection
                          ↓
                   QualityReconciler
                   (excludes degraded)
                          ↓
                   SchedulerReconciler
                   (penalizes slow)
                          ↓
Backend Response → record_outcome() → QualityMetricsStore
                          ↓
            quality_reconciliation_loop (30s)
                          ↓
            recompute_all() → Prometheus Gauges
```

---

## Key Files

| Path | Purpose |
|------|---------|
| `src/agent/quality.rs` | QualityMetricsStore, RequestOutcome, recomputation logic |
| `src/config/quality.rs` | QualityConfig (thresholds, intervals) |
| `src/routing/reconciler/quality.rs` | QualityReconciler (exclusion logic) |
| `src/routing/reconciler/scheduler.rs` | SchedulerReconciler (TTFT penalties) |
| `src/metrics/handler.rs` | /v1/stats endpoint with quality metrics |
| `src/metrics/mod.rs` | Prometheus metrics export |

---

## Common Tasks

### 1. Adding a New Quality Metric

**Scenario**: You want to track "request queueing time" as a quality metric.

**Step 1: Extend AgentQualityMetrics**
```rust
// src/agent/quality.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentQualityMetrics {
    pub error_rate_1h: f32,
    pub avg_ttft_ms: u32,
    pub success_rate_24h: f32,
    pub last_failure_ts: Option<Instant>,
    pub request_count_1h: u32,
    
    // NEW: Average queue time in milliseconds
    pub avg_queue_time_ms: u32,
}
```

**Step 2: Extend RequestOutcome**
```rust
// src/agent/quality.rs
struct RequestOutcome {
    timestamp: Instant,
    success: bool,
    ttft_ms: u32,
    
    // NEW: Queue time in milliseconds
    queue_time_ms: u32,
}
```

**Step 3: Update recompute logic**
```rust
// src/agent/quality.rs in compute_metrics()
let avg_queue_time_ms = if count_1h > 0 {
    (queue_time_sum_1h / count_1h as u64) as u32
} else {
    0
};

AgentQualityMetrics {
    // ... existing fields
    avg_queue_time_ms,
}
```

**Step 4: Export to Prometheus**
```rust
// In quality_reconciliation_loop()
metrics::gauge!("nexus_agent_queue_time_seconds", "agent_id" => &agent_id)
    .set(metrics.avg_queue_time_ms as f64 / 1000.0);
```

**Step 5: Add to /v1/stats JSON**
```rust
// src/metrics/handler.rs
#[derive(Serialize)]
struct QualityStats {
    // ... existing fields
    avg_queue_time_ms: Option<u32>,
}
```

---

### 2. Adjusting Quality Thresholds

**Scenario**: Default 50% error rate threshold is too permissive for production.

**Option A: TOML Configuration** (recommended)
```toml
# nexus.toml
[quality]
error_rate_threshold = 0.15        # Exclude backends with >15% error rate
ttft_penalty_threshold_ms = 2000   # Penalize TTFT > 2 seconds
metrics_interval_seconds = 15      # Refresh metrics every 15 seconds
```

**Option B: Environment Variables**
```bash
export NEXUS_QUALITY__ERROR_RATE_THRESHOLD=0.15
export NEXUS_QUALITY__TTFT_PENALTY_THRESHOLD_MS=2000
export NEXUS_QUALITY__METRICS_INTERVAL_SECONDS=15
```

**Option C: CLI Arguments**
```bash
nexus serve \
  --quality-error-rate-threshold 0.15 \
  --quality-ttft-penalty-threshold-ms 2000
```

---

### 3. Debugging Quality-Based Routing

**Enable Debug Logging**
```bash
RUST_LOG=nexus::routing::reconciler=debug nexus serve
```

**Trace a Request**
```rust
// In QualityReconciler::run()
tracing::debug!(
    agent_id = %agent.id,
    error_rate_1h = metrics.error_rate_1h,
    threshold = config.error_rate_threshold,
    excluded = metrics.error_rate_1h >= config.error_rate_threshold,
    "Quality reconciler decision"
);
```

**Check Quality Store State**
```bash
curl http://localhost:8080/v1/stats | jq '.backends[] | select(.id=="your_backend") | .quality'
```

**Inspect Prometheus Metrics**
```bash
curl http://localhost:8080/metrics | grep nexus_agent_error_rate
```

---

### 4. Testing Quality Tracking

**Unit Test: Metric Computation**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_rate_computation() {
        let store = QualityMetricsStore::new();
        let agent_id = "test_agent";
        
        // Record 10 requests: 3 failures, 7 successes
        for i in 0..10 {
            store.record_outcome(agent_id, RequestOutcome {
                timestamp: Instant::now(),
                success: i < 7,
                ttft_ms: 100,
            });
        }
        
        store.recompute_all();
        let metrics = store.get_metrics(agent_id).unwrap();
        
        assert_eq!(metrics.request_count_1h, 10);
        assert!((metrics.error_rate_1h - 0.3).abs() < 0.01);  // 3/10 = 0.3
        assert_eq!(metrics.avg_ttft_ms, 100);
    }
}
```

**Integration Test: Reconciler Exclusion**
```rust
#[tokio::test]
async fn test_quality_reconciler_excludes_degraded_backend() {
    let config = QualityConfig {
        error_rate_threshold: 0.2,
        ..Default::default()
    };
    let store = Arc::new(QualityMetricsStore::new());
    
    // Simulate 50% error rate
    for i in 0..100 {
        store.record_outcome("bad_backend", RequestOutcome {
            timestamp: Instant::now(),
            success: i % 2 == 0,
            ttft_ms: 100,
        });
    }
    store.recompute_all();
    
    // Run reconciler
    let mut intent = RoutingIntent::new();
    intent.add_candidate(Agent { id: "bad_backend".into(), .. });
    
    let reconciler = QualityReconciler::new(store, config);
    reconciler.run(&mut intent).await.unwrap();
    
    assert!(intent.is_excluded("bad_backend"));
}
```

**Property-Based Test: Metrics Always Bounded**
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_error_rate_always_bounded(
        successes in 0u32..1000,
        failures in 0u32..1000
    ) {
        let store = QualityMetricsStore::new();
        let agent_id = "test";
        
        // Record random success/failure mix
        for _ in 0..successes {
            store.record_outcome(agent_id, RequestOutcome {
                timestamp: Instant::now(),
                success: true,
                ttft_ms: 100,
            });
        }
        for _ in 0..failures {
            store.record_outcome(agent_id, RequestOutcome {
                timestamp: Instant::now(),
                success: false,
                ttft_ms: 100,
            });
        }
        
        store.recompute_all();
        let metrics = store.get_metrics(agent_id).unwrap();
        
        // Invariant: error_rate_1h ∈ [0.0, 1.0]
        assert!(metrics.error_rate_1h >= 0.0);
        assert!(metrics.error_rate_1h <= 1.0);
    }
}
```

---

### 5. Monitoring Quality Tracking Performance

**Reconciliation Loop Overhead**
```promql
# 95th percentile recompute duration
histogram_quantile(0.95, rate(nexus_quality_recompute_duration_seconds_bucket[5m]))

# Should be < 1ms for 100 backends
```

**QualityReconciler Latency**
```promql
# 95th percentile reconciler duration
histogram_quantile(0.95, 
  rate(nexus_reconciler_duration_seconds_bucket{reconciler="quality"}[5m])
)

# Should be < 0.1ms (target: <50μs)
```

**Memory Usage**
```bash
# Check VecDeque growth
ps aux | grep nexus | awk '{print $6}'  # RSS in KB

# Expected: ~5MB baseline + ~500KB per backend
```

---

## Advanced Topics

### Custom Reconciliation Interval

**Problem**: You need faster metric updates for real-time dashboards.

**Solution**: Reduce `metrics_interval_seconds` but watch CPU usage.

```toml
[quality]
metrics_interval_seconds = 10  # Refresh every 10 seconds
```

**Tradeoff**:
- **Faster**: Metrics lag reduced from 30s to 10s
- **Cost**: 3x more CPU cycles for recomputation
- **Recommendation**: Profile with `cargo flamegraph` before deploying

---

### Weighted Quality Scores

**Problem**: You want to prioritize backends with good quality, not just exclude bad ones.

**Solution**: Extend SchedulerReconciler to boost scores for high-quality backends.

```rust
// In SchedulerReconciler::run()
let quality_boost = if metrics.error_rate_1h < 0.05 {
    0.2  // +0.2 bonus for <5% error rate
} else {
    0.0
};

let ttft_boost = if metrics.avg_ttft_ms < 1000 {
    0.1  // +0.1 bonus for <1s TTFT
} else {
    0.0
};

score += quality_boost + ttft_boost;
```

---

### Graceful Degradation (All Backends Unhealthy)

**Current Behavior**: If all backends exceed error threshold, include all with penalties.

**Customization**: Add fallback routing to known-stable cloud backend.

```rust
// In QualityReconciler::run()
let all_degraded = intent.candidates.iter()
    .all(|a| {
        store.get_metrics(&a.id)
            .map_or(false, |m| m.error_rate_1h >= threshold)
    });

if all_degraded {
    tracing::warn!("All backends degraded, enabling fallback mode");
    // Option 1: Include all with warnings
    // Option 2: Force route to specific backend
    intent.set_fallback_backend("stable_cloud_backend");
}
```

---

## Troubleshooting

### Metrics Not Updating

**Symptom**: Prometheus gauges stuck at zero or stale values.

**Check**:
1. Is `quality_reconciliation_loop` running?
   ```bash
   RUST_LOG=nexus::agent::quality=debug nexus serve
   # Look for "Recomputed quality metrics for N agents"
   ```

2. Are requests being completed?
   ```bash
   curl http://localhost:8080/v1/stats | jq '.total_requests'
   ```

3. Is the store recording outcomes?
   ```rust
   // Add debug log in record_outcome()
   tracing::debug!(agent_id, success = outcome.success, "Recorded outcome");
   ```

---

### High Error Rates Not Excluding Backends

**Symptom**: Backend with 80% error rate still receiving requests.

**Check**:
1. Verify threshold configuration:
   ```bash
   curl http://localhost:8080/v1/stats | jq '.config.quality'
   ```

2. Check if QualityReconciler is running:
   ```bash
   RUST_LOG=nexus::routing::reconciler::quality=debug nexus serve
   ```

3. Confirm backend is registered with correct agent_id:
   ```bash
   curl http://localhost:8080/v1/models
   ```

---

### Memory Growth Over Time

**Symptom**: RSS increases linearly over 24 hours.

**Check**:
1. Is pruning working?
   ```rust
   // Add debug log in recompute_all()
   tracing::debug!(agent_id, before = outcomes.len(), after = new_len, "Pruned outcomes");
   ```

2. Check for zombie agents (backends removed but metrics retained):
   ```rust
   // In Registry::remove_backend()
   quality_store.remove_agent(&backend_id);  // Ensure this is called
   ```

---

## Performance Tuning

### Reduce Reconciliation Overhead

**Problem**: `recompute_all()` takes >5ms with 100 backends.

**Solution 1: Reduce retention window**
```rust
// Keep only 1-hour history if 24-hour metrics aren't critical
let one_hour = Duration::from_secs(3600);
while let Some(front) = outcomes.front() {
    if now.duration_since(front.timestamp) > one_hour {
        outcomes.pop_front();
    } else {
        break;
    }
}
```

**Solution 2: Parallel recomputation**
```rust
use rayon::prelude::*;

pub fn recompute_all(&self) {
    self.outcomes.par_iter().for_each(|entry| {
        // Compute metrics in parallel
        let metrics = compute_metrics(&entry.value().read().unwrap());
        self.metrics.insert(entry.key().clone(), metrics);
    });
}
```

---

### Reduce Prometheus Cardinality

**Problem**: 1000 backends × 20 models = 20K time series.

**Solution: Aggregate at backend level**
```rust
// Instead of per-agent metrics, expose per-backend rollup
metrics::gauge!("nexus_backend_error_rate", "backend_id" => &backend_id)
    .set(avg_error_rate_across_models);
```

---

## References

- **Data Model**: `specs/019-quality-tracking/data-model.md`
- **API Contracts**: `specs/019-quality-tracking/contracts/`
- **Research**: `specs/019-quality-tracking/research.md`
- **Constitution**: `.specify/memory/constitution.md`
- **Rust VecDeque**: https://doc.rust-lang.org/std/collections/struct.VecDeque.html
- **DashMap**: https://docs.rs/dashmap/latest/dashmap/
- **Tokio Interval**: https://docs.rs/tokio/latest/tokio/time/struct.Interval.html

---

## Next Steps

1. **Read the spec**: `specs/019-quality-tracking/spec.md`
2. **Review data model**: `specs/019-quality-tracking/data-model.md`
3. **Run tests**: `cargo test --package nexus --lib agent::quality::tests`
4. **Profile performance**: `cargo bench -- routing`
5. **Check Prometheus**: `curl http://localhost:8080/metrics | grep nexus_agent`

For questions or suggestions, see `.github/CONTRIBUTING.md`.
