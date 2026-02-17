# Data Model: Quality Tracking, Embeddings & Request Queuing (Phase 2.5)

**Date**: 2026-02-17  
**Feature**: Phase 2.5 — F15, F16, F17, F18  
**Phase**: Design & Implementation

---

## Overview

This document defines the data structures, relationships, and validation rules for
Phase 2.5. Three subsystems are introduced: Quality Metrics, Embeddings API, and
Request Queue — all integrating with the existing Reconciler Pipeline.

---

## Entity Relationship Diagram

```
┌──────────────────────────────────────────────────────────────────┐
│                        Quality Subsystem                         │
│                                                                  │
│  ┌─────────────────┐    records     ┌───────────────────────┐   │
│  │ RequestOutcome   │──────────────▶│ QualityMetricsStore    │   │
│  │ (per request)    │               │ (DashMap per agent)    │   │
│  └─────────────────┘               └───────────┬───────────┘   │
│                                           computes │              │
│                                                    ▼              │
│                                    ┌───────────────────────┐     │
│                                    │ AgentQualityMetrics    │     │
│                                    │ (snapshot per agent)   │     │
│                                    └───────────┬───────────┘     │
│                                          read by │               │
│                              ┌──────────────────┼──────────┐    │
│                              ▼                   ▼          │    │
│                   ┌──────────────────┐ ┌────────────────┐  │    │
│                   │QualityReconciler │ │SchedulerRecon. │  │    │
│                   │(exclude agents)  │ │(TTFT penalty)  │  │    │
│                   └──────────────────┘ └────────────────┘  │    │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│                        Queue Subsystem                           │
│                                                                  │
│  ┌─────────────────┐   enqueue    ┌────────────────────────┐    │
│  │ CompletionsHdlr │────────────▶│ RequestQueue            │    │
│  │ (on Queue dec.) │             │ high_ch ──┐             │    │
│  └─────────────────┘             │ normal_ch ─┤ try_dequeue│    │
│                                  └────────────┼────────────┘    │
│                                         drain │                  │
│                                               ▼                  │
│                                  ┌────────────────────────┐     │
│                                  │ queue_drain_loop        │     │
│                                  │ (re-run pipeline)       │     │
│                                  └────────────────────────┘     │
└──────────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────────┐
│                      Embeddings Subsystem                        │
│                                                                  │
│  ┌─────────────────┐  route   ┌──────────────────────────┐      │
│  │ POST /v1/embed  │────────▶│ Reconciler Pipeline       │      │
│  │ (EmbeddingReq)  │         │ (select capable agent)    │      │
│  └─────────────────┘         └───────────┬──────────────┘      │
│                                    delegate │                    │
│                                            ▼                    │
│                               ┌──────────────────────┐         │
│                               │ agent.embeddings()    │         │
│                               │ (Ollama / OpenAI)     │         │
│                               └──────────────────────┘         │
└──────────────────────────────────────────────────────────────────┘
```

---

## E1: RequestOutcome

**Location**: `src/agent/quality.rs`  
**Purpose**: Records the outcome of a single inference request for quality tracking.

```rust
pub struct RequestOutcome {
    pub timestamp: Instant,
    pub success: bool,
    pub ttft_ms: u32,
}
```

| Field | Type | Description |
|-------|------|-------------|
| `timestamp` | `Instant` | When the request completed (for window pruning) |
| `success` | `bool` | Whether the backend returned a successful response |
| `ttft_ms` | `u32` | Time To First Token in milliseconds |

**Size**: ~24 bytes per entry  
**Populated by**: `QualityMetricsStore::record_outcome()` (called from completions handler)  
**Consumed by**: `QualityMetricsStore::recompute_all()` (background quality loop)  
**Lifetime**: Pruned after 24 hours during recomputation

---

## E2: AgentQualityMetrics

**Location**: `src/agent/mod.rs` (lines 49–94)  
**Purpose**: Computed snapshot of an agent's quality health.

```rust
pub struct AgentQualityMetrics {
    pub error_rate_1h: f32,
    pub avg_ttft_ms: u32,
    pub success_rate_24h: f32,
    pub last_failure_ts: Option<Instant>,
    pub request_count_1h: u32,
}
```

| Field | Type | Range | Default | Description |
|-------|------|-------|---------|-------------|
| `error_rate_1h` | `f32` | 0.0–1.0 | 0.0 | Fraction of failed requests in last hour |
| `avg_ttft_ms` | `u32` | 0–u32::MAX | 0 | Average Time To First Token in last hour |
| `success_rate_24h` | `f32` | 0.0–1.0 | 1.0 | Fraction of successful requests in last 24h |
| `last_failure_ts` | `Option<Instant>` | — | `None` | Timestamp of most recent failure |
| `request_count_1h` | `u32` | 0–u32::MAX | 0 | Total requests processed in last hour |

**Computed by**: `QualityMetricsStore::recompute_all()` every 30 seconds  
**Read by**: `QualityReconciler::reconcile()`, `SchedulerReconciler::apply_ttft_penalty()`  
**Validation**: `is_healthy()` returns `error_rate_1h < 0.5 && (request_count_1h > 0 || last_failure_ts.is_none())`

---

## E3: QualityMetricsStore

**Location**: `src/agent/quality.rs` (lines 28–90)  
**Purpose**: Thread-safe storage for recording outcomes and computing quality metrics.

```rust
pub struct QualityMetricsStore {
    outcomes: DashMap<String, RwLock<VecDeque<RequestOutcome>>>,
    metrics: DashMap<String, AgentQualityMetrics>,
    config: QualityConfig,
}
```

| Field | Type | Description |
|-------|------|-------------|
| `outcomes` | `DashMap<String, RwLock<VecDeque<RequestOutcome>>>` | Raw outcome history per agent |
| `metrics` | `DashMap<String, AgentQualityMetrics>` | Computed metric snapshots per agent |
| `config` | `QualityConfig` | Thresholds and intervals |

**Methods**:
- `record_outcome(agent_id, success, ttft_ms)` — Appends to agent's outcome history
- `get_metrics(agent_id) → AgentQualityMetrics` — Returns computed metrics (or default)
- `get_all_metrics() → HashMap<String, AgentQualityMetrics>` — Snapshot of all agents
- `recompute_all()` — Prunes entries >24h, recomputes 1h/24h aggregates
- `config() → &QualityConfig` — Configuration reference

**Concurrency pattern**: Writers (record_outcome, recompute_all) acquire per-agent RwLock write.
Readers (get_metrics) acquire per-agent RwLock read. No global contention.

---

## E4: QualityConfig

**Location**: `src/config/quality.rs` (lines 21–49)

```rust
pub struct QualityConfig {
    pub error_rate_threshold: f32,
    pub ttft_penalty_threshold_ms: u32,
    pub metrics_interval_seconds: u64,
}
```

| Field | Type | Default | TOML Key | Description |
|-------|------|---------|----------|-------------|
| `error_rate_threshold` | `f32` | 0.5 | `quality.error_rate_threshold` | Exclude agents above this |
| `ttft_penalty_threshold_ms` | `u32` | 3000 | `quality.ttft_penalty_threshold_ms` | Penalize agents above this |
| `metrics_interval_seconds` | `u64` | 30 | `quality.metrics_interval_seconds` | Background loop interval |

---

## E5: QueueConfig

**Location**: `src/config/queue.rs` (lines 20–58)

```rust
pub struct QueueConfig {
    pub enabled: bool,
    pub max_size: u32,
    pub max_wait_seconds: u64,
}
```

| Field | Type | Default | TOML Key | Description |
|-------|------|---------|----------|-------------|
| `enabled` | `bool` | `true` | `queue.enabled` | Master switch for queuing |
| `max_size` | `u32` | 100 | `queue.max_size` | Maximum queued requests |
| `max_wait_seconds` | `u64` | 30 | `queue.max_wait_seconds` | Timeout before 503 |

**Validation**: `is_enabled()` returns `self.enabled && self.max_size > 0`

---

## E6: QueuedRequest

**Location**: `src/queue/mod.rs` (lines 33–44)

```rust
pub struct QueuedRequest {
    pub intent: RoutingIntent,
    pub request: ChatCompletionRequest,
    pub response_tx: oneshot::Sender<QueueResponse>,
    pub enqueued_at: Instant,
    pub priority: Priority,
}
```

| Field | Type | Description |
|-------|------|-------------|
| `intent` | `RoutingIntent` | The routing state at time of queueing |
| `request` | `ChatCompletionRequest` | Original client request |
| `response_tx` | `oneshot::Sender` | Channel to deliver response back to handler |
| `enqueued_at` | `Instant` | For timeout detection |
| `priority` | `Priority` | High or Normal (from `X-Nexus-Priority` header) |

---

## E7: Priority

**Location**: `src/queue/mod.rs` (lines 16–30)

```rust
pub enum Priority {
    High,
    Normal,
}
```

**Parsing**: `Priority::from_header("high") → High`, anything else → `Normal`

---

## E8: RequestQueue

**Location**: `src/queue/mod.rs` (lines 74–170)

```rust
pub struct RequestQueue {
    high_tx: mpsc::Sender<QueuedRequest>,
    high_rx: tokio::sync::Mutex<mpsc::Receiver<QueuedRequest>>,
    normal_tx: mpsc::Sender<QueuedRequest>,
    normal_rx: tokio::sync::Mutex<mpsc::Receiver<QueuedRequest>>,
    depth: Arc<AtomicUsize>,
    config: QueueConfig,
}
```

**Invariants**:
1. `depth.load() <= config.max_size` (enforced by enqueue)
2. High-priority requests always drain before normal-priority
3. FIFO ordering maintained within each priority level
4. `depth` accurately reflects total items across both channels

---

## E9: EmbeddingRequest / EmbeddingResponse

**Location**: `src/api/embeddings.rs` (lines 18–64)

```rust
pub enum EmbeddingInput {
    Single(String),
    Batch(Vec<String>),
}

pub struct EmbeddingRequest {
    pub model: String,
    pub input: EmbeddingInput,
    pub encoding_format: Option<String>,
}

pub struct EmbeddingResponse {
    pub object: String,              // "list"
    pub data: Vec<EmbeddingObject>,
    pub model: String,
    pub usage: EmbeddingUsage,
}

pub struct EmbeddingObject {
    pub object: String,              // "embedding"
    pub embedding: Vec<f32>,
    pub index: usize,
}

pub struct EmbeddingUsage {
    pub prompt_tokens: u32,
    pub total_tokens: u32,
}
```

**Matches**: [OpenAI Embeddings API](https://platform.openai.com/docs/api-reference/embeddings) format exactly.

---

## Validation Rules

1. `AgentQualityMetrics.error_rate_1h` ∈ [0.0, 1.0] — computed as `failures / total`
2. `AgentQualityMetrics.success_rate_24h` ∈ [0.0, 1.0] — computed as `successes / total`
3. `QueueConfig.max_size = 0` effectively disables queuing regardless of `enabled`
4. `EmbeddingInput` must have at least one non-empty string
5. `QueuedRequest.enqueued_at + max_wait_seconds` determines timeout deadline
6. `TTFT penalty` is capped at 100% of score (`penalty_ratio.min(1.0)`)
7. `score.saturating_sub(penalty)` ensures score ≥ 0 (no underflow)

---

## Migration Notes

### Backward Compatibility

- All new config sections (`[quality]`, `[queue]`) are optional with serde defaults
- Existing config files parse without modification
- The `/v1/embeddings` endpoint is additive — no existing endpoints changed
- QualityReconciler was a pass-through stub; replacing with real logic does not
  change behavior for agents with no recorded outcomes (default = healthy)

### New Dependencies on Existing Types

- `QualityReconciler` now reads from `QualityMetricsStore` (previously standalone)
- `SchedulerReconciler` now accepts `quality_store` and `quality_config` parameters
- `RequestRequirements` gains `prefers_streaming: bool` (defaults to `false`)
- `NexusConfig` gains `quality: QualityConfig` and `queue: QueueConfig` fields
