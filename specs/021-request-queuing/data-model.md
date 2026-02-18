# Data Model: Request Queuing & Prioritization

**Phase 1 Design** | **Date**: 2026-02-15 (Retrospective) | **Feature**: F18

This document defines the data structures, types, and relationships for the request queuing feature.

## Core Entities

### 1. Priority (Enum)

**Purpose**: Defines the priority level for queued requests.

**Definition**:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    High,
    Normal,
}
```

**Fields**:
- `High`: Urgent requests that should be processed first
- `Normal`: Standard priority requests

**Validation Rules**:
- Default: `Normal` when header is missing or invalid
- Case-insensitive parsing from `X-Nexus-Priority` header
- Invalid values (e.g., "low", "medium") default to `Normal`

**State Transitions**:
- N/A (immutable once set)

**Methods**:
```rust
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

---

### 2. QueuedRequest (Struct)

**Purpose**: Represents a request waiting in the queue, including all context needed to process it later.

**Definition**:
```rust
pub struct QueuedRequest {
    /// Routing intent from the reconciler pipeline
    pub intent: RoutingIntent,
    
    /// Original chat completion request
    pub request: ChatCompletionRequest,
    
    /// Channel to send the response back to the waiting handler
    pub response_tx: oneshot::Sender<QueueResponse>,
    
    /// When the request was enqueued
    pub enqueued_at: Instant,
    
    /// Request priority
    pub priority: Priority,
}
```

**Fields**:
- `intent: RoutingIntent`: Routing context (request ID, model, requirements, candidates)
- `request: ChatCompletionRequest`: Original API request from client
- `response_tx: oneshot::Sender<QueueResponse>`: One-time channel to send response back
- `enqueued_at: Instant`: Timestamp for timeout calculation
- `priority: Priority`: High or Normal priority level

**Relationships**:
- Contains `RoutingIntent` from `crate::routing::reconciler::intent`
- Contains `ChatCompletionRequest` from `crate::api`
- Sends `QueueResponse` via oneshot channel
- Owned by `RequestQueue` until dequeued

**Validation Rules**:
- `enqueued_at` must be in the past (set on creation)
- `response_tx` is consumed when sending response (oneshot semantic)
- `request` must be valid `ChatCompletionRequest` (validated before enqueue)

**Lifecycle**:
1. Created in API handler when `RoutingError::Queue` is caught
2. Enqueued via `RequestQueue::enqueue()`
3. Dequeued via `RequestQueue::try_dequeue()` in drain loop
4. Processed or timed out
5. Response sent via `response_tx`, struct is dropped

---

### 3. QueueError (Enum)

**Purpose**: Represents errors that can occur during queue operations.

**Definition**:
```rust
#[derive(Debug, Error)]
pub enum QueueError {
    /// Queue is full (total depth == max_size)
    #[error("Queue is full ({max_size} requests)")]
    Full { max_size: u32 },

    /// Queue is disabled
    #[error("Request queuing is disabled")]
    Disabled,
}
```

**Variants**:
- `Full { max_size: u32 }`: Queue has reached capacity
  - **When**: `depth >= max_size` when enqueuing
  - **Action**: Return 503 "Queue is full"
- `Disabled`: Queuing is disabled in config
  - **When**: `config.enabled = false` or `config.max_size = 0`
  - **Action**: Return 503 "All backends at capacity"

**Error Handling**:
```rust
match queue.enqueue(queued) {
    Ok(()) => { /* Wait on oneshot */ }
    Err(QueueError::Full { .. }) => {
        // Return 503 with "Queue is full" message
    }
    Err(QueueError::Disabled) => {
        // Return 503 with "All backends at capacity" message
    }
}
```

---

### 4. QueueResponse (Type Alias)

**Purpose**: Type alias for the response sent through oneshot channels.

**Definition**:
```rust
pub type QueueResponse = Result<axum::response::Response, crate::api::ApiError>;
```

**Semantics**:
- `Ok(Response)`: Successful completion, contains chat completion JSON
- `Err(ApiError)`: Error occurred (routing failure, timeout, backend error)

**Usage**:
```rust
// Send success response
let response = axum::response::Json(completion).into_response();
let _ = queued.response_tx.send(Ok(response));

// Send error response
let error = crate::api::ApiError::service_unavailable("Timed out");
let _ = queued.response_tx.send(Err(error));
```

---

### 5. RequestQueue (Struct)

**Purpose**: Bounded dual-priority queue with atomic depth tracking.

**Definition**:
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

**Fields**:
- `high_tx: mpsc::Sender<QueuedRequest>`: Sender for high-priority channel (clonable)
- `high_rx: Mutex<mpsc::Receiver<QueuedRequest>>`: Receiver for high-priority channel (exclusive access)
- `normal_tx: mpsc::Sender<QueuedRequest>`: Sender for normal-priority channel (clonable)
- `normal_rx: Mutex<mpsc::Receiver<QueuedRequest>>`: Receiver for normal-priority channel (exclusive access)
- `depth: Arc<AtomicUsize>`: Total depth across both channels (thread-safe)
- `config: QueueConfig`: Configuration (enabled, max_size, max_wait_seconds)

**Invariants**:
- `depth <= max_size` (enforced atomically)
- `depth = high_queue.len() + normal_queue.len()` (logical, not enforced)
- Receivers are mutex-wrapped for exclusive dequeue access
- Senders are clonable for concurrent enqueue

**Operations**:
```rust
impl RequestQueue {
    /// Create new queue from configuration
    pub fn new(config: QueueConfig) -> Self;
    
    /// Enqueue a request (returns QueueError::Full if at capacity)
    pub fn enqueue(&self, request: QueuedRequest) -> Result<(), QueueError>;
    
    /// Try to dequeue (high priority first, then normal)
    pub async fn try_dequeue(&self) -> Option<QueuedRequest>;
    
    /// Current total queue depth
    pub fn depth(&self) -> usize;
    
    /// Get queue configuration
    pub fn config(&self) -> &QueueConfig;
}
```

**Concurrency Model**:
- **Enqueue**: Lock-free (atomic depth check, non-blocking try_send)
- **Dequeue**: Mutex-protected (exclusive access to receivers)
- **Depth**: Atomic operations (SeqCst ordering for accuracy)

---

### 6. QueueConfig (Struct)

**Purpose**: Configuration for request queuing behavior.

**Definition**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QueueConfig {
    /// Whether request queuing is enabled
    pub enabled: bool,
    
    /// Maximum number of queued requests
    pub max_size: u32,
    
    /// Maximum wait time for queued requests in seconds
    pub max_wait_seconds: u64,
}
```

**Fields**:
- `enabled: bool`: Master switch for queuing (default: `true`)
- `max_size: u32`: Capacity limit (default: `100`)
- `max_wait_seconds: u64`: Timeout for queued requests (default: `30`)

**Defaults**:
```rust
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

**Validation**:
```rust
impl QueueConfig {
    /// Check if queuing is effectively enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled && self.max_size > 0
    }
}
```

**Special Cases**:
- `enabled = false`: Queuing disabled, all enqueue attempts return `QueueError::Disabled`
- `max_size = 0`: Queuing disabled (equivalent to `enabled = false`)
- `max_wait_seconds = 0`: Requests timeout immediately (not recommended)

**TOML Example**:
```toml
[queue]
enabled = true
max_size = 100
max_wait_seconds = 30
```

---

## Relationships

### Entity Relationship Diagram

```text
┌─────────────────┐
│  QueueConfig    │
│  - enabled      │
│  - max_size     │
│  - max_wait_s   │
└────────┬────────┘
         │ configures
         ▼
┌─────────────────────────┐
│  RequestQueue           │
│  - high_tx/high_rx      │
│  - normal_tx/normal_rx  │
│  - depth (AtomicUsize)  │
│  - config               │
└────────┬────────────────┘
         │ contains 0..*
         ▼
┌─────────────────────────┐
│  QueuedRequest          │
│  - intent               │
│  - request              │
│  - response_tx          │
│  - enqueued_at          │
│  - priority ───────┐    │
└─────────────────────┼───┘
                      │
         ┌────────────┘
         ▼
┌─────────────────┐
│  Priority       │
│  - High         │
│  - Normal       │
└─────────────────┘
```

### Data Flow

```text
1. API Handler
   ↓ (catch RoutingError::Queue)
2. Create QueuedRequest
   ↓ (with oneshot channel)
3. RequestQueue::enqueue()
   ↓ (route to high_tx or normal_tx)
4. Queue (in-memory)
   ↓ (poll every 50ms)
5. queue_drain_loop::try_dequeue()
   ↓ (high priority first)
6. Re-run routing
   ↓ (select_backend)
7. process_queued_request()
   ↓ (agent.chat_completion)
8. Send response via response_tx
   ↓ (oneshot::Sender::send)
9. API Handler receives response
   ↓ (rx.await)
10. Return to client
```

---

## Memory Layout

### Per-Request Memory

```text
QueuedRequest (stack/heap hybrid)
├── intent: RoutingIntent (~200 bytes)
│   ├── request_id: String (heap)
│   ├── requested_model: String (heap)
│   ├── actual_model: String (heap)
│   ├── requirements: RequestRequirements (stack)
│   └── candidates: Vec<Candidate> (heap)
├── request: ChatCompletionRequest (~500 bytes)
│   ├── model: String (heap)
│   ├── messages: Vec<Message> (heap)
│   └── other fields (stack/heap)
├── response_tx: oneshot::Sender (~32 bytes)
├── enqueued_at: Instant (16 bytes)
└── priority: Priority (1 byte)
────────────────────────────────────
Total: ~750 bytes + variable (messages)
```

### Queue Memory

```text
RequestQueue (Arc-wrapped, shared)
├── high_tx: mpsc::Sender (32 bytes)
├── high_rx: Mutex<mpsc::Receiver> (48 bytes)
├── normal_tx: mpsc::Sender (32 bytes)
├── normal_rx: Mutex<mpsc::Receiver> (48 bytes)
├── depth: Arc<AtomicUsize> (16 bytes)
└── config: QueueConfig (16 bytes)
────────────────────────────────────
Total: ~192 bytes (fixed overhead)

Channel buffers (max_size = 100):
├── high channel: 100 * 8 bytes (pointers) = 800 bytes
└── normal channel: 100 * 8 bytes (pointers) = 800 bytes
────────────────────────────────────
Total buffer: ~1600 bytes

Max memory (100 queued requests):
├── Fixed overhead: 192 bytes
├── Channel buffers: 1600 bytes
└── Requests: 100 * 1KB ≈ 100 KB
────────────────────────────────────
Total: ~102 KB (well under budget)
```

---

## Performance Characteristics

### Time Complexity

| Operation | Best Case | Average Case | Worst Case |
|-----------|-----------|--------------|------------|
| `enqueue()` | O(1) | O(1) | O(1) |
| `try_dequeue()` | O(1) | O(1) | O(1) |
| `depth()` | O(1) | O(1) | O(1) |
| Priority sorting | N/A (channels enforce order) | N/A | N/A |

### Space Complexity

| Component | Space | Notes |
|-----------|-------|-------|
| Queue struct | O(1) | Fixed 192 bytes |
| Channel buffers | O(n) | n = max_size |
| Queued requests | O(n) | n = current depth |
| **Total** | **O(n)** | **Bounded by max_size** |

### Atomic Operations

- **Ordering**: `SeqCst` for accuracy (stronger than necessary, but safe)
- **Contention**: Low (depth updates are infrequent compared to routing)
- **Cache coherency**: MESI protocol handles depth counter efficiently

---

## Edge Cases

### 1. Empty Queue
- **Behavior**: `try_dequeue()` returns `None`
- **Handling**: Drain loop sleeps 50ms and retries

### 2. Full Queue
- **Behavior**: `enqueue()` returns `QueueError::Full`
- **Handling**: API handler returns 503 "Queue is full"

### 3. Disabled Queue
- **Behavior**: `enqueue()` returns `QueueError::Disabled`
- **Handling**: API handler returns 503 "All backends at capacity"

### 4. Timeout While Queued
- **Behavior**: Drain loop checks `enqueued_at.elapsed() > max_wait`
- **Handling**: Return 503 with `Retry-After` header, discard request

### 5. Oneshot Receiver Dropped
- **Behavior**: Client request was cancelled (HTTP connection closed)
- **Handling**: `response_tx.send()` fails silently, request is discarded

### 6. Graceful Shutdown
- **Behavior**: Cancellation token signals drain loop to stop
- **Handling**: `drain_remaining()` sends 503 to all queued requests

### 7. Race Condition (Concurrent Enqueue)
- **Behavior**: Multiple threads call `enqueue()` simultaneously
- **Handling**: Atomic depth check may allow slight over-capacity (acceptable)
- **Mitigation**: `try_send()` fails if channel is full (second line of defense)

### 8. High-Priority Starvation
- **Behavior**: Continuous high-priority traffic blocks normal priority
- **Mitigation**: Not implemented (high-priority is minority traffic in practice)

---

## Validation Rules Summary

| Entity | Rule | Enforcement |
|--------|------|-------------|
| `Priority` | High or Normal only | Parse with default to Normal |
| `QueuedRequest` | `enqueued_at` in past | Set to `Instant::now()` on creation |
| `RequestQueue` | `depth <= max_size` | Atomic check in `enqueue()` |
| `QueueConfig` | `max_size >= 0` | Type system (u32 is non-negative) |
| `QueueConfig` | `max_wait_seconds >= 0` | Type system (u64 is non-negative) |
| `QueueConfig` | Effective enable | `enabled && max_size > 0` |

---

## Testing Strategy

### Unit Tests (src/queue/mod.rs)
- FIFO ordering within each priority level
- Priority ordering (high before normal)
- Capacity enforcement (reject when full)
- Depth counter accuracy
- Disabled queue behavior
- Priority parsing (valid/invalid headers)

### Integration Tests (tests/queue_test.rs)
- End-to-end enqueue → dequeue → response flow
- Timeout behavior
- Graceful shutdown (drain remaining)
- Concurrent enqueue stress test

### Property-Based Tests
- Not implemented (simple enough for example-based tests)
- Could add proptest for concurrent enqueue/dequeue invariants

---

**Data Model Complete** | **Ready for contract definition** | **Phase 1**
