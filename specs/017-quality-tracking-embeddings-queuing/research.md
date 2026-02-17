# Research: Quality Tracking, Embeddings & Request Queuing (Phase 2.5)

**Date**: 2026-02-17  
**Feature**: Phase 2.5 — F15 (Speculative Router), F16 (Quality Tracking), F17 (Embeddings), F18 (Request Queuing)  
**Context**: Populating Phase 2 extension points with real data and new API surfaces

---

## Executive Summary

Phase 2 (v0.3) established the Reconciler Pipeline with stub implementations. Phase 2.5
fills these stubs with real logic. Research confirms:

1. ✅ **QualityReconciler**: Exists as pass-through stub — ready for real filtering logic
2. ✅ **RoutingDecision::Queue**: Variant defined in `decision.rs` — drain task and queue needed
3. ✅ **agent.embeddings()**: Default returns `Unsupported` — agent-specific overrides needed
4. ✅ **RequestRequirements**: Struct exists — `prefers_streaming` field needed
5. ⚠️ **AgentQualityMetrics**: Does NOT exist — new struct required
6. ⚠️ **QualityMetricsStore**: Does NOT exist — rolling window storage required
7. ⚠️ **RequestQueue**: Does NOT exist — bounded dual-channel queue required
8. ⚠️ **QualityConfig / QueueConfig**: Do NOT exist — config sections required

---

## R1: Rolling Window vs. Fixed Counter for Quality Metrics

### Context

We need to track per-agent error rates and latency to inform routing decisions.
Two approaches: fixed counters that reset periodically, or rolling time windows.

### Decision

**Rolling time window using `VecDeque<RequestOutcome>`.**

### Rationale

- Rolling windows give continuous, smooth metrics (no "reset cliff" where a counter resets
  to zero and a bad backend suddenly looks healthy)
- `VecDeque` supports efficient push_back/pop_front for time-based pruning
- Memory is bounded: at 100 req/min, 1 hour = ~6,000 entries × ~24 bytes = ~144KB per agent

### Alternatives Considered

| Approach | Pros | Cons | Why Rejected |
|----------|------|------|--------------|
| Fixed counters (reset every N minutes) | Simpler, O(1) memory | Reset cliff creates blind spots | Constitution X: Precise Measurement |
| Exponential Moving Average (EMA) | O(1) memory, no storage | Cannot compute success_rate_24h | Insufficient for multi-window metrics |
| Database (SQLite) | Persistent across restarts | Adds external dependency, latency | Constitution: Single Binary |

### Implementation Notes

- `RequestOutcome { timestamp: Instant, success: bool, ttft_ms: u32 }` — 24 bytes per entry
- Pruning happens in the background quality loop (every 30s), not on the hot path
- Two windows computed from same data: 1h (error_rate, TTFT) and 24h (success_rate)
- Constitution Principle X (Precise Measurement): Rolling windows are more accurate than counters

---

## R2: RwLock vs. Atomics for Quality Metrics Access

### Context

Quality metrics are written by the background loop (every 30s) and read by every request
during the reconciler pipeline. The access pattern is many-reader, infrequent-writer.

### Decision

**`DashMap<String, AgentQualityMetrics>` for computed metrics, `DashMap<String, RwLock<VecDeque<RequestOutcome>>>` for raw outcomes.**

### Rationale

- DashMap provides sharded concurrent access without global locking
- RwLock on VecDeque allows concurrent reads during routing while the background
  loop holds write locks briefly during pruning
- Computed `AgentQualityMetrics` is a small Copy-friendly struct (20 bytes) — read
  contention is negligible

### Alternatives Considered

| Approach | Pros | Cons | Why Rejected |
|----------|------|------|--------------|
| Single `Mutex<HashMap>` | Simple | Global lock blocks all readers during write | Hot-path contention |
| Per-field atomics | Lock-free reads | Cannot atomically update related fields (error_rate + count) | Consistency risk |
| Arc<RwLock<HashMap>> | Standard pattern | Global RwLock still serializes all readers vs writer | DashMap is more granular |

### Implementation Notes

- `get_metrics()` returns a clone of `AgentQualityMetrics` (cheap: 20 bytes)
- `record_outcome()` acquires write lock on a single agent's VecDeque — does not block other agents
- Background loop calls `recompute_all()` which iterates agents and updates metrics DashMap

---

## R3: Dual-Channel Queue vs. Single Sorted Queue

### Context

Requests need priority support: `X-Nexus-Priority: high` requests should drain before
normal requests. Two approaches: a single priority-sorted queue, or two separate FIFO channels.

### Decision

**Two separate `tokio::sync::mpsc` channels (high and normal priority).**

### Rationale

- Simpler than maintaining a sorted data structure (no `BinaryHeap` or custom comparator)
- `try_dequeue()` checks high channel first, then normal — O(1) priority check
- FIFO ordering within each priority level is guaranteed by mpsc semantics
- tokio's mpsc is specifically designed for async — no waker management needed

### Alternatives Considered

| Approach | Pros | Cons | Why Rejected |
|----------|------|------|--------------|
| `BinaryHeap<QueuedRequest>` | Arbitrary priority levels | Requires Ord impl, not async-native, needs Mutex wrapper | Complexity for 2 priorities |
| `tokio::sync::broadcast` | Multiple consumers | Doesn't support dequeue semantics | Wrong abstraction |
| `crossbeam::deque` | Work-stealing, very fast | Not async, extra dependency | tokio mpsc is sufficient |
| Single channel + sort | One channel to manage | Sort on every dequeue is O(n log n) | Unnecessary overhead |

### Implementation Notes

- Queue depth tracked via `Arc<AtomicUsize>` (shared between enqueue/dequeue paths and Prometheus gauge)
- `QueueConfig.is_enabled()` returns `enabled && max_size > 0` — two ways to disable
- Drain loop polls every 50ms — balances responsiveness vs CPU usage
- Constitution Principle IX: Queue timeout produces actionable 503 with `Retry-After` header

---

## R4: Embeddings API — Single Endpoint vs. Per-Backend Passthrough

### Context

Embedding APIs vary across backends: Ollama uses `/api/embed` (different JSON format),
OpenAI uses `/v1/embeddings`. We need a unified endpoint.

### Decision

**Single `POST /v1/embeddings` endpoint with OpenAI-compatible format. Each agent
translates internally.**

### Rationale

- Constitution Principle III (OpenAI-Compatible): Clients should use one format
- Translation happens inside `agent.embeddings()` — the handler is backend-agnostic
- Batch support handled uniformly via `EmbeddingInput::Single` and `EmbeddingInput::Batch`

### Alternatives Considered

| Approach | Pros | Cons | Why Rejected |
|----------|------|------|--------------|
| Per-backend endpoints | No translation needed | Breaks single-API promise | Constitution III |
| Generic passthrough | Zero Nexus logic | No routing, no metrics, no failover | Defeats purpose |
| Separate embedding router | Dedicated routing | Duplicates reconciler pipeline | Anti-abstraction gate |

### Implementation Notes

- `EmbeddingInput` enum with `into_vec()` normalizes single vs batch
- Capability detection: `agent.profile().capabilities.embeddings` checked before forwarding
- Model name heuristic: names containing "embed" flagged as embedding-capable
- Tokens estimated as `sum(input_lengths) / 4` for routing (heuristic, not billing)

---

## R5: TTFT Penalty — Proportional vs. Hard Cutoff

### Context

When an agent responds slowly (high Time To First Token), we want to prefer faster
alternatives. Two approaches: hard cutoff (exclude above threshold) or proportional
penalty (score reduction).

### Decision

**Proportional penalty in SchedulerReconciler scoring.**

### Rationale

- An agent at 3001ms TTFT shouldn't be treated the same as one at 10000ms
- The proportional approach creates a gradient: slightly-slow agents get slightly lower scores,
  very-slow agents get severely penalized
- Hard cutoff in QualityReconciler already handles truly broken agents (error rate)

### Implementation Notes

```rust
let excess = avg_ttft_ms - threshold;
let penalty_ratio = (excess as f64 / threshold as f64).min(1.0); // Cap at 100%
let penalty = (score as f64 * penalty_ratio) as u32;
score.saturating_sub(penalty)
```

- At threshold=3000ms, agent at 4500ms loses 50% of score, agent at 6000ms+ loses 100%
- `saturating_sub` prevents underflow — score floors at 0
- Quality exclusion (QualityReconciler) and speed penalty (SchedulerReconciler) are independent
  — an agent can be slow but reliable, or fast but error-prone

---

## R6: Background Loop Interval — 30s vs. Real-Time

### Context

Quality metrics need periodic recomputation. Too frequent wastes CPU; too infrequent
means stale data.

### Decision

**30-second interval (configurable via `metrics_interval_seconds`).**

### Rationale

- Backend degradation is typically gradual (minutes), not instant (seconds)
- 30s means worst-case 30s delay before a failing backend is excluded — acceptable
  given that individual request failures are also handled by retry logic
- CPU cost: iterating all agents every 30s is negligible (<1ms per cycle)

### Alternatives Considered

| Interval | Latency to Detect | CPU Impact | Why |
|----------|-------------------|------------|-----|
| 1s | Near-real-time | Measurable at scale | Overkill for gradual degradation |
| 10s | Good | Low | Reasonable but 30s is sufficient |
| **30s** | **Acceptable** | **Negligible** | **Chosen — matches health check cadence** |
| 60s | Slow | Minimal | Too slow for error bursts |
| Real-time (per-request) | Instant | High | Violates <1ms pipeline budget |

---

## Decision Summary

| # | Decision | Constitution Principle |
|---|----------|----------------------|
| R1 | Rolling window VecDeque for metrics | X (Precise Measurement) |
| R2 | DashMap + RwLock for concurrent access | Performance (<1ms pipeline) |
| R3 | Dual-channel mpsc for priority queue | I (Simplicity) |
| R4 | Single OpenAI-compatible embeddings endpoint | III (OpenAI-Compatible) |
| R5 | Proportional TTFT penalty (not hard cutoff) | IX (Explicit Contracts) |
| R6 | 30-second background quality loop | Performance (negligible CPU) |

---

## Open Questions — Resolved

**Q1: Should quality metrics persist across restarts?**
A: No. Restarting Nexus gives all backends a clean slate. This is deliberate: a backend
that was failing before restart may have been fixed. Constitution: Stateless by Design.

**Q2: What happens when all backends are excluded by QualityReconciler?**
A: The pipeline continues with empty candidates → SchedulerReconciler returns Reject →
503 with rejection_reasons. Operators see exactly which agents were excluded and why.

**Q3: Should the queue support more than two priority levels?**
A: Not in Phase 2.5. Two levels (high/normal) cover the 80% use case. Additional levels
would require a sorted data structure — deferred per Constitution Principle I (Simplicity).

**Q4: What about embedding models that aren't named "embed"?**
A: The name heuristic is a starting point. Backends that explicitly declare embedding
capability via `capabilities.embeddings = true` in their agent profile are also detected.
The heuristic catches common models (nomic-embed-text, text-embedding-3-small).
