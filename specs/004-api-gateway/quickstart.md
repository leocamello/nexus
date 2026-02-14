# Quickstart: API Gateway

**Feature**: F04 API Gateway  
**Status**: ✅ Implemented  
**Prerequisites**: Rust 1.87+, Nexus codebase cloned, at least one LLM backend running

---

## Overview

The API Gateway is Nexus's HTTP interface — an OpenAI-compatible API that routes requests to the best available backend. It supports chat completions (streaming and non-streaming), model listing, and health monitoring. Nexus adds metadata via `X-Nexus-*` headers but never modifies the response JSON body.

This guide shows how to send requests, handle streaming, use aliases and fallbacks, and interpret error responses.

---

## Development Setup

### 1. Build and Start Nexus

```bash
cargo build
```

### 2. Create a Config with Backends

```bash
cat > nexus.toml << 'EOF'
[server]
port = 8000

[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
priority = 1

[routing]
strategy = "smart"
max_retries = 2

[routing.aliases]
"gpt-4" = "llama3:70b"
"gpt-3.5-turbo" = "mistral:7b"

[routing.fallbacks]
"llama3:70b" = ["qwen2:72b", "mistral:7b"]
EOF
```

### 3. Start Nexus

```bash
cargo run -- serve -c nexus.toml
```

### 4. Start a Backend

```bash
# Ollama
ollama serve
ollama pull mistral:7b
```

---

## Project Structure

```
nexus/
├── src/
│   ├── api/
│   │   ├── mod.rs              # Axum router setup, AppState, route definitions
│   │   ├── completions.rs      # POST /v1/chat/completions handler (streaming + non-streaming)
│   │   ├── models.rs           # GET /v1/models handler
│   │   ├── health.rs           # GET /health handler
│   │   └── types.rs            # Request/response types, OpenAI error format
│   ├── routing/
│   │   ├── mod.rs              # Router — alias resolution, fallbacks, scoring
│   │   ├── strategies.rs       # Smart, RoundRobin, PriorityOnly, Random
│   │   └── requirements.rs     # Vision, tools, context window detection
│   └── registry/
│       ├── mod.rs              # Backend/model storage (source of truth)
│       └── backend.rs          # Backend, Model types
└── tests/                      # Integration tests with mock backends
```

---

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/v1/chat/completions` | Chat completion (streaming and non-streaming) |
| `GET` | `/v1/models` | List all available models from healthy backends |
| `GET` | `/health` | System health with backend/model counts |

---

## Usage Guide

### Endpoint 1: Chat Completions (`POST /v1/chat/completions`)

#### Non-Streaming Request

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "mistral:7b",
    "messages": [
      {"role": "system", "content": "You are a helpful assistant."},
      {"role": "user", "content": "What is Rust?"}
    ],
    "temperature": 0.7,
    "max_tokens": 200
  }' | jq .
```

**Expected Response:**

```json
{
  "id": "chatcmpl-abc123",
  "object": "chat.completion",
  "created": 1699999999,
  "model": "mistral:7b",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Rust is a systems programming language focused on safety..."
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 24,
    "completion_tokens": 45,
    "total_tokens": 69
  }
}
```

#### Streaming Request

```bash
curl -N http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "mistral:7b",
    "messages": [
      {"role": "user", "content": "Count from 1 to 5."}
    ],
    "stream": true
  }'
```

**Expected Output (SSE format):**

```
data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1699999999,"model":"mistral:7b","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}

data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1699999999,"model":"mistral:7b","choices":[{"index":0,"delta":{"content":"1"},"finish_reason":null}]}

data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1699999999,"model":"mistral:7b","choices":[{"index":0,"delta":{"content":", "},"finish_reason":null}]}

...

data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1699999999,"model":"mistral:7b","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]
```

#### Using Model Aliases

Aliases let you use OpenAI model names and route to local models:

```bash
# "gpt-4" alias routes to "llama3:70b" (configured in nexus.toml)
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4",
    "messages": [{"role": "user", "content": "Hello"}]
  }' | jq .model
# Expected: "llama3:70b" (the resolved model)
```

#### Fallback Chains

When a model is unavailable, Nexus tries fallback models:

```bash
# If "llama3:70b" is unavailable, Nexus tries "qwen2:72b", then "mistral:7b"
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:70b",
    "messages": [{"role": "user", "content": "Hello"}]
  }'

# Check response headers for fallback info
curl -sI http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:70b",
    "messages": [{"role": "user", "content": "Hello"}]
  }' 2>&1 | grep -i x-nexus
# If fallback was used: x-nexus-fallback-model: mistral:7b
```

#### Request Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `model` | string | required | Model ID or alias |
| `messages` | array | required | Conversation messages |
| `stream` | bool | `false` | Enable SSE streaming |
| `temperature` | float | — | Sampling temperature (0-2) |
| `max_tokens` | int | — | Maximum tokens to generate |
| `top_p` | float | — | Nucleus sampling threshold |
| `presence_penalty` | float | — | Presence penalty (-2 to 2) |
| `frequency_penalty` | float | — | Frequency penalty (-2 to 2) |
| `stop` | string/array | — | Stop sequences |
| `user` | string | — | End-user identifier |

### Endpoint 2: List Models (`GET /v1/models`)

```bash
curl -s http://localhost:8000/v1/models | jq .
```

**Expected Response:**

```json
{
  "object": "list",
  "data": [
    {
      "id": "mistral:7b",
      "object": "model",
      "created": 1699999999,
      "owned_by": "nexus",
      "context_length": 4096,
      "capabilities": {
        "vision": false,
        "tools": true,
        "json_mode": true
      }
    },
    {
      "id": "llama3:70b",
      "object": "model",
      "created": 1699999999,
      "owned_by": "nexus",
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

**Filter models from a specific backend (via CLI):**

```bash
cargo run -- models --backend local-ollama --json
```

### Endpoint 3: Health Check (`GET /health`)

```bash
curl -s http://localhost:8000/health | jq .
```

**Expected Response:**

```json
{
  "status": "healthy",
  "uptime_seconds": 3600,
  "backends": {
    "total": 1,
    "healthy": 1,
    "unhealthy": 0
  },
  "models": 3
}
```

**Status values:**
- `"healthy"` — all backends healthy (or no backends configured)
- `"degraded"` — some backends unhealthy
- `"unhealthy"` — no healthy backends available

---

## Error Responses

All errors follow the OpenAI error format:

### Model Not Found (404)

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "nonexistent-model", "messages": [{"role": "user", "content": "hi"}]}' | jq .
```

```json
{
  "error": {
    "message": "Model 'nonexistent-model' not found. Available: mistral:7b, llama3:70b",
    "type": "invalid_request_error",
    "param": "model",
    "code": "model_not_found"
  }
}
```

### No Healthy Backends (503)

```bash
# When all backends are down
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "mistral:7b", "messages": [{"role": "user", "content": "hi"}]}' | jq .
```

```json
{
  "error": {
    "message": "No healthy backends available for model 'mistral:7b'",
    "type": "server_error",
    "param": null,
    "code": "service_unavailable"
  }
}
```

### Other Error Codes

| Code | HTTP | Cause |
|------|------|-------|
| `bad_request` | 400 | Missing `model` or `messages` field |
| `bad_gateway` | 502 | Backend returned an error |
| `gateway_timeout` | 504 | Request exceeded `request_timeout_seconds` |

---

## Using with OpenAI Client Libraries

Nexus is a drop-in replacement for the OpenAI API. Point any OpenAI-compatible client at Nexus:

### Python

```python
from openai import OpenAI

client = OpenAI(base_url="http://localhost:8000/v1", api_key="not-needed")

# Non-streaming
response = client.chat.completions.create(
    model="mistral:7b",
    messages=[{"role": "user", "content": "What is Rust?"}],
)
print(response.choices[0].message.content)

# Streaming
for chunk in client.chat.completions.create(
    model="mistral:7b", messages=[{"role": "user", "content": "Count to 5"}], stream=True
):
    if chunk.choices[0].delta.content:
        print(chunk.choices[0].delta.content, end="", flush=True)
```

### JavaScript/TypeScript

```javascript
import OpenAI from 'openai';
const client = new OpenAI({ baseURL: 'http://localhost:8000/v1', apiKey: 'not-needed' });

const response = await client.chat.completions.create({
  model: 'mistral:7b',
  messages: [{ role: 'user', content: 'What is Rust?' }],
});
console.log(response.choices[0].message.content);
```

---

## Manual Testing

### Test 1: Basic Non-Streaming Request

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "mistral:7b",
    "messages": [{"role": "user", "content": "Say hello in 3 words"}]
  }' | jq '.choices[0].message.content'
# Expected: a short greeting string
```

### Test 2: Streaming Request

```bash
curl -N http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "mistral:7b",
    "messages": [{"role": "user", "content": "Count 1 to 5"}],
    "stream": true
  }'
# Expected: SSE data lines with incremental content, ending with data: [DONE]
```

### Test 3: Model Not Found Error

```bash
curl -s -o /dev/null -w "%{http_code}" http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "does-not-exist", "messages": [{"role": "user", "content": "hi"}]}'
# Expected: 404
```

### Test 4: Invalid Requests

```bash
# No model field → 400
curl -s -o /dev/null -w "%{http_code}" http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"messages": [{"role": "user", "content": "hi"}]}'

# No messages field → 400
curl -s -o /dev/null -w "%{http_code}" http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "mistral:7b"}'

# Empty body → 400
curl -s -o /dev/null -w "%{http_code}" http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{}'
```

### Test 5: Models Endpoint

```bash
curl -s http://localhost:8000/v1/models | jq '.data | length'   # > 0
curl -s http://localhost:8000/v1/models | jq '.object'          # "list"
```

### Test 6: Health Endpoint

```bash
curl -s http://localhost:8000/health | jq .status               # "healthy"
curl -s -o /dev/null -w "%{http_code}" http://localhost:8000/health  # 200
```

### Test 7: Alias Resolution

```bash
# Configure alias: "gpt-3.5-turbo" → "mistral:7b"
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-3.5-turbo",
    "messages": [{"role": "user", "content": "What model are you?"}]
  }' | jq .model
# Expected: "mistral:7b" (the resolved model, not the alias)
```

### Test 8: Concurrent Requests

```bash
# Send 10 requests in parallel
for i in $(seq 1 10); do
  curl -s http://localhost:8000/v1/chat/completions \
    -H "Content-Type: application/json" \
    -d "{\"model\": \"mistral:7b\", \"messages\": [{\"role\": \"user\", \"content\": \"Say $i\"}]}" &
done
wait
echo "All requests completed"
```

### Test 9: Run Tests

```bash
cargo test api::
cargo test routing::
cargo test
```

---

## Debugging Tips

### Request Not Reaching Backend

1. Check the model exists:
   ```bash
   curl -s http://localhost:8000/v1/models | jq '.data[].id'
   ```

2. Check backend is healthy:
   ```bash
   curl -s http://localhost:8000/health | jq .backends
   ```

3. Enable debug logging to see routing decisions:
   ```bash
   RUST_LOG=debug cargo run -- serve
   # Watch for: routing decision, selected backend, scoring details
   ```

### Streaming Not Working

1. Use `curl -N` (no buffering) to see chunks immediately.
2. Verify `"stream": true` is set in the request body.
3. Check that the backend supports streaming (most do).

### Slow Responses

1. Check backend latency via `cargo run -- backends list` (Latency column).
2. Compare with a direct backend request to isolate Nexus overhead (should be < 5ms).
3. Enable debug logging: `RUST_LOG=debug cargo run -- serve`

### Response Headers

Nexus adds metadata via headers (never modifies the response JSON body):

| Header | When Present | Value |
|--------|-------------|-------|
| `x-nexus-fallback-model` | Fallback used | The fallback model name |

---

## Code Style

- Handlers extract `State<Arc<AppState>>` — shared state via `Arc`, never cloned
- Error responses always use `OpenAI error format` — consistent JSON structure
- Streaming uses `axum::response::Sse` with `tokio_stream` for backpressure
- Request validation happens before routing — fail fast on bad input
- Capability matching inspects the request payload for vision/tools requirements
- All response types implement `IntoResponse` — type-safe HTTP responses

---

## References

- **Feature Spec**: `specs/004-api-gateway/spec.md`
- **Data Model**: `specs/004-api-gateway/data-model.md`
- **Implementation Walkthrough**: `specs/004-api-gateway/walkthrough.md`
- **OpenAI API Reference**: https://platform.openai.com/docs/api-reference/chat
- **Axum Docs**: https://docs.rs/axum/latest/axum/
- **Example Config**: `nexus.example.toml`
