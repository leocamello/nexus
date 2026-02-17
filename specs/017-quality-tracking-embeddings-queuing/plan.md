# Implementation Plan: Quality Tracking, Embeddings & Request Queuing (Phase 2.5)

**Branch**: `feature/017-quality-embeddings-queuing` | **Date**: 2026-02-17 | **Spec**: `specs/017-quality-tracking-embeddings-queuing/spec.md`
**Input**: RFC-001 v2 Phase 2.5, Feature specification

## Summary

Phase 2.5 transforms Nexus from load-based routing to quality-aware routing. It populates
the QualityReconciler stub (currently pass-through) with real metrics, adds the Embeddings
API endpoint, and implements the Request Queue that was defined but never wired in Phase 2.

**Technical approach**: Incremental enhancement — each user story adds substance to existing
extension points (QualityReconciler, RoutingDecision::Queue, agent.embeddings()) without
restructuring the pipeline.

## Technical Context

**Language/Version**: Rust 1.75+, Edition 2021
**Primary Dependencies**: tokio, axum, reqwest, dashmap, serde, tracing, prometheus
**Storage**: In-memory (DashMap, AtomicU32/U64, VecDeque for request history)
**Testing**: cargo test (unit + integration), existing 1005+ tests as regression baseline
**Target Platform**: Linux/macOS/Windows, single binary
**Performance Goals**: Reconciler pipeline < 1ms p95, queue ops < 100μs, quality loop < 1ms/cycle
**Constraints**: No new external services, no persistence layer, single-binary deployment
**Scale/Scope**: Handles 100s of concurrent requests, 10s of backends

## Constitution Check

| Principle | Status | Notes |
|-----------|--------|-------|
| III (OpenAI-Compatible) | ✅ Pass | Embeddings follows OpenAI format exactly |
| IX (Explicit Contracts) | ✅ Pass | Queue timeout produces actionable 503 with retry_after |
| X (Precise Measurement) | ✅ Pass | Rolling window metrics with configurable thresholds |
| Latency budget < 1ms | ✅ Pass | Quality loop is async background; pipeline unchanged |

## Project Structure

### Source Code Changes

```text
src/
├── agent/
│   └── mod.rs              # MODIFY: Add AgentQualityMetrics, update embeddings() signature
├── api/
│   ├── mod.rs              # MODIFY: Add /v1/embeddings route
│   └── embeddings.rs       # CREATE: Embedding request handler
├── config/
│   └── mod.rs              # MODIFY: Add QualityConfig, QueueConfig to NexusConfig
├── metrics/
│   ├── mod.rs              # MODIFY: Add quality + queue Prometheus metrics
│   ├── handler.rs          # MODIFY: Include quality metrics in /v1/stats
│   └── types.rs            # MODIFY: Add QualityStats to StatsResponse
├── routing/
│   ├── requirements.rs     # MODIFY: Add prefers_streaming to RequestRequirements
│   └── reconciler/
│       ├── quality.rs      # MODIFY: Implement real quality filtering (replace stub)
│       ├── scheduler.rs    # MODIFY: Factor in TTFT penalty, produce Queue decision
│       └── mod.rs          # MODIFY: Wire quality loop startup
└── queue/
    └── mod.rs              # CREATE: RequestQueue, QueuedRequest, drain task

tests/
├── quality_test.rs         # CREATE: Quality reconciler unit tests
├── embeddings_test.rs      # CREATE: Embeddings API integration tests
└── queue_test.rs           # CREATE: Queue behavior tests
```

## Implementation Phases

### Phase A: Foundation — AgentQualityMetrics + Config (blocks all stories)

Add the data structures and configuration that all features depend on.

1. **AgentQualityMetrics** struct in `src/agent/mod.rs`
   - Fields: error_rate_1h (f32), avg_ttft_ms (u32), success_rate_24h (f32), last_failure_ts (Option<Instant>), request_count_1h (u32)
   - Thread-safe: wrap in `Arc<RwLock<AgentQualityMetrics>>` or use atomics
   - Default: all-healthy values (error_rate=0.0, ttft=0, success_rate=1.0)

2. **QualityConfig + QueueConfig** in `src/config/mod.rs`
   - `error_rate_threshold`, `ttft_penalty_threshold_ms`, `metrics_interval_seconds`
   - `queue.enabled`, `queue.max_size`, `queue.max_wait_seconds`
   - TOML deserialization with serde defaults

3. **RequestRequirements** enhancement
   - Add `prefers_streaming: bool` field

### Phase B: Quality Tracking (US1 — P1)

Populate QualityReconciler with real logic.

1. **Quality metrics recording** — After each request completes, record success/failure + TTFT
   - Hook into the completions handler post-response path
   - Store in a rolling window (VecDeque<RequestOutcome> per agent, capped at 1h of data)

2. **Background quality loop** — `quality_reconciliation_loop()`
   - Runs every 30s (configurable)
   - Iterates all agents, computes rolling 1h/24h aggregates
   - Updates AgentQualityMetrics

3. **QualityReconciler implementation** — Replace pass-through stub
   - Read each candidate's AgentQualityMetrics
   - Exclude candidates with error_rate_1h > threshold
   - Add rejection_reason for excluded candidates

4. **SchedulerReconciler TTFT penalty**
   - Penalize score for candidates with avg_ttft_ms > ttft_penalty_threshold_ms

5. **Prometheus metrics**
   - `nexus_agent_error_rate` (gauge, per agent)
   - `nexus_agent_ttft_seconds` (histogram, per agent)
   - `nexus_agent_success_rate_24h` (gauge, per agent)

6. **Stats API** — Add quality fields to `/v1/stats`

### Phase C: Embeddings API (US2 — P2)

New API surface, independent of quality/queue.

1. **Types** — `EmbeddingRequest`, `EmbeddingResponse`, `EmbeddingObject` in `src/api/embeddings.rs`
2. **Handler** — `POST /v1/embeddings` handler using reconciler pipeline
3. **Route** — Register in `create_router()`
4. **Agent implementation** — Override `embeddings()` in Ollama and OpenAI agents

### Phase D: Request Queuing (US3 — P3)

Most complex — builds on quality data.

1. **RequestQueue** — Bounded dual-channel queue (high/normal priority)
   - `enqueue()`, `try_dequeue()`, `depth()` methods
   - oneshot channel for response delivery
2. **Drain task** — Background tokio task
   - Polls queue when backends become available
   - Re-runs reconciler pipeline for dequeued requests
   - Handles timeout (sends 503 with retry_after)
3. **SchedulerReconciler Queue integration**
   - When no candidate has capacity → return `RoutingDecision::Queue`
   - When queue disabled (max_size=0) → return `RoutingDecision::Reject`
4. **Completions handler integration**
   - Handle `RoutingDecision::Queue` → enqueue and await response
5. **Prometheus gauge** — `nexus_queue_depth`
6. **X-Nexus-Priority header** parsing

## Dependencies & Execution Order

```
Phase A (Foundation) ─────────────────────┐
                                          ├──→ Phase B (Quality) ──→ Phase D (Queuing)
Phase C (Embeddings) ─────────────────────┘
                                          (C is independent, can parallel with B)
```

## Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| Quality loop contention with request path | High | Use RwLock; loop only writes, request path only reads |
| Queue drain task race conditions | Medium | Single drain task, tokio::sync::mpsc for ordering |
| Embeddings varies across backends | Low | Start with OpenAI format; Ollama already supports it |
| Rolling window memory usage | Low | Cap at 1h of data, ~50KB per agent at 100 req/min |
