# Agent Method Contracts

**Feature**: F12 NII Extraction (RFC-001 Phase 1)  
**Date**: 2026-02-15

This document defines the detailed contracts for all `InferenceAgent` trait methods, including inputs, outputs, error conditions, and implementation requirements.

---

## Method: `health_check()`

### Signature

```rust
async fn health_check(&self) -> Result<HealthStatus, AgentError>;
```

### Purpose

Check backend reachability and functional status. Used by health checker loop to determine if backend should receive traffic.

### Returns

**Success**:
- `Ok(HealthStatus::Healthy { model_count })` — Backend is reachable and functional
- `Ok(HealthStatus::Unhealthy)` — Backend returned error response

**Failure**:
- `Err(AgentError::Network(msg))` — Connection failed
- `Err(AgentError::Timeout(ms))` — Request exceeded deadline
- `Err(AgentError::InvalidResponse(msg))` — Response parsing failed

### Implementation Requirements

1. Respect client-level timeout
2. Endpoint: GET /api/tags (Ollama) or GET /v1/models (others)
3. Error mapping: 4xx/5xx → Unhealthy, connection → Network, timeout → Timeout

### Caller Expectations

- Called every 30s by HealthChecker
- Updates Backend.status based on result
- No internal retry (caller handles)

---

## Method: `list_models()`

### Signature

```rust
async fn list_models(&self) -> Result<Vec<ModelCapability>, AgentError>;
```

### Purpose

Discover available models with capabilities. Used by health checker to populate Registry model index.

### Returns

**Success**: `Ok(Vec<ModelCapability>)`  
**Failure**: `Err(AgentError::*)` for network/timeout/parse errors

### Implementation

- OllamaAgent: GET /api/tags + POST /api/show per model
- Others: GET /v1/models + name heuristics

---

## Method: `chat_completion()`

### Signature

```rust
async fn chat_completion(
    &self,
    request: ChatCompletionRequest,
    headers: Option<&HeaderMap>,
) -> Result<ChatCompletionResponse, AgentError>;
```

### Purpose

Execute non-streaming chat completion. Forwards to backend, parses response.

### Returns

- `Ok(ChatCompletionResponse)` on success
- `Err(AgentError::Upstream { status, message })` for 4xx/5xx
- `Err(AgentError::Network/Timeout)` for connection issues

### Implementation

POST /v1/chat/completions with Authorization header forwarding.

---

## Method: `chat_completion_stream()`

### Signature

```rust
async fn chat_completion_stream(
    &self,
    request: ChatCompletionRequest,
    headers: Option<&HeaderMap>,
) -> Result<BoxStream<'static, Result<StreamChunk, AgentError>>, AgentError>;
```

### Purpose

Execute streaming chat completion. Returns SSE stream.

### Returns

- `Ok(BoxStream)` — Stream of SSE chunks
- `Err(AgentError::*)` — Errors before streaming starts
- Stream emits errors during streaming

### Cancellation Safety

Dropping stream aborts HTTP request (reqwest guarantees).

---

## Optional Methods (Phase 1 defaults)

All return `Err(AgentError::Unsupported)` or safe fallbacks:

- `embeddings()` → Unsupported
- `load_model()` → Unsupported
- `count_tokens()` → Heuristic (chars/4)
- `resource_usage()` → Empty struct

---

## Error Handling Summary

| Condition | Agent Error | Caller Action |
|-----------|-------------|---------------|
| Connection failed | `Network(msg)` | Retry next backend |
| Timeout | `Timeout(ms)` | Retry next backend |
| Backend error | `Upstream { status, msg }` | Retry next backend |
| Parse error | `InvalidResponse(msg)` | Retry next backend |
| Unsupported | `Unsupported(method)` | Return 501 to client |
| Config error | `Configuration(msg)` | Log, don't retry |

See full contracts in `specs/012-nii-extraction/contracts/agent-methods.md`.
