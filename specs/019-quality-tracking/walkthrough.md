# F16: Quality Tracking & Backend Profiling — Code Walkthrough

**Feature**: Quality Tracking & Backend Profiling (F16)  
**Audience**: Junior developers joining the project  
**Last Updated**: 2025-07-25

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: agent/mod.rs — The Quality Report Card](#file-1-agentmodrs--the-quality-report-card)
4. [File 2: config/quality.rs — The Tuning Knobs](#file-2-configqualityrs--the-tuning-knobs)
5. [File 3: agent/quality.rs — The Metrics Engine](#file-3-agentqualityrs--the-metrics-engine)
6. [File 4: routing/reconciler/quality.rs — The Bouncer](#file-4-routingreconcilerqualityrs--the-bouncer)
7. [File 5: routing/reconciler/scheduler.rs — The Speed Penalty](#file-5-routingreconcilerschedulerrs--the-speed-penalty)
8. [File 6: metrics/handler.rs — The Stats Window](#file-6-metricshandlerrs--the-stats-window)
9. [Data Flow: End to End](#data-flow-end-to-end)
10. [Understanding the Tests](#understanding-the-tests)
11. [Configuration Options](#configuration-options)
12. [Key Rust Concepts](#key-rust-concepts)
13. [Common Patterns in This Codebase](#common-patterns-in-this-codebase)
14. [Next Steps](#next-steps)

---

## The Big Picture

Imagine you run a **taxi dispatch service** with several drivers. Some drivers are reliable — they always pick up passengers on time. Others have been breaking down, arriving late, or canceling rides. When a new passenger calls, you don't want to send the driver who's been late 8 out of 10 times today. You want to check each driver's recent track record and send the most reliable one.

That's what **Quality Tracking** does for LLM backends. It monitors every request outcome (success or failure) and measures how fast each backend responds (time to first token). This data feeds into the routing pipeline to automatically avoid degraded backends and prefer faster ones.

### What Problem Does This Solve?

Without F16, Nexus routes requests purely based on static factors like priority, pending load, and latency EMA. If a backend starts returning errors 80% of the time, Nexus would keep sending it requests until the health check marks it fully unhealthy. That means many requests fail unnecessarily.

With F16, Nexus:
- **Tracks error rates** per backend over 1-hour rolling windows
- **Excludes degraded backends** before they're fully unhealthy (early detection)
- **Penalizes slow backends** that have high time-to-first-token (TTFT)
- **Exposes quality metrics** via Prometheus and `/v1/stats` for operational visibility
- **Recovers automatically** — when a backend improves, it's re-included

### How F16 Fits Into Nexus

```
┌────────────────────────────────────────────────────────────────────────┐
│                              Nexus                                     │
│                                                                        │
│  Client Request                                                        │
│    │  POST /v1/chat/completions                                        │
│    ▼                                                                   │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  RECONCILER PIPELINE (runs for every request, <1ms total)        │  │
│  │                                                                  │  │
│  │  ① RequestAnalyzer   → Extract model, resolve aliases            │  │
│  │  ② PrivacyReconciler → Filter by privacy zone (F13)              │  │
│  │  ③ BudgetReconciler  → Check budget limits (F14)                 │  │
│  │  ④ TierReconciler    → Filter by capability tier (F13)           │  │
│  │  ⑤ QualityReconciler → Exclude high-error backends      ◄── F16 │  │
│  │  ⑥ SchedulerReconciler → Score, apply TTFT penalty,     ◄── F16 │  │
│  │                          select best backend                     │  │
│  │                                                                  │  │
│  │  Result: Route | Queue | Reject                                  │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│    │                                                                   │
│    ▼                                                                   │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  Forward to Selected Backend → Get Response                      │  │
│  │                                                                  │  │
│  │  After response:                                        ◄── F16  │  │
│  │    record_outcome(agent_id, success, ttft_ms)                    │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                        │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │  BACKGROUND LOOP (every 30s)                            ◄── F16  │  │
│  │                                                                  │  │
│  │  quality_reconciliation_loop:                                    │  │
│  │    1. Recompute metrics from rolling window                      │  │
│  │    2. Update Prometheus gauges                                    │  │
│  └──────────────────────────────────────────────────────────────────┘  │
│                                                                        │
│  Data Flow:                                                            │
│    Request outcome → VecDeque (rolling window) → recompute_all()       │
│    → AgentQualityMetrics → QualityReconciler (exclusion)               │
│                          → SchedulerReconciler (TTFT penalty)          │
│                          → /v1/stats (JSON) + /metrics (Prometheus)    │
└────────────────────────────────────────────────────────────────────────┘
```

### Key Design Decisions

| Decision | Why |
|----------|-----|
| Periodic batch recomputation (30s) | Simpler than incremental updates; amortizes computation across many requests |
| DashMap for per-agent isolation | Each agent's data is independently lockable; no global contention |
| VecDeque for rolling windows | O(1) push/pop; natural FIFO for time-ordered data |
| Neutral scores for new backends | New backends get a fair chance — no penalty until data exists |
| Binary exclusion in QualityReconciler | Error rate above threshold = excluded. Simple, predictable, debuggable |
| Proportional TTFT penalty in Scheduler | Gradual degradation — slightly slow backends lose a little, very slow ones lose a lot |

---

## File Structure

```
src/
├── agent/
│   ├── mod.rs           ← F16: AgentQualityMetrics struct (lines 34-94)
│   └── quality.rs       ← F16: QualityMetricsStore, RequestOutcome, reconciliation loop
│                            (219 lines, 7 unit tests)
├── config/
│   └── quality.rs       ← F16: QualityConfig with thresholds and defaults
│                            (49 lines)
├── routing/
│   └── reconciler/
│       ├── quality.rs   ← F16: QualityReconciler (error rate exclusion)
│       │                   (246 lines, 8 unit tests)
│       └── scheduler.rs ← F16: TTFT penalty in apply_ttft_penalty()
│                            (801 lines, 3 TTFT-specific tests)
├── metrics/
│   ├── types.rs         ← F16: BackendStats quality fields (error_rate_1h, etc.)
│   └── handler.rs       ← F16: compute_backend_stats() quality integration
│                            (425 lines)
```

**F16 Contribution**: 1 new file (`agent/quality.rs`), 1 new reconciler (`reconciler/quality.rs`), 1 new config (`config/quality.rs`), 3 modified files. ~500 lines added, 18 unit tests.

---

## File 1: agent/mod.rs — The Quality Report Card

**Purpose**: Define the `AgentQualityMetrics` struct — the "report card" for each backend agent.  
**Lines**: 34-94 (within the agent module)  |  **Tests**: 6  |  **Status**: MODIFIED

### Why Does This Exist?

Every backend needs a quality summary that can be quickly read during routing decisions. `AgentQualityMetrics` is that summary — a small struct with five fields that capture everything the router needs to know about a backend's recent performance.

### The Struct

```rust
// src/agent/mod.rs

#[derive(Debug, Clone)]
pub struct AgentQualityMetrics {
    pub error_rate_1h: f32,                  // 0.0 = perfect, 1.0 = all errors
    pub avg_ttft_ms: u32,                    // Average time to first token (ms)
    pub success_rate_24h: f32,               // Long-term reliability
    pub last_failure_ts: Option<Instant>,     // When did it last fail?
    pub request_count_1h: u32,               // How many requests in the last hour
}
```

Each field answers one routing question:
- `error_rate_1h`: "Should we exclude this backend?" (QualityReconciler checks this)
- `avg_ttft_ms`: "Should we penalize this backend's score?" (SchedulerReconciler checks this)
- `success_rate_24h`: "What's the long-term reliability?" (shown in `/v1/stats`)
- `last_failure_ts`: "Has it ever failed?" (distinguishes new backends from recovered ones)
- `request_count_1h`: "Do we have enough data?" (backends with 0 requests get neutral scores)

### The Default: Innocent Until Proven Guilty

```rust
impl Default for AgentQualityMetrics {
    fn default() -> Self {
        Self {
            error_rate_1h: 0.0,        // No errors assumed
            avg_ttft_ms: 0,            // No data
            success_rate_24h: 1.0,     // Assume perfect until proven otherwise
            last_failure_ts: None,     // No failures
            request_count_1h: 0,       // No data yet
        }
    }
}
```

This default is critical: when a backend first appears (via mDNS discovery or config), it has no history. The safe default gives it a neutral quality score so it's included in routing and gets a chance to prove itself.

### The `is_healthy()` Method

```rust
pub fn is_healthy(&self) -> bool {
    self.error_rate_1h < 0.5
        && (self.request_count_1h > 0 || self.last_failure_ts.is_none())
}
```

A backend is healthy if its error rate is below 50% **and** it either has processed requests recently or has never failed. This prevents excluding a brand-new backend that has zero history.

---

## File 2: config/quality.rs — The Tuning Knobs

**Purpose**: Define configurable thresholds for quality tracking behavior.  
**Lines**: 49  |  **Tests**: 0 (struct with serde, tested via deserialization)  |  **Status**: NEW

### Why Does This Exist?

Different deployments have different tolerance for errors and latency. A hobby setup might accept 50% error rates before excluding a backend, while a production deployment might exclude at 10%. The `QualityConfig` lets operators tune these thresholds without changing code.

### The Struct

```rust
// src/config/quality.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QualityConfig {
    /// Error rate above which backends are excluded (default: 0.5 = 50%)
    pub error_rate_threshold: f32,

    /// TTFT above which backends get a scoring penalty (default: 3000ms)
    pub ttft_penalty_threshold_ms: u32,

    /// How often the background loop recomputes metrics (default: 30s)
    pub metrics_interval_seconds: u64,
}
```

The `#[serde(default)]` attribute means that if the operator omits the `[quality]` section entirely from their TOML config, all fields get their default values. Zero config by default.

### TOML Configuration Example

```toml
[quality]
error_rate_threshold = 0.5         # Exclude backends with >50% errors
ttft_penalty_threshold_ms = 3000   # Penalize backends slower than 3 seconds
metrics_interval_seconds = 30      # Recompute every 30 seconds
```

---

## File 3: agent/quality.rs — The Metrics Engine

**Purpose**: Core quality tracking logic — storing outcomes, computing rolling window metrics, and running the background reconciliation loop.  
**Lines**: 219  |  **Tests**: 7  |  **Status**: NEW

This is the heart of F16. It has three responsibilities:
1. **Store request outcomes** as they happen (called from the completions handler)
2. **Recompute aggregate metrics** from the rolling window (called periodically)
3. **Expose metrics** for routing decisions and observability

### RequestOutcome: The Raw Data

```rust
#[derive(Debug, Clone)]
pub struct RequestOutcome {
    pub timestamp: Instant,  // When the request completed
    pub success: bool,       // Did it succeed?
    pub ttft_ms: u32,        // Time to first token in milliseconds
}
```

Each request produces one `RequestOutcome`. These accumulate in a `VecDeque` per agent and are pruned when they're older than 24 hours.

**Why `Instant` instead of `SystemTime`?** `Instant` uses a monotonic clock — it only moves forward, even if the system clock is adjusted. This prevents time-skew bugs in distributed setups.

### QualityMetricsStore: The Brain

```rust
pub struct QualityMetricsStore {
    outcomes: DashMap<String, RwLock<VecDeque<RequestOutcome>>>,  // Raw data
    metrics: DashMap<String, AgentQualityMetrics>,                // Computed
    config: QualityConfig,                                        // Thresholds
}
```

Two `DashMap`s serve different purposes:
- `outcomes`: Raw request history per agent (written by request handlers, read by recompute)
- `metrics`: Precomputed aggregates per agent (written by recompute, read by routing)

This separation means routing reads (`get_metrics`) never touch the raw data and are O(1) DashMap lookups — well under the 1ms routing budget.

### Recording Outcomes

```rust
pub fn record_outcome(&self, agent_id: &str, success: bool, ttft_ms: u32) {
    let outcome = RequestOutcome {
        timestamp: Instant::now(),
        success,
        ttft_ms,
    };
    self.outcomes
        .entry(agent_id.to_string())
        .or_insert_with(|| RwLock::new(VecDeque::new()))
        .value()
        .write()
        .expect("RwLock poisoned")
        .push_back(outcome);
}
```

Called after every request completes (success or failure). The `entry().or_insert_with()` pattern lazily creates the VecDeque for new agents. The `RwLock` ensures that concurrent writes to the same agent's outcome list are serialized, while different agents can be written to in parallel (thanks to DashMap's per-shard locking).

### Recomputing Metrics: The Rolling Window

The `recompute_all()` method is the most important function in this file. It scans every agent's outcome history and computes fresh metrics:

```rust
pub fn recompute_all(&self) {
    let now = Instant::now();
    let one_hour = Duration::from_secs(3600);
    let twenty_four_hours = Duration::from_secs(86400);

    for entry in self.outcomes.iter() {
        let agent_id = entry.key().clone();
        let mut outcomes = entry.value().write().expect("RwLock poisoned");

        // Step 1: Prune outcomes older than 24 hours
        while let Some(front) = outcomes.front() {
            if now.duration_since(front.timestamp) > twenty_four_hours {
                outcomes.pop_front();
            } else {
                break;
            }
        }

        // Step 2: Scan remaining outcomes, compute 1h and 24h metrics
        let mut count_1h = 0;
        let mut errors_1h = 0;
        let mut ttft_sum_1h: u64 = 0;
        let mut count_24h = 0;
        let mut successes_24h = 0;
        let mut last_failure: Option<Instant> = None;

        for outcome in outcomes.iter() {
            let age = now.duration_since(outcome.timestamp);
            count_24h += 1;
            if outcome.success { successes_24h += 1; }
            else { last_failure = Some(outcome.timestamp); }

            if age <= one_hour {
                count_1h += 1;
                if !outcome.success { errors_1h += 1; }
                ttft_sum_1h += outcome.ttft_ms as u64;
            }
        }

        // Step 3: Compute ratios with safe division
        let error_rate_1h = if count_1h > 0 {
            errors_1h as f32 / count_1h as f32
        } else { 0.0 };

        // Step 4: Store computed metrics
        self.metrics.insert(agent_id, AgentQualityMetrics { ... });
    }
}
```

Key points:
- **Pruning first**: Old outcomes are removed before computation to save memory
- **Two windows in one pass**: Both 1h and 24h metrics are computed in a single loop
- **Safe division**: Zero-request agents get `0.0` error rate and `1.0` success rate
- **VecDeque pruning is O(k)** where k = number of expired entries (popping from front is O(1))

### The Background Loop

```rust
pub async fn quality_reconciliation_loop(
    store: Arc<QualityMetricsStore>,
    cancel_token: CancellationToken,
) {
    let interval_secs = store.config().metrics_interval_seconds;
    let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => { break; }
            _ = interval.tick() => {
                store.recompute_all();

                // Update Prometheus gauges for each agent
                for (agent_id, m) in store.get_all_metrics() {
                    metrics::gauge!("nexus_agent_error_rate",
                        "agent_id" => agent_id.clone(),
                    ).set(m.error_rate_1h as f64);
                    // ... success_rate_24h and ttft_seconds ...
                }
            }
        }
    }
}
```

This loop runs as a background Tokio task, spawned during server startup (`cli/serve.rs`). It:
1. Sleeps for 30 seconds (configurable)
2. Recomputes all metrics from raw outcomes
3. Updates Prometheus gauges so `/metrics` endpoint reflects current state
4. Respects the cancellation token for graceful shutdown

---

## File 4: routing/reconciler/quality.rs — The Bouncer

**Purpose**: Exclude backends with high error rates from the candidate list.  
**Lines**: 246  |  **Tests**: 8  |  **Status**: NEW

### Why Does This Exist?

The QualityReconciler acts like a bouncer at a club — if your error rate is above the threshold, you're not getting in. It sits in the reconciler pipeline after TierReconciler and before SchedulerReconciler.

### Pipeline Position

```
RequestAnalyzer → PrivacyReconciler → BudgetReconciler → TierReconciler
→ **QualityReconciler** → SchedulerReconciler
```

This ordering is intentional: privacy and budget constraints are checked first (hard policy rules), then quality filtering removes degraded backends, and finally the scheduler picks the best remaining candidate.

### The Reconciler Logic

```rust
impl Reconciler for QualityReconciler {
    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        let candidates: Vec<String> = intent.candidate_agents.clone();

        for agent_id in &candidates {
            let metrics = self.store.get_metrics(agent_id);

            // New backends with no history pass through (safe default)
            if metrics.request_count_1h == 0 && metrics.last_failure_ts.is_none() {
                continue;
            }

            // Backends above the error threshold are excluded
            if metrics.error_rate_1h >= self.config.error_rate_threshold {
                intent.exclude_agent(
                    agent_id.clone(),
                    "QualityReconciler",
                    format!("Error rate {:.1}% exceeds threshold {:.1}%",
                        metrics.error_rate_1h * 100.0,
                        self.config.error_rate_threshold * 100.0),
                    "Wait for agent error rate to decrease".to_string(),
                );
            }
        }
        Ok(())
    }
}
```

Key behaviors:
1. **New backends pass through**: `request_count_1h == 0 && last_failure_ts.is_none()` catches backends that have never been used. They get a chance.
2. **Threshold comparison**: `error_rate_1h >= threshold` means 50% (default threshold) is the boundary. Exactly 50% = excluded.
3. **Actionable rejection reasons**: When a backend is excluded, the reason tells the operator *why* (error rate percentage) and *what to do* (wait for recovery).
4. **All excluded is valid**: If every backend exceeds the threshold, `candidate_agents` becomes empty. The pipeline will produce a `Reject` decision with all rejection reasons, giving the client an actionable 503.

---

## File 5: routing/reconciler/scheduler.rs — The Speed Penalty

**Purpose**: Apply TTFT penalties to backend scores during candidate selection.  
**Lines**: 801 total (F16 adds ~30 lines + 3 tests)  |  **Status**: MODIFIED

### Why Does This Exist?

The SchedulerReconciler was already the final step in the pipeline — it scores backends and picks the best one. F16 adds a TTFT penalty: backends that are slow to start generating tokens get their scores reduced.

### The TTFT Penalty Method

```rust
fn apply_ttft_penalty(&self, score: u32, agent_id: &str) -> u32 {
    let metrics = self.quality_store.get_metrics(agent_id);
    let threshold = self.quality_config.ttft_penalty_threshold_ms;

    // No penalty if threshold is disabled (0) or TTFT is below threshold
    if threshold == 0 || metrics.avg_ttft_ms <= threshold {
        return score;
    }

    // Proportional penalty: the further above threshold, the bigger
    let excess = metrics.avg_ttft_ms - threshold;
    let penalty_ratio = (excess as f64 / threshold as f64).min(1.0);
    let penalty = (score as f64 * penalty_ratio) as u32;
    score.saturating_sub(penalty)
}
```

### How the Penalty Works (Example)

Given `ttft_penalty_threshold_ms = 3000` (default) and a base score of 1000:

| Backend TTFT | Excess | Penalty Ratio | Penalty | Final Score |
|-------------|--------|---------------|---------|-------------|
| 2000ms      | 0      | 0%            | 0       | 1000        |
| 3500ms      | 500    | 16.7%         | 167     | 833         |
| 4500ms      | 1500   | 50%           | 500     | 500         |
| 6000ms+     | 3000+  | 100% (capped) | 1000    | 0           |

The penalty is **proportional**: a backend that's slightly above threshold loses a little score, while one that's double the threshold loses its entire score. The `min(1.0)` caps the ratio so scores never go negative (reinforced by `saturating_sub`).

### Where It's Applied

The TTFT penalty is applied in the `Smart` routing strategy during candidate scoring:

```rust
RoutingStrategy::Smart => {
    let best = candidates.iter().max_by_key(|b| {
        let raw_score = score_backend(b.priority, b.pending, b.latency, &weights);
        let budget_adj = self.apply_budget_adjustment(raw_score, b, intent);
        self.apply_ttft_penalty(budget_adj, &b.id)  // ◄── F16
    });
    // ...
}
```

The scoring chain is: raw score → budget adjustment (F14) → TTFT penalty (F16) → final score. Each step can only reduce the score, creating a layered penalty system.

---

## File 6: metrics/handler.rs — The Stats Window

**Purpose**: Expose quality metrics through the `/v1/stats` JSON endpoint.  
**Lines**: 425  |  **Tests**: 11  |  **Status**: MODIFIED

### What Changed?

The `compute_backend_stats()` function was modified to include quality metrics when available:

```rust
pub fn compute_backend_stats(
    registry: &Registry,
    quality_store: Option<&QualityMetricsStore>,  // ◄── F16: optional quality data
) -> Vec<BackendStats> {
    backends.into_iter().map(|backend| {
        // Fetch quality metrics if store is available
        let (error_rate_1h, avg_ttft_ms, success_rate_24h) =
            if let Some(store) = quality_store {
                let m = store.get_metrics(&backend.id);
                if m.request_count_1h > 0 || m.last_failure_ts.is_some() {
                    (Some(m.error_rate_1h), Some(m.avg_ttft_ms),
                     Some(m.success_rate_24h))
                } else {
                    (None, None, None)  // No data yet
                }
            } else {
                (None, None, None)
            };

        BackendStats {
            id: backend.id, name: backend.name,
            requests: ..., average_latency_ms: ..., pending: ...,
            error_rate_1h,      // ◄── F16
            avg_ttft_ms,        // ◄── F16
            success_rate_24h,   // ◄── F16
        }
    }).collect()
}
```

Key design choices:
- **`Option<f32>` fields**: Quality metrics are `None` for backends with no history, avoiding misleading zeros. The `#[serde(skip_serializing_if = "Option::is_none")]` attribute on `BackendStats` means these fields are omitted from JSON when `None`.
- **Graceful degradation**: `quality_store` is `Option<&QualityMetricsStore>` so the stats endpoint works even if quality tracking is somehow unavailable.

### Example /v1/stats Response

```json
{
  "uptime_seconds": 3600,
  "backends": [
    {
      "id": "ollama-local",
      "name": "Ollama (localhost)",
      "requests": 150,
      "average_latency_ms": 45.0,
      "pending": 2,
      "error_rate_1h": 0.05,
      "avg_ttft_ms": 250,
      "success_rate_24h": 0.98
    },
    {
      "id": "vllm-gpu-server",
      "name": "vLLM GPU",
      "requests": 0
    }
  ]
}
```

Notice the second backend has no quality fields — it hasn't processed any requests yet.

---

## Data Flow: End to End

Here's the complete journey of quality data through the system:

```
1. REQUEST ARRIVES
   └─ POST /v1/chat/completions
      └─ api/completions.rs routes to selected backend

2. OUTCOME RECORDED (after response)
   └─ store.record_outcome("agent-id", true/false, ttft_ms)
      └─ Pushes RequestOutcome to VecDeque in DashMap

3. BACKGROUND LOOP (every 30s)
   └─ quality_reconciliation_loop()
      ├─ store.recompute_all()
      │  ├─ Prune entries > 24h old
      │  ├─ Compute error_rate_1h, avg_ttft_ms, success_rate_24h
      │  └─ Store in metrics DashMap
      └─ Update Prometheus gauges
         ├─ nexus_agent_error_rate
         ├─ nexus_agent_success_rate_24h
         └─ nexus_agent_ttft_seconds

4. ROUTING DECISION (next request)
   └─ Reconciler pipeline
      ├─ QualityReconciler reads get_metrics(agent_id)
      │  └─ Excludes agents with error_rate_1h >= threshold
      └─ SchedulerReconciler reads get_metrics(agent_id)
         └─ Applies TTFT penalty to scoring

5. OBSERVABILITY (any time)
   ├─ GET /v1/stats → JSON with per-backend quality fields
   └─ GET /metrics  → Prometheus gauges for alerting
```

### Timing

| Operation | Frequency | Latency |
|-----------|-----------|---------|
| `record_outcome()` | Per request | ~1μs (DashMap insert) |
| `recompute_all()` | Every 30s | <1ms for 100 agents |
| `get_metrics()` | Per routing decision | ~10μs (DashMap lookup) |
| Prometheus gauge update | Every 30s | ~100μs total |

---

## Understanding the Tests

### QualityMetricsStore Tests (agent/quality.rs)

| Test | What It Verifies |
|------|------------------|
| `store_returns_default_for_unknown_agent` | Unknown agents get healthy defaults (0% error, 100% success) |
| `record_outcome_stores_data` | After recording 1 success + 1 failure, error rate = 50%, avg TTFT = 150ms |
| `recompute_handles_empty_store` | Calling `recompute_all()` on empty store doesn't panic |
| `success_rate_24h_computed` | 2 successes + 1 failure = 66.7% success rate |
| `all_successes_give_zero_error_rate` | 10 successes = 0% error rate, 100% success rate |
| `last_failure_ts_tracked` | After a failure, `last_failure_ts` is `Some(...)` |
| `get_all_metrics_returns_all` | `get_all_metrics()` returns entries for all recorded agents |

### QualityReconciler Tests (routing/reconciler/quality.rs)

| Test | What It Verifies |
|------|------------------|
| `excludes_high_error_agents_above_threshold` | Agent with 75% errors is excluded; agent with 0% stays |
| `preserves_healthy_agents_below_threshold` | Both agents at 20% errors stay in candidates |
| `all_excluded_produces_rejection_reasons` | Both agents at 100% errors → both excluded, 2 rejection reasons |
| `fresh_start_no_history_all_pass` | Agents with no outcomes pass through (safe default) |
| `pass_through_preserves_all_candidates` | No outcomes recorded → all agents preserved |
| `pass_through_with_empty_candidates` | Empty candidate list → empty result (no crash) |
| `name_returns_quality_reconciler` | `name()` returns "QualityReconciler" |
| `default_creates_pass_through` | With no history, single candidate passes through |

### SchedulerReconciler TTFT Tests (routing/reconciler/scheduler.rs)

| Test | What It Verifies |
|------|------------------|
| `high_ttft_reduces_score` | Backend with 5000ms TTFT (above 3000ms threshold) loses to 200ms backend |
| `ttft_penalty_proportional_to_threshold_excess` | 3500ms gets less penalty than 10000ms |
| `no_penalty_below_threshold` | Both backends below 3000ms → no TTFT penalty applied |

---

## Configuration Options

### TOML Configuration

```toml
[quality]
# Maximum error rate (1h window) before excluding a backend.
# Range: 0.0 (exclude on any error) to 1.0 (never exclude)
# Default: 0.5
error_rate_threshold = 0.5

# TTFT threshold in milliseconds. Backends slower than this get
# proportionally penalized in routing score.
# Set to 0 to disable TTFT penalties entirely.
# Default: 3000 (3 seconds)
ttft_penalty_threshold_ms = 3000

# How often the background loop recomputes quality metrics.
# Lower values = fresher metrics but more CPU.
# Default: 30
metrics_interval_seconds = 30
```

### Prometheus Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `nexus_agent_error_rate` | Gauge | `agent_id` | Error rate (0.0-1.0) over last hour |
| `nexus_agent_success_rate_24h` | Gauge | `agent_id` | Success rate (0.0-1.0) over last 24h |
| `nexus_agent_ttft_seconds` | Histogram | `agent_id` | Average TTFT in seconds |

---

## Key Rust Concepts

### DashMap (Concurrent HashMap)

```rust
use dashmap::DashMap;

let map: DashMap<String, i32> = DashMap::new();
map.insert("key".to_string(), 42);

// Thread-safe: multiple threads can read/write different keys concurrently
// Per-shard locking: only the shard containing the key is locked
```

F16 uses two DashMaps: one for raw outcomes, one for computed metrics. This means reading metrics during routing (hot path) doesn't block writing outcomes (request handler path).

### VecDeque (Double-ended Queue)

```rust
use std::collections::VecDeque;

let mut queue: VecDeque<i32> = VecDeque::new();
queue.push_back(1);    // Add to end (new data)
queue.pop_front();     // Remove from front (old data)
queue.front();         // Peek at oldest element
```

Perfect for rolling windows: new outcomes are pushed to the back, old ones are popped from the front. Both operations are O(1).

### RwLock (Reader-Writer Lock)

```rust
use std::sync::RwLock;

let lock = RwLock::new(VecDeque::new());
let read_guard = lock.read().unwrap();    // Multiple readers allowed
let write_guard = lock.write().unwrap();  // Exclusive writer
```

F16 wraps each agent's VecDeque in an RwLock. The reconciliation loop takes a write lock (to prune and scan), while `record_outcome` also takes a write lock (to push). Multiple routing reads happen via the `metrics` DashMap, which doesn't need the RwLock.

### CancellationToken (Graceful Shutdown)

```rust
use tokio_util::sync::CancellationToken;

let token = CancellationToken::new();
tokio::select! {
    _ = token.cancelled() => { /* shutdown */ }
    _ = interval.tick() => { /* do work */ }
}
```

The quality reconciliation loop uses `tokio::select!` to respond to either the timer tick or the cancellation signal. When the server receives SIGINT, the token is cancelled, and the loop exits cleanly.

---

## Common Patterns in This Codebase

### Pattern: Two-Stage Data (Raw → Computed)

F16 stores raw outcomes separately from computed metrics. This is the same pattern used throughout Nexus:
- `Backend` (live atomics) → `BackendView` (serializable snapshot)
- `RequestOutcome` (raw data) → `AgentQualityMetrics` (aggregated metrics)

The raw data has interior mutability for writes; the computed data is a simple `Clone` struct for reads.

### Pattern: Reconciler Pipeline

Each reconciler follows the same trait:
```rust
trait Reconciler {
    fn name(&self) -> &'static str;
    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError>;
}
```

QualityReconciler fits naturally: it reads from `QualityMetricsStore`, modifies `RoutingIntent` by calling `exclude_agent()`, and returns `Ok(())`.

### Pattern: Background Loop with Cancellation

Multiple features use the same pattern:
```rust
async fn some_loop(state: Arc<T>, cancel: CancellationToken) {
    let mut interval = tokio::time::interval(duration);
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = interval.tick() => { /* work */ }
        }
    }
}
```

F16's `quality_reconciliation_loop` follows this exact pattern, consistent with the health checker and other background tasks.

---

## Next Steps

Now that you understand F16, here are paths to explore:

1. **Trace a request**: Add `RUST_LOG=debug` and watch quality metrics update after requests
2. **Tune thresholds**: Edit `[quality]` in your config and observe routing changes
3. **Read F15 (Speculative Router)**: Understand how `RequestRequirements` are extracted upstream of quality tracking
4. **Read F14 (Budget Management)**: See how the budget reconciler and TTFT penalty work together in the scoring chain
5. **Explore the SchedulerReconciler**: Follow the full scoring algorithm in `scheduler.rs` to see how priority, load, latency, budget, and TTFT all combine
