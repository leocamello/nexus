# API Contract: Request Queue Operations

**Phase 1 Contract** | **Date**: 2026-02-15 (Retrospective) | **Feature**: F18

This document defines the public API operations for the request queuing feature.

## RequestQueue API

### Constructor

#### `RequestQueue::new(config: QueueConfig) -> Self`

**Purpose**: Create a new bounded dual-priority request queue.

**Parameters**:
- `config: QueueConfig` - Configuration (enabled, max_size, max_wait_seconds)

**Returns**: `Self` - New RequestQueue instance

**Side Effects**:
- Creates two mpsc channels (high and normal priority)
- Initializes atomic depth counter to 0
- Allocates channel buffers with capacity = `config.max_size`

**Panics**: Never

**Thread Safety**: Thread-safe (can be wrapped in `Arc<RequestQueue>`)

**Example**:
```rust
use nexus::config::QueueConfig;
use nexus::queue::RequestQueue;
use std::sync::Arc;

let config = QueueConfig {
    enabled: true,
    max_size: 100,
    max_wait_seconds: 30,
};

let queue = Arc::new(RequestQueue::new(config));
```

---

### Enqueue Operation

#### `queue.enqueue(request: QueuedRequest) -> Result<(), QueueError>`

**Purpose**: Enqueue a request for later processing.

**Parameters**:
- `request: QueuedRequest` - Request to enqueue (contains intent, request, response_tx, timestamp, priority)

**Returns**:
- `Ok(())` - Request successfully enqueued
- `Err(QueueError::Full { max_size })` - Queue is at capacity
- `Err(QueueError::Disabled)` - Queuing is disabled in config

**Side Effects**:
- Increments atomic depth counter (on success)
- Updates `nexus_queue_depth` metric (on success)
- Sends request to appropriate channel (high_tx or normal_tx)

**Atomicity**:
- Depth check and increment are atomic (SeqCst ordering)
- Race condition possible: depth may briefly exceed max_size (acceptable)
- `try_send()` provides second line of defense (returns error if channel full)

**Time Complexity**: O(1)

**Thread Safety**: Thread-safe (can be called concurrently from multiple threads)

**Example**:
```rust
use nexus::queue::{QueuedRequest, QueueError, Priority};
use std::time::Instant;

let (tx, rx) = tokio::sync::oneshot::channel();
let queued = QueuedRequest {
    intent: routing_intent,
    request: chat_request,
    response_tx: tx,
    enqueued_at: Instant::now(),
    priority: Priority::High,
};

match queue.enqueue(queued) {
    Ok(()) => {
        // Success: wait on rx for response
        let response = rx.await?;
    }
    Err(QueueError::Full { max_size }) => {
        // Queue full: return 503
        eprintln!("Queue full ({} requests)", max_size);
    }
    Err(QueueError::Disabled) => {
        // Queue disabled: return 503
        eprintln!("Queue disabled");
    }
}
```

**Error Conditions**:

| Condition | Error | HTTP Response |
|-----------|-------|---------------|
| `depth >= max_size` | `QueueError::Full` | 503 "Queue is full" |
| `config.enabled = false` | `QueueError::Disabled` | 503 "All backends at capacity" |
| `config.max_size = 0` | `QueueError::Disabled` | 503 "All backends at capacity" |
| Channel send fails | `QueueError::Full` | 503 "Queue is full" |

---

### Dequeue Operation

#### `queue.try_dequeue() -> Option<QueuedRequest>`

**Purpose**: Try to dequeue a request (high priority first, then normal).

**Parameters**: None

**Returns**:
- `Some(QueuedRequest)` - Successfully dequeued a request
- `None` - Queue is empty

**Side Effects**:
- Decrements atomic depth counter (on success)
- Updates `nexus_queue_depth` metric (on success)
- Acquires mutex on high_rx, then normal_rx (sequential, not concurrent)

**Priority Ordering**:
1. Try `high_rx.try_recv()` (lock high_rx mutex)
2. If high is empty, try `normal_rx.try_recv()` (lock normal_rx mutex)
3. Return `None` if both are empty

**FIFO Guarantee**:
- Within high-priority queue: FIFO (first enqueued, first dequeued)
- Within normal-priority queue: FIFO
- Across priorities: High always before Normal (no interleaving)

**Time Complexity**: O(1)

**Thread Safety**: Thread-safe (mutex-protected receivers)

**Example**:
```rust
// In drain loop (single background task)
while let Some(queued) = queue.try_dequeue().await {
    // Check timeout
    let max_wait = Duration::from_secs(queue.config().max_wait_seconds);
    if queued.enqueued_at.elapsed() > max_wait {
        // Timed out: send 503 response
        let error = build_timeout_response("30");
        let _ = queued.response_tx.send(Ok(error));
        continue;
    }
    
    // Re-run routing
    match state.router.select_backend(&requirements, mode) {
        Ok(routing_result) => {
            // Process request
            let response = process_queued_request(...).await;
            let _ = queued.response_tx.send(response);
        }
        Err(_) => {
            // No capacity, re-enqueue if not timed out
            if queued.enqueued_at.elapsed() < max_wait {
                let _ = queue.enqueue(queued);
            }
        }
    }
}
```

---

### Depth Query

#### `queue.depth() -> usize`

**Purpose**: Get current total queue depth (high + normal).

**Parameters**: None

**Returns**: `usize` - Current number of queued requests

**Side Effects**: None (read-only)

**Atomicity**: Atomic load (SeqCst ordering)

**Time Complexity**: O(1)

**Thread Safety**: Thread-safe

**Example**:
```rust
let depth = queue.depth();
println!("Current queue depth: {}/{}", depth, queue.config().max_size);

// Use for metrics
metrics::gauge!("nexus_queue_depth").set(depth as f64);

// Use for health checks
if depth > queue.config().max_size * 8 / 10 {
    warn!("Queue is 80% full ({} requests)", depth);
}
```

---

### Configuration Access

#### `queue.config() -> &QueueConfig`

**Purpose**: Get immutable reference to queue configuration.

**Parameters**: None

**Returns**: `&QueueConfig` - Configuration reference

**Side Effects**: None (read-only)

**Lifetime**: Tied to `&self` lifetime (valid as long as queue exists)

**Thread Safety**: Thread-safe (immutable reference)

**Example**:
```rust
let config = queue.config();
println!("Queue enabled: {}", config.enabled);
println!("Queue max size: {}", config.max_size);
println!("Queue max wait: {}s", config.max_wait_seconds);

// Use for timeout calculation
let max_wait = Duration::from_secs(config.max_wait_seconds);
```

---

## Background Tasks

### Drain Loop

#### `queue_drain_loop(queue: Arc<RequestQueue>, state: Arc<AppState>, cancel: CancellationToken) -> ()`

**Purpose**: Background task that processes queued requests as capacity becomes available.

**Parameters**:
- `queue: Arc<RequestQueue>` - Shared queue instance
- `state: Arc<AppState>` - Application state (router, registry, agents)
- `cancel: CancellationToken` - Graceful shutdown signal

**Returns**: `()` (never returns normally, only on cancellation)

**Lifecycle**:
1. Started on Nexus startup (after queue initialization)
2. Runs continuously until cancellation
3. Drains remaining requests on shutdown (sends 503 to all)

**Poll Interval**: 50ms (configurable in code, not exposed in config)

**Processing Logic**:
```rust
loop {
    tokio::select! {
        _ = cancel.cancelled() => {
            // Graceful shutdown
            drain_remaining(&queue).await;
            break;
        }
        _ = tokio::time::sleep(Duration::from_millis(50)) => {
            // Poll for queued requests
            while let Some(queued) = queue.try_dequeue().await {
                // 1. Check timeout
                if queued.enqueued_at.elapsed() > max_wait {
                    send_timeout_response(queued);
                    continue;
                }
                
                // 2. Re-run routing
                let result = state.router.select_backend(...);
                
                // 3. Process or re-enqueue
                match result {
                    Ok(routing) => {
                        let response = process_queued_request(...).await;
                        let _ = queued.response_tx.send(response);
                    }
                    Err(_) => {
                        if not_timed_out {
                            let _ = queue.enqueue(queued); // Re-enqueue
                        } else {
                            send_timeout_response(queued);
                        }
                    }
                }
            }
        }
    }
}
```

**Error Handling**:
- Oneshot send failure: Ignored (client disconnected)
- Re-enqueue failure: Logged as warning, request is dropped
- Routing failure: Re-enqueue until timeout, then 503

**Metrics**:
- `nexus_queue_depth`: Updated on enqueue/dequeue
- No drain loop-specific metrics (could add: requests processed, timeouts, re-enqueues)

---

### Process Queued Request

#### `process_queued_request(state: &AppState, routing_result: &RoutingResult, request: &ChatCompletionRequest) -> QueueResponse`

**Purpose**: Forward a queued request to the selected backend.

**Parameters**:
- `state: &AppState` - Application state (registry with agents)
- `routing_result: &RoutingResult` - Selected backend and model
- `request: &ChatCompletionRequest` - Original request (will be cloned)

**Returns**: `QueueResponse` (Result<Response, ApiError>)

**Processing Steps**:
1. Clone request and update model to `routing_result.actual_model`
2. Increment backend's pending request counter
3. Get agent from registry
4. Call `agent.chat_completion(request, None)`
5. Decrement backend's pending request counter
6. Convert response to HTTP response or API error

**Side Effects**:
- Increments/decrements backend pending counter
- Makes HTTP request to backend
- Updates backend metrics (via agent)

**Time Complexity**: O(1) + O(backend latency)

**Error Conditions**:
- Agent not found: 502 Bad Gateway
- Chat completion error: Propagated as ApiError

---

### Build Timeout Response

#### `build_timeout_response(retry_after: &str) -> axum::response::Response`

**Purpose**: Build a 503 response with Retry-After header for timed-out requests.

**Parameters**:
- `retry_after: &str` - Seconds to wait before retry (typically `config.max_wait_seconds`)

**Returns**: `axum::response::Response` - HTTP 503 response

**Response Format**:
```http
HTTP/1.1 503 Service Unavailable
Retry-After: 30
Content-Type: application/json

{
  "error": {
    "message": "Request timed out in queue",
    "type": "service_unavailable",
    "code": 503
  }
}
```

**Thread Safety**: Thread-safe (no shared state)

---

### Drain Remaining

#### `drain_remaining(queue: &Arc<RequestQueue>) -> ()`

**Purpose**: Drain all remaining requests on shutdown (graceful shutdown).

**Parameters**:
- `queue: &Arc<RequestQueue>` - Queue to drain

**Returns**: `()` (async function, awaits all drains)

**Side Effects**:
- Dequeues all requests
- Sends 503 with `Retry-After: 5` to each request
- Updates queue depth metric

**Processing**:
```rust
while let Some(queued) = queue.try_dequeue().await {
    let error_response = build_timeout_response("5");
    let _ = queued.response_tx.send(Ok(error_response));
}
```

**Error Handling**:
- Oneshot send failure: Ignored (client already disconnected)

---

## Metrics Contract

### `nexus_queue_depth` Gauge

**Type**: Gauge (Prometheus)

**Description**: Current number of requests waiting in queue (high + normal)

**Labels**: None (global queue)

**Value Range**: `[0, config.max_size]`

**Update Frequency**: On every enqueue and dequeue

**Example Queries**:
```promql
# Current queue depth
nexus_queue_depth

# Queue utilization percentage
nexus_queue_depth / nexus_queue_max_size * 100

# Queue depth over time (5-minute window)
rate(nexus_queue_depth[5m])
```

**Alerting Examples**:
```promql
# Alert: Queue is 80% full
nexus_queue_depth / nexus_queue_max_size > 0.8

# Alert: Queue has been non-empty for 5 minutes
min_over_time(nexus_queue_depth[5m]) > 0
```

---

## Integration Points

### API Handler Integration

**Location**: `src/api/completions.rs`

**Integration Flow**:
```rust
// In chat_completions handler
let result = state.router.select_backend(&requirements, mode);

match result {
    Ok(routing_result) => {
        // Normal path: route to backend
        let response = route_to_backend(...).await;
        return Ok(response);
    }
    
    Err(RoutingError::Queue { reason, estimated_wait_ms }) => {
        // Queue path: enqueue request
        if let Some(ref queue) = state.queue {
            let priority = extract_priority(&headers);
            let (tx, rx) = tokio::sync::oneshot::channel();
            
            let queued = QueuedRequest {
                intent: routing_intent,
                request: request.clone(),
                response_tx: tx,
                enqueued_at: Instant::now(),
                priority,
            };
            
            match queue.enqueue(queued) {
                Ok(()) => {
                    // Wait on oneshot receiver
                    let max_wait = Duration::from_secs(queue.config().max_wait_seconds);
                    match tokio::time::timeout(max_wait, rx).await {
                        Ok(Ok(resp)) => return resp,
                        _ => return timeout_response(),
                    }
                }
                Err(QueueError::Full { .. }) => {
                    return Ok(ApiError::service_unavailable(
                        "All backends at capacity and queue is full"
                    ).into_response());
                }
                Err(QueueError::Disabled) => {
                    return Ok(ApiError::service_unavailable(
                        "All backends at capacity"
                    ).into_response());
                }
            }
        }
    }
    
    Err(other_error) => {
        // Other errors: handle normally
        return Err(other_error.into());
    }
}
```

### Routing Integration

**Location**: `src/routing/mod.rs`

**RoutingError::Queue Variant**:
```rust
pub enum RoutingError {
    // ... other variants
    
    Queue {
        reason: String,
        estimated_wait_ms: u64,
    },
}
```

**Emission Logic**:
```rust
// In Router::select_backend()
if let RoutingDecision::Queue { reason, estimated_wait_ms, .. } = &decision {
    return Err(RoutingError::Queue {
        reason: reason.clone(),
        estimated_wait_ms: *estimated_wait_ms,
    });
}
```

---

## Error Recovery

### Oneshot Receiver Dropped

**Cause**: Client HTTP connection closed before response ready

**Detection**: `response_tx.send()` returns `Err(response)`

**Handling**: Ignore error, discard response (logged at debug level)

**Impact**: Wasted processing, but prevents resource leak

---

### Re-enqueue Failure

**Cause**: Queue is full when attempting to re-enqueue

**Detection**: `queue.enqueue()` returns `QueueError::Full`

**Handling**: Log warning, discard request

**Impact**: Request is lost (client will timeout and retry)

---

### Routing Failure Loop

**Cause**: Routing continuously fails, request keeps re-enqueuing

**Detection**: `enqueued_at.elapsed() > max_wait`

**Handling**: Send 503 with Retry-After, stop re-enqueue loop

**Impact**: Request times out after `max_wait_seconds`

---

## Performance Characteristics

| Operation | Latency | Throughput | Memory |
|-----------|---------|------------|--------|
| `enqueue()` | < 100μs | 10,000+ ops/sec | O(1) per request |
| `try_dequeue()` | < 100μs | 10,000+ ops/sec | O(1) per request |
| `depth()` | < 10ns | Unlimited | O(1) |
| Drain loop poll | 50ms | 20 checks/sec | O(1) |
| Process request | Variable | Depends on backend | O(1) |

---

**API Contract Complete** | **Phase 1** | **Ready for implementation**
