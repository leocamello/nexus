# Phase 2.5: Quality Tracking, Embeddings & Request Queuing — Code Walkthrough

**Feature**: Phase 2.5 — F15 (Speculative Router), F16 (Quality Tracking), F17 (Embeddings API), F18 (Request Queuing)  
**Audience**: Junior developers joining the project  
**Last Updated**: 2026-02-17

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: agent/mod.rs — The Health Report Card](#file-1-agentmodrs--the-health-report-card)
4. [File 2: agent/quality.rs — The Hospital Records Room](#file-2-agentqualityrs--the-hospital-records-room)
5. [File 3: config/quality.rs — The Triage Rules](#file-3-configqualityrs--the-triage-rules)
6. [File 4: config/queue.rs — The Waiting Room Capacity](#file-4-configqueuers--the-waiting-room-capacity)
7. [File 5: routing/reconciler/quality.rs — The Triage Nurse](#file-5-routingreconcilerqualityrs--the-triage-nurse)
8. [File 6: routing/reconciler/scheduler.rs — The Speed Penalty](#file-6-routingreconcilerschedulerrs--the-speed-penalty)
9. [File 7: queue/mod.rs — The Waiting Room](#file-7-queuemodrs--the-waiting-room)
10. [File 8: api/embeddings.rs — The Translation Desk](#file-8-apiembeddingsrs--the-translation-desk)
11. [File 9: api/completions.rs — The Intake Coordinator](#file-9-apicompletionsrs--the-intake-coordinator)
12. [Understanding the Tests](#understanding-the-tests)
13. [Key Rust Concepts](#key-rust-concepts)
14. [Common Patterns in This Codebase](#common-patterns-in-this-codebase)
15. [Next Steps](#next-steps)

---

## The Big Picture

Imagine Nexus is a **hospital emergency department**. Patients (requests) arrive and
need to be seen by doctors (backends). Before Phase 2.5, the hospital had no way to
track which doctors were dropping patients, no waiting room for overflow, and no way
to handle lab work (embeddings) — only consultations (chat completions).

**Phase 2.5 adds three departments:**

1. **Quality Tracking (F16)** — Like a hospital quality board that monitors each
   doctor's patient outcomes. If Dr. Smith has a 70% complication rate, the triage
   nurse stops sending patients to Dr. Smith until the rate improves.

2. **Request Queuing (F18)** — Like a waiting room. When all doctors are busy,
   patients wait in line instead of being turned away. There's a "fast track" line
   for urgent cases (high-priority requests) and a regular line.

3. **Embeddings API (F17)** — Like adding a lab department to the hospital. Now the
   hospital can run blood tests (convert text to vectors) alongside the usual
   consultations (chat completions).

### What Problem Does This Solve?

Before Phase 2.5:
- A failing backend would keep receiving requests until its health check detected the
  problem (could take minutes)
- When all backends were busy, requests were immediately rejected with 503
- There was no API endpoint for embedding text into vectors

After Phase 2.5:
- Quality metrics are tracked per-request — a backend with a 60% error rate is
  excluded within 30 seconds
- Requests queue up with configurable timeout and dual priority levels
- `POST /v1/embeddings` provides OpenAI-compatible embedding API

### How Phase 2.5 Fits Into Nexus

```
┌──────────── Nexus Control Plane ─────────────────────────────────┐
│                                                                   │
│  Request arrives                                                  │
│       │                                                           │
│       ▼                                                           │
│  ┌─────────────────────────────────────────────────────────┐     │
│  │          Reconciler Pipeline (< 1ms total)              │     │
│  │                                                         │     │
│  │  RequestAnalyzer ──► PrivacyReconciler ──►              │     │
│  │  BudgetReconciler ──► TierReconciler ──►                │     │
│  │  ★ QualityReconciler ──► SchedulerReconciler            │     │
│  │       │                    │                             │     │
│  │       │ (filters by        │ (TTFT penalty)             │     │
│  │       │  error rate)       │                             │     │
│  └───────┼────────────────────┼─────────────────────────┘     │
│          ▼                    ▼                                   │
│   ┌─────────────────────────────────┐                            │
│   │     RoutingDecision              │                            │
│   │  ┌─────────────────────────┐    │                            │
│   │  │ Route → send to backend │    │                            │
│   │  │ ★ Queue → waiting room  │    │                            │
│   │  │ Reject → 503 response   │    │                            │
│   │  └─────────────────────────┘    │                            │
│   └─────────────────────────────────┘                            │
│                                                                   │
│  ★ Background Quality Loop (every 30s)                           │
│   ┌──────────────────────────────────────────┐                   │
│   │ QualityMetricsStore.recompute_all()       │                   │
│   │  → Prune entries > 24h                    │                   │
│   │  → Compute error_rate_1h, avg_ttft_ms     │                   │
│   │  → Update Prometheus gauges               │                   │
│   └──────────────────────────────────────────┘                   │
│                                                                   │
│  ★ Queue Drain Loop (every 50ms)                                 │
│   ┌──────────────────────────────────────────┐                   │
│   │ Dequeue (high priority first)             │                   │
│   │  → Check timeout → 503 with Retry-After   │                   │
│   │  → Re-run reconciler pipeline             │                   │
│   │  → Route to available backend             │                   │
│   └──────────────────────────────────────────┘                   │
│                                                                   │
│  ★ POST /v1/embeddings                                           │
│   ┌──────────────────────────────────────────┐                   │
│   │ Route through pipeline → agent.embed()    │                   │
│   │ Return OpenAI-compatible response         │                   │
│   └──────────────────────────────────────────┘                   │
└───────────────────────────────────────────────────────────────────┘

★ = New in Phase 2.5
```

### Key Design Decisions

1. **Rolling window, not fixed counters**: Quality metrics use a `VecDeque` of
   timestamped outcomes. This avoids the "reset cliff" where a counter resets and
   a broken backend suddenly looks healthy.

2. **Dual-channel queue, not sorted**: Two `mpsc` channels (high/normal) instead of
   a `BinaryHeap`. Simpler, async-native, O(1) dequeue.

3. **Proportional TTFT penalty, not hard cutoff**: A backend at 3001ms TTFT gets a
   small penalty; one at 6000ms gets a severe penalty. Gradual degradation is better
   than binary exclusion for speed.

4. **Single embeddings endpoint**: All backends get the same `/v1/embeddings` URL.
   Each agent translates to its native format internally (Ollama → `/api/embed`,
   OpenAI → `/v1/embeddings`).

---

## File Structure

```
src/
├── agent/
│   ├── mod.rs              ★ AgentQualityMetrics struct (lines 49-94)
│   └── quality.rs          ★ QualityMetricsStore + quality_reconciliation_loop
├── api/
│   ├── completions.rs      ★ Outcome recording + queue integration
│   ├── embeddings.rs       ★ POST /v1/embeddings handler + types
│   └── mod.rs              ★ Route registration (line 190)
├── config/
│   ├── quality.rs          ★ QualityConfig struct
│   └── queue.rs            ★ QueueConfig struct
├── queue/
│   └── mod.rs              ★ RequestQueue + queue_drain_loop
└── routing/reconciler/
    ├── quality.rs           ★ QualityReconciler (error rate filtering)
    └── scheduler.rs         ★ apply_ttft_penalty (lines 109-121)

tests/
├── embeddings_test.rs      ★ Integration tests for /v1/embeddings
└── queue_test.rs           ★ Integration tests for request queue

★ = New or modified in Phase 2.5
```

---

## File 1: agent/mod.rs — The Health Report Card

**Purpose**: Defines the `AgentQualityMetrics` struct — a snapshot of how well a
backend has been performing. Think of it as each doctor's scorecard on the hospital
quality board.

```rust
#[derive(Debug, Clone)]
pub struct AgentQualityMetrics {
    pub error_rate_1h: f32,                    // 0.0–1.0
    pub avg_ttft_ms: u32,                      // Average response speed
    pub success_rate_24h: f32,                 // 0.0–1.0
    pub last_failure_ts: Option<Instant>,       // When did they last fail?
    pub request_count_1h: u32,                 // How busy are they?
}
```

The `Default` implementation is crucial: a new backend starts with a **clean slate**
(0% errors, 100% success). This means Nexus trusts new backends until they prove
unreliable — a deliberate design choice.

```rust
impl Default for AgentQualityMetrics {
    fn default() -> Self {
        Self {
            error_rate_1h: 0.0,
            avg_ttft_ms: 0,
            success_rate_24h: 1.0,     // Innocent until proven guilty
            last_failure_ts: None,
            request_count_1h: 0,
        }
    }
}
```

The `is_healthy()` method provides a quick check:

```rust
pub fn is_healthy(&self) -> bool {
    self.error_rate_1h < 0.5
        && (self.request_count_1h > 0 || self.last_failure_ts.is_none())
}
```

---

## File 2: agent/quality.rs — The Hospital Records Room

**Purpose**: Stores raw request outcomes and computes aggregate quality metrics. This
is the filing cabinet where every patient interaction is recorded, and the background
process that summarizes those records into the scorecard (File 1).

The core storage structure:

```rust
pub struct QualityMetricsStore {
    outcomes: DashMap<String, RwLock<VecDeque<RequestOutcome>>>,
    metrics: DashMap<String, AgentQualityMetrics>,
    config: QualityConfig,
}
```

Each request is recorded as a `RequestOutcome`:

```rust
pub struct RequestOutcome {
    pub timestamp: Instant,    // When it happened
    pub success: bool,         // Did it work?
    pub ttft_ms: u32,          // How fast was first response?
}
```

The `record_outcome()` method is called from the completions handler on every request:

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

The `recompute_all()` method runs every 30 seconds in the background loop. It:
1. Prunes entries older than 24 hours
2. Computes 1-hour metrics (error rate, avg TTFT, request count)
3. Computes 24-hour metrics (success rate)
4. Updates the `metrics` DashMap

The background loop also publishes Prometheus gauges:

```rust
pub async fn quality_reconciliation_loop(
    store: Arc<QualityMetricsStore>,
    cancel_token: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => break,
            _ = interval.tick() => {
                store.recompute_all();
                // Update Prometheus gauges
                for (agent_id, m) in store.get_all_metrics() {
                    metrics::gauge!("nexus_agent_error_rate", "agent_id" => agent_id)
                        .set(m.error_rate_1h as f64);
                    // ... more gauges
                }
            }
        }
    }
}
```

### Key Tests

```rust
#[test]
fn record_outcome_stores_data() {
    let store = QualityMetricsStore::new(default_config());
    store.record_outcome("agent-1", true, 100);
    store.record_outcome("agent-1", false, 200);
    store.recompute_all();

    let m = store.get_metrics("agent-1");
    assert_eq!(m.request_count_1h, 2);
    assert_eq!(m.error_rate_1h, 0.5);      // 1 failure / 2 total
    assert_eq!(m.avg_ttft_ms, 150);         // (100 + 200) / 2
}

#[test]
fn store_returns_default_for_unknown_agent() {
    let store = QualityMetricsStore::new(default_config());
    let m = store.get_metrics("unknown");
    assert_eq!(m.error_rate_1h, 0.0);       // Unknown = healthy
    assert_eq!(m.success_rate_24h, 1.0);
}

#[test]
fn all_successes_give_zero_error_rate() {
    let store = QualityMetricsStore::new(default_config());
    for _ in 0..10 {
        store.record_outcome("a", true, 100);
    }
    store.recompute_all();
    let m = store.get_metrics("a");
    assert_eq!(m.error_rate_1h, 0.0);
    assert_eq!(m.success_rate_24h, 1.0);
}
```

---

## File 3: config/quality.rs — The Triage Rules

**Purpose**: Configuration for quality tracking. These are the hospital's quality
policies — at what point do we consider a doctor unreliable?

```rust
pub struct QualityConfig {
    pub error_rate_threshold: f32,          // Default: 0.5 (50%)
    pub ttft_penalty_threshold_ms: u32,     // Default: 3000 (3 seconds)
    pub metrics_interval_seconds: u64,      // Default: 30
}
```

All fields have `#[serde(default)]` — existing TOML config files work without changes.

---

## File 4: config/queue.rs — The Waiting Room Capacity

**Purpose**: Configuration for request queuing.

```rust
pub struct QueueConfig {
    pub enabled: bool,           // Default: true
    pub max_size: u32,           // Default: 100
    pub max_wait_seconds: u64,   // Default: 30
}
```

The `is_enabled()` method has a subtle double-check:

```rust
pub fn is_enabled(&self) -> bool {
    self.enabled && self.max_size > 0
}
```

Setting `max_size = 0` disables queuing even if `enabled = true`. This is a safety
valve for operators who want to explicitly prevent any queuing.

---

## File 5: routing/reconciler/quality.rs — The Triage Nurse

**Purpose**: The QualityReconciler is a pipeline stage that removes unreliable backends
from the candidate list. It's the triage nurse who checks each doctor's scorecard before
assigning patients.

```rust
pub struct QualityReconciler {
    store: Arc<QualityMetricsStore>,
    config: QualityConfig,
}

impl Reconciler for QualityReconciler {
    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        for agent_id in &candidates {
            let metrics = self.store.get_metrics(agent_id);

            // New backends with no history pass through
            if metrics.request_count_1h == 0 && metrics.last_failure_ts.is_none() {
                continue;
            }

            // Exclude agents above the error threshold
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

Key behavior: agents with **no history** always pass through. This ensures new
backends get a chance to prove themselves. The `exclude_agent()` method adds
structured rejection reasons — if all agents are excluded, the 503 response tells
the operator exactly **why** each one was rejected.

### Key Tests

```rust
#[test]
fn excludes_high_error_agents_above_threshold() {
    let store = make_store();
    // agent-healthy: 10 successes
    for _ in 0..10 { store.record_outcome("agent-healthy", true, 100); }
    // agent-failing: 75% errors (3 fail, 1 success)
    store.record_outcome("agent-failing", false, 100);
    store.record_outcome("agent-failing", false, 100);
    store.record_outcome("agent-failing", false, 100);
    store.record_outcome("agent-failing", true, 100);
    store.recompute_all();

    let reconciler = QualityReconciler::new(store, default_config());
    let mut intent = create_intent("llama3:8b",
        vec!["agent-healthy".into(), "agent-failing".into()]);

    reconciler.reconcile(&mut intent).unwrap();

    assert_eq!(intent.candidate_agents.len(), 1);        // Only healthy survives
    assert_eq!(intent.candidate_agents[0], "agent-healthy");
    assert!(!intent.rejection_reasons.is_empty());        // Actionable reason added
}

#[test]
fn fresh_start_no_history_all_pass() {
    let store = make_store();
    // No outcomes recorded at all
    let reconciler = QualityReconciler::new(store, default_config());
    let mut intent = create_intent("llama3:8b",
        vec!["new-agent-1".into(), "new-agent-2".into()]);

    reconciler.reconcile(&mut intent).unwrap();

    assert_eq!(intent.candidate_agents.len(), 2);  // Both pass through
    assert!(intent.excluded_agents.is_empty());
}

#[test]
fn all_excluded_produces_rejection_reasons() {
    let store = make_store();
    for _ in 0..5 {
        store.record_outcome("agent-bad-1", false, 100);
        store.record_outcome("agent-bad-2", false, 100);
    }
    store.recompute_all();

    let reconciler = QualityReconciler::new(store, default_config());
    let mut intent = create_intent("llama3:8b",
        vec!["agent-bad-1".into(), "agent-bad-2".into()]);

    reconciler.reconcile(&mut intent).unwrap();

    assert!(intent.candidate_agents.is_empty());     // All excluded
    assert!(intent.rejection_reasons.len() >= 2);    // One reason per agent
}
```

---

## File 6: routing/reconciler/scheduler.rs — The Speed Penalty

**Purpose**: The SchedulerReconciler scores and ranks backends. Phase 2.5 adds a
TTFT penalty: slow backends get lower scores, making them less likely to be chosen.

The penalty is **proportional**, not binary:

```rust
fn apply_ttft_penalty(&self, score: u32, agent_id: &str) -> u32 {
    let metrics = self.quality_store.get_metrics(agent_id);
    let threshold = self.quality_config.ttft_penalty_threshold_ms;
    if threshold == 0 || metrics.avg_ttft_ms <= threshold {
        return score;  // No penalty — within limits
    }

    // How far above the threshold?
    let excess = metrics.avg_ttft_ms - threshold;
    // Cap ratio at 1.0 (100% penalty)
    let penalty_ratio = (excess as f64 / threshold as f64).min(1.0);
    let penalty = (score as f64 * penalty_ratio) as u32;
    score.saturating_sub(penalty)
}
```

**Example with threshold = 3000ms:**
- Backend at 3000ms: No penalty (exactly at threshold)
- Backend at 4500ms: 50% penalty (excess=1500, ratio=0.5)
- Backend at 6000ms: 100% penalty (excess=3000, ratio=1.0, score → 0)

The `saturating_sub` prevents underflow — the score can never go below 0.

---

## File 7: queue/mod.rs — The Waiting Room

**Purpose**: The `RequestQueue` holds requests when all backends are busy. High-priority
requests get seen first, like a hospital fast-track lane.

The queue uses two `tokio::sync::mpsc` channels:

```rust
pub struct RequestQueue {
    high_tx: mpsc::Sender<QueuedRequest>,
    high_rx: tokio::sync::Mutex<mpsc::Receiver<QueuedRequest>>,
    normal_tx: mpsc::Sender<QueuedRequest>,
    normal_rx: tokio::sync::Mutex<mpsc::Receiver<QueuedRequest>>,
    depth: Arc<AtomicUsize>,      // Shared counter for Prometheus gauge
    config: QueueConfig,
}
```

Each queued request carries a one-shot channel to deliver the response:

```rust
pub struct QueuedRequest {
    pub intent: RoutingIntent,
    pub request: ChatCompletionRequest,
    pub response_tx: oneshot::Sender<QueueResponse>,  // The "buzzer" for the patient
    pub enqueued_at: Instant,                          // For timeout detection
    pub priority: Priority,                            // High or Normal
}
```

The `enqueue()` method enforces capacity atomically:

```rust
pub fn enqueue(&self, request: QueuedRequest) -> Result<(), QueueError> {
    if !self.config.is_enabled() {
        return Err(QueueError::Disabled);
    }
    let current = self.depth.load(Ordering::SeqCst);
    if current >= self.config.max_size as usize {
        return Err(QueueError::Full { max_size: self.config.max_size });
    }
    self.depth.fetch_add(1, Ordering::SeqCst);
    // ... send to high or normal channel
}
```

The `try_dequeue()` method always checks high-priority first:

```rust
pub async fn try_dequeue(&self) -> Option<QueuedRequest> {
    // Try high priority first
    { let mut rx = self.high_rx.lock().await;
      if let Ok(req) = rx.try_recv() { /* ... */ return Some(req); } }
    // Then normal priority
    { let mut rx = self.normal_rx.lock().await;
      if let Ok(req) = rx.try_recv() { /* ... */ return Some(req); } }
    None
}
```

The `queue_drain_loop()` runs in the background, polling every 50ms:
1. Dequeue a request (high priority first)
2. Check if it's timed out → send 503 with `Retry-After` header
3. Re-run the reconciler pipeline to find an available backend
4. If a backend is available, forward the request and send response via oneshot
5. If still no capacity, re-enqueue (if not yet timed out)

### Key Tests

```rust
#[tokio::test]
async fn priority_ordering_high_drains_first() {
    let queue = RequestQueue::new(make_config(10, 30));

    let (normal1, _) = make_queued(Priority::Normal);
    let (high1, _) = make_queued(Priority::High);
    let (normal2, _) = make_queued(Priority::Normal);

    queue.enqueue(normal1).unwrap();
    queue.enqueue(high1).unwrap();       // Enqueued second...
    queue.enqueue(normal2).unwrap();

    let d1 = queue.try_dequeue().await.unwrap();
    assert_eq!(d1.priority, Priority::High);    // ...but dequeued first!
}

#[tokio::test]
async fn capacity_limits_reject_when_full() {
    let queue = RequestQueue::new(make_config(2, 30));

    let (req1, _) = make_queued(Priority::Normal);
    let (req2, _) = make_queued(Priority::Normal);
    let (req3, _) = make_queued(Priority::Normal);

    queue.enqueue(req1).unwrap();
    queue.enqueue(req2).unwrap();
    let result = queue.enqueue(req3);   // Third request rejected

    assert!(matches!(result, Err(QueueError::Full { max_size: 2 })));
}

#[tokio::test]
async fn timeout_response_has_retry_after() {
    let response = build_timeout_response("30");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let retry = response.headers().get(RETRY_AFTER).unwrap();
    assert_eq!(retry.to_str().unwrap(), "30");
}

#[test]
fn priority_from_header_high() {
    assert_eq!(Priority::from_header("high"), Priority::High);
    assert_eq!(Priority::from_header("HIGH"), Priority::High);
    assert_eq!(Priority::from_header(" High "), Priority::High);
}

#[test]
fn priority_from_header_invalid_defaults_to_normal() {
    assert_eq!(Priority::from_header(""), Priority::Normal);
    assert_eq!(Priority::from_header("urgent"), Priority::Normal);
}
```

---

## File 8: api/embeddings.rs — The Translation Desk

**Purpose**: Handles `POST /v1/embeddings` — the new lab department. Accepts
OpenAI-compatible embedding requests, routes to a capable backend, and returns
OpenAI-compatible responses.

The type system handles both single and batch input:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    Single(String),
    Batch(Vec<String>),
}

impl EmbeddingInput {
    pub fn into_vec(self) -> Vec<String> {
        match self {
            EmbeddingInput::Single(s) => vec![s],
            EmbeddingInput::Batch(v) => v,
        }
    }
}
```

The handler follows the standard Nexus request flow:

```rust
pub async fn handle(
    State(state): State<Arc<AppState>>,
    _headers: HeaderMap,
    Json(request): Json<EmbeddingRequest>,
) -> Result<Response, ApiError> {
    let input_texts = request.input.into_vec();

    // 1. Build routing requirements
    let requirements = RequestRequirements { model: request.model.clone(), ... };

    // 2. Route through reconciler pipeline
    let routing_result = state.router.select_backend(&requirements, None)?;

    // 3. Get agent and verify embedding capability
    let agent = state.registry.get_agent(&backend.id)?;
    if !agent.profile().capabilities.embeddings {
        return Err(ApiError::service_unavailable("Backend does not support embeddings"));
    }

    // 4. Delegate to agent
    let vectors = agent.embeddings(input_texts).await?;

    // 5. Build OpenAI-compatible response
    Ok(Json(EmbeddingResponse { object: "list", data, model, usage }).into_response())
}
```

### Key Tests

```rust
#[test]
fn embedding_request_deserialize_single_input() {
    let json = r#"{"model":"text-embedding-ada-002","input":"hello world"}"#;
    let req: EmbeddingRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.model, "text-embedding-ada-002");
    match &req.input {
        EmbeddingInput::Single(s) => assert_eq!(s, "hello world"),
        _ => panic!("Expected Single variant"),
    }
}

#[test]
fn embedding_request_deserialize_batch_input() {
    let json = r#"{"model":"text-embedding-ada-002","input":["hello","world"]}"#;
    let req: EmbeddingRequest = serde_json::from_str(json).unwrap();
    match &req.input {
        EmbeddingInput::Batch(v) => assert_eq!(v.len(), 2),
        _ => panic!("Expected Batch variant"),
    }
}

#[test]
fn embedding_response_serialization_matches_openai() {
    let response = EmbeddingResponse {
        object: "list".to_string(),
        data: vec![EmbeddingObject {
            object: "embedding".to_string(),
            embedding: vec![0.1, 0.2, 0.3],
            index: 0,
        }],
        model: "text-embedding-ada-002".to_string(),
        usage: EmbeddingUsage { prompt_tokens: 10, total_tokens: 10 },
    };
    let json = serde_json::to_value(&response).unwrap();
    assert_eq!(json["object"], "list");            // OpenAI format
    assert_eq!(json["data"][0]["object"], "embedding");
    assert_eq!(json["usage"]["prompt_tokens"], 10);
}
```

---

## File 9: api/completions.rs — The Intake Coordinator

**Purpose**: The completions handler is modified to integrate quality tracking and
queue handling. After every request, it records the outcome. When routing returns
`Queue`, it enqueues the request instead of rejecting immediately.

**Outcome recording** (called after every request):

```rust
// In the completions handler, after receiving a response from the agent:
if let Some(quality_store) = &state.quality_store {
    let success = response.status().is_success();
    let ttft_ms = start_time.elapsed().as_millis() as u32;
    quality_store.record_outcome(&backend.id, success, ttft_ms);
}
```

**Queue integration** (when RoutingDecision is Queue):

```rust
RoutingDecision::Queue { reason } => {
    if let Some(queue) = &state.request_queue {
        let (tx, rx) = oneshot::channel();
        let queued = QueuedRequest {
            intent,
            request,
            response_tx: tx,
            enqueued_at: Instant::now(),
            priority: Priority::from_header(
                headers.get("X-Nexus-Priority").unwrap_or("normal")),
        };
        match queue.enqueue(queued) {
            Ok(()) => {
                // Wait for the drain loop to process this request
                match rx.await {
                    Ok(response) => return response,
                    Err(_) => return Err(ApiError::service_unavailable("Queue closed")),
                }
            }
            Err(QueueError::Full { .. }) => {
                return Err(ApiError::service_unavailable("Queue is full"));
            }
        }
    }
}
```

---

## Understanding the Tests

Phase 2.5 adds 44 tests across 4 modules and 2 integration test files:

| Location | Count | Focus |
|----------|-------|-------|
| `src/agent/quality.rs` | 7 | Metrics store: recording, computing, defaults |
| `src/routing/reconciler/quality.rs` | 8 | Reconciler: filtering, exclusion, rejection reasons |
| `src/queue/mod.rs` | 13 | Queue: FIFO, priority, capacity, timeout, priority parsing |
| `src/api/embeddings.rs` | 8 | Embedding types: serialization, deserialization, roundtrip |
| `tests/embeddings_test.rs` | 5 | Integration: endpoint exists, valid response, errors, batch |
| `tests/queue_test.rs` | 3 | Integration: capacity overflow, priority ordering |

### Test Patterns

- **Record → Recompute → Assert**: Quality tests follow a three-step pattern:
  record outcomes, call `recompute_all()`, then assert computed metrics.
- **Make helpers**: Tests use `make_queued()`, `make_config()`, `make_store()`
  factory functions to reduce boilerplate.
- **Oneshot channels in tests**: Queue tests create `(tx, rx)` pairs but only use
  `rx` when needed — the `_rx` convention signals "intentionally unused."
- **Serde roundtrip**: Embedding tests verify that serializing and deserializing
  produces identical results, ensuring OpenAI compatibility.

---

## Key Rust Concepts

### 1. DashMap — Concurrent HashMap

`DashMap` is a sharded concurrent map. Unlike `HashMap`, multiple threads can read
and write to different keys without blocking each other:

```rust
// QualityMetricsStore uses DashMap for both raw outcomes and computed metrics
outcomes: DashMap<String, RwLock<VecDeque<RequestOutcome>>>,
metrics: DashMap<String, AgentQualityMetrics>,
```

### 2. RwLock — Read-Write Lock

`std::sync::RwLock` allows many concurrent readers OR one exclusive writer. Perfect
for quality metrics: the background loop writes every 30s, but the reconciler reads
on every request:

```rust
// Many readers (reconciler pipeline, every request)
let metrics = self.store.get_metrics(agent_id);  // Reads DashMap, no global lock

// One writer (background loop, every 30s)
outcomes.write().expect("RwLock poisoned").push_back(outcome);
```

### 3. tokio::sync::mpsc — Async Multi-Producer Single-Consumer

The queue uses `mpsc` channels — multiple request handlers can enqueue simultaneously,
but the drain loop is the single consumer:

```rust
let (high_tx, high_rx) = mpsc::channel(capacity);
// Multiple producers: request handlers call queue.enqueue() → high_tx.try_send()
// Single consumer: drain loop calls queue.try_dequeue() → high_rx.try_recv()
```

### 4. tokio::sync::oneshot — One-Shot Response Channel

Each queued request gets a `oneshot` channel to receive its response. The handler
sends a `oneshot::Sender`, then `await`s the `oneshot::Receiver`. The drain loop
sends the response via the `Sender`:

```rust
// Handler side (waiting patient)
let (tx, rx) = oneshot::channel();
queue.enqueue(QueuedRequest { response_tx: tx, ... });
let response = rx.await?;  // Blocks until drain loop responds

// Drain loop side (doctor)
let _ = queued.response_tx.send(Ok(response));  // Deliver the response
```

### 5. AtomicUsize — Lock-Free Counter

The queue depth is tracked with an `AtomicUsize` — no lock needed for incrementing
or reading a single counter:

```rust
depth: Arc<AtomicUsize>,

// Increment on enqueue
self.depth.fetch_add(1, Ordering::SeqCst);

// Decrement on dequeue
self.depth.fetch_sub(1, Ordering::SeqCst);

// Read for Prometheus gauge
metrics::gauge!("nexus_queue_depth").set(self.depth() as f64);
```

### 6. saturating_sub — Underflow Protection

Rust integers panic on underflow in debug mode. `saturating_sub` prevents this by
clamping at zero:

```rust
let penalty = (score as f64 * penalty_ratio) as u32;
score.saturating_sub(penalty)  // Never goes below 0
```

### 7. serde(untagged) — Flexible Deserialization

The `#[serde(untagged)]` attribute tries each enum variant in order until one matches.
This lets `EmbeddingInput` accept both `"hello"` (string) and `["hello","world"]` (array):

```rust
#[serde(untagged)]
pub enum EmbeddingInput {
    Single(String),       // Tried first
    Batch(Vec<String>),   // Tried if Single fails
}
```

---

## Common Patterns in This Codebase

### 1. Background Loop + CancellationToken

Both the quality loop and the queue drain loop follow the same pattern:

```rust
pub async fn some_background_loop(
    state: Arc<SomeState>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            _ = interval.tick() => {
                // Do periodic work
            }
        }
    }
}
```

This pattern ensures clean shutdown: when the main process drops the
`CancellationToken`, both loops exit gracefully.

### 2. Reconciler Pipeline Pattern

Every reconciler follows the same interface:

```rust
impl Reconciler for MyReconciler {
    fn name(&self) -> &'static str { "MyReconciler" }
    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        // Filter intent.candidate_agents
        // Use intent.exclude_agent() to remove + record reason
        Ok(())
    }
}
```

### 3. Factory Helpers in Tests

Tests use `make_*` functions for setup consistency:

```rust
fn make_config(max_size: u32, max_wait: u64) -> QueueConfig { ... }
fn make_store() -> Arc<QualityMetricsStore> { ... }
fn make_queued(priority: Priority) -> (QueuedRequest, oneshot::Receiver) { ... }
fn make_intent() -> RoutingIntent { ... }
```

### 4. Actionable Error Responses

When all backends are excluded or the queue is full, the error response includes
structured information the operator can act on:

```rust
intent.exclude_agent(
    agent_id,
    "QualityReconciler",              // Which reconciler excluded it
    "Error rate 75% exceeds 50%",     // Human-readable reason
    "Wait for error rate to decrease", // Suggested action
);
```

---

## Next Steps

### Questions to Investigate

1. **How does the quality loop interact with the health checker?** The quality loop
   tracks request-level outcomes. The health checker probes backend connectivity.
   They are independent but complementary — a backend can be "healthy" (responds to
   pings) but have a high error rate (returns errors on real requests).

2. **What happens during startup?** All agents start with default quality metrics
   (healthy). The first 30 seconds have no quality data. This is intentional — new
   backends get a grace period.

3. **How does the queue drain loop scale?** The 50ms polling interval means up to
   20 drain attempts per second. At high load, this is sufficient because each
   iteration processes all available requests, not just one.

4. **How are embeddings different from completions in routing?** Embedding requests
   check `agent.profile().capabilities.embeddings` — most chat models don't support
   embeddings. This means the candidate pool for embeddings is usually smaller.

### Related Features to Explore

- **F13 (Privacy Zones)**: The PrivacyReconciler runs before QualityReconciler in
  the pipeline. Restricted-zone requests never reach cloud backends regardless of
  quality scores.

- **F14 (Budget Management)**: The BudgetReconciler runs before QualityReconciler.
  A backend can be healthy and fast but budget-excluded.

- **F15 (Speculative Router)**: The `prefers_streaming` field in `RequestRequirements`
  was added in Phase 2.5 to eventually enable stream-aware routing.
