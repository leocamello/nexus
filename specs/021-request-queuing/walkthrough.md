# F18: Request Queuing & Prioritization — Code Walkthrough

**Feature**: Request Queuing & Prioritization (F18)  
**Audience**: Junior developers joining the project  
**Last Updated**: 2025-02-17

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: queue/mod.rs — The Waiting Room](#file-1-queuemodrs--the-waiting-room)
4. [File 2: config/queue.rs — The Admission Policy](#file-2-configqueuers--the-admission-policy)
5. [File 3: routing/reconciler/decision.rs — The Triage Tag](#file-3-routingreconcilerdecisionrs--the-triage-tag)
6. [File 4: api/completions.rs — The Intake Desk](#file-4-apicompletionsrs--the-intake-desk)
7. [File 5: cli/serve.rs — The Hospital Opening & Closing](#file-5-cliservers--the-hospital-opening--closing)
8. [Understanding the Tests](#understanding-the-tests)
9. [Key Rust Concepts](#key-rust-concepts)
10. [Common Patterns in This Codebase](#common-patterns-in-this-codebase)
11. [Next Steps](#next-steps)

---

## The Big Picture

Imagine Nexus is a **hospital emergency room**. Patients (requests) arrive at the front desk. Doctors (backends) treat them. Normally there are enough doctors for everyone, and patients go straight to treatment. But during a flu outbreak (burst traffic), every doctor is busy. Without a waiting room, the hospital would turn people away at the door — "come back later."

That's what F18 adds: a **waiting room** for requests. When all backends are saturated, instead of immediately returning a 503 ("go away"), Nexus seats the request in a bounded waiting room with two sections — an **urgent care lane** (high priority) and a **general waiting area** (normal priority). A **triage nurse** (the drain loop) checks every 50ms whether a doctor has become available, and sends the next patient in.

### What Problem Does This Solve?

Without F18, a momentary spike — say 20 requests hitting 5 busy backends simultaneously — would cause 15 immediate 503 failures. But backends finish requests constantly. If we just wait 200ms, three backends might free up. F18 smooths over these burst gaps, turning transient overload into a brief wait instead of a hard failure.

F18 does **not** solve sustained overload. If backends are permanently saturated, requests will eventually time out in the queue and get a 503 with a `Retry-After` header. The queue is a **shock absorber**, not infinite capacity.

### How F18 Fits Into Nexus

```
┌──────────────────────────────────────────────────────────────────────────┐
│                              Nexus                                      │
│                                                                         │
│  Client Request                                                         │
│    │  POST /v1/chat/completions                                         │
│    │  X-Nexus-Priority: high                                            │
│    ▼                                                                    │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │  ① Handler (api/completions.rs)                                 │    │
│  │     Calls Router::select_backend()                              │    │
│  │     Router returns RoutingError::Queue                          │    │
│  └──┼──────────────────────────────────────────────────────────────┘    │
│     │                                                                   │
│     ▼                                                                   │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │  ② Intake Desk (api/completions.rs)                ◄── F18     │    │
│  │     • Extract priority from X-Nexus-Priority header             │    │
│  │     • Create oneshot channel for response                       │    │
│  │     • queue.enqueue(request) — CAS loop + try_send              │    │
│  │     • Await oneshot (with tokio::time::timeout)                 │    │
│  └──┼──────────────────────────────────────────────────────────────┘    │
│     │                                                                   │
│     ▼                                                                   │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │  ③ Waiting Room (queue/mod.rs)                     ◄── F18     │    │
│  │                                                                 │    │
│  │     ┌────────────────────┐  ┌──────────────────────┐           │    │
│  │     │  HIGH-PRIORITY     │  │  NORMAL-PRIORITY     │           │    │
│  │     │  mpsc channel      │  │  mpsc channel        │           │    │
│  │     │  (urgent care)     │  │  (general waiting)   │           │    │
│  │     └────────┬───────────┘  └──────────┬───────────┘           │    │
│  │              │                         │                       │    │
│  │              ▼                         ▼                       │    │
│  │     AtomicUsize depth (total across both channels)             │    │
│  └──┼──────────────────────────────────────────────────────────────┘    │
│     │                                                                   │
│     ▼                                                                   │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │  ④ Triage Nurse — queue_drain_loop (50ms poll)     ◄── F18     │    │
│  │     • Dequeue: high channel first, then normal                  │    │
│  │     • Timeout check: elapsed > max_wait → 503 + Retry-After    │    │
│  │     • Re-run select_backend() for dequeued request              │    │
│  │     • Success → forward to agent → send response via oneshot    │    │
│  │     • No capacity → re-enqueue (preserving original timestamp)  │    │
│  │     • Shutdown → drain all remaining with 503                   │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                                                         │
│  Data Flow: RoutingError::Queue → extract_priority(headers)            │
│             → RequestQueue.enqueue() (CAS loop)                        │
│             → queue_drain_loop (50ms poll)                              │
│             → select_backend() retry → agent.chat_completion()         │
│             → oneshot response → client                                │
└──────────────────────────────────────────────────────────────────────────┘
```

### Key Design Decisions

| Decision | Why |
|----------|-----|
| Two `mpsc` channels instead of a `BinaryHeap` | Lock-free `try_send()` on enqueue; no custom comparator or heap rebalancing. Priority is structural (drain high first), not per-element |
| `AtomicUsize` for depth, not `Mutex<usize>` | The enqueue path is latency-sensitive. Atomics avoid lock contention entirely. CAS loop prevents TOCTOU race |
| CAS loop in `enqueue()` (not `load`+`fetch_add`) | A simple `load(); check; fetch_add()` has a TOCTOU race under concurrency — two threads could both see `depth=99`, both increment to 101, exceeding `max_size=100`. The `compare_exchange` loop eliminates this |
| 50ms poll interval | Balances latency (acceptable for inference taking seconds) against CPU (~20 wakeups/sec). Not configurable yet — hardcoded for simplicity |
| Re-enqueue on routing failure | Capacity may free up 50ms later. Preserves original `enqueued_at` so the timeout deadline doesn't reset |
| Don't queue streaming requests | SSE connections must begin immediately. Holding the connection open with no output breaks client expectations |
| `Retry-After` header on timeout | HTTP/1.1 semantics for 503. Tells well-behaved clients exactly when to retry, preventing thundering-herd retries |
| Dual timeout (handler + drain) | Both sides check expiry independently — the handler via `tokio::time::timeout` on the oneshot, the drain loop via `enqueued_at.elapsed()`. A request can time out on either side |

---

## File Structure

```
src/
├── queue/
│   └── mod.rs                       ← F18: RequestQueue, drain loop, all types (632 lines, 14 tests)
├── config/
│   └── queue.rs                     ← F18: QueueConfig struct (58 lines, 0 tests)
├── routing/
│   └── reconciler/
│       └── decision.rs              ← F18: RoutingDecision::Queue variant (42 lines, 0 tests)
├── api/
│   └── completions.rs               ← F18: Queue integration in handler (lines 409–472)
├── cli/
│   └── serve.rs                     ← F18: Queue startup & shutdown (lines 224–333)

tests/
└── queue_test.rs                    ← F18: Integration tests (169 lines, 2 tests)
```

**F18 Contribution**: 1 new file (`queue/mod.rs`), 1 new config file (`config/queue.rs`), 1 new variant (`RoutingDecision::Queue`), 2 modified files (`completions.rs`, `serve.rs`), 1 integration test file. ~900 lines added, 14 unit tests, 2 integration tests.

---

## File 1: queue/mod.rs — The Waiting Room

**Purpose**: The core waiting room — data structures, enqueue/dequeue logic, the background drain loop, and all unit tests.  
**Lines**: 632  |  **Tests**: 14  |  **Status**: NEW

### Why Does This Exist?

When `Router::select_backend()` returns `RoutingError::Queue`, the request needs somewhere to wait. This file provides the bounded, dual-priority queue and the background loop that periodically retries routing for waiting requests. Think of it as the hospital's waiting room combined with the triage nurse who keeps checking if a doctor is free.

### The Types (Priority, QueuedRequest, QueueError)

```rust
// src/queue/mod.rs, lines 16–68

/// Priority level for queued requests
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    High,
    Normal,
}

impl Priority {
    /// Parse priority from header value. Invalid values default to Normal.
    pub fn from_header(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "high" => Priority::High,
            _ => Priority::Normal,
        }
    }
}
```

Only two levels — `High` and `Normal`. In the hospital metaphor, this is like having an "urgent care" lane and a "general" lane. The parser is deliberately permissive: only the exact string `"high"` (case-insensitive, trimmed) gets priority treatment. Everything else — `"normal"`, `"urgent"`, `"low"`, `""`, garbage — maps to `Normal`. This is a safe default: no one accidentally gets priority.

```rust
// src/queue/mod.rs, lines 33–53

/// A request waiting in the queue
pub struct QueuedRequest {
    pub intent: RoutingIntent,           // Routing metadata from reconciler
    pub request: ChatCompletionRequest,  // Original request body
    pub response_tx: oneshot::Sender<QueueResponse>,  // Response channel back to handler
    pub enqueued_at: Instant,            // When the patient sat down (for timeout)
    pub priority: Priority,              // Urgent care or general waiting?
}
```

Each waiting patient carries everything needed to resume treatment later. The `response_tx` is the critical piece — it's a one-shot channel that connects back to the HTTP handler waiting for a response. When the drain loop finishes processing, it sends the result through this channel, and the handler unblocks and returns the response to the client.

```rust
// src/queue/mod.rs, lines 58–68

#[derive(Debug, Error)]
pub enum QueueError {
    #[error("Queue is full ({max_size} requests)")]
    Full { max_size: u32 },

    #[error("Request queuing is disabled")]
    Disabled,
}
```

Two reasons a request can't enter the waiting room: it's full (all chairs taken), or the waiting room is closed. Both result in an immediate 503 to the client.

### The Queue Structure (RequestQueue)

```rust
// src/queue/mod.rs, lines 70–81

pub struct RequestQueue {
    high_tx: mpsc::Sender<QueuedRequest>,
    high_rx: tokio::sync::Mutex<mpsc::Receiver<QueuedRequest>>,
    normal_tx: mpsc::Sender<QueuedRequest>,
    normal_rx: tokio::sync::Mutex<mpsc::Receiver<QueuedRequest>>,
    depth: Arc<AtomicUsize>,
    config: QueueConfig,
}
```

The waiting room has two physical sections — an urgent care lane (`high_tx`/`high_rx`) and a general area (`normal_tx`/`normal_rx`). Both use tokio's `mpsc` (multi-producer, single-consumer) channels.

Why `Mutex<Receiver>`? Because `mpsc::Receiver::try_recv()` takes `&mut self`, but `RequestQueue` is shared via `Arc`. The `Mutex` provides interior mutability. Since only the drain loop calls `try_dequeue()`, contention is zero in practice — the `Mutex` is a compile-time requirement, not a runtime bottleneck.

Both channels are created with capacity equal to `max_size` (line 89). The **primary** bound is the atomic `depth` counter; the per-channel capacities are a safety net.

### The Enqueue Method (CAS Loop)

This is the most important method in the file — the "hot path" that every queued request flows through:

```rust
// src/queue/mod.rs, lines 102–142

pub fn enqueue(&self, request: QueuedRequest) -> Result<(), QueueError> {
    if !self.config.is_enabled() {
        return Err(QueueError::Disabled);
    }

    // CAS loop to atomically check-and-increment depth, preventing TOCTOU race
    loop {
        let current = self.depth.load(Ordering::SeqCst);
        if current >= self.config.max_size as usize {
            return Err(QueueError::Full {
                max_size: self.config.max_size,
            });
        }
        if self
            .depth
            .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            break;
        }
    }
    metrics::gauge!("nexus_queue_depth").set(self.depth() as f64);

    let tx = match request.priority {
        Priority::High => &self.high_tx,
        Priority::Normal => &self.normal_tx,
    };

    // try_send is non-blocking; if the channel is full, we already checked
    // depth so this should succeed unless there's a race (acceptable).
    if tx.try_send(request).is_err() {
        self.depth.fetch_sub(1, Ordering::SeqCst);
        metrics::gauge!("nexus_queue_depth").set(self.depth() as f64);
        return Err(QueueError::Full {
            max_size: self.config.max_size,
        });
    }

    Ok(())
}
```

Let's trace through this carefully, because the CAS (Compare-And-Swap) loop is the most subtle part of the entire feature:

1. **Gate check** (line 104): If the waiting room is closed (`enabled=false` or `max_size=0`), reject immediately.

2. **CAS loop** (lines 109–123): This is the critical section. We need to atomically check "is there room?" AND increment the counter. A naive approach would be:
   ```rust
   let current = depth.load();         // Thread A sees 99
   if current >= max_size { return Err; }  // Thread A: 99 < 100, OK
   depth.fetch_add(1);                 // Thread A increments to 100
   // But Thread B also saw 99, also passed the check, and increments to 101!
   ```
   The `compare_exchange` solves this: it says "set depth to `current + 1`, but **only if** it's still `current`." If another thread snuck in and changed the value, `compare_exchange` fails and we retry the loop.

3. **Metrics update** (line 124): After successfully reserving a slot, update the Prometheus gauge.

4. **Channel send** (lines 126–139): Route to the correct channel by priority. `try_send()` is non-blocking — it either succeeds instantly or fails. If it fails (shouldn't happen given the depth check, but defensive), we roll back the depth counter.

### The Dequeue Method

```rust
// src/queue/mod.rs, lines 144–167

pub async fn try_dequeue(&self) -> Option<QueuedRequest> {
    // Try high priority first
    {
        let mut rx = self.high_rx.lock().await;
        if let Ok(req) = rx.try_recv() {
            self.depth.fetch_sub(1, Ordering::SeqCst);
            metrics::gauge!("nexus_queue_depth").set(self.depth() as f64);
            return Some(req);
        }
    }

    // Then normal priority
    {
        let mut rx = self.normal_rx.lock().await;
        if let Ok(req) = rx.try_recv() {
            self.depth.fetch_sub(1, Ordering::SeqCst);
            metrics::gauge!("nexus_queue_depth").set(self.depth() as f64);
            return Some(req);
        }
    }

    None
}
```

The triage nurse always checks the urgent care lane first. The `{}` blocks are important — they scope the `Mutex` lock. Without them, we'd hold the high-priority lock while checking the normal-priority channel, which would block any concurrent `try_dequeue` (though in practice, only the drain loop calls this).

### The Drain Loop

```rust
// src/queue/mod.rs, lines 185–279

pub async fn queue_drain_loop(
    queue: Arc<RequestQueue>,
    state: Arc<crate::api::AppState>,
    cancel: tokio_util::sync::CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                drain_remaining(&queue).await;   // Send 503 to everyone still waiting
                break;
            }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                while let Some(queued) = queue.try_dequeue().await {
                    let max_wait = Duration::from_secs(queue.config().max_wait_seconds);

                    // Drain-side timeout: skip requests already expired before processing.
                    // The handler also has its own timeout guard (tokio::time::timeout on
                    // the oneshot rx), so a request may time out on either side.
                    if queued.enqueued_at.elapsed() > max_wait {
                        let _ = queued.response_tx.send(Ok(build_timeout_response(...)));
                        continue;
                    }

                    // Re-run routing
                    let result = state.router.select_backend(&requirements, Some(Strict));
                    match result {
                        Ok(routing_result) => {
                            let response = process_queued_request(&state, &routing_result, &queued.request).await;
                            let _ = queued.response_tx.send(response);
                        }
                        Err(_) => {
                            // Still no capacity — re-enqueue with original timestamp
                            if queued.enqueued_at.elapsed() < max_wait {
                                let re_queued = QueuedRequest {
                                    enqueued_at: queued.enqueued_at,  // ◄── preserve original!
                                    ..
                                };
                                let _ = queue.enqueue(re_queued);
                            } else {
                                let _ = queued.response_tx.send(Ok(build_timeout_response(...)));
                            }
                        }
                    }
                }
            }
        }
    }
}
```

The drain loop is the triage nurse's routine. Every 50ms, she:

1. **Checks the waiting room** — dequeues all available requests (inner `while let` loop)
2. **Checks the patient's wristband** — if they've been waiting longer than `max_wait_seconds`, send them home with a 503 and `Retry-After` header
3. **Tries to find a doctor** — re-runs `select_backend()` with `TierEnforcementMode::Strict`
4. **Doctor available?** — forward to the agent via `process_queued_request()`, send response through the oneshot channel
5. **No doctor yet?** — put the patient back in the waiting room, keeping their original arrival timestamp so the timeout doesn't reset
6. **Hospital closing?** — `CancellationToken` fires, `drain_remaining()` sends 503 to everyone

The **double timeout comment** (lines 211–213) is worth calling out: both the handler side (`tokio::time::timeout` on the oneshot receiver) and the drain side (`enqueued_at.elapsed()` check) independently enforce the timeout. A request can expire on either side — whichever fires first wins.

### Key Tests

The file contains 14 unit tests organized into three groups:

```rust
// T021: Queue structure tests
#[tokio::test]
async fn fifo_ordering_normal_priority() {
    // Enqueue 3 normal requests, verify they dequeue in FIFO order
    let queue = RequestQueue::new(make_config(10, 30));
    queue.enqueue(req1).unwrap();
    queue.enqueue(req2).unwrap();
    queue.enqueue(req3).unwrap();
    let d1 = queue.try_dequeue().await.unwrap();
    assert_eq!(d1.enqueued_at, t1);  // First in = first out
}

#[tokio::test]
async fn priority_ordering_high_drains_first() {
    // Enqueue: normal, high, normal → dequeue: high, normal, normal
    let d1 = queue.try_dequeue().await.unwrap();
    assert_eq!(d1.priority, Priority::High);  // ◄── High always first
}

#[tokio::test]
async fn capacity_limits_reject_when_full() {
    let queue = RequestQueue::new(make_config(2, 30));
    queue.enqueue(req1).unwrap();  // depth=1
    queue.enqueue(req2).unwrap();  // depth=2
    let result = queue.enqueue(req3);
    assert!(matches!(result, Err(QueueError::Full { max_size: 2 })));
}
```

```rust
// T028: Priority parsing tests
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
    assert_eq!(Priority::from_header("low"), Priority::Normal);
}
```

```rust
// CAS loop correctness test (newest — validates the TOCTOU fix)
#[tokio::test]
async fn concurrent_enqueue_respects_max_size() {
    let queue = Arc::new(RequestQueue::new(make_config(10, 30)));
    let mut handles = vec![];

    // Spawn 50 concurrent tasks trying to enqueue
    for _ in 0..50 {
        let q = Arc::clone(&queue);
        handles.push(tokio::spawn(async move {
            let (req, _rx) = make_queued(Priority::Normal);
            q.enqueue(req)
        }));
    }

    let results: Vec<_> = futures::future::join_all(handles).await;
    let successes = results.iter().filter(|r| r.as_ref().unwrap().is_ok()).count();

    assert_eq!(successes, 10, "Exactly max_size requests should succeed");
    assert_eq!(queue.depth(), 10);
}
```

This last test is the most important — it proves the CAS loop works under contention. 50 tasks race to enqueue into a queue with `max_size=10`. Without the CAS loop, more than 10 could slip through. With it, **exactly** 10 succeed.

---

## File 2: config/queue.rs — The Admission Policy

**Purpose**: Configuration for the waiting room — how many chairs, how long patients can wait, and whether it's open at all.  
**Lines**: 58  |  **Tests**: 0  |  **Status**: NEW

### Why Does This Exist?

Every hospital has an admission policy: maximum capacity, maximum wait time, whether the waiting room is open. This file encodes those policies as a TOML-deserializable struct with sensible defaults.

### The Struct

```rust
// src/config/queue.rs, lines 18–39

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QueueConfig {
    /// Whether request queuing is enabled.
    /// Default: true
    pub enabled: bool,

    /// Maximum number of queued requests.
    /// Default: 100
    /// When max_size is 0, queuing is disabled (equivalent to enabled=false).
    pub max_size: u32,

    /// Maximum wait time for queued requests in seconds.
    /// Default: 30 seconds
    pub max_wait_seconds: u64,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_size: 100,
            max_wait_seconds: 30,
        }
    }
}
```

Three knobs, sensible defaults. The `#[serde(default)]` attribute means all fields are optional in the TOML file — any missing field gets the `Default` implementation value.

The `is_enabled()` helper (line 55) catches a subtle edge case:

```rust
// src/config/queue.rs, lines 51–57

pub fn is_enabled(&self) -> bool {
    self.enabled && self.max_size > 0
}
```

A waiting room that's "enabled" but has zero chairs is effectively closed. This dual-check prevents a confusing configuration where `enabled=true` but `max_size=0` — both must be satisfied.

Configuration in `nexus.example.toml`:

```toml
[queue]
enabled = true
max_size = 100
max_wait_seconds = 30
```

---

## File 3: routing/reconciler/decision.rs — The Triage Tag

**Purpose**: Defines `RoutingDecision::Queue` — the triage tag that says "this patient needs to wait."  
**Lines**: 42  |  **Tests**: 0  |  **Status**: MODIFIED (Queue variant added)

### Why Does This Exist?

The reconciler pipeline (Privacy → Budget → Tier → Quality → Scheduler) produces a decision for every request. Before F18, there were only two outcomes: `Route` (send to a backend) and `Reject` (refuse with reasons). F18 adds a third: `Queue` — "we have backends that could serve this, but they're all busy right now."

### The Queue Variant

```rust
// src/routing/reconciler/decision.rs, lines 9–42

pub enum RoutingDecision {
    /// Successful routing to an agent
    Route {
        agent_id: String,
        model: String,
        reason: String,
        cost_estimate: CostEstimate,
    },

    /// Agent is busy, queue or wait required
    Queue {
        /// Reason for queueing
        reason: String,
        /// Estimated wait time in milliseconds
        estimated_wait_ms: u64,
        /// Fallback agent if available
        fallback_agent: Option<String>,
    },

    /// No viable agents, request rejected
    Reject {
        rejection_reasons: Vec<RejectionReason>,
    },
}
```

The `Queue` variant carries context that helps downstream code make decisions:

- **`reason`**: Human-readable explanation (e.g., "All backends at capacity")
- **`estimated_wait_ms`**: How long the caller might wait — used for logging, not enforcement (the actual timeout comes from `QueueConfig.max_wait_seconds`)
- **`fallback_agent`**: If there's a fallback backend that might handle the request — currently unused in the drain loop but available for future optimization

When the `SchedulerReconciler` scores all candidates and finds every backend is overloaded, it produces a `Queue` decision. The `Router::select_backend()` method converts this into a `RoutingError::Queue`, which the HTTP handler catches.

---

## File 4: api/completions.rs — The Intake Desk

**Purpose**: The HTTP handler that catches `RoutingError::Queue`, creates a `QueuedRequest`, enqueues it, and waits for the response.  
**Lines**: ~1100 total (F18 adds lines 409–472 and the `extract_priority` function at line 75)  
**Status**: MODIFIED

### Why Does This Exist?

This is the front desk of the hospital. When a patient arrives and the triage tag says "wait," the intake desk handles the paperwork: extracts the priority from the patient's wristband (the `X-Nexus-Priority` header), creates the waiting room record, hands it to the queue, and then sits with the patient (awaits the oneshot channel) until the triage nurse sends them to a doctor.

### Queue Integration

```rust
// src/api/completions.rs, lines 409–472

crate::routing::RoutingError::Queue {
    reason,
    estimated_wait_ms,
} => {
    // T027: Enqueue if queue available
    if let Some(ref queue) = state.queue {
        let priority = extract_priority(&headers);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let intent = crate::routing::reconciler::intent::RoutingIntent::new(
            request_id.clone(),
            requested_model.clone(),
            requested_model.clone(),
            requirements.clone(),
            vec![],
        );
        let queued = crate::queue::QueuedRequest {
            intent,
            request: request.clone(),
            response_tx: tx,
            enqueued_at: std::time::Instant::now(),
            priority,
        };

        match queue.enqueue(queued) {
            Ok(()) => {
                info!(
                    reason = %reason,
                    estimated_wait_ms,
                    priority = ?priority,
                    "Request enqueued"
                );

                let max_wait =
                    std::time::Duration::from_secs(queue.config().max_wait_seconds);
                match tokio::time::timeout(max_wait, rx).await {
                    Ok(Ok(resp)) => {
                        return resp;
                    }
                    _ => {
                        return Ok(crate::queue::build_timeout_response(
                            &queue.config().max_wait_seconds.to_string(),
                        ));
                    }
                }
            }
            Err(crate::queue::QueueError::Full { .. }) => {
                warn!("Queue full, rejecting request");
                return Ok(ApiError::service_unavailable(
                    "All backends at capacity and queue is full",
                )
                .into_response());
            }
            Err(crate::queue::QueueError::Disabled) => {
                return Ok(ApiError::service_unavailable(
                    "All backends at capacity",
                )
                .into_response());
            }
        }
    } else {
        return Ok(ApiError::service_unavailable("All backends at capacity")
            .into_response());
    }
}
```

Let's trace the flow:

1. **Queue available?** (line 414): `state.queue` is `Option<Arc<RequestQueue>>`. If `None`, there's no waiting room — immediate 503.

2. **Create the patient record** (lines 415–430): Bundle the priority, the original request, a oneshot sender, and the current timestamp into a `QueuedRequest`.

3. **Try to enter** (line 432): Call `queue.enqueue()`. Three outcomes:
   - **`Ok(())`**: Patient is seated. Now we wait.
   - **`Err(Full)`**: Waiting room full. Immediate 503: "queue is full."
   - **`Err(Disabled)`**: Waiting room closed. Immediate 503: "at capacity."

4. **Wait for the doctor** (lines 441–452): `tokio::time::timeout(max_wait, rx).await` blocks the HTTP handler until either:
   - The drain loop sends a response through the oneshot → return it to the client
   - The timeout expires → return a 503 with `Retry-After` header

This is the handler-side timeout. The drain loop also checks timeouts independently (dual timeout design).

### Priority Extraction

```rust
// src/api/completions.rs, lines 71–81

/// Header name for request priority (T028)
const PRIORITY_HEADER: &str = "x-nexus-priority";

fn extract_priority(headers: &HeaderMap) -> crate::queue::Priority {
    headers
        .get(PRIORITY_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(crate::queue::Priority::from_header)
        .unwrap_or(crate::queue::Priority::Normal)
}
```

Reading the patient's wristband. The `and_then` chain safely handles: header missing → `None` → `unwrap_or(Normal)`. Header present but invalid UTF-8 → `to_str()` returns `None` → defaults to `Normal`. Header present and valid → `Priority::from_header()` parses it (case-insensitive).

Streaming requests (line 876) are **not** queued — they get an immediate 503 because SSE connections must begin transmitting immediately.

---

## File 5: cli/serve.rs — The Hospital Opening & Closing

**Purpose**: Creates the queue at server startup, starts the drain loop as a background task, and shuts both down gracefully.  
**Lines**: ~340 total (F18 adds lines 224–237 and 288–333)  
**Status**: MODIFIED

### Why Does This Exist?

Someone has to unlock the waiting room doors in the morning and lock them at night. The `serve` command handles the full lifecycle: create the queue → inject it into `AppState` → enable queue-aware routing → spawn the drain loop → wait for shutdown → drain remaining → clean up.

### Queue Startup and Shutdown

**Step 1: Create the waiting room** (lines 224–237):

```rust
// src/cli/serve.rs, lines 224–237

// 4. Create request queue (if enabled) (T030)
let queue = if config.queue.is_enabled() {
    tracing::info!(
        max_size = config.queue.max_size,
        max_wait_seconds = config.queue.max_wait_seconds,
        "Request queuing enabled"
    );
    Some(Arc::new(crate::queue::RequestQueue::new(
        config.queue.clone(),
    )))
} else {
    tracing::debug!("Request queuing disabled");
    None
};
```

If the config says the waiting room should be open, create it wrapped in `Arc` for shared ownership. Otherwise, `None` — no queue, no drain loop.

**Step 2: Wire it into the API** (lines 160–168):

```rust
// src/cli/serve.rs, lines 160–168

app_state.queue = queue;
// Enable queue-aware routing when queue is configured
if config.queue.is_enabled() {
    if let Some(router) = Arc::get_mut(&mut app_state.router) {
        router.set_queue_enabled(true);
    }
}
```

Two things happen: the queue is stored in `AppState` (so the handler can access it), and the router is told that queuing is available (so it returns `RoutingError::Queue` instead of `RoutingError::NoBackend` when backends are saturated).

**Step 3: Start the drain loop** (lines 288–299):

```rust
// src/cli/serve.rs, lines 288–299

// 4.8. Start queue drain loop (T030)
let queue_handle = if let Some(ref q) = queue {
    tracing::info!("Starting queue drain loop");
    let queue_clone = Arc::clone(q);
    let state_clone = Arc::clone(&app_state);
    let queue_cancel = cancel_token.clone();
    Some(tokio::spawn(async move {
        crate::queue::queue_drain_loop(queue_clone, state_clone, queue_cancel).await;
    }))
} else {
    None
};
```

The drain loop runs as a `tokio::spawn` task — a long-lived background green thread. It receives clones of the `Arc`s it needs and a `CancellationToken` for shutdown signaling.

**Step 4: Graceful shutdown** (lines 330–333):

```rust
// src/cli/serve.rs, lines 330–333

if let Some(handle) = queue_handle {
    tracing::info!("Waiting for queue drain loop to stop");
    handle.await?;
}
```

When the server shuts down (Ctrl-C → `CancellationToken` fires), the drain loop's `tokio::select!` picks up the cancellation, calls `drain_remaining()` (which sends 503 to all waiting requests), and exits. The `handle.await?` ensures we don't exit before all patients have been notified.

---

## Understanding the Tests

### Test Helpers

All test modules use factory functions to create test fixtures. This keeps individual tests focused on what they're verifying, not on boilerplate setup:

```rust
// src/queue/mod.rs, lines 343–386

fn make_config(max_size: u32, max_wait_seconds: u64) -> QueueConfig {
    QueueConfig { enabled: true, max_size, max_wait_seconds }
}

fn make_intent() -> RoutingIntent {
    RoutingIntent::new("req-1".to_string(), "llama3:8b".to_string(), ...)
}

fn make_request() -> ChatCompletionRequest {
    serde_json::from_value(serde_json::json!({
        "model": "llama3:8b",
        "messages": [{"role": "user", "content": "hello"}]
    })).unwrap()
}

fn make_queued(priority: Priority) -> (QueuedRequest, oneshot::Receiver<QueueResponse>) {
    let (tx, rx) = oneshot::channel();
    let req = QueuedRequest { intent: make_intent(), request: make_request(), response_tx: tx, enqueued_at: Instant::now(), priority };
    (req, rx)
}
```

`make_queued()` returns **both** the request and the receiver — the test can enqueue the request and then check whether a response was sent through the receiver.

### Test Organization

| Group | Test Count | What It Covers |
|-------|------------|----------------|
| T021: Queue structure (unit) | 7 | FIFO ordering, capacity limits, priority ordering, depth accuracy, disabled/empty states |
| T022: Timeout behavior (unit) | 3 | `Retry-After` header, timeout detection, timeout speed |
| T028: Priority parsing (unit) | 3 | `from_header()` with valid, invalid, and edge-case inputs |
| CAS correctness (unit) | 1 | 50 concurrent enqueues respect `max_size=10` |
| T023: Integration | 2 | End-to-end capacity+overflow, mixed-priority drain ordering |

### Testing Patterns

**Pattern 1: Enqueue → Dequeue → Assert Order**

Most queue tests follow this shape: fill the queue in a known order, drain it, and verify the output order matches expectations.

```rust
#[tokio::test]
async fn fifo_ordering_normal_priority() {
    let queue = RequestQueue::new(make_config(10, 30));
    // Enqueue with known timestamps
    queue.enqueue(req1).unwrap();
    queue.enqueue(req2).unwrap();
    // Dequeue and verify order
    let d1 = queue.try_dequeue().await.unwrap();
    assert_eq!(d1.enqueued_at, t1);  // First in = first out
}
```

**Pattern 2: Boundary Testing**

Queue tests systematically hit boundaries: exactly at capacity, one over capacity, zero capacity, disabled state.

```rust
#[tokio::test]
async fn capacity_limits_reject_when_full() {
    let queue = RequestQueue::new(make_config(2, 30));  // max_size=2
    queue.enqueue(req1).unwrap();  // 1/2 — OK
    queue.enqueue(req2).unwrap();  // 2/2 — OK
    let result = queue.enqueue(req3);  // 3/2 — FULL
    assert!(matches!(result, Err(QueueError::Full { max_size: 2 })));
}
```

**Pattern 3: Concurrent Stress Testing**

The CAS loop test uses `tokio::spawn` to create real concurrency, then counts successes to verify the atomic invariant holds under contention.

```rust
#[tokio::test]
async fn concurrent_enqueue_respects_max_size() {
    // 50 tasks race to enqueue into max_size=10
    let successes = results.iter().filter(|r| r.as_ref().unwrap().is_ok()).count();
    assert_eq!(successes, 10, "Exactly max_size requests should succeed");
}
```

---

## Key Rust Concepts

### 1. `compare_exchange` (CAS) for Lock-Free Atomics

```rust
self.depth.compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
```

`compare_exchange` is the atomic equivalent of: "if the value is still what I think it is, update it; otherwise, tell me what it actually is." It takes four arguments: `current` (expected value), `new` (desired value), `success_ordering`, `failure_ordering`. Returns `Ok(previous)` on success or `Err(actual)` on failure. The CAS loop wrapping this retries until it succeeds or the capacity check fails.

### 2. `oneshot::channel` for One-Time Response Delivery

```rust
let (tx, rx) = tokio::sync::oneshot::channel();
// Handler holds rx, awaits it with timeout
// Drain loop holds tx (inside QueuedRequest), sends response through it
```

A `oneshot` channel can send exactly one value. It's perfect for request-response patterns: the handler creates the channel, gives the sender to the queue, and awaits the receiver. When the drain loop processes the request, it sends the response and the handler unblocks. If the sender is dropped without sending (e.g., the drain loop panics), the receiver gets an error.

### 3. `tokio::select!` for Concurrent Waiting

```rust
tokio::select! {
    _ = cancel.cancelled() => { /* shutdown */ }
    _ = tokio::time::sleep(Duration::from_millis(50)) => { /* poll queue */ }
}
```

`select!` races multiple futures and executes the branch of whichever completes first. Here, it races the cancellation signal against a 50ms sleep. If cancellation fires during the sleep, the drain loop exits immediately without waiting for the full 50ms. This is how Nexus achieves responsive shutdown.

### 4. `Arc<T>` for Shared Ownership Across Tasks

```rust
let queue = Arc::new(RequestQueue::new(config));
let queue_clone = Arc::clone(&queue);
tokio::spawn(async move { queue_drain_loop(queue_clone, ...).await; });
```

`Arc` (Atomic Reference Count) allows multiple owners of the same data. When the last `Arc` is dropped, the data is freed. The queue needs to be owned by both `AppState` (for the handler) and the drain loop task — `Arc` makes this possible without copying the queue.

### 5. `Ordering::SeqCst` for Sequential Consistency

```rust
self.depth.load(Ordering::SeqCst);
self.depth.compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst);
```

`SeqCst` (Sequentially Consistent) is the strongest memory ordering — it guarantees that all threads see atomic operations in the same order. It's slower than `Relaxed` or `Acquire`/`Release`, but for a queue depth counter that's checked on every enqueue, correctness matters more than nanoseconds. The queue processes at most ~20 requests/sec (bounded by the 50ms poll), so `SeqCst` overhead is negligible.

---

## Common Patterns in This Codebase

### 1. The Dual-Channel Priority Pattern

Instead of a single channel with a priority field and a sorted data structure:

```
                    ┌─── High Channel ───┐
Request ──enqueue──►│  mpsc (try_send)   │──dequeue──► (checked first)
                    └────────────────────┘
                    ┌─── Normal Channel ──┐
                    │  mpsc (try_send)    │──dequeue──► (checked second)
                    └────────────────────┘
```

Priority is **structural**: two separate channels, always drain high first. This avoids sorting, custom comparators, and heap rebalancing. Adding a new priority level means adding a new channel pair — `O(1)` code change, `O(1)` runtime per enqueue/dequeue.

### 2. The Oneshot Response Bridge Pattern

The queue acts as a bridge between two execution contexts — the HTTP handler's synchronous request-response flow and the drain loop's asynchronous polling:

```
Handler Task                     Drain Loop Task
─────────────                    ───────────────
let (tx, rx) = oneshot()
queue.enqueue(QueuedRequest{tx})
                                 let req = queue.try_dequeue()
                                 let response = agent.chat_completion()
                                 req.response_tx.send(response)
rx.await ◄── response arrives
return response to client
```

The handler doesn't know or care when the drain loop runs. It just awaits the oneshot receiver. The drain loop doesn't know or care about HTTP. It just sends responses. The oneshot channel decouples them completely.

### 3. The Graceful Drain Pattern

On shutdown, long-lived background tasks need to clean up:

```rust
tokio::select! {
    _ = cancel.cancelled() => {
        drain_remaining(&queue).await;  // Notify all waiting clients
        break;
    }
    _ = sleep(50ms) => { /* normal work */ }
}
```

The `CancellationToken` propagates shutdown through the entire system. Each component handles it by cleaning up its own resources. The queue's cleanup is `drain_remaining()` — it dequeues every request and sends a 503, so no HTTP handler is left hanging forever.

### 4. The Rollback-on-Failure Pattern

Atomic operations that can fail use a rollback strategy:

```rust
// Reserve a slot (optimistic)
self.depth.compare_exchange(current, current + 1, ...);

// Try the actual operation
if tx.try_send(request).is_err() {
    // Rollback the reservation
    self.depth.fetch_sub(1, Ordering::SeqCst);
    return Err(QueueError::Full { ... });
}
```

This is a lightweight form of transaction: increment first (optimistic), attempt the real work, undo the increment on failure. It keeps the depth counter accurate even when channel sends fail.

---

## Next Steps

After understanding F18, here's what to explore next:

1. **`src/routing/mod.rs`** — Find where `RoutingError::Queue` is produced. Look at `select_backend()` and how `queue_enabled` influences the decision between `Queue` and `NoBackend`.

2. **F15: Speculative Router** — The `RequestRequirements` struct that the drain loop uses when re-running `select_backend()` (see `specs/018-speculative-router/walkthrough.md`).

3. **Metrics** — The `nexus_queue_depth` gauge at `/metrics`. Try watching it during a load test to see the queue fill and drain in real time.

4. **Try it yourself**: Set `max_size=2` in config, send 5 concurrent requests to saturated backends, and observe which get queued, which get rejected, and what the `Retry-After` header says.

### Questions to Investigate

- What happens if the drain loop dequeues a request, but the handler's `tokio::time::timeout` already expired? (Hint: `response_tx.send()` returns `Err` because the receiver was dropped — the `let _ =` silently ignores this)
- Why does re-enqueue preserve the original `enqueued_at` instead of resetting it? (Hint: resetting would let a request bounce in the queue forever, never timing out)
- How would you add a `Priority::Low` level? (Hint: add a third `mpsc` channel pair, update `enqueue()`/`try_dequeue()`/`from_header()`, and add a new dequeue check after normal)
- Why use `SeqCst` ordering instead of `Acquire`/`Release`? (Hint: correctness over performance — the queue processes ~20 requests/sec max, so the overhead is negligible)
