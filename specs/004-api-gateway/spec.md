# Feature Specification: Core API Gateway

**Feature Branch**: `004-api-gateway`  
**Created**: 2026-02-03  
**Status**: 📋 Specified  
**Priority**: P0 (MVP)  
**Depends On**: F02 (Backend Registry), F03 (Health Checker)

## Overview

HTTP server exposing OpenAI-compatible endpoints that proxy requests to backends. This is the primary interface for all clients (Claude Code, Continue.dev, Cursor, etc.). The API Gateway receives requests, routes them to appropriate backends via the Router, and streams responses back to clients.

## User Scenarios & Testing

### User Story 1 - Chat Completion (Non-Streaming) (Priority: P1)

As a developer using an OpenAI SDK, I want to send chat completion requests to Nexus so that I can get responses from my local LLM backends.

**Why this priority**: This is the core functionality. Without it, Nexus serves no purpose.

**Independent Test**: Can be tested with a mock backend returning a fixed JSON response.

**Acceptance Scenarios**:

1. **Given** a healthy backend with model "llama3", **When** I POST to /v1/chat/completions with model "llama3", **Then** I receive a valid ChatCompletionResponse
2. **Given** a request with invalid JSON, **When** I POST to /v1/chat/completions, **Then** I receive 400 Bad Request in OpenAI error format
3. **Given** multiple backends with same model, **When** I send a request, **Then** the router selects the best backend

---

### User Story 2 - Chat Completion (Streaming) (Priority: P1)

As a developer, I want to receive streaming responses so that I can show real-time token generation to users.

**Why this priority**: Streaming is essential for interactive applications. Most LLM clients expect SSE streaming.

**Independent Test**: Can be tested by verifying SSE format and chunk structure.

**Acceptance Scenarios**:

1. **Given** a request with `stream: true`, **When** I POST to /v1/chat/completions, **Then** I receive Server-Sent Events with `data: {...}` lines
2. **Given** a streaming request, **When** the backend sends chunks, **Then** each chunk is forwarded immediately (no buffering)
3. **Given** a streaming request, **When** the stream completes, **Then** a final `data: [DONE]` message is sent

---

### User Story 3 - List Models (Priority: P1)

As a client application, I want to list available models so that I can show users what models they can use.

**Why this priority**: Model listing is required by most OpenAI-compatible clients for configuration.

**Independent Test**: Can be tested by verifying response matches OpenAI ModelsResponse format.

**Acceptance Scenarios**:

1. **Given** healthy backends with models, **When** I GET /v1/models, **Then** I receive all models in OpenAI format
2. **Given** duplicate model names across backends, **When** I list models, **Then** each unique model appears once with aggregated metadata
3. **Given** unhealthy backends, **When** I list models, **Then** models from unhealthy backends are excluded

---

### User Story 4 - Health Check (Priority: P2)

As an operator, I want to check Nexus health so that I can monitor system status.

**Why this priority**: Health endpoint is important for monitoring but not required for basic functionality.

**Independent Test**: Can be tested by checking JSON response structure.

**Acceptance Scenarios**:

1. **Given** all backends healthy, **When** I GET /health, **Then** status is "healthy"
2. **Given** some backends unhealthy, **When** I GET /health, **Then** status is "degraded"
3. **Given** no healthy backends, **When** I GET /health, **Then** status is "unhealthy"

---

### User Story 5 - Error Handling (Priority: P1)

As a developer, I want proper error responses so that I can handle failures gracefully.

**Why this priority**: Proper error handling is essential for reliable client integration.

**Independent Test**: Can be tested by triggering each error condition and verifying response format.

**Acceptance Scenarios**:

1. **Given** a request for non-existent model, **When** I POST, **Then** I receive 404 with available models hint
2. **Given** a backend timeout, **When** the request times out, **Then** I receive 504 Gateway Timeout
3. **Given** no healthy backends, **When** I request any model, **Then** I receive 503 Service Unavailable
4. **Given** a backend connection failure, **When** the request fails, **Then** the router retries with next backend

---

### Edge Cases

- What happens when request body exceeds max size? → Return 413 Payload Too Large
- What happens when backend returns non-JSON response? → Return 502 Bad Gateway
- What happens during graceful shutdown? → Complete in-flight requests, reject new ones
- What happens with malformed Authorization header? → Pass through to backend, let it decide
- What happens when all retries fail? → Return 502 with last error message
- What happens with extremely long model names? → Accept if backend accepts, return backend's error otherwise

## Requirements

### Functional Requirements

- **FR-001**: Server MUST expose POST /v1/chat/completions endpoint
- **FR-002**: Server MUST support both streaming and non-streaming responses
- **FR-003**: Server MUST expose GET /v1/models endpoint
- **FR-004**: Server MUST expose GET /health endpoint
- **FR-005**: Server MUST forward Authorization headers to backends
- **FR-006**: Server MUST return usage stats (prompt_tokens, completion_tokens)
- **FR-007**: Server MUST handle concurrent requests (100+)
- **FR-008**: Server MUST retry failed requests with next backend (configurable)
- **FR-009**: Server MUST gracefully shutdown on SIGTERM
- **FR-010**: All error responses MUST follow OpenAI error format

### Non-Functional Requirements

- **NFR-001**: Request timeout MUST be configurable (default: 5 minutes)
- **NFR-002**: Proxy overhead MUST be < 5ms per request
- **NFR-003**: Server MUST handle 100+ concurrent connections
- **NFR-004**: Streaming chunks MUST be forwarded with < 10ms additional latency
- **NFR-005**: All requests MUST be logged via tracing

## API Specification

### POST /v1/chat/completions

**Request**:
```json
{
  "model": "llama3:70b",
  "messages": [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "Hello!"}
  ],
  "stream": false,
  "temperature": 0.7,
  "max_tokens": 1000
}
```

**Response (non-streaming)**:
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
        "content": "Hello! How can I help you today?"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 20,
    "completion_tokens": 10,
    "total_tokens": 30
  }
}
```

**Response (streaming)**: Server-Sent Events
```
data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1699999999,"model":"llama3:70b","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}

data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1699999999,"model":"llama3:70b","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}

data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1699999999,"model":"llama3:70b","choices":[{"index":0,"delta":{"content":"!"},"finish_reason":"stop"}]}

data: [DONE]
```

### GET /v1/models

**Response**:
```json
{
  "object": "list",
  "data": [
    {
      "id": "llama3:70b",
      "object": "model",
      "created": 1699999999,
      "owned_by": "nexus",
      "permission": [],
      "root": "llama3:70b",
      "parent": null,
      "context_length": 8192,
      "capabilities": {
        "vision": false,
        "tools": true,
        "json_mode": true
      }
    }
  ]
}
```

### GET /health

**Response**:
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

### Error Response Format

All errors follow OpenAI format:
```json
{
  "error": {
    "message": "Model 'nonexistent' not found. Available models: llama3:70b, mistral:7b",
    "type": "invalid_request_error",
    "param": "model",
    "code": "model_not_found"
  }
}
```

**Error Codes**:
| HTTP Status | Code | Condition |
|-------------|------|-----------|
| 400 | invalid_request_error | Malformed JSON or invalid parameters |
| 404 | model_not_found | Requested model not available |
| 408 | request_timeout | Client-side timeout |
| 413 | payload_too_large | Request body exceeds limit |
| 500 | internal_error | Unexpected server error |
| 502 | bad_gateway | Backend returned invalid response |
| 503 | service_unavailable | No healthy backends available |
| 504 | gateway_timeout | Backend request timed out |

## Data Structures

### ChatCompletionRequest

```rust
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub stop: Option<Vec<String>>,
    #[serde(default)]
    pub presence_penalty: Option<f32>,
    #[serde(default)]
    pub frequency_penalty: Option<f32>,
    #[serde(default)]
    pub user: Option<String>,
    // Pass through any additional fields
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

pub struct ChatMessage {
    pub role: String,           // "system", "user", "assistant"
    pub content: MessageContent, // String or array (for vision)
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),  // For multimodal
}

pub struct ContentPart {
    pub r#type: String,          // "text" or "image_url"
    pub text: Option<String>,
    pub image_url: Option<ImageUrl>,
}
```

### ChatCompletionResponse

```rust
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,          // "chat.completion"
    pub created: i64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

pub struct Choice {
    pub index: u32,
    pub message: ChatMessage,
    pub finish_reason: Option<String>, // "stop", "length", "tool_calls"
}

pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}
```

### ChatCompletionChunk (Streaming)

```rust
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,          // "chat.completion.chunk"
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
}

pub struct ChunkChoice {
    pub index: u32,
    pub delta: ChunkDelta,
    pub finish_reason: Option<String>,
}

pub struct ChunkDelta {
    pub role: Option<String>,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}
```

### ApiError

```rust
pub struct ApiError {
    pub error: ApiErrorBody,
}

pub struct ApiErrorBody {
    pub message: String,
    pub r#type: String,
    pub param: Option<String>,
    pub code: String,
}
```

## Architecture

### Request Flow

```
Client Request
     │
     ▼
┌─────────────────┐
│   API Gateway   │
│  (Axum Router)  │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Request Parser │
│ (Validation)    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│     Router      │
│ (Backend Select)│◄──── Registry (backends, models)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│   HTTP Client   │
│   (reqwest)     │
└────────┬────────┘
         │
         ▼
    Backend (Ollama, vLLM, etc.)
         │
         ▼
┌─────────────────┐
│ Response Stream │
│ (SSE Forward)   │
└────────┬────────┘
         │
         ▼
    Client Response
```

### Module Structure

```
src/api/
├── mod.rs           # Router setup, shared state
├── completions.rs   # POST /v1/chat/completions handler
├── models.rs        # GET /v1/models handler  
├── health.rs        # GET /health handler
├── error.rs         # OpenAI-format error types
├── types.rs         # Request/Response types
└── streaming.rs     # SSE streaming utilities
```

## Technical Stack

- **HTTP Server**: Axum 0.7
- **HTTP Client**: reqwest with connection pooling
- **Streaming**: async-stream + axum's SSE support
- **Serialization**: serde + serde_json
- **Runtime**: tokio (full features)

## Success Criteria

### Measurable Outcomes

- **SC-001**: POST /v1/chat/completions returns valid response for valid request
- **SC-002**: Streaming response sends `data: [DONE]` as final message
- **SC-003**: GET /v1/models returns all models from healthy backends
- **SC-004**: GET /health returns correct status based on backend health
- **SC-005**: 100 concurrent requests complete without errors
- **SC-006**: Proxy overhead is < 5ms (measured with mock backend)
- **SC-007**: All error responses match OpenAI format

### Definition of Done

- [ ] POST /v1/chat/completions handler implemented (non-streaming)
- [ ] POST /v1/chat/completions handler implemented (streaming)
- [ ] GET /v1/models handler implemented
- [ ] GET /health handler updated with full status
- [ ] Error handling returns OpenAI-format errors
- [ ] Request timeout is configurable
- [ ] Retry logic works with router
- [ ] Integration tests with mock backends pass
- [ ] Concurrent request handling tested
- [ ] Code passes clippy and fmt checks
- [ ] Module has `#[cfg(test)] mod tests` blocks
