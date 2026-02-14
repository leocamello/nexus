# Data Model: API Gateway (F04)

**Date**: 2025-01-10  
**Phase**: Phase 1 - Foundation

This document defines the data entities and their relationships for the API Gateway feature.

## Core Entities

### 1. AppState

**Purpose**: Shared application state accessible to all Axum handlers via `State<Arc<AppState>>`.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `registry` | `Arc<Registry>` | Shared backend registry |
| `config` | `Arc<NexusConfig>` | Immutable server configuration |
| `http_client` | `reqwest::Client` | Connection-pooled client; timeout from `config.server.request_timeout_seconds` |
| `router` | `Arc<routing::Router>` | Configured with aliases, fallbacks, and weights from config |
| `start_time` | `Instant` | Server startup time for uptime tracking |
| `metrics_collector` | `Arc<MetricsCollector>` | Prometheus metrics management |
| `request_history` | `Arc<RequestHistory>` | Ring buffer for dashboard history |
| `ws_broadcast` | `broadcast::Sender<WebSocketUpdate>` | WebSocket channel (capacity 1000) |

**Responsibilities**:
- Initialize HTTP client with timeout and connection pool (`pool_max_idle_per_host: 10`)
- Create Router from config (strategy, weights, aliases, fallbacks)
- Initialize Prometheus metrics exporter
- Provide shared state to all endpoint handlers

**Lifecycle**: Created once at server startup via `AppState::new()`. Shared via `Arc<AppState>` across all handlers. Lives until server shutdown.

**Thread Safety**: All fields are either `Arc`-wrapped or inherently thread-safe (`Instant`, `broadcast::Sender`).

---

### 2. ChatCompletionRequest

**Purpose**: Incoming chat completion request matching the OpenAI API format.

**Attributes**:

| Attribute | Type | Default | Constraints |
|-----------|------|---------|-------------|
| `model` | `String` | — | Required; used for routing |
| `messages` | `Vec<ChatMessage>` | — | Required; conversation history |
| `stream` | `bool` | `false` | Enables SSE streaming response |
| `temperature` | `Option<f32>` | `None` | Passed through to backend |
| `max_tokens` | `Option<u32>` | `None` | Passed through to backend |
| `top_p` | `Option<f32>` | `None` | Passed through to backend |
| `stop` | `Option<Vec<String>>` | `None` | Stop sequences |
| `presence_penalty` | `Option<f32>` | `None` | Passed through to backend |
| `frequency_penalty` | `Option<f32>` | `None` | Passed through to backend |
| `user` | `Option<String>` | `None` | End-user identifier |
| `extra` | `HashMap<String, serde_json::Value>` | `{}` | Catch-all for unknown fields (`#[serde(flatten)]`) |

**Responsibilities**:
- Deserialize any OpenAI-compatible request body
- Pass through unknown fields to backends via `extra` (forward-compatible)
- Provide `model` for routing and `messages` for capability detection

**Validation**: Axum JSON extractor handles deserialization errors. No additional validation beyond serde.

---

### 3. ChatMessage

**Purpose**: A single message in the conversation with role and content.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `role` | `String` | `"system"`, `"user"`, `"assistant"`, `"tool"` |
| `content` | `MessageContent` | Flattened enum: text or multimodal parts |
| `name` | `Option<String>` | Optional sender name |

---

### 4. MessageContent (Enum)

**Purpose**: Supports both plain text and multimodal (vision) message content.

**Variants**:

| Variant | Fields | Deserialization |
|---------|--------|-----------------|
| `Text` | `content: String` | `{ "content": "Hello" }` |
| `Parts` | `content: Vec<ContentPart>` | `{ "content": [{ "type": "text", ... }] }` |

**Serde**: `#[serde(untagged)]` — deserialized by trying `Text` first, then `Parts`.

**Used by Router**: `Parts` variant with `image_url` parts triggers vision requirement detection.

---

### 5. ContentPart

**Purpose**: A single content element in a multimodal message.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `part_type` | `String` | `"text"` or `"image_url"` (serialized as `"type"`) |
| `text` | `Option<String>` | Present when `part_type` = `"text"` |
| `image_url` | `Option<ImageUrl>` | Present when `part_type` = `"image_url"` |

---

### 6. ImageUrl

**Purpose**: Image reference for vision requests.

**Attributes**: `url: String` — Base64 data URI or HTTP URL.

---

### 7. ChatCompletionResponse

**Purpose**: Non-streaming response from a backend in OpenAI format.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `id` | `String` | Unique completion ID (e.g., `"chatcmpl-123"`) |
| `object` | `String` | Always `"chat.completion"` |
| `created` | `i64` | Unix timestamp |
| `model` | `String` | Model that generated the response |
| `choices` | `Vec<Choice>` | One or more completion choices |
| `usage` | `Option<Usage>` | Token usage statistics (backend-dependent) |

---

### 8. Choice / Usage

**Choice**: A single completion choice — `index: u32`, `message: ChatMessage`, `finish_reason: Option<String>` (`"stop"`, `"length"`, etc.).

**Usage**: Token statistics — `prompt_tokens: u32`, `completion_tokens: u32`, `total_tokens: u32`. Optional in response (not all backends provide counts).

---

### 9. Streaming Types (ChatCompletionChunk, ChunkChoice, ChunkDelta)

**Purpose**: SSE streaming response types mirroring OpenAI's chunked format.

**ChatCompletionChunk**: Same structure as `ChatCompletionResponse` but with `object: "chat.completion.chunk"` and `Vec<ChunkChoice>` instead of `Vec<Choice>`.

**ChunkChoice**: Contains `index: u32`, `delta: ChunkDelta`, `finish_reason: Option<String>` (set on final chunk).

**ChunkDelta**: Incremental content — `role: Option<String>` (first chunk only), `content: Option<String>` (token fragment; `None` in final chunk).

---

### 10. ApiError

**Purpose**: OpenAI-compatible error response with HTTP status code mapping.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `error` | `ApiErrorBody` | Nested error details |

**Factory Methods**:

| Method | HTTP Status | Code |
|--------|-------------|------|
| `bad_request(msg)` | 400 | `invalid_request_error` |
| `model_not_found(model, available)` | 404 | `model_not_found` |
| `bad_gateway(msg)` | 502 | `bad_gateway` |
| `gateway_timeout()` | 504 | `gateway_timeout` |
| `service_unavailable(msg)` | 503 | `service_unavailable` |

**Status Code Mapping**: `status_code()` matches on `error.code` field. Unknown codes → 500.

**Implements**: `IntoResponse` for Axum — returns `(StatusCode, Json(self))`.

---

### 11. ApiErrorBody

**Purpose**: Error detail fields matching OpenAI error schema.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `message` | `String` | Human-readable error description |
| `type` | `String` | Error category (e.g., `"invalid_request_error"`, `"server_error"`) |
| `param` | `Option<String>` | Parameter that caused the error (e.g., `"model"`) |
| `code` | `Option<String>` | Machine-readable error code |

---

### 12. HealthResponse / BackendCounts

**Purpose**: System health response from `GET /health`.

**HealthResponse**: `status: String`, `uptime_seconds: u64`, `backends: BackendCounts`, `models: usize`.

**BackendCounts**: `total: usize`, `healthy: usize`, `unhealthy: usize`.

**Status Logic**: All healthy + count > 0 → `"healthy"`; some healthy → `"degraded"`; none → `"unhealthy"`.

---

### 13. ModelsResponse / ModelObject

**Purpose**: Model listing from `GET /v1/models` in OpenAI format.

**ModelsResponse**: `object: "list"`, `data: Vec<ModelObject>` (sorted by ID).

**ModelObject**: `id`, `object: "model"`, `created` (timestamp), `owned_by: "nexus"`, `context_length: Option<u32>`, `capabilities: Option<ModelCapabilities>` (vision, tools, json_mode).

**Deduplication**: Models deduplicated by ID across healthy backends via `HashMap`. First occurrence wins.

---

## Entity Relationships

```
┌─────────────────────────────┐
│         AppState            │
│                             │
│  registry ──────────────────┼──► Arc<Registry>
│  config ────────────────────┼──► Arc<NexusConfig>
│  http_client                │
│  router ────────────────────┼──► Arc<routing::Router>
│  start_time                 │
│  metrics_collector ─────────┼──► Arc<MetricsCollector>
│  request_history ───────────┼──► Arc<RequestHistory>
│  ws_broadcast               │
└─────────────────────────────┘
         │
         │ State<Arc<AppState>>
         ▼
┌─────────────────────────────────────────────┐
│              Axum Router                     │
│                                             │
│  POST /v1/chat/completions ─► completions   │
│    Input:  ChatCompletionRequest            │
│    Output: ChatCompletionResponse (JSON)    │
│            OR Sse<ChatCompletionChunk>       │
│            OR ApiError                       │
│                                             │
│  GET /v1/models ──────────► models          │
│    Output: ModelsResponse                    │
│                                             │
│  GET /health ─────────────► health          │
│    Output: HealthResponse                    │
└─────────────────────────────────────────────┘

ChatCompletionRequest
  ├── model: String
  ├── messages: Vec<ChatMessage>
  │     ├── role: String
  │     └── content: MessageContent
  │           ├── Text { content }
  │           └── Parts { Vec<ContentPart> }
  │                  ├── text
  │                  └── image_url: ImageUrl
  ├── stream: bool
  └── extra: HashMap (passthrough)

ChatCompletionResponse          ChatCompletionChunk
  ├── choices: Vec<Choice>        ├── choices: Vec<ChunkChoice>
  │     ├── message                │     ├── delta: ChunkDelta
  │     └── finish_reason          │     └── finish_reason
  └── usage: Option<Usage>        └── (no usage)
```

---

## State Transitions

### Non-Streaming Request Flow

```
POST /v1/chat/completions (stream: false)
    ↓
Parse JSON → ChatCompletionRequest
    ↓
Extract RequestRequirements → router.select_backend()
    ├── Error → ApiError (404/503/400) + record error metrics
    └── Ok(RoutingResult { backend, actual_model, fallback_used })
            ↓
        For attempt in 0..=max_retries:
            increment_pending → proxy_request
            ├── Ok → decrement_pending, record metrics/history
            │        Add X-Nexus-Fallback-Model if fallback
            │        Return Json(response)
            └── Err → decrement_pending, record error, retry
            ↓
        All retries failed → ApiError::bad_gateway
```

### Streaming Request Flow

```
POST /v1/chat/completions (stream: true)
    ↓
router.select_backend(requirements) → Error → ApiError
    ↓ Ok(backend)
registry.increment_pending(backend_id)
    ↓
Create SSE stream (async_stream):
  POST to backend with stream: true
  ├── Connection/HTTP error → yield error chunk + [DONE]
  └── Success → buffer bytes → parse "data: " lines
        ├── "[DONE]" → yield [DONE]
        └── JSON → yield as Event::data
    ↓
registry.decrement_pending(backend_id)
Return Sse::new(stream) + X-Nexus-Fallback-Model if applicable
```

---

## Validation & Constraints

### Request Body Size Limit

**Rule**: Request body limited to 10 MB via `RequestBodyLimitLayer`.

**Constant**: `MAX_BODY_SIZE = 10 * 1024 * 1024`

---

### Authorization Header Forwarding

**Rule**: If the incoming request contains an `Authorization` header, it is forwarded to the backend. No header manipulation or token injection by Nexus.

---

### Fallback Header

**Rule**: When a fallback model is used, the response includes `X-Nexus-Fallback-Model: <actual_model>` header. Never modifies the JSON response body (OpenAI compatibility principle).

**Constant**: `FALLBACK_HEADER = "x-nexus-fallback-model"` (lowercase for HTTP/2).

---

### Error Format Compliance

**Rule**: All errors are returned in OpenAI error format:
```json
{
  "error": {
    "message": "...",
    "type": "...",
    "param": "...",
    "code": "..."
  }
}
```

---

### Model Deduplication

**Rule**: `GET /v1/models` returns each model ID only once, even if available on multiple backends. First occurrence wins. Results sorted alphabetically by ID.

---

### Streaming Error Handling

**Rule**: Streaming errors are communicated as SSE events (not HTTP error codes, since headers are already sent). Error chunks use `finish_reason: "error"` and content prefixed with `[Error: ...]`. Stream always terminates with `data: [DONE]`.

---

## Performance Characteristics

| Operation | Target Latency | Implementation |
|-----------|----------------|----------------|
| Request parsing | < 1ms | Axum JSON extractor (serde) |
| Router selection | < 1ms | Score computation + alias resolution |
| Proxy overhead | < 5ms | reqwest with connection pool |
| SSE chunk forwarding | < 0.1ms | Byte stream passthrough |
| Health endpoint | < 1ms | Registry iteration + counting |
| Models endpoint | < 1ms | Registry iteration + HashMap dedup |
| Error construction | < 0.1ms | String formatting |

**Connection Pool**: `pool_max_idle_per_host: 10` — keeps up to 10 idle TCP connections per backend.

**Request Timeout**: Configurable via `config.server.request_timeout_seconds` (default 300s). Applied at the reqwest client level.

**Broadcast Channel**: Capacity 1000 messages. Oldest messages dropped on overflow (non-blocking send).

**Memory**: `AppState` ~500 bytes. Per-request types are short-lived and stack-allocated where possible.

---

## Future Extensions

### Not in Current Scope

1. **Request queuing**: No request queue; backends receive all requests immediately
2. **Load shedding**: No active rejection based on backend load
3. **Request caching**: No response caching for identical requests
4. **Multi-model requests**: Each request targets exactly one model
5. **Custom response headers**: Only `X-Nexus-Fallback-Model` added currently
6. **WebSocket proxying**: Only SSE streaming supported; no WebSocket pass-through

These are mentioned for awareness but are NOT part of F04 implementation.
