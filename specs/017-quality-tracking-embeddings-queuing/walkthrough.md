# Walkthrough: Quality Tracking, Embeddings & Request Queuing (Phase 2.5)

> **Audience**: A junior developer joining the Nexus project.
> **Goal**: Understand what Phase 2.5 adds, how the pieces fit together, and where to find the code.

---

## What Did We Build?

Phase 2.5 adds three major capabilities to Nexus:

1. **Quality Tracking (F16)** — Nexus now *learns* which backends are reliable. If a backend starts returning errors or responding slowly, Nexus automatically routes traffic away from it.

2. **Embeddings API (F17)** — A new `POST /v1/embeddings` endpoint that lets you generate vector embeddings (for RAG, search, etc.) through the same Nexus gateway you use for chat.

3. **Request Queuing (F18)** — When all backends are busy, instead of immediately returning a 503 error, Nexus can queue the request and wait for a backend to become available.

These features build on the Reconciler Pipeline from Phase 2 (v0.3). Think of the pipeline as a series of filters that each request passes through:

```
Request → RequestAnalyzer → Privacy → Budget → Tier → Quality → Scheduler → Backend
```

Phase 2.5 made the **Quality** stage do real work (it was a pass-through stub before) and taught the **Scheduler** how to produce a "queue this request" decision.

---

## File-by-File Tour

### 1. Quality Metrics Storage

#### `src/agent/mod.rs` (lines 50–94) — The Data

```rust
pub struct AgentQualityMetrics {
    pub error_rate_1h: f32,              // 0.0 = perfect, 1.0 = all errors
    pub avg_ttft_ms: u32,                // Time To First Token (average)
    pub success_rate_24h: f32,           // Longer-term reliability signal
    pub last_failure_ts: Option<Instant>, // When the last error occurred
    pub request_count_1h: u32,           // How many requests in last hour
}
```

**Why these fields?** The error rate over 1 hour tells us if a backend is *currently* broken. The 24-hour success rate tells us if it's *generally* reliable. TTFT tells us if it's *slow*. Together, they let the router make informed decisions.

The `Default` implementation starts with healthy values (error_rate=0.0, success_rate=1.0) so brand-new backends aren't penalized.

#### `src/agent/quality.rs` (lines 29–90) — The Store

```rust
pub struct QualityMetricsStore {
    outcomes: DashMap<String, RwLock<VecDeque<RequestOutcome>>>,
}
```

This is where request outcomes (success/failure + TTFT) are recorded. Think of it as a per-agent logbook. The key methods:

- **`record_outcome(agent_id, success, ttft_ms)`** — Called after every request. Appends to the agent's history.
- **`get_metrics(agent_id)`** — Computes `AgentQualityMetrics` from the rolling history. If no history exists, returns healthy defaults.
- **`compute_metrics()`** — Background method that prunes old entries (>1h) and updates aggregates.

The data structure is a `VecDeque` (double-ended queue) per agent, which lets us efficiently push new entries at the back and pop expired ones from the front.

**Key test**: `src/agent/quality.rs` mod tests — verifies default metrics, outcome recording, and metric computation.

---

### 2. Configuration

#### `src/config/quality.rs` (lines 21–49) — Quality Config

```rust
pub struct QualityConfig {
    pub error_rate_threshold: f32,        // Default: 0.5 (50%)
    pub ttft_penalty_threshold_ms: u32,   // Default: 3000 (3 seconds)
    pub metrics_interval_seconds: u64,    // Default: 30
}
```

**What the defaults mean**: If a backend's error rate exceeds 50%, it's excluded from routing. If its average TTFT exceeds 3 seconds, its score gets penalized (but it's not excluded — just ranked lower). The quality loop runs every 30 seconds.

#### `src/config/queue.rs` (lines 20–58) — Queue Config

```rust
pub struct QueueConfig {
    pub enabled: bool,      // Default: true
    pub max_size: u32,      // Default: 100
    pub max_wait_seconds: u64, // Default: 30
}
```

Notable: `is_enabled()` returns `self.enabled && self.max_size > 0`. Setting `max_size = 0` is a backdoor to disable queuing even when `enabled = true`.

Both configs are added to `NexusConfig` and parsed from `[quality]` and `[queue]` TOML sections. If these sections are missing from the config file, defaults are used — zero configuration.

---

### 3. The QualityReconciler

#### `src/routing/reconciler/quality.rs` (lines 23–70) — The Brain

This was a pass-through stub in v0.3. Now it does real filtering:

```rust
fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
    // For each candidate agent...
    intent.candidate_agents.retain(|agent_id| {
        let metrics = self.store.get_metrics(agent_id);
        
        // If error rate exceeds threshold, exclude this agent
        if metrics.error_rate_1h >= self.config.error_rate_threshold {
            intent.rejection_reasons.push(format!(
                "Agent {} excluded: error rate {:.1}% exceeds {:.1}%",
                agent_id, metrics.error_rate_1h * 100.0,
                self.config.error_rate_threshold * 100.0
            ));
            return false; // Remove from candidates
        }
        true // Keep this candidate
    });
    Ok(())
}
```

**What happens when ALL candidates are excluded?** The pipeline continues to the SchedulerReconciler with an empty candidate list, which produces a 503 response. The `rejection_reasons` are included in the error response so the operator knows *why* no backend was available — not just "503 Service Unavailable" but "Agent gpu-box-1 excluded: error rate 75.0% exceeds 50.0%".

**Key tests** (lines 100–220):
- `excludes_high_error_rate_agents` — Agent at 80% error rate gets filtered out
- `all_excluded_produces_rejection_reasons` — When both candidates are bad, rejection reasons are populated
- `preserves_healthy_agents` — Agent at 10% error rate passes through
- `fresh_agent_passes` — Agent with no history (default metrics) is not penalized

---

### 4. TTFT Penalty in SchedulerReconciler

#### `src/routing/reconciler/scheduler.rs` (lines 109–125)

The Scheduler doesn't *exclude* slow backends — it just ranks them lower:

```rust
fn apply_ttft_penalty(&self, score: u32, agent_id: &str) -> u32 {
    let metrics = self.quality_store.get_metrics(agent_id);
    let threshold = self.quality_config.ttft_penalty_threshold_ms;
    
    if threshold == 0 || metrics.avg_ttft_ms <= threshold {
        return score; // No penalty
    }
    
    // Proportional penalty: further above threshold = bigger reduction
    let excess = metrics.avg_ttft_ms - threshold;
    let penalty_ratio = (excess as f64 / threshold as f64).min(1.0);
    let penalty = (score as f64 * penalty_ratio) as u32;
    score.saturating_sub(penalty)
}
```

**Example**: Threshold is 3000ms. Agent A has avg_ttft=5000ms (excess=2000ms, penalty_ratio=0.67). If its base score was 100, it becomes 100 - 67 = 33. Agent B with avg_ttft=1000ms keeps its full score of 100. Result: Agent B is strongly preferred.

The `saturating_sub` ensures the score never goes below 0 (no integer underflow).

**Key test**: `ttft_penalty_reduces_score` — verifies high-TTFT agent scores lower than healthy agent.

---

### 5. Enhanced Request Analysis

#### `src/routing/requirements.rs` (lines 6–77)

One small but useful addition:

```rust
pub struct RequestRequirements {
    pub model: String,
    pub estimated_tokens: u32,
    pub needs_vision: bool,
    pub needs_tools: bool,
    pub needs_json_mode: bool,
    pub prefers_streaming: bool,  // ← NEW in Phase 2.5
}
```

Set from `request.stream` (line 69). This signal can be used by future reconcilers to prefer backends that are optimized for streaming responses.

---

### 6. Embeddings API

#### `src/api/embeddings.rs` — The New Endpoint

**Types** (lines 35–65):

```rust
pub struct EmbeddingRequest {
    pub model: String,
    pub input: EmbeddingInput,       // Single string or array of strings
    pub encoding_format: Option<String>,
}

pub struct EmbeddingResponse {
    pub object: String,              // Always "list"
    pub data: Vec<EmbeddingObject>,  // One per input string
    pub model: String,
    pub usage: EmbeddingUsage,
}
```

These match the [OpenAI Embeddings API](https://platform.openai.com/docs/api-reference/embeddings) format exactly.

**Handler** (lines 71+):

The handler follows the same pattern as `/v1/chat/completions`:
1. Parse the request body
2. Build `RequestRequirements` (model name, estimated tokens)
3. Route through the reconciler pipeline via `router.select_backend()`
4. Call `agent.embeddings()` on the selected backend
5. Return the response with `X-Nexus-*` headers

**Route registration** in `src/api/mod.rs` (line 190):
```rust
.route("/v1/embeddings", post(embeddings::handle))
```

**Agent implementations**:
- `src/agent/ollama.rs` — Uses Ollama's `/api/embed` endpoint
- `src/agent/openai.rs` — Forwards to OpenAI's `/v1/embeddings` with API key auth

**Key tests** (in `src/api/embeddings.rs` mod tests and `tests/embeddings_test.rs`):
- Type serialization/deserialization (single input, batch input)
- Route existence and HTTP method validation
- Response format compliance

---

### 7. Request Queue

#### `src/queue/mod.rs` — The Queue System

This is the most complex new module. Let's break it down:

**QueuedRequest** (lines 33–45):
```rust
pub struct QueuedRequest {
    pub intent: RoutingIntent,
    pub request: ChatCompletionRequest,
    pub response_tx: oneshot::Sender<QueuedResponse>,
    pub enqueued_at: Instant,
    pub priority: QueuePriority, // High or Normal
}
```

Each queued request carries a `oneshot::Sender` — this is how the drain task sends the response back to the original HTTP handler that's waiting for it. Think of it as a "callback channel".

**RequestQueue** (lines 74–170):

```rust
pub struct RequestQueue {
    high_tx: mpsc::Sender<QueuedRequest>,     // High-priority channel
    high_rx: Mutex<mpsc::Receiver<QueuedRequest>>,
    normal_tx: mpsc::Sender<QueuedRequest>,   // Normal-priority channel
    normal_rx: Mutex<mpsc::Receiver<QueuedRequest>>,
    depth: Arc<AtomicUsize>,                  // Shared counter
    config: QueueConfig,
}
```

**Why two channels?** Priority. The `try_dequeue()` method (line 137) always checks the high-priority channel first. Requests with the `X-Nexus-Priority: high` header go into the high channel; everything else goes into normal.

**Key operations**:
- `enqueue()` — Checks if queue is enabled, not full, then sends to the right channel. Updates `nexus_queue_depth` Prometheus gauge.
- `try_dequeue()` — Tries high channel first, then normal. Decrements depth.
- `depth()` — Returns current queue depth (atomic read, O(1)).

**The Drain Loop** (lines 177+) — `queue_drain_loop()`:

This is a background tokio task that runs forever (until shutdown):

```
loop {
    1. Dequeue a request
    2. Check if it has timed out (enqueued_at + max_wait > now)
       → If yes: send 503 with Retry-After header via the oneshot channel
    3. Try to route it: run the reconciler pipeline again
       → If a backend is now available: process the request, send response
       → If still no capacity: re-enqueue (if not timed out)
    4. Sleep 50ms and repeat
}
```

The drain loop also handles graceful shutdown: when the `CancellationToken` is cancelled, it drains remaining requests with 503 errors.

**Key tests** (`tests/queue_test.rs` and `src/queue/mod.rs` mod tests):
- FIFO ordering within priority level
- High-priority requests drain before normal
- Queue rejects when full
- Queue disabled when `max_size = 0`
- Timeout produces 503

---

### 8. Completions Handler Integration

#### `src/api/completions.rs` — Tying It All Together

**Outcome Recording** (line 1094+):

After every request completes (success or failure), `record_request_completion()` is called:
- Records to the quality metrics store (line 524, 655)
- Updates backend request counters
- Pushes to request history for the dashboard
- Broadcasts WebSocket update

**Queue Handling** (lines 430–470):

When the router returns a "queue" decision:
```rust
// 1. Create a oneshot channel for the response
let (tx, rx) = oneshot::channel();

// 2. Enqueue the request
queue.enqueue(QueuedRequest { ..., response_tx: tx }).await?;

// 3. Wait for the drain task to process it (with timeout)
match timeout(Duration::from_secs(max_wait), rx).await {
    Ok(Ok(response)) => // Return the response to the client
    Ok(Err(_)) => // Channel closed (drain task error)
    Err(_) => // Timeout: return 503 with Retry-After
}
```

**Error handling** (lines 454–462):
- `QueueError::Full` → 503 "Queue is full"
- `QueueError::Disabled` → 503 "Queuing is disabled"

---

### 9. Metrics

#### `src/metrics/mod.rs` — Prometheus Instrumentation

New metrics added:
- **`nexus_agent_ttft_seconds`** (histogram) — Time to first token per agent
- **`nexus_queue_depth`** (gauge) — Current number of queued requests

These are updated:
- TTFT: recorded on each request completion
- Queue depth: updated on every `enqueue()` and `try_dequeue()` call

The quality metrics (error rate, success rate) are published through the `/v1/stats` JSON endpoint rather than as individual Prometheus gauges, since they're computed from rolling windows.

---

### 10. Configuration Example

#### `nexus.example.toml` — New Sections

```toml
[quality]
error_rate_threshold = 0.5
ttft_penalty_threshold_ms = 3000
metrics_interval_seconds = 30

[queue]
enabled = true
max_size = 100
max_wait_seconds = 30
```

Both sections are optional. If omitted, the defaults above are used.

---

## How the Pieces Connect

Here's the flow for a typical chat completion request after Phase 2.5:

```
Client sends POST /v1/chat/completions
     │
     ▼
[RequestAnalyzer] Extract model, tokens, vision/tools/streaming needs
     │
     ▼
[PrivacyReconciler] Filter by privacy zone (local vs cloud)
     │
     ▼
[BudgetReconciler] Check cloud spending limits
     │
     ▼
[TierReconciler] Filter by capability tier
     │
     ▼
[QualityReconciler] ← NEW: Exclude agents with high error rates
     │
     ▼
[SchedulerReconciler] ← ENHANCED: Score with TTFT penalty, may return Queue
     │
     ├── Route → Forward to selected backend
     │           ├── Success → Record outcome (success=true, ttft_ms)
     │           └── Failure → Record outcome (success=false), try retry
     │
     ├── Queue → Enqueue request, wait for drain task
     │           └── Drain task re-runs pipeline when capacity frees
     │
     └── Reject → Return 503 with rejection_reasons
```

The quality loop runs independently every 30 seconds, pruning old data and computing fresh aggregates. This means the QualityReconciler always has recent (within 30s) data to work with.

---

## Key Design Decisions

1. **RwLock over Mutex for quality metrics** — Multiple requests read metrics simultaneously during routing; only the background loop writes. RwLock allows concurrent reads.

2. **Dual-channel queue over single sorted queue** — Simpler than maintaining a priority-sorted data structure. Two FIFO channels with "check high first" achieves the same result with less code.

3. **50ms drain loop interval** — Balances responsiveness (requests wait at most 50ms extra) against CPU usage (not spinning as fast as possible).

4. **Proportional TTFT penalty over hard cutoff** — A backend at 3001ms TTFT shouldn't be treated the same as one at 10000ms. The proportional penalty provides a gradient.

5. **Rolling window with VecDeque over fixed-size ring buffer** — VecDeque handles variable request rates naturally. At 100 req/min, each agent stores ~6000 entries per hour (~50KB) which is acceptable.

---

## Testing Strategy

| Area | Test Location | What's Tested |
|------|--------------|---------------|
| Quality metrics | `src/agent/quality.rs` mod tests | Default values, outcome recording, metric computation |
| Quality reconciler | `src/routing/reconciler/quality.rs` mod tests | Filtering, all-excluded edge case, fresh agents |
| TTFT penalty | `src/routing/reconciler/scheduler.rs` mod tests | Score reduction, threshold boundary |
| Embedding types | `src/api/embeddings.rs` mod tests | Serde round-trip, single vs batch input |
| Embedding endpoint | `tests/embeddings_test.rs` | Route existence, response format |
| Queue operations | `src/queue/mod.rs` mod tests | FIFO, priority, capacity, disabled state |
| Queue integration | `tests/queue_test.rs` | Concurrent requests, capacity overflow |
| Config parsing | `src/config/quality.rs`, `queue.rs` tests | Defaults, TOML parsing |

---

## Common Tasks for New Developers

### "I want to adjust when a backend is excluded"
→ Change `error_rate_threshold` in `[quality]` config (or the default in `src/config/quality.rs`).

### "I want to add a new quality signal"
→ Add a field to `AgentQualityMetrics`, update `compute_metrics()` in `quality.rs`, then use it in `QualityReconciler::reconcile()` or `SchedulerReconciler::apply_ttft_penalty()`.

### "I want to support embeddings for a new backend type"
→ Override `fn embeddings()` in the agent's implementation (e.g., `src/agent/anthropic.rs`). The routing and API handler work automatically.

### "I want to change queue priority levels"
→ Modify `QueuePriority` enum in `src/queue/mod.rs` and add a new mpsc channel pair in `RequestQueue`. Update `X-Nexus-Priority` header parsing in `src/api/completions.rs`.
