# API Contract: Request Queue Types

**Phase 1 Contract** | **Date**: 2026-02-15 (Retrospective) | **Feature**: F18

This document defines the public API types for the request queuing feature.

## Public Types

### Priority Enum

**Location**: `src/queue/mod.rs`

**Visibility**: `pub`

**Definition**:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    High,
    Normal,
}
```

**Public Methods**:
```rust
impl Priority {
    /// Parse priority from header value. Invalid values default to Normal.
    ///
    /// # Examples
    /// ```
    /// use nexus::queue::Priority;
    /// assert_eq!(Priority::from_header("high"), Priority::High);
    /// assert_eq!(Priority::from_header("HIGH"), Priority::High);
    /// assert_eq!(Priority::from_header("normal"), Priority::Normal);
    /// assert_eq!(Priority::from_header("invalid"), Priority::Normal);
    /// assert_eq!(Priority::from_header(""), Priority::Normal);
    /// ```
    pub fn from_header(value: &str) -> Self;
}
```

**Header Contract**:
- **Header Name**: `X-Nexus-Priority`
- **Valid Values**: `"high"`, `"normal"` (case-insensitive)
- **Default**: `Normal` (when header is missing or invalid)
- **Invalid Values**: All other strings default to `Normal`

**Examples**:
```text
X-Nexus-Priority: high      → Priority::High
X-Nexus-Priority: HIGH      → Priority::High
X-Nexus-Priority: normal    → Priority::Normal
X-Nexus-Priority: low       → Priority::Normal (invalid)
(no header)                 → Priority::Normal (default)
```

---

### QueuedRequest Struct

**Location**: `src/queue/mod.rs`

**Visibility**: `pub`

**Definition**:
```rust
pub struct QueuedRequest {
    pub intent: RoutingIntent,
    pub request: ChatCompletionRequest,
    pub response_tx: oneshot::Sender<QueueResponse>,
    pub enqueued_at: Instant,
    pub priority: Priority,
}
```

**Field Contracts**:
- `intent`: Routing context from reconciler pipeline (carries request ID, model, requirements)
- `request`: Original chat completion request from client (validated before enqueue)
- `response_tx`: One-time channel to send response back to waiting handler
- `enqueued_at`: Timestamp when request was enqueued (for timeout calculation)
- `priority`: High or Normal priority level

**Debug Implementation**:
```rust
impl std::fmt::Debug for QueuedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueuedRequest")
            .field("priority", &self.priority)
            .field("enqueued_at", &self.enqueued_at)
            .finish()
    }
}
```
*Note*: Omits `response_tx` from debug output (oneshot::Sender is not Debug).

**Lifecycle Contract**:
1. Created in API handler when `RoutingError::Queue` is caught
2. Enqueued via `RequestQueue::enqueue()`
3. Dequeued via `RequestQueue::try_dequeue()` in drain loop
4. Processed or timed out
5. Response sent via `response_tx`, struct is consumed

---

### QueueError Enum

**Location**: `src/queue/mod.rs`

**Visibility**: `pub`

**Definition**:
```rust
#[derive(Debug, Error)]
pub enum QueueError {
    #[error("Queue is full ({max_size} requests)")]
    Full { max_size: u32 },

    #[error("Request queuing is disabled")]
    Disabled,
}
```

**Error Variants**:

#### `Full { max_size: u32 }`
- **When**: Queue depth has reached configured `max_size`
- **Cause**: `depth >= config.max_size` when calling `enqueue()`
- **HTTP Response**: 503 Service Unavailable
- **Body**: `{"error": {"message": "All backends at capacity and queue is full", ...}}`
- **Retry Strategy**: Exponential backoff, check `max_size` for capacity hint

#### `Disabled`
- **When**: Queuing is disabled in configuration
- **Cause**: `config.enabled = false` OR `config.max_size = 0`
- **HTTP Response**: 503 Service Unavailable
- **Body**: `{"error": {"message": "All backends at capacity", ...}}`
- **Retry Strategy**: Exponential backoff, no capacity hint available

**Error Handling Contract**:
```rust
match queue.enqueue(queued) {
    Ok(()) => {
        // Success: wait on oneshot receiver
    }
    Err(QueueError::Full { max_size }) => {
        // Queue full: return 503 with capacity hint
        warn!("Queue full ({} requests)", max_size);
        return ApiError::service_unavailable(
            "All backends at capacity and queue is full"
        ).into_response();
    }
    Err(QueueError::Disabled) => {
        // Queue disabled: return 503 without hint
        return ApiError::service_unavailable(
            "All backends at capacity"
        ).into_response();
    }
}
```

---

### QueueResponse Type Alias

**Location**: `src/queue/mod.rs`

**Visibility**: `pub`

**Definition**:
```rust
pub type QueueResponse = Result<axum::response::Response, crate::api::ApiError>;
```

**Semantics**:
- `Ok(Response)`: Successful completion
  - Contains HTTP response with chat completion JSON body
  - Status: 200 OK
  - Headers: OpenAI-compatible + Nexus transparent headers
- `Err(ApiError)`: Error occurred during processing
  - Contains structured API error (OpenAI format)
  - Status: 4xx or 5xx (depends on error type)
  - Examples: 503 (timeout), 502 (backend error), 400 (validation error)

**Channel Contract**:
```rust
// Sender side (drain loop)
let response: QueueResponse = process_queued_request(...).await;
let _ = queued.response_tx.send(response);

// Receiver side (API handler)
let response: QueueResponse = rx.await
    .map_err(|_| ApiError::internal("Response channel closed"))?;
match response {
    Ok(http_response) => return http_response,
    Err(api_error) => return api_error.into_response(),
}
```

---

### QueueConfig Struct

**Location**: `src/config/queue.rs`

**Visibility**: `pub`

**Definition**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QueueConfig {
    pub enabled: bool,
    pub max_size: u32,
    pub max_wait_seconds: u64,
}
```

**Field Contracts**:

#### `enabled: bool`
- **Purpose**: Master switch for queuing
- **Default**: `true`
- **Effect**: When `false`, all enqueue attempts return `QueueError::Disabled`

#### `max_size: u32`
- **Purpose**: Maximum number of queued requests (capacity limit)
- **Default**: `100`
- **Range**: `0..=u32::MAX`
- **Special**: When `0`, queuing is disabled (equivalent to `enabled = false`)

#### `max_wait_seconds: u64`
- **Purpose**: Maximum time a request can wait in queue before timeout
- **Default**: `30` seconds
- **Range**: `0..=u64::MAX`
- **Warning**: Setting to `0` causes immediate timeouts (not recommended)

**TOML Serialization**:
```toml
[queue]
enabled = true
max_size = 100
max_wait_seconds = 30
```

**Default Implementation**:
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

**Public Methods**:
```rust
impl QueueConfig {
    /// Check if queuing is effectively enabled.
    ///
    /// Queuing is disabled if either enabled=false or max_size=0.
    ///
    /// # Examples
    /// ```
    /// use nexus::config::QueueConfig;
    /// 
    /// let config = QueueConfig { enabled: true, max_size: 100, max_wait_seconds: 30 };
    /// assert!(config.is_enabled());
    /// 
    /// let config = QueueConfig { enabled: false, max_size: 100, max_wait_seconds: 30 };
    /// assert!(!config.is_enabled());
    /// 
    /// let config = QueueConfig { enabled: true, max_size: 0, max_wait_seconds: 30 };
    /// assert!(!config.is_enabled());
    /// ```
    pub fn is_enabled(&self) -> bool {
        self.enabled && self.max_size > 0
    }
}
```

---

## HTTP Contract

### Request Headers

#### `X-Nexus-Priority` (Optional)

**Type**: String  
**Valid Values**: `"high"`, `"normal"` (case-insensitive)  
**Default**: `"normal"`  
**Invalid Behavior**: Defaults to `"normal"`

**Examples**:
```http
POST /v1/chat/completions HTTP/1.1
X-Nexus-Priority: high
Content-Type: application/json

{
  "model": "llama3:8b",
  "messages": [{"role": "user", "content": "urgent request"}]
}
```

```http
POST /v1/chat/completions HTTP/1.1
X-Nexus-Priority: normal
Content-Type: application/json

{
  "model": "llama3:8b",
  "messages": [{"role": "user", "content": "background task"}]
}
```

### Response Headers (Timeout)

#### `Retry-After` (Seconds)

**Type**: Integer (seconds)  
**When**: Request timed out in queue  
**Value**: Equal to `config.max_wait_seconds`  
**HTTP Status**: 503 Service Unavailable

**Example**:
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

---

## Compatibility Guarantees

### Semantic Versioning Contract

**Public API Surface**:
- `Priority` enum and `from_header()` method
- `QueuedRequest` struct (fields are public)
- `QueueError` enum variants
- `QueueResponse` type alias
- `QueueConfig` struct and `is_enabled()` method
- `X-Nexus-Priority` header name and values

**Breaking Changes** (require major version bump):
- Removing `Priority::High` or `Priority::Normal`
- Changing `X-Nexus-Priority` header name
- Removing `QueueError::Full` or `QueueError::Disabled`
- Changing `QueueConfig` field names
- Changing default values in `QueueConfig::default()`

**Non-Breaking Changes** (minor/patch version):
- Adding new priority levels (if enum is marked `#[non_exhaustive]`)
- Adding new fields to `QueuedRequest` (struct is already non-exhaustive)
- Adding new error variants to `QueueError` (if enum is marked `#[non_exhaustive]`)
- Changing internal implementation (channel types, depth tracking, etc.)

### OpenAI Compatibility

**Guaranteed**:
- Error responses match OpenAI format exactly
- `X-Nexus-*` headers are additive (don't break OpenAI clients)
- Priority header is optional (clients can ignore it)
- Queue behavior is transparent to clients (requests may be delayed but responses are identical)

**Not Guaranteed**:
- Queue position in error responses (not exposed)
- Estimated wait time in error responses (not exposed)
- Per-tenant queue quotas (not implemented)

---

## Deprecation Policy

**Current Status**: All types are active, no deprecations.

**Future Deprecations** (if needed):
1. Deprecation warning in docs (1 minor version)
2. `#[deprecated]` attribute (1 minor version)
3. Removal (next major version)

**Example**:
```rust
// Minor version N
pub enum Priority {
    High,
    Normal,
    #[deprecated(since = "0.5.0", note = "Use High instead")]
    Critical,
}

// Major version N+1
pub enum Priority {
    High,
    Normal,
    // Critical removed
}
```

---

## Examples

### Example 1: Enqueue High-Priority Request

```rust
use nexus::queue::{Priority, QueuedRequest};
use std::time::Instant;

// Extract priority from header
let priority = headers
    .get("x-nexus-priority")
    .and_then(|v| v.to_str().ok())
    .map(Priority::from_header)
    .unwrap_or(Priority::Normal);

// Create queued request
let (tx, rx) = tokio::sync::oneshot::channel();
let queued = QueuedRequest {
    intent: routing_intent,
    request: chat_request,
    response_tx: tx,
    enqueued_at: Instant::now(),
    priority,
};

// Enqueue and wait
match queue.enqueue(queued) {
    Ok(()) => {
        let max_wait = Duration::from_secs(queue.config().max_wait_seconds);
        match tokio::time::timeout(max_wait, rx).await {
            Ok(Ok(response)) => return response,
            _ => return timeout_response(),
        }
    }
    Err(e) => return error_response(e),
}
```

### Example 2: Handle Queue Errors

```rust
match queue.enqueue(queued) {
    Ok(()) => { /* success path */ }
    
    Err(QueueError::Full { max_size }) => {
        warn!("Queue full: {}/{} requests", depth, max_size);
        return ApiError::service_unavailable(
            "All backends at capacity and queue is full"
        ).into_response();
    }
    
    Err(QueueError::Disabled) => {
        return ApiError::service_unavailable(
            "All backends at capacity"
        ).into_response();
    }
}
```

### Example 3: Configuration

```toml
# Enable queuing with custom settings
[queue]
enabled = true
max_size = 200          # Hold up to 200 requests
max_wait_seconds = 60   # Wait up to 60 seconds

# Disable queuing
[queue]
enabled = false

# Alternative: disable via max_size
[queue]
enabled = true
max_size = 0            # Equivalent to enabled=false
```

---

**Type Contract Complete** | **Phase 1** | **Ready for implementation**
