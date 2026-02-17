# REST API Reference

Nexus exposes an [OpenAI-compatible](https://platform.openai.com/docs/api-reference) API gateway that unifies local and cloud LLM backends behind a single endpoint. All responses follow the OpenAI format — Nexus-specific metadata is conveyed exclusively through `X-Nexus-*` headers.

> For setup and configuration, see the [Getting Started guide](../getting-started.md).

## Quick Reference

| Method | Path | Description |
|--------|------|-------------|
| `POST` | [`/v1/chat/completions`](#post-v1chatcompletions) | Chat completion (streaming and non-streaming) |
| `GET` | [`/v1/models`](#get-v1models) | List available models from healthy backends |
| `GET` | [`/health`](#get-health) | System health with backend/model counts |
| `GET` | [`/v1/stats`](#get-v1stats) | JSON stats: uptime, request counts, per-backend metrics |
| `GET` | [`/metrics`](#get-metrics) | Prometheus text format metrics |
| `GET` | [`/`](#get-) | Web dashboard (embedded, real-time via WebSocket) |

---

## Endpoints

### POST `/v1/chat/completions`

OpenAI-compatible chat completion endpoint. Supports both streaming and non-streaming responses.

**Request:**

```json
{
  "model": "llama3:70b",
  "messages": [
    { "role": "system", "content": "You are a helpful assistant." },
    { "role": "user", "content": "Hello!" }
  ],
  "stream": true,
  "temperature": 0.7,
  "max_tokens": 1000
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `model` | string | Yes | Model identifier (supports [aliases](../roadmap.md)) |
| `messages` | array | Yes | Conversation messages (`system`, `user`, `assistant`) |
| `stream` | boolean | No | Enable Server-Sent Events streaming (default: `false`) |
| `temperature` | number | No | Sampling temperature (0.0–2.0) |
| `max_tokens` | integer | No | Maximum tokens to generate |

**Response (non-streaming):**

```json
{
  "id": "chatcmpl-abc123",
  "object": "chat.completion",
  "created": 1700000000,
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

**Response (streaming):**

When `stream: true`, the response uses Server-Sent Events (SSE). Each event is a `data:` line containing a JSON chunk, terminated by `data: [DONE]`:

```text
data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","choices":[{"delta":{"content":"Hello"},"index":0}]}

data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","choices":[{"delta":{"content":"!"},"index":0,"finish_reason":"stop"}]}

data: [DONE]
```

---

### GET `/v1/models`

Lists all available models from healthy backends. Each entry corresponds to a specific model on a specific backend.

**Response:**

```json
{
  "object": "list",
  "data": [
    {
      "id": "llama3:70b",
      "object": "model",
      "created": 1700000000,
      "owned_by": "backend-name"
    }
  ]
}
```

---

### GET `/health`

System health check with backend and model counts.

**Response:**

```json
{
  "status": "healthy",
  "version": "0.3.0",
  "uptime_seconds": 3600,
  "backends": { "total": 3, "healthy": 2, "unhealthy": 1 },
  "models": { "total": 5 }
}
```

---

### GET `/v1/stats`

JSON stats endpoint for dashboards and debugging. Returns uptime, per-backend request counts, latency, and pending request depth.

**Example response fields:**

- `uptime_seconds` — time since Nexus started
- `total_requests` — aggregate request count
- `backends[]` — per-backend stats including request count, average latency, and pending depth

---

### GET `/metrics`

Prometheus text format metrics. Configure your Prometheus scraper to target:

```
http://<nexus-host>:8000/metrics
```

**Exported metrics include:**

- Request counters and duration histograms
- Error rates
- Backend latency
- Token usage
- Fleet state gauges
- Reconciler pipeline timing

---

### GET `/`

Embedded web dashboard (HTML/JS/CSS) with real-time monitoring via WebSocket. See the [WebSocket Protocol](websocket.md) documentation for details on the real-time update format.

---

## Nexus-Transparent Protocol Headers

Nexus adds `X-Nexus-*` response headers to expose routing decisions **without modifying the OpenAI-compatible JSON body**. This keeps Nexus fully transparent to existing OpenAI client libraries.

### Response Headers

| Header | Description | Example |
|--------|-------------|---------|
| `X-Nexus-Backend` | Backend that handled the request | `local-ollama` |
| `X-Nexus-Backend-Type` | `local` or `cloud` | `local` |
| `X-Nexus-Route-Reason` | Why this backend was chosen | `capability-match` |
| `X-Nexus-Cost-Estimated` | Estimated cost in USD (cloud only) | `0.0023` |
| `X-Nexus-Privacy-Zone` | Privacy zone of the backend | `restricted` |
| `X-Nexus-Fallback-Model` | Model used if fallback occurred | `gpt-3.5-turbo` |
| `X-Nexus-Rejection-Reasons` | Why backends were excluded (on 503) | `privacy_zone_mismatch` |
| `X-Nexus-Rejection-Details` | Detailed rejection context (on 503) | JSON details |

### Request Headers

| Header | Description |
|--------|-------------|
| `X-Nexus-Strict` | Enforce same-or-higher capability tier (default behavior) |
| `X-Nexus-Flexible` | Allow higher-tier substitution when the exact tier is unavailable |

---

## Actionable Error Responses

When no backend can serve a request, Nexus returns **HTTP 503** with actionable context instead of a generic error. This follows the project principle of _honest failures over silent quality downgrades_.

```json
{
  "error": {
    "message": "No backend available for model 'gpt-4' with required capabilities",
    "type": "service_unavailable",
    "code": "no_available_backend",
    "context": {
      "required_tier": 4,
      "available_backends": ["ollama-local"],
      "privacy_zone_required": "restricted",
      "eta_seconds": null
    }
  }
}
```

The `context` object provides enough information for clients to take corrective action — for example, relaxing privacy constraints or falling back to a different model.
