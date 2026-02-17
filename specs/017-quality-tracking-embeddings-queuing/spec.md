# Feature Specification: Quality Tracking, Embeddings & Request Queuing (Phase 2.5)

**Feature Branch**: `feature/017-quality-embeddings-queuing`
**Created**: 2026-02-17
**Status**: Draft
**Input**: RFC-001 v2 Phase 2.5 — Quality + Queuing (enables F15–F18)

## Overview

Phase 2.5 populates the quality metrics fields defined in Phase 2 with real data,
adds the Embeddings API, and enables the Queue routing decision. This unlocks
intelligence-driven routing where Nexus learns which backends perform best and can
defer requests when all backends are saturated.

**Prerequisites**: Phase 2 (Control Plane Reconciler Pipeline) — ✅ Complete in v0.3

## User Scenarios & Testing

### User Story 1 — Quality-Aware Routing (Priority: P1)

As an operator running multiple backends (local + cloud), I want Nexus to
automatically avoid backends that are returning errors or responding slowly,
so that my users get routed to the most reliable backend without manual intervention.

**Why this priority**: Quality tracking is the foundation — it produces the data
that F15 (Speculative Router) and the QualityReconciler consume. Without it, routing
remains load-based only.

**Independent Test**: Configure two backends serving the same model. Simulate one
returning 500 errors. Verify subsequent requests route to the healthy backend and
that `/v1/stats` shows the error rate and TTFT metrics.

**Acceptance Scenarios**:

1. **Given** two backends serve `llama3:70b`, **When** backend-A returns 5 consecutive errors,
   **Then** the QualityReconciler excludes backend-A from candidates (error_rate_1h > threshold).
2. **Given** backend-A recovers after 5 minutes, **When** the quality loop runs,
   **Then** backend-A's error_rate_1h drops and it becomes eligible for routing again.
3. **Given** backend-A has avg_ttft_ms=5000 and backend-B has avg_ttft_ms=200,
   **When** a request arrives, **Then** SchedulerReconciler penalizes backend-A's score.
4. **Given** a fresh Nexus start with no history, **When** requests arrive,
   **Then** all backends start with default quality scores (no penalty).

---

### User Story 2 — Embeddings API (Priority: P2)

As a developer building RAG applications, I want to send embedding requests to
`POST /v1/embeddings` and have Nexus route them to a capable backend, so that I
can use the same Nexus endpoint for both chat and embeddings.

**Why this priority**: Embeddings is a new API surface — high user value but
independent of the quality/queuing infrastructure.

**Independent Test**: Configure a backend that supports embeddings. Send a
`POST /v1/embeddings` request. Verify the response matches OpenAI format.

**Acceptance Scenarios**:

1. **Given** an Ollama backend with `nomic-embed-text` loaded, **When** I POST
   `/v1/embeddings` with `model: "nomic-embed-text"`, **Then** I receive an
   OpenAI-compatible embedding response with a `data[].embedding` array.
2. **Given** a batch request with 3 input strings, **When** I POST `/v1/embeddings`,
   **Then** I receive 3 embedding objects in `data[]`.
3. **Given** no backend supports embeddings for the requested model, **When** I POST
   `/v1/embeddings`, **Then** I receive a 503 with actionable context.
4. **Given** an OpenAI cloud backend, **When** I POST `/v1/embeddings` with
   `model: "text-embedding-3-small"`, **Then** the request routes to OpenAI with
   correct auth headers.

---

### User Story 3 — Request Queuing (Priority: P3)

As an operator with limited GPU capacity, I want Nexus to queue requests when all
backends are saturated instead of immediately returning 503, so that burst traffic
is handled gracefully with reasonable wait times.

**Why this priority**: Queuing depends on quality data (to know when backends are
"saturated") and is the most complex feature in this phase.

**Independent Test**: Configure a single backend. Send concurrent requests exceeding
the backend's capacity. Verify requests are queued and completed when capacity frees.

**Acceptance Scenarios**:

1. **Given** all backends have max pending requests, **When** a new request arrives,
   **Then** the SchedulerReconciler returns `RoutingDecision::Queue` and the request
   is placed in the bounded queue.
2. **Given** a queued request, **When** a backend becomes available within the timeout,
   **Then** the drain task re-runs the pipeline and routes the request.
3. **Given** a queued request, **When** the max_wait timeout expires,
   **Then** the client receives 503 with `retry_after` in the response.
4. **Given** queue is at max capacity (default 100), **When** another request arrives,
   **Then** the request is immediately rejected with 503 (queue full).
5. **Given** a request with `X-Nexus-Priority: high`, **When** the queue has pending
   requests, **Then** the high-priority request is drained first.
6. **Given** queued requests, **When** `/metrics` is scraped, **Then**
   `nexus_queue_depth` gauge reflects the current queue size.

---

### User Story 4 — Enhanced Request Analysis (Priority: P2)

As a developer, I want the RequestAnalyzer to better estimate token counts and
detect streaming preferences, so that routing decisions are more accurate.

**Why this priority**: Improves routing accuracy for all requests — builds on
existing RequestAnalyzer infrastructure.

**Independent Test**: Send requests with varying message lengths. Verify token
estimates in `X-Nexus-Estimated-Tokens` header are within 20% of actual.

**Acceptance Scenarios**:

1. **Given** a request with 3 messages totaling ~2000 characters, **When** analyzed,
   **Then** estimated_tokens is within 20% of chars/4 heuristic (the default tokenization baseline).
2. **Given** a request with `stream: true`, **When** analyzed, **Then**
   RequestRequirements includes `prefers_streaming: true`.

---

### Edge Cases

- What happens when the quality loop runs but no requests have been made yet?
  → All backends keep default scores (no penalty).
- What happens when all backends are excluded by QualityReconciler?
  → SchedulerReconciler receives empty candidates → 503 with rejection_reasons.
- What happens when the queue drain task picks up a request but all backends are still full?
  → Re-queue with updated wait time; eventually timeout.
- What happens when an embedding model is requested but only chat models are available?
  → 503 with `"no backend supports embeddings for model X"`.
- What happens when queue max_size is 0 (disabled)?
  → Queuing is disabled; SchedulerReconciler returns Reject instead of Queue.

## Requirements

### Functional Requirements

- **FR-001**: System MUST track per-agent error rates over a rolling 1-hour window.
- **FR-002**: System MUST track per-agent average TTFT (time to first token) over a rolling 1-hour window.
- **FR-003**: System MUST track per-agent success rate over a rolling 24-hour window.
- **FR-004**: QualityReconciler MUST exclude agents with `error_rate_1h` exceeding a configurable threshold (default: 0.5).
- **FR-005**: SchedulerReconciler MUST penalize agents with high `avg_ttft_ms` in scoring.
- **FR-006**: Background quality loop MUST compute rolling metrics every 30 seconds.
- **FR-007**: System MUST expose quality metrics via Prometheus:
  - `nexus_agent_error_rate`: gauge, labels: {agent_id, model}, updated every metrics_interval_seconds
  - `nexus_agent_ttft_seconds`: histogram, labels: {agent_id, model}, buckets: [0.05, 0.1, 0.5, 1.0, 5.0]
  - `nexus_agent_success_rate_24h`: gauge, labels: {agent_id}, updated every metrics_interval_seconds
- **FR-008**: System MUST expose quality metrics via `/v1/stats` JSON endpoint.
- **FR-009**: `POST /v1/embeddings` MUST accept OpenAI-compatible embedding requests.
- **FR-010**: Embedding requests MUST route through the reconciler pipeline.
- **FR-011**: Batch embedding requests (multiple inputs) MUST be supported.
- **FR-012**: System MUST maintain a bounded request queue with configurable max size (default: 100).
- **FR-013**: Queued requests MUST timeout after configurable max_wait (default: 30s).
- **FR-014**: Timed-out queue requests MUST return 503 with `retry_after` field.
- **FR-015**: `X-Nexus-Priority: high|normal` header MUST control queue priority.
- **FR-016**: Queue depth MUST be exposed as Prometheus gauge (`nexus_queue_depth`).
- **FR-017**: RequestAnalyzer MUST include `prefers_streaming` in RequestRequirements.
- **FR-018**: Queue size of 0 MUST disable queuing (immediate reject on saturation).

### Key Entities

- **AgentQualityMetrics**: Per-agent rolling statistics — error_rate_1h, avg_ttft_ms, success_rate_24h, last_failure_ts, request_count_1h.
- **QueuedRequest**: Bundled request awaiting routing — intent, original request, response channel, enqueued_at, priority.
- **RequestQueue**: Bounded dual-channel queue (high/normal priority) with drain task.
- **EmbeddingRequest/Response**: OpenAI-compatible embedding types.

### Configuration

```toml
[quality]
error_rate_threshold = 0.5     # Exclude agents above this error rate
ttft_penalty_threshold_ms = 3000  # Penalize agents above this TTFT
metrics_interval_seconds = 30  # How often to compute rolling metrics

[queue]
enabled = true
max_size = 100                 # Maximum queued requests (0 = disabled)
max_wait_seconds = 30          # Timeout for queued requests
```

## Success Criteria

### Measurable Outcomes

- **SC-001**: QualityReconciler excludes high-error backends within 60 seconds of degradation.
- **SC-002**: Quality metrics loop adds < 1ms overhead per cycle.
- **SC-003**: Embedding requests complete with < 5ms Nexus overhead (excluding backend latency).
- **SC-004**: Queue enqueue/dequeue operations complete in < 100μs.
- **SC-005**: All 1005+ existing tests continue to pass.
- **SC-006**: Full reconciler pipeline remains < 1ms p95 (performance budget from RFC-001).
