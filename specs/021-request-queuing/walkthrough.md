# F18: Request Queuing & Prioritization — Code Walkthrough

> A junior-developer-friendly guide to how request queuing works in Nexus.

## 1. Architecture Overview

When all backends are at capacity, Nexus queues incoming requests instead of immediately returning 503. The queue is a bounded, dual-priority FIFO backed by two tokio `mpsc` channels (one for high-priority, one for normal). A background drain loop polls every 50ms, re-runs routing for dequeued requests, and sends responses back through oneshot channels.

```
Client  ──POST /v1/chat/completions──▶  Handler  ──Router──▶  RoutingError::Queue
                                       (completions.rs)              │
                                             │                       ▼
                                             │◀── oneshot ── RequestQueue (dual mpsc)
                                             │                       ▲
                                             │              queue_drain_loop (50ms poll)
                                             │                       │
                                             ▼              re-runs select_backend()
                                        Response to client           │
                                                            Agent ──▶ Backend
```

**Key architectural concepts:**

- **Dual-priority FIFO:** Two separate `mpsc` channels (high + normal). High-priority channel is always drained first. Within each channel, ordering is FIFO.
- **Lock-free depth tracking:** An `AtomicUsize` tracks total depth across both channels — no mutex needed for the critical enqueue path.
- **Oneshot response channel:** Each queued request carries a `oneshot::Sender`. The handler `await`s the corresponding `Receiver`. When the drain loop processes the request, it sends the response through the oneshot, unblocking the handler.
- **Graceful shutdown:** The drain loop listens on a `CancellationToken`. On shutdown, it drains all remaining requests with 503 responses.

## 2. Request Flow

Here's what happens step-by-step when all backends are saturated:

```
┌──────────────────────────────────────────────────────────────────────┐
│  1. Handler calls Router::select_backend()                          │
│  2. Reconciler pipeline returns RoutingDecision::Queue              │
│  3. Router converts to RoutingError::Queue { reason, est_wait_ms } │
│  4. Handler catches RoutingError::Queue in match arm                │
│  5. Extract priority from X-Nexus-Priority header (default: Normal) │
│  6. Create QueuedRequest with oneshot::channel()                    │
│  7. queue.enqueue(queued) — atomic depth check + try_send           │
│  8. Handler awaits oneshot Receiver (with tokio::time::timeout)     │
│  ─── meanwhile, in the background ───                               │
│  9. queue_drain_loop polls every 50ms                               │
│ 10. try_dequeue() checks high channel first, then normal            │
│ 11. Check timeout: if elapsed > max_wait → 503 + Retry-After       │
│ 12. Re-run select_backend() for the dequeued request                │
│ 13. If routed: forward to agent, send response via oneshot          │
│ 14. If still no capacity: re-enqueue (preserving original timestamp)│
│ 15. Handler receives response through oneshot, returns to client    │
└──────────────────────────────────────────────────────────────────────┘
```

### Error cases

| Scenario | What happens | HTTP Status |
|----------|-------------|-------------|
| Queue disabled (`enabled=false` or `max_size=0`) | Immediate rejection | 503 |
| Queue full (depth ≥ `max_size`) | Immediate rejection: "queue is full" | 503 |
| Request times out in queue | 503 with `Retry-After` header | 503 |
| Shutdown while requests queued | All drained with 503 | 503 |
| Streaming request (`stream: true`) | Queuing skipped, immediate 503 | 503 |

## 3. Key Files

### `src/queue/mod.rs` — Core Queue (595 lines)

This is the heart of the feature. It defines the queue data structure, the drain loop, and all unit tests.

**Types:**

```rust
/// Two priority levels, parsed from X-Nexus-Priority header
pub enum Priority {
    High,
    Normal,
}

/// A request waiting in the queue
pub struct QueuedRequest {
    pub intent: RoutingIntent,           // Routing metadata from reconciler
    pub request: ChatCompletionRequest,  // Original request body
    pub response_tx: oneshot::Sender<QueueResponse>,  // Response channel
    pub enqueued_at: Instant,            // For timeout detection
    pub priority: Priority,
}

/// Errors from queue operations
pub enum QueueError {
    Full { max_size: u32 },  // Queue at capacity
    Disabled,                // Queuing is off
}
```

**`RequestQueue` — the dual-channel bounded queue:**

```rust
pub struct RequestQueue {
    high_tx: mpsc::Sender<QueuedRequest>,
    high_rx: tokio::sync::Mutex<mpsc::Receiver<QueuedRequest>>,
    normal_tx: mpsc::Sender<QueuedRequest>,
    normal_rx: tokio::sync::Mutex<mpsc::Receiver<QueuedRequest>>,
    depth: Arc<AtomicUsize>,  // lock-free total depth
    config: QueueConfig,
}
```

Both channels are created with capacity equal to `max_size`. The total depth is enforced atomically in `enqueue()` — the per-channel capacities are a safety net, not the primary bound.

**`enqueue()` — the hot path:**

```rust
pub fn enqueue(&self, request: QueuedRequest) -> Result<(), QueueError> {
    if !self.config.is_enabled() { return Err(QueueError::Disabled); }

    let current = self.depth.load(Ordering::SeqCst);
    if current >= self.config.max_size as usize {
        return Err(QueueError::Full { max_size: self.config.max_size });
    }

    self.depth.fetch_add(1, Ordering::SeqCst);
    // Route to correct channel by priority
    let tx = match request.priority {
        Priority::High => &self.high_tx,
        Priority::Normal => &self.normal_tx,
    };
    // try_send is non-blocking; rollback depth on failure
    if tx.try_send(request).is_err() {
        self.depth.fetch_sub(1, Ordering::SeqCst);
        return Err(QueueError::Full { max_size: self.config.max_size });
    }
    Ok(())
}
```

Note: There's a small TOCTOU race between `depth.load()` and `fetch_add()` — this is acceptable because the per-channel capacity provides a hard backstop.

**`try_dequeue()` — priority ordering:**

```rust
pub async fn try_dequeue(&self) -> Option<QueuedRequest> {
    // Try high priority first
    { let mut rx = self.high_rx.lock().await; /* try_recv */ }
    // Then normal priority
    { let mut rx = self.normal_rx.lock().await; /* try_recv */ }
    None
}
```

The `Mutex<Receiver>` is required because `mpsc::Receiver::try_recv()` takes `&mut self`. Since only the drain loop calls `try_dequeue()`, contention is zero in practice.

**`queue_drain_loop()` — the background processor:**

The drain loop runs as a `tokio::spawn` task. It polls every 50ms using `tokio::select!` with the cancellation token:

1. **Dequeue** — tries all available requests per poll cycle (inner `while let`)
2. **Timeout check** — if `enqueued_at.elapsed() > max_wait`, sends 503 + `Retry-After`
3. **Re-route** — calls `state.router.select_backend()` with `TierEnforcementMode::Strict`
4. **Success** — forwards to `process_queued_request()`, sends response via oneshot
5. **No capacity** — re-enqueues with original `enqueued_at` (preserves timeout deadline)
6. **Shutdown** — `drain_remaining()` sends 503 to all queued requests

**`process_queued_request()` — forwarding to backend:**

```rust
async fn process_queued_request(state, routing_result, request) -> QueueResponse {
    let backend = &routing_result.backend;
    state.registry.increment_pending(&backend.id);

    let result = if let Some(agent) = state.registry.get_agent(&backend.id) {
        agent.chat_completion(request, None).await
    } else {
        Err(ApiError::bad_gateway("Agent not found"))
    };

    state.registry.decrement_pending(&backend.id);
    // Convert to Response
}
```

This mirrors the normal completion handler flow — increment pending, call agent, decrement pending.

### `src/config/queue.rs` — Configuration (58 lines)

Three fields with serde defaults:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QueueConfig {
    pub enabled: bool,           // default: true
    pub max_size: u32,           // default: 100
    pub max_wait_seconds: u64,   // default: 30
}
```

`is_enabled()` returns `false` when either `enabled == false` or `max_size == 0`. This dual-check prevents a confusing state where queuing is "enabled" but has zero capacity.

**Configuration in `nexus.example.toml`:**

```toml
[queue]
enabled = true
max_size = 100
max_wait_seconds = 30
```

### `src/routing/reconciler/decision.rs` — RoutingDecision::Queue

The reconciler pipeline can produce three outcomes:

```rust
pub enum RoutingDecision {
    Route { agent_id, model, reason, cost_estimate },
    Queue { reason, estimated_wait_ms, fallback_agent },  // ← triggers queuing
    Reject { rejection_reasons },
}
```

When the `SchedulerReconciler` determines all candidates are at capacity, it returns `Queue`. The `Router::select_backend()` method converts this to `RoutingError::Queue`, which the handler catches.

### `src/api/completions.rs` — Handler Integration (lines ~409-470)

The handler catches `RoutingError::Queue` and interacts with the queue:

```rust
crate::routing::RoutingError::Queue { reason, estimated_wait_ms } => {
    if let Some(ref queue) = state.queue {
        let priority = extract_priority(&headers);  // X-Nexus-Priority
        let (tx, rx) = tokio::sync::oneshot::channel();
        let queued = QueuedRequest { intent, request, response_tx: tx, ... };

        match queue.enqueue(queued) {
            Ok(()) => {
                // Wait for drain loop to process, with timeout
                match tokio::time::timeout(max_wait, rx).await {
                    Ok(Ok(resp)) => return resp,
                    _ => return Ok(build_timeout_response(...)),
                }
            }
            Err(QueueError::Full { .. }) => { /* 503: queue full */ }
            Err(QueueError::Disabled) => { /* 503: at capacity */ }
        }
    } else {
        // No queue configured — immediate 503
    }
}
```

**Priority extraction** (line 75):

```rust
fn extract_priority(headers: &HeaderMap) -> Priority {
    headers.get("x-nexus-priority")
        .and_then(|v| v.to_str().ok())
        .map(Priority::from_header)
        .unwrap_or(Priority::Normal)
}
```

`Priority::from_header()` is case-insensitive — `"high"`, `"HIGH"`, `" High "` all map to `High`. Everything else defaults to `Normal`.

**Streaming requests** are not queued (line ~875) — they get an immediate 503 because queuing doesn't make sense for SSE streams.

### `src/cli/serve.rs` — Startup & Shutdown

**Queue creation** (line ~224):

```rust
let queue = if config.queue.is_enabled() {
    Some(Arc::new(RequestQueue::new(config.queue.clone())))
} else { None };
```

The queue is passed into `AppState` and also used to enable queue-aware routing on the `Router`:

```rust
app_state.queue = queue;
if config.queue.is_enabled() {
    router.set_queue_enabled(true);
}
```

**Drain loop startup** (line ~288):

```rust
let queue_handle = if let Some(ref q) = queue {
    let queue_clone = Arc::clone(q);
    let queue_cancel = cancel_token.clone();
    Some(tokio::spawn(async move {
        queue_drain_loop(queue_clone, state_clone, queue_cancel).await;
    }))
} else { None };
```

**Graceful shutdown** (line ~330):

```rust
if let Some(handle) = queue_handle {
    handle.await?;  // waits for drain_remaining() to finish
}
```

The `CancellationToken` triggers `drain_remaining()` inside the drain loop, which sends 503 to all in-flight requests before the task exits.

## 4. Design Decisions

### Why two mpsc channels instead of a priority queue?

A `BinaryHeap` or similar priority queue would require a `Mutex` for every enqueue/dequeue. Using two `mpsc` channels gives us:

- **Lock-free enqueue** — `try_send()` is non-blocking
- **Natural priority** — always drain `high_rx` before `normal_rx`
- **Bounded by construction** — tokio channels enforce capacity limits
- **Simple implementation** — no custom comparator or heap rebalancing

The trade-off is that within a priority level, we get FIFO (not arbitrary ordering), which is exactly what we want.

### Why AtomicUsize for depth?

The queue needs to enforce a total capacity across both channels. An `AtomicUsize` lets us check and update depth without holding a lock. The `SeqCst` ordering ensures visibility across the enqueue path and the drain loop.

### Why re-enqueue on routing failure?

When the drain loop dequeues a request but `select_backend()` still fails, the request goes back into the queue rather than being dropped. This is because capacity may free up on the next poll cycle (50ms later). The original `enqueued_at` timestamp is preserved so the timeout deadline doesn't reset.

### Why 50ms poll interval?

This balances latency against CPU usage. At 50ms:
- Worst-case added latency: 50ms (acceptable for inference requests that take seconds)
- CPU overhead: negligible (~20 wakeups/sec when idle)
- Response time: requests are served within one poll cycle of capacity becoming available

### Why not queue streaming requests?

Streaming (`stream: true`) uses Server-Sent Events. The HTTP response must begin immediately to start the SSE connection. Queuing would mean holding the connection open with no output, which breaks SSE client expectations. Instead, streaming requests get an immediate 503.

### Why Retry-After header?

The `Retry-After` header (set to `max_wait_seconds`) tells well-behaved HTTP clients exactly how long to wait before retrying. This prevents thundering-herd retries and follows HTTP/1.1 semantics for 503 responses.

## 5. Metrics

The queue emits a single Prometheus gauge:

```rust
metrics::gauge!("nexus_queue_depth").set(self.depth() as f64);
```

This is updated on every `enqueue()` and `try_dequeue()` call, giving real-time visibility into queue depth.

## 6. Testing

### Unit Tests (13 tests in `src/queue/mod.rs`)

| Test | What it verifies |
|------|-----------------|
| `fifo_ordering_normal_priority` | Normal-priority requests dequeue in FIFO order |
| `capacity_limits_reject_when_full` | Enqueue fails with `QueueError::Full` at capacity |
| `priority_ordering_high_drains_first` | High-priority dequeues before normal regardless of enqueue order |
| `depth_accuracy` | `depth()` accurately reflects enqueue/dequeue operations |
| `max_size_zero_rejects_immediately` | `max_size=0` treated as disabled → `QueueError::Disabled` |
| `disabled_queue_rejects` | `enabled=false` → `QueueError::Disabled` |
| `empty_dequeue_returns_none` | `try_dequeue()` on empty queue returns `None` |
| `timeout_response_has_retry_after` | 503 response includes `Retry-After` header |
| `enqueued_request_timeout_detection` | Elapsed time comparison correctly detects timeouts |
| `timeout_completes_within_time_limit` | Timeout detection + oneshot response is fast |
| `priority_from_header_high` | `"high"`, `"HIGH"`, `" High "` → `Priority::High` |
| `priority_from_header_normal` | `"normal"`, `"NORMAL"` → `Priority::Normal` |
| `priority_from_header_invalid_defaults_to_normal` | `""`, `"urgent"`, `"low"` → `Priority::Normal` |

### Integration Tests (2 tests in `tests/queue_test.rs`)

| Test | What it verifies |
|------|-----------------|
| `queue_accepts_up_to_capacity_and_rejects_overflow` | N requests accepted, N+1 rejected with `Full`, all N drain correctly |
| `queue_drains_high_priority_first_integration` | Mixed-priority enqueue drains high first, then normal in FIFO |

### Running the tests

```bash
# All queue tests (unit + integration)
cargo test queue

# Unit tests only
cargo test queue::tests

# Integration tests only
cargo test --test queue_test

# Single test
cargo test priority_ordering_high_drains_first
```

## 7. Common Modifications

### Changing the poll interval

The 50ms interval is hardcoded in `queue_drain_loop()`:

```rust
_ = tokio::time::sleep(Duration::from_millis(50)) => {
```

To make it configurable, add a `poll_interval_ms` field to `QueueConfig` and pass it through.

### Adding a new priority level

To add e.g. `Priority::Low`:

1. Add the variant to `enum Priority` in `src/queue/mod.rs`
2. Add a third `mpsc` channel pair in `RequestQueue::new()`
3. Update `enqueue()` to route to the correct channel
4. Update `try_dequeue()` to check high → normal → low
5. Update `Priority::from_header()` to parse `"low"`

### Monitoring queue health

Query the `nexus_queue_depth` Prometheus gauge at `/metrics`. A sustained non-zero depth indicates backends are saturated. Combine with `nexus_backend_pending_requests` to correlate queue depth with backend load.
