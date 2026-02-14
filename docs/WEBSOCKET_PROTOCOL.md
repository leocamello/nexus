# WebSocket Protocol

The Nexus dashboard uses WebSockets for real-time updates. This document describes the WebSocket message format and update types.

## Connection

Connect to the WebSocket endpoint at:
```
ws://localhost:8000/ws
```

The WebSocket connection is used for server-to-client updates only. Clients do not send messages to the server (except for ping/pong and close frames).

## Message Format

All messages are JSON-encoded with the following structure:

```json
{
  "update_type": "BackendStatus|ModelChange|RequestComplete",
  "data": { ... }
}
```

## Update Types

### BackendStatus

Sent when backend health status changes or metrics update.

**Trigger:** Health check completion (every 5 seconds)

**Example:**
```json
{
  "update_type": "BackendStatus",
  "data": [
    {
      "id": "ollama-local",
      "url": "http://localhost:11434",
      "status": "Healthy",
      "models": ["llama3:70b", "gemma:7b"],
      "pending_requests": 2,
      "total_requests": 1523,
      "avg_latency_ms": 1250.5
    }
  ]
}
```

**Fields:**
- `id` (string): Backend identifier
- `url` (string): Backend URL
- `status` (string): "Healthy", "Unhealthy", or "Unknown"
- `models` (array): List of model names available on this backend
- `pending_requests` (number): Current queue depth
- `total_requests` (number): Total requests sent to this backend
- `avg_latency_ms` (number): Average response latency in milliseconds

### ModelChange

Sent when models are added/removed from a backend.

**Trigger:** Backend model list changes (health check detects change)

**Example:**
```json
{
  "update_type": "ModelChange",
  "data": {
    "backend_id": "ollama-local",
    "models": [
      {
        "id": "llama3:70b",
        "name": "llama3:70b",
        "context_length": 8192,
        "supports_vision": false,
        "supports_tools": true,
        "supports_json_mode": true
      }
    ]
  }
}
```

**Fields:**
- `backend_id` (string): Backend identifier
- `models` (array): List of model objects
  - `id` (string): Model identifier
  - `name` (string): Human-readable model name
  - `context_length` (number): Maximum context window size in tokens
  - `supports_vision` (boolean): Whether model supports vision/image inputs
  - `supports_tools` (boolean): Whether model supports function calling
  - `supports_json_mode` (boolean): Whether model supports JSON mode

### RequestComplete

Sent when a request completes (success or error).

**Trigger:** Chat completion request finishes

**Example (Success):**
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

**Example (Error):**
```json
{
  "update_type": "RequestComplete",
  "data": {
    "timestamp": 1704067200,
    "model": "gpt-4",
    "backend_id": "openai-cloud",
    "duration_ms": 250,
    "status": "Error",
    "error_message": "Model not found: gpt-4"
  }
}
```

**Fields:**
- `timestamp` (number): Unix timestamp (seconds since epoch)
- `model` (string): Requested model name
- `backend_id` (string): Backend that handled the request
- `duration_ms` (number): Request duration in milliseconds
- `status` (string): "Success" or "Error"
- `error_message` (string|null): Error message if status is "Error"

## Message Size Limits

- Maximum message size: **10KB**
- Messages exceeding this limit are dropped (not sent to clients)
- History entry validation:
  - Model name truncated to 256 characters
  - Error messages truncated to 1024 characters
  - Timestamps validated (not in future, allowing 60s clock skew)

## Connection Handling

### Client Behavior

1. **Initial Connection**: Client connects and subscribes to updates
2. **Reconnection**: If connection drops, client attempts reconnection with exponential backoff:
   - Attempt 1: 3 seconds
   - Attempt 2: 6 seconds
   - Attempt 3: 12 seconds
   - Attempt 4: 24 seconds
   - Attempt 5: 48 seconds (capped at 60s)
3. **Fallback**: After 5 failed reconnection attempts, client falls back to HTTP polling (5-second interval)

### Server Behavior

- Sends updates via broadcast channel to all connected clients
- Automatically handles ping/pong for keepalive
- Closes connection on client disconnect or error

## Ping/Pong

The WebSocket connection uses standard ping/pong frames for keepalive. The Axum WebSocket handler automatically responds to ping frames with pong frames.

## Error Handling

Clients should handle these error scenarios:

1. **Connection failed**: Retry with exponential backoff
2. **Message parse error**: Log and ignore malformed messages
3. **Unknown update type**: Log warning and ignore
4. **WebSocket closed**: Attempt reconnection

## Rate Limiting

The dashboard endpoints do not currently enforce rate limiting, but it's recommended to:

- Limit WebSocket connections per IP to prevent DoS
- Consider rate limiting dashboard HTTP endpoints if exposed publicly
- Monitor concurrent WebSocket connections (recommended limit: 1000)

## Security Considerations

- WebSocket messages do not contain sensitive data (API keys, tokens, etc.)
- All updates are read-only (server-to-client only)
- Consider using TLS/WSS in production (`wss://` instead of `ws://`)
- Validate message size to prevent memory exhaustion
- History entries sanitized before broadcasting (length limits, timestamp validation)

## Example Client (JavaScript)

```javascript
const ws = new WebSocket('ws://localhost:8000/ws');

ws.onopen = () => {
  console.log('Connected to Nexus WebSocket');
};

ws.onmessage = (event) => {
  const update = JSON.parse(event.data);
  
  switch (update.update_type) {
    case 'BackendStatus':
      console.log('Backend status:', update.data);
      break;
    case 'ModelChange':
      console.log('Models changed:', update.data);
      break;
    case 'RequestComplete':
      console.log('Request completed:', update.data);
      break;
    default:
      console.warn('Unknown update type:', update.update_type);
  }
};

ws.onerror = (error) => {
  console.error('WebSocket error:', error);
};

ws.onclose = (event) => {
  console.log('WebSocket closed:', event.code, event.reason);
  // Implement reconnection logic here
};
```

## Testing

Test the WebSocket connection:

```bash
# Using websocat (https://github.com/vi/websocat)
websocat ws://localhost:8000/ws

# Using wscat (npm install -g wscat)
wscat -c ws://localhost:8000/ws
```

You should see JSON messages as backends update, models change, or requests complete.
