# WebSocket Protocol: Dashboard Real-Time Updates

**Endpoint**: `ws://[host]/ws`  
**Protocol**: WebSocket (RFC 6455)  
**Encoding**: JSON (UTF-8 text frames)

---

## Connection Lifecycle

### Client → Server: Connection Request

HTTP GET upgrade request to establish WebSocket connection.

**Request**:
```http
GET /ws HTTP/1.1
Host: localhost:8000
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==
Sec-WebSocket-Version: 13
```

**Response** (Success):
```http
HTTP/1.1 101 Switching Protocols
Upgrade: websocket
Connection: Upgrade
Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=
```

### Server → Client: Update Messages

Server pushes updates when state changes. All messages follow the `WebSocketUpdate` schema.

### Client → Server: Ping/Pong

Standard WebSocket ping/pong frames for keep-alive. No application-level messages from client to server (read-only dashboard).

---

## Message Schemas

### Base Message Structure

All messages from server to client follow this envelope:

```typescript
interface WebSocketUpdate {
  update_type: "backend_status" | "request_complete" | "model_change";
  data: object; // Structure depends on update_type
}
```

---

## Update Type 1: backend_status

**Trigger**: Backend health status changes, pending request count changes, or latency is updated.

**Message Schema**:

```json
{
  "update_type": "backend_status",
  "data": {
    "id": "string",
    "name": "string",
    "status": "healthy" | "unhealthy" | "unknown" | "draining",
    "last_health_check": "2024-02-14T10:30:45.123Z",
    "pending_requests": 3,
    "avg_latency_ms": 1250.5
  }
}
```

---

## Update Type 2: request_complete

**Trigger**: A request completes (success or error).

**Message Schema**:

```json
{
  "update_type": "request_complete",
  "data": {
    "timestamp": "2024-02-14T10:30:45.123Z",
    "model": "llama3:70b",
    "backend_id": "ollama-local-001",
    "duration_ms": 1250,
    "status": "success" | "error",
    "error_message": "string | null"
  }
}
```

---

## Update Type 3: model_change

**Trigger**: Backend models are added or removed.

**Message Schema**:

```json
{
  "update_type": "model_change",
  "data": {
    "backend_id": "string",
    "models": [
      {
        "id": "string",
        "name": "string",
        "context_length": 8192,
        "supports_vision": false,
        "supports_tools": true,
        "supports_json_mode": true,
        "max_output_tokens": null
      }
    ]
  }
}
```

---

## Fallback: HTTP Polling

When WebSocket is unavailable, client polls:

| Endpoint | Method | Frequency |
|----------|--------|-----------|
| `/v1/stats` | GET | Every 5 seconds |
| `/v1/models` | GET | Every 30 seconds |

---

## Summary

Real-time updates via WebSocket with automatic fallback to HTTP polling for resilience.
