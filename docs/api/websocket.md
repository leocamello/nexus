# WebSocket API Reference

Real-time WebSocket API for the Nexus dashboard. Provides server-to-client push updates for backend status, model changes, and request completions.

---

## Connection

Connect to `ws://localhost:8000/ws`. All communication is server-to-client only (read-only).

---

## Message Format

All messages are JSON with two fields:

| Field | Type | Description |
|-------|------|-------------|
| `update_type` | string | One of: `BackendStatus`, `ModelChange`, `RequestComplete` |
| `data` | object/array | Payload specific to the update type |

---

## Update Types

### BackendStatus

Sent on every health check cycle (default: every 5 seconds). Contains the current state of all registered backends.

```json
{
  "update_type": "BackendStatus",
  "data": [{
    "id": "ollama-local",
    "url": "http://localhost:11434",
    "status": "Healthy",
    "models": ["llama3:70b"],
    "pending_requests": 2,
    "total_requests": 1523,
    "avg_latency_ms": 1250.5,
    "backend_type": "ollama",
    "privacy_zone": "restricted",
    "tier": 3
  }]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Backend identifier |
| `url` | string | Backend base URL |
| `status` | string | `Healthy`, `Unhealthy`, `Unknown`, or `Draining` |
| `models` | string[] | Currently loaded model names |
| `pending_requests` | integer | In-flight requests |
| `total_requests` | integer | Lifetime request count |
| `avg_latency_ms` | float | Exponential moving average latency |
| `backend_type` | string | `ollama`, `vllm`, `llamacpp`, `exo`, `openai`, `lmstudio`, `generic` |
| `privacy_zone` | string | `local`, `restricted`, or `public` |
| `tier` | integer | Capability tier (1–5) |

### ModelChange

Sent when models are added to or removed from a backend (detected during health checks).

```json
{
  "update_type": "ModelChange",
  "data": {
    "backend_id": "ollama-local",
    "models": [{
      "id": "llama3:70b",
      "name": "llama3:70b",
      "context_length": 8192,
      "supports_vision": false,
      "supports_tools": true,
      "supports_json_mode": true
    }]
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `backend_id` | string | Backend that changed |
| `models` | object[] | Current model list for the backend |
| `models[].id` | string | Model identifier |
| `models[].name` | string | Display name |
| `models[].context_length` | integer | Maximum context window (tokens) |
| `models[].supports_vision` | boolean | Image input support |
| `models[].supports_tools` | boolean | Tool/function calling support |
| `models[].supports_json_mode` | boolean | Structured JSON output support |

### RequestComplete

Sent when a proxied chat completion request finishes (success or failure).

```json
{
  "update_type": "RequestComplete",
  "data": {
    "timestamp": 1704067200,
    "model": "llama3:70b",
    "backend_id": "ollama-local",
    "duration_ms": 1523,
    "status": "Success",
    "error_message": null
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `timestamp` | integer | Unix timestamp (seconds) |
| `model` | string | Requested model |
| `backend_id` | string | Backend that handled the request |
| `duration_ms` | integer | Total request duration |
| `status` | string | `Success` or `Error` |
| `error_message` | string \| null | Error details (null on success) |

---

## Message Size Limits

| Constraint | Limit |
|-----------|-------|
| Maximum message size | 10 KB |
| Model name length | 256 characters |
| Error message length | 1024 characters |

---

## Connection Handling

### Client Reconnection

Use exponential backoff on disconnection:

| Attempt | Delay |
|---------|-------|
| 1 | 3s |
| 2 | 6s |
| 3 | 12s |
| 4 | 24s |
| 5 | 48s |
| 5+ | Cap at 60s |

After 5 consecutive failures, fall back to HTTP polling via `GET /v1/stats` at a 5-second interval.

### Ping/Pong

The server handles WebSocket ping/pong frames automatically. No client-side keepalive is required.

---

## Security

- **No sensitive data** is included in WebSocket messages
- **Read-only** — server-to-client only, no client commands accepted
- Use `wss://` (TLS) in production environments
- Consider limiting concurrent connections (~1000 recommended maximum)

---

## Example Client (JavaScript)

```javascript
const ws = new WebSocket('ws://localhost:8000/ws');

ws.onmessage = (event) => {
  const update = JSON.parse(event.data);
  switch (update.update_type) {
    case 'BackendStatus':
      console.log('Backends:', update.data);
      break;
    case 'ModelChange':
      console.log('Models:', update.data);
      break;
    case 'RequestComplete':
      console.log('Request:', update.data);
      break;
  }
};
```

---

## Testing

```bash
# Using websocat
websocat ws://localhost:8000/ws

# Using wscat
wscat -c ws://localhost:8000/ws
```
