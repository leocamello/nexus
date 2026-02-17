# Tasks: Quality Tracking, Embeddings & Request Queuing (Phase 2.5)

**Input**: `specs/017-quality-tracking-embeddings-queuing/spec.md`, `plan.md`
**Prerequisites**: Phase 2 Control Plane (✅ complete in v0.3)

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: US1=Quality, US2=Embeddings, US3=Queuing, US4=RequestAnalyzer

---

## Phase 1: Foundation (Blocking Prerequisites)

**Purpose**: Data structures and configuration that ALL features depend on

- [X] T001 [US1] Add `AgentQualityMetrics` struct to `src/agent/mod.rs` — fields: error_rate_1h (f32), avg_ttft_ms (u32), success_rate_24h (f32), last_failure_ts (Option<Instant>), request_count_1h (u32). Wrap in `Arc<RwLock<>>`. Default: all-healthy values.
  - **AC**: Struct compiles, default values are error_rate=0.0, success_rate=1.0, ttft=0
- [X] T002 [P] Add `QualityConfig` and `QueueConfig` to `src/config/mod.rs` — `error_rate_threshold` (f32, default 0.5), `ttft_penalty_threshold_ms` (u32, default 3000), `metrics_interval_seconds` (u64, default 30), `queue.enabled` (bool, default true), `queue.max_size` (u32, default 100), `queue.max_wait_seconds` (u64, default 30). Add to NexusConfig struct and TOML deserialization.
  - **AC**: Config parses from TOML, defaults applied when section missing, `nexus.example.toml` updated
- [X] T003 [P] [US4] Add `prefers_streaming: bool` to `RequestRequirements` in `src/routing/requirements.rs`. Extract from `stream` field in chat request.
  - **AC**: Field populated during request analysis, existing tests still pass

**Checkpoint**: Foundation types available — user story implementation can begin

---

## Phase 2: Quality Tracking — US1 (Priority: P1) 🎯 MVP

**Goal**: Nexus learns backend reliability and routes away from failing backends

**Independent Test**: Two backends, same model. One returns errors. Verify routing avoids it.

### Tests for US1

- [X] T004 [P] [US1] Unit tests for `AgentQualityMetrics` — default values, recording outcomes, rolling window computation. Location: `src/agent/mod.rs` (mod tests)
  - **AC**: Tests cover: default all-healthy, record_success increments count, record_failure updates error_rate, rolling window drops entries older than 1h
- [X] T005 [P] [US1] Unit tests for `QualityReconciler` — exclude high-error agents, preserve healthy agents, empty candidates produces rejection. Location: `src/routing/reconciler/quality.rs` (mod tests)
  - **AC**: Tests cover: agent above threshold excluded, agent below threshold kept, all excluded → rejection_reasons populated, fresh start (no history) → all candidates pass
- [X] T006 [P] [US1] Unit tests for TTFT penalty in `SchedulerReconciler` — high TTFT reduces score. Location: `src/routing/reconciler/scheduler.rs` (mod tests)
  - **AC**: Agent with TTFT above threshold gets lower score than agent with low TTFT

### Implementation for US1

- [ ] T007 [US1] Implement request outcome recording in completions handler (`src/api/completions.rs`) — after each response, record success/failure + TTFT to the agent's quality metrics. TTFT measured from `Instant::now()` captured at handler entry (post-routing) to first byte written to response stream; for non-streaming, measure to response body ready.
  - **AC**: Every chat completion updates the serving agent's quality metrics
- [ ] T008 [US1] Implement `quality_reconciliation_loop()` background task — runs every `metrics_interval_seconds`, iterates all agents, computes rolling 1h and 24h aggregates from recorded outcomes, updates `AgentQualityMetrics`. Location: new function in `src/routing/reconciler/quality.rs` or `src/agent/mod.rs`.
  - **AC**: Loop runs on configurable interval, computes aggregates, handles empty history gracefully
- [ ] T009 [US1] Implement `QualityReconciler` (replace pass-through stub in `src/routing/reconciler/quality.rs`) — read each candidate's `AgentQualityMetrics`, exclude candidates with `error_rate_1h > config.error_rate_threshold`, add rejection_reason for excluded candidates.
  - **AC**: Stub replaced with filtering logic, agents above threshold excluded, rejection reasons populated
- [ ] T010 [US1] Add TTFT penalty to `SchedulerReconciler` (`src/routing/reconciler/scheduler.rs`) — when scoring candidates, penalize agents with `avg_ttft_ms > config.ttft_penalty_threshold_ms`. Penalty: reduce score proportionally to how far above threshold.
  - **AC**: Scoring incorporates TTFT, high-TTFT agents score lower
- [ ] T011 [P] [US1] Add Prometheus quality metrics in `src/metrics/mod.rs` — `nexus_agent_error_rate` (gauge, labels: agent_id), `nexus_agent_ttft_seconds` (histogram, labels: agent_id), `nexus_agent_success_rate_24h` (gauge, labels: agent_id). Update metrics on each quality loop cycle.
  - **AC**: Metrics appear in `/metrics` output with correct labels and values
- [ ] T012 [US1] Add quality data to `/v1/stats` response — extend `StatsResponse` in `src/metrics/types.rs` with per-backend quality fields (error_rate_1h, avg_ttft_ms, success_rate_24h). Populate in stats handler.
  - **AC**: `/v1/stats` JSON includes quality fields per backend
- [ ] T013 [US1] Wire quality loop startup in server initialization — start `quality_reconciliation_loop` as a background task with `CancellationToken`. Location: `src/api/mod.rs` or `src/cli/serve.rs`.
  - **AC**: Loop starts on server boot, stops on graceful shutdown

**Checkpoint**: Quality-aware routing works end-to-end. Failing backends are automatically avoided.

---

## Phase 3: Embeddings API — US2 (Priority: P2)

**Goal**: `POST /v1/embeddings` routes embedding requests to capable backends

**Independent Test**: Send embedding request, get OpenAI-compatible response

### Tests for US2

- [ ] T014 [P] [US2] Unit tests for embedding types — `EmbeddingRequest` deserialization (single input, batch input), `EmbeddingResponse` serialization. Location: `src/api/embeddings.rs` (mod tests)
  - **AC**: Tests cover: single string input, array of strings, response format matches OpenAI spec
- [ ] T015 [P] [US2] Integration test for `/v1/embeddings` endpoint — mock backend, verify routing and response format. Location: `tests/` directory
  - **AC**: Test sends embedding request, receives valid response with embedding vectors

### Implementation for US2

- [ ] T016 [US2] Create `src/api/embeddings.rs` — define `EmbeddingRequest` (model, input as string or array), `EmbeddingResponse` (object, data, model, usage), `EmbeddingObject` (object, embedding, index). Implement `handle()` axum handler that routes through reconciler pipeline and delegates to `agent.embeddings()`.
  - **AC**: Handler compiles, routes request, returns OpenAI-compatible JSON
- [ ] T017 [US2] Register `POST /v1/embeddings` route in `src/api/mod.rs`
  - **AC**: Route accessible, returns 404 → valid response after handler wired
- [ ] T018 [P] [US2] Implement `embeddings()` in OllamaAgent (`src/agent/ollama.rs`) — use Ollama's `/api/embeddings` endpoint, translate to OpenAI format
  - **AC**: Ollama embedding requests return vectors in correct format
- [ ] T019 [P] [US2] Implement `embeddings()` in OpenAIAgent (`src/agent/openai.rs`) — forward to OpenAI's `/v1/embeddings`, pass auth headers
  - **AC**: OpenAI embedding requests forwarded correctly with API key
- [ ] T020 [US2] Add embedding capability detection to agent model listing — when listing models, detect if model supports embeddings (heuristic: model name contains "embed" or backend reports embedding capability)
  - **AC**: Embedding-capable models identified, routing only considers capable backends

**Checkpoint**: Embedding requests route to capable backends and return OpenAI-compatible responses

---

## Phase 4: Request Queuing — US3 (Priority: P3)

**Goal**: Burst traffic handled gracefully via bounded queue with priority support

**Independent Test**: Saturate backends, verify requests queue and drain

### Tests for US3

- [ ] T021 [P] [US3] Unit tests for `RequestQueue` — enqueue/dequeue ordering, capacity limits, priority ordering, depth reporting. Location: `src/queue/mod.rs` (mod tests)
  - **AC**: Tests cover: FIFO ordering, max_size rejection, high-priority drains first, depth() accurate, max_size=0 rejects immediately
- [ ] T022 [P] [US3] Unit tests for queue timeout — enqueued request times out, receives 503 with retry_after. Location: `src/queue/mod.rs` (mod tests)
  - **AC**: Timeout test completes within 2x the configured max_wait
- [ ] T023 [P] [US3] Integration test for queue behavior — concurrent requests exceeding capacity, verify queuing and draining. Location: `tests/` directory
  - **AC**: Test sends N+1 requests to N-capacity backend, all complete successfully

### Implementation for US3

- [ ] T024 [US3] Create `src/queue/mod.rs` — `RequestQueue` struct with bounded dual-channel (high/normal priority) using `tokio::sync::mpsc`. Methods: `enqueue(QueuedRequest)`, `try_dequeue() -> Option<QueuedRequest>`, `depth() -> usize`. `QueuedRequest` contains: RoutingIntent, original request, oneshot::Sender for response, enqueued_at (Instant), priority (High/Normal).
  - **AC**: Queue compiles, respects max_size, priority ordering works
- [ ] T025 [US3] Implement drain task — background tokio task that watches for available backend capacity, dequeues requests, re-runs reconciler pipeline, sends response via oneshot channel. Handles timeout: if request exceeds max_wait_seconds, send 503 with `retry_after`.
  - **AC**: Drain task processes queued requests, timeout produces actionable 503
- [ ] T026 [US3] Update `SchedulerReconciler` to produce `RoutingDecision::Queue` — when no candidate has capacity and queue is enabled, return Queue with estimated_wait_ms. When queue disabled (max_size=0), return Reject.
  - **AC**: Scheduler returns Queue when saturated and queue enabled, Reject when disabled
- [ ] T027 [US3] Update completions handler (`src/api/completions.rs`) — handle `RoutingDecision::Queue`: enqueue request, await response from oneshot channel with timeout.
  - **AC**: Completions handler supports all three routing decisions (Route, Queue, Reject)
- [ ] T028 [US3] Parse `X-Nexus-Priority` header — extract priority level (high/normal, default normal) from incoming request headers. Pass to RoutingIntent.
  - **AC**: Priority header parsed, invalid values default to "normal"
- [ ] T029 [P] [US3] Add Prometheus gauge `nexus_queue_depth` in `src/metrics/mod.rs` — updated by RequestQueue on enqueue/dequeue.
  - **AC**: Gauge visible in `/metrics`, reflects actual queue depth
- [ ] T030 [US3] Wire RequestQueue startup in server initialization — create queue, pass to completions handler and drain task. Start drain task with CancellationToken.
  - **AC**: Queue available to handler, drain task runs, stops on shutdown

**Checkpoint**: Burst traffic queued and drained. Priority requests served first.

---

## Phase 5: Polish & Cross-Cutting

- [ ] T031 [P] Update `nexus.example.toml` with `[quality]` and `[queue]` sections
- [ ] T032 [P] Update `docs/getting-started.md` with embeddings usage example
- [ ] T033 [P] Update `docs/api/rest.md` with `/v1/embeddings` endpoint documentation
- [ ] T034 [P] Update `docs/roadmap.md` — change Phase 2.5 status from "Planned" to "In Progress" / "Complete"
- [ ] T035 Run full test suite — verify all 1005+ existing tests still pass
- [ ] T036 Run `cargo clippy --all-targets -- -D warnings` — zero warnings
- [ ] T037 Run `cargo fmt --all -- --check` — formatting clean

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Foundation)**: No dependencies — start immediately
- **Phase 2 (Quality/US1)**: Depends on T001 (AgentQualityMetrics), T002 (QualityConfig)
- **Phase 3 (Embeddings/US2)**: Depends on T002 (config, for routing), independent of US1
- **Phase 4 (Queuing/US3)**: Depends on T002 (QueueConfig), T009 (QualityReconciler — for drain re-routing)
- **Phase 5 (Polish)**: Depends on all implementation phases

### Parallel Opportunities

- T002 (config) + T003 (requirements) can run in parallel with T001 (metrics struct)
- Phase 3 (Embeddings) can run in parallel with Phase 2 (Quality)
- All test tasks within a phase marked [P] can run in parallel
- T011 (Prometheus), T031-T034 (docs) can run in parallel

### Within Each Phase

- Tests MUST be written and FAIL before implementation
- Types/structs before logic
- Logic before handler integration
- Handler before server wiring
