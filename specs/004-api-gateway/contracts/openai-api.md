# OpenAI-Compatible API Gateway Contract

This document defines the external HTTP API endpoints, request/response schemas, streaming protocol, error format, and custom headers.

**Source**: `src/api/mod.rs`, `src/api/completions.rs`, `src/api/models.rs`, `src/api/health.rs`, `src/api/types.rs`

---

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/v1/chat/completions` | Chat completion (streaming and non-streaming) |
| `GET` | `/v1/models` | List available models from healthy backends |
| `GET` | `/health` | System health status |

**Base URL**: `http://{host}:{port}` (default: `http://0.0.0.0:8000`)
**Body Limit**: 10 MB maximum request body size

---

## `POST /v1/chat/completions`

### Request

**Content-Type**: `application/json`

```json
{
  "model": "llama3:70b",
  "messages": [
    {
      "role": "system",
      "content": "You are a helpful assistant."
    },
    {
      "role": "user",
      "content": "Hello, how are you?"
    }
  ],
  "stream": false,
  "temperature": 0.7,
  "max_tokens": 1000,
  "top_p": 0.9,
  "stop": ["\n"],
  "presence_penalty": 0.0,
  "frequency_penalty": 0.0,
  "user": "user-123"
}
```

### Request Fields

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `model` | string | Yes | — | Model ID (supports aliases) |
| `messages` | array | Yes | — | Conversation messages |
| `stream` | bool | No | `false` | Enable SSE streaming |
| `temperature` | float | No | — | Sampling temperature |
| `max_tokens` | u32 | No | — | Maximum output tokens |
| `top_p` | float | No | — | Nucleus sampling threshold |
| `stop` | string[] | No | — | Stop sequences |
| `presence_penalty` | float | No | — | Presence penalty |
| `frequency_penalty` | float | No | — | Frequency penalty |
| `user` | string | No | — | End-user identifier |

Additional fields are passed through to the backend via `#[serde(flatten)]`.

### Message Format

**Text message**:
```json
{
  "role": "user",
  "content": "Hello"
}
```

**Multimodal message** (vision):
```json
{
  "role": "user",
  "content": [
    { "type": "text", "text": "What's in this image?" },
    { "type": "image_url", "image_url": { "url": "data:image/png;base64,..." } }
  ]
}
```

**Role values**: `"system"`, `"user"`, `"assistant"`

### Non-Streaming Response

**Status**: `200 OK`
**Content-Type**: `application/json`

```json
{
  "id": "chatcmpl-abc123",
  "object": "chat.completion",
  "created": 1699999999,
  "model": "llama3:70b",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Hello! I'm doing well, thank you for asking."
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 25,
    "completion_tokens": 12,
    "total_tokens": 37
  }
}
```

### Response Fields

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Unique completion ID |
| `object` | string | Always `"chat.completion"` |
| `created` | integer | Unix timestamp |
| `model` | string | Model used (from backend) |
| `choices` | array | Completion choices |
| `usage` | object? | Token usage statistics (optional) |

#### Choice Object

| Field | Type | Description |
|-------|------|-------------|
| `index` | u32 | Choice index |
| `message` | object | Assistant's response message |
| `finish_reason` | string? | `"stop"`, `"length"`, or `null` |

#### Usage Object

| Field | Type | Description |
|-------|------|-------------|
| `prompt_tokens` | u32 | Input token count |
| `completion_tokens` | u32 | Output token count |
| `total_tokens` | u32 | Total token count |

---

## Streaming Response (SSE)

When `stream: true`, the response uses Server-Sent Events.

**Status**: `200 OK`
**Content-Type**: `text/event-stream`

### SSE Stream Format

Each event is a `data:` line followed by a blank line:

```
data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1699999999,"model":"llama3:70b","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}

data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1699999999,"model":"llama3:70b","choices":[{"index":0,"delta":{"content":"Hello!"},"finish_reason":null}]}

data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1699999999,"model":"llama3:70b","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]

```

### Chunk Schema

```json
{
  "id": "chatcmpl-abc123",
  "object": "chat.completion.chunk",
  "created": 1699999999,
  "model": "llama3:70b",
  "choices": [
    {
      "index": 0,
      "delta": {
        "role": "assistant",
        "content": "Hello"
      },
      "finish_reason": null
    }
  ]
}
```

#### Delta Object

| Field | Type | Description |
|-------|------|-------------|
| `role` | string? | Set on first chunk only |
| `content` | string? | Token text (omitted when empty) |

### Stream Termination

- Normal completion: `data: [DONE]` event
- Backend error during stream: Error chunk with `finish_reason: "error"` followed by `data: [DONE]`
- Error chunk uses `model: "error"` and `id: "chatcmpl-error-{uuid}"`
- Error content format: `"[Error: {message}]"`

---

## `GET /v1/models`

Lists all models from healthy backends. Each model appears once per backend that
serves it, with `owned_by` set to the backend name for multi-backend visibility.

### Response

**Status**: `200 OK`
**Content-Type**: `application/json`

```json
{
  "object": "list",
  "data": [
    {
      "id": "llama3:70b",
      "object": "model",
      "created": 1699999999,
      "owned_by": "local-ollama",
      "context_length": 4096,
      "capabilities": {
        "vision": false,
        "tools": false,
        "json_mode": false
      }
    },
    {
      "id": "llama3:70b",
      "object": "model",
      "created": 1699999999,
      "owned_by": "gpu-server",
      "context_length": 4096,
      "capabilities": {
        "vision": false,
        "tools": false,
        "json_mode": false
      }
    },
    {
      "id": "mistral:7b",
      "object": "model",
      "created": 1699999999,
      "owned_by": "local-ollama",
      "context_length": 4096,
      "capabilities": {
        "vision": false,
        "tools": true,
        "json_mode": false
      }
    }
  ]
}
```

### Model Object Fields

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Model identifier |
| `object` | string | Always `"model"` |
| `created` | integer | Unix timestamp (set at response time) |
| `owned_by` | string | Backend name serving this model |
| `context_length` | u32? | Maximum context window (Nexus extension) |
| `capabilities` | object? | Model capability flags (Nexus extension) |

**Notes**:
- `context_length` and `capabilities` are Nexus extensions — serialized with `skip_serializing_if = "Option::is_none"`
- Models sorted alphabetically by `id`, then by `owned_by`; only healthy backends included
- Same model served by multiple backends appears multiple times (one entry per backend)

---

## `GET /health`

System health status endpoint. See `specs/002-health-checker/contracts/health-endpoints.md` for full details.

### Response

**Status**: `200 OK`
**Content-Type**: `application/json`

```json
{
  "status": "healthy",
  "uptime_seconds": 3600,
  "backends": {
    "total": 3,
    "healthy": 2,
    "unhealthy": 1
  },
  "models": 5
}
```

---

## Error Responses

All errors follow the OpenAI error format:

```json
{
  "error": {
    "message": "Human-readable error description",
    "type": "error_category",
    "param": "field_name_or_null",
    "code": "machine_readable_code"
  }
}
```

### Error Types

| HTTP Status | Code | Type | When |
|-------------|------|------|------|
| `400` | `invalid_request_error` | `invalid_request_error` | Invalid request or capability mismatch |
| `404` | `model_not_found` | `invalid_request_error` | Model not found or fallback chain exhausted |
| `502` | `bad_gateway` | `server_error` | Backend connection failed or returned error |
| `503` | `service_unavailable` | `server_error` | No healthy backend for requested model |
| `504` | `gateway_timeout` | `server_error` | Backend request timed out |
| `500` | (other/none) | `server_error` | Unknown error code |

### Error Examples

**Model Not Found (404)**:
```json
{
  "error": {
    "message": "Model 'gpt-4' not found. Available: llama3:70b, mistral:7b",
    "type": "invalid_request_error",
    "param": "model",
    "code": "model_not_found"
  }
}
```

**No Healthy Backend (503)**:
```json
{
  "error": {
    "message": "No healthy backend available for model 'llama3:70b'",
    "type": "server_error",
    "param": null,
    "code": "service_unavailable"
  }
}
```

**Bad Gateway (502)**: `"Backend returned 500: Internal Server Error"` with `code: "bad_gateway"`

**Gateway Timeout (504)**: `"Backend request timed out"` with `code: "gateway_timeout"`

**Capability Mismatch (400)**: `"Model 'mistral:7b' lacks required capabilities: [\"vision\"]"` with `code: "invalid_request_error"`

---

## Custom Headers

Nexus adds metadata via `X-Nexus-*` response headers (never modifies the JSON response body).

| Header | Value | When |
|--------|-------|------|
| `x-nexus-fallback-model` | Actual model ID used | Request was served by a fallback model (alias or fallback chain) |

**Example**:
```http
HTTP/1.1 200 OK
Content-Type: application/json
x-nexus-fallback-model: llama3:70b

{"id":"chatcmpl-abc123","object":"chat.completion",...}
```

---

## Request Routing Flow

1. **Parse request** — Validate JSON, extract model name and requirements
2. **Resolve aliases** — Map model name through alias chain (max 3 levels)
3. **Extract requirements** — Detect vision (image_url in content), tools, context needs
4. **Select backend** — Router scores candidates by strategy (smart, round_robin, etc.)
5. **Proxy request** — Forward to `{backend.url}/v1/chat/completions`
6. **Retry on failure** — Up to `max_retries` attempts on the same backend
7. **Return response** — Forward backend response with optional `X-Nexus-*` headers

### Header Forwarding

- `Authorization` header is forwarded from client to backend
- Other client headers are not forwarded

### Retry Logic

- Maximum retries configured via `routing.max_retries` (default: 2)
- Retries are attempted on the same backend
- `pending_requests` is incremented before each attempt and decremented after
- If all retries fail, returns the last error as `502 Bad Gateway`

---

## Metrics Integration

Each request records the following metrics:

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `nexus_requests_total` | Counter | `model`, `backend`, `status` | Request count |
| `nexus_request_duration_seconds` | Histogram | `model`, `backend` | Request duration |
| `nexus_fallbacks_total` | Counter | `from_model`, `to_model` | Fallback usage |
| `nexus_tokens_total` | Histogram | `model`, `backend`, `type` | Token counts (`prompt`/`completion`) |
| `nexus_errors_total` | Counter | `error_type`, `model` | Error count |

**Error Types**: `"model_not_found"`, `"fallback_exhausted"`, `"no_healthy_backend"`, `"capability_mismatch"`, `"timeout"`, `"backend_error"`

---

## Example Request/Response

---

## Implementation Notes

### AppState

Shared state available to all handlers via Axum's `State` extractor:

```rust
pub struct AppState {
    pub registry: Arc<Registry>,
    pub config: Arc<NexusConfig>,
    pub http_client: reqwest::Client,
    pub router: Arc<routing::Router>,
    pub start_time: Instant,
    pub metrics_collector: Arc<MetricsCollector>,
    pub request_history: Arc<RequestHistory>,
    pub ws_broadcast: broadcast::Sender<WebSocketUpdate>,
}
```

### HTTP Client

- Built with `reqwest::Client::builder()`
- Timeout: `config.server.request_timeout_seconds` (default: 300s)
- Connection pool: 10 max idle connections per host

### Request ID

Each request generates a unique request ID via `generate_request_id()` for correlation in structured logs.

### Structured Logging

The completions handler uses `#[instrument]` with span fields:
`request_id`, `model`, `actual_model`, `backend`, `backend_type`, `status`, `status_code`, `error_message`, `latency_ms`, `tokens_prompt`, `tokens_completion`, `tokens_total`, `stream`, `route_reason`, `retry_count`, `fallback_chain`

---

## Testing Strategy

### Unit Tests
1. Request/response type deserialization (text, multimodal, minimal, full)
2. Error type serialization and status code mapping
3. Streaming chunk serialization
4. Usage statistics serialization
5. All error constructors and their HTTP status codes

### Integration Tests
1. Non-streaming and streaming chat completion with mock backend
2. Model alias resolution and fallback chain activation end-to-end
3. Retry logic on backend failure
4. `GET /v1/models` with healthy/unhealthy backends
5. Error responses for missing model, no healthy backend, timeout
6. `X-Nexus-Fallback-Model` header presence on fallback
7. Request body size limit enforcement
