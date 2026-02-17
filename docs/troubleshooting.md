# Troubleshooting

## Server Issues

### Server stops when running in background

```bash
# Option 1: Use two terminals (recommended)
# Terminal 1: nexus serve
# Terminal 2: curl commands

# Option 2: Detach from terminal
nohup nexus serve > nexus.log 2>&1 &

# Option 3: Use tmux
tmux new-session -d -s nexus 'nexus serve'
```

### Port already in use

```bash
lsof -i :8000
nexus serve --port 8001
```

## Backend Issues

### Backend shows Unhealthy

```bash
# Verify the backend is running
curl http://localhost:11434/api/tags  # Ollama
curl http://localhost:1234/v1/models  # LM Studio

# Check Nexus logs for details
RUST_LOG=debug nexus serve
```

### No models found

```bash
ollama list
ollama pull llama3.2:latest
```

### Cloud backend not connecting

- Verify API key env var is set: `echo $OPENAI_API_KEY`
- If the env var is missing, Nexus logs a warning and skips the backend (zero-config principle)
- Check connectivity: `curl -H "Authorization: Bearer $OPENAI_API_KEY" https://api.openai.com/v1/models`

## API Issues

### Streaming output is buffered

```bash
curl --no-buffer -s http://localhost:8000/v1/chat/completions ...
```

### 503 "No backend available"

- Check which backends are healthy: `nexus backends list`
- Check the `X-Nexus-Rejection-Reasons` response header for details
- If privacy zones are configured, ensure the requested model is available on a backend in the correct zone
- If budget limits are configured, check if the monthly budget has been exceeded

## Dashboard Issues

### Dashboard not loading

```bash
curl -s http://localhost:8000/ | head -5
# Should return: <!DOCTYPE html>...
curl -sI http://localhost:8000/assets/dashboard.js | head -1
# Should return: HTTP/1.1 200 OK
```

### Dashboard shows stale data

- WebSocket may have disconnected — refresh the page
- If behind a reverse proxy, ensure WebSocket upgrade is supported

## Docker Issues

### mDNS discovery not working in Docker

```bash
# Use host network mode for mDNS
docker run -d --network host leocamello/nexus
```

## Configuration Issues

### Config file not found

```bash
nexus config init          # Generate default config
nexus serve --config nexus.toml  # Explicit path
```

## Logging

### Enable debug logging

```bash
RUST_LOG=debug nexus serve
# Or for specific modules:
RUST_LOG=nexus::routing=debug,nexus::health=trace nexus serve
```

Note: Filter directives use full module paths (`nexus::routing`, not `routing`).
