# Nexus Manual Testing Guide

This guide walks you through testing all MVP features of Nexus. Each section covers a specific feature with step-by-step commands and expected outputs.

## Prerequisites

Before testing, ensure you have:

1. **Rust toolchain** installed (`cargo` available)
2. **At least one LLM backend** running (e.g., Ollama)
3. **curl** for HTTP requests
4. **jq** (optional) for JSON formatting

### Quick Setup

```bash
# Build Nexus
cargo build --release

# Verify installation
./target/release/nexus --version

# If using Ollama, ensure it's running
ollama serve  # or systemctl start ollama
```

---

## Table of Contents

1. [F04: CLI and Configuration](#f04-cli-and-configuration)
2. [F02: Backend Registry](#f02-backend-registry)
3. [F03: Health Checker](#f03-health-checker)
4. [F01: Core API Gateway](#f01-core-api-gateway)

> **Note**: Features are listed in testing order, not feature number order, because CLI/Config is needed first to set up backends.

---

## F04: CLI and Configuration

### 4.1 Generate Configuration File

```bash
# Initialize default configuration
nexus config init

# Verify file was created
cat nexus.toml
```

**Expected**: A `nexus.toml` file with default settings:
```toml
[server]
host = "0.0.0.0"
port = 8000
...
```

### 4.2 Generate Configuration with Custom Path

```bash
# Initialize with custom path
nexus config init --output /tmp/my-nexus.toml

# Verify
cat /tmp/my-nexus.toml
```

**Expected**: Configuration file created at specified path.

### 4.3 Test Configuration Validation

```bash
# Create an invalid config
echo "invalid_key = true" > /tmp/bad.toml

# Try to use it (should fail gracefully)
nexus serve -c /tmp/bad.toml
```

**Expected**: Error message indicating invalid configuration.

### 4.4 Environment Variable Overrides

```bash
# Override port via environment
NEXUS_PORT=9000 nexus serve &
SERVER_PID=$!
sleep 2

# Verify port
curl http://localhost:9000/health

# Cleanup
kill $SERVER_PID
```

**Expected**: Server runs on port 9000 instead of default 8000.

### 4.5 Command-Line Overrides

```bash
# Override via CLI (highest priority)
nexus serve --port 9001 --host 127.0.0.1 &
SERVER_PID=$!
sleep 2

# Verify
curl http://127.0.0.1:9001/health

# Cleanup
kill $SERVER_PID
```

**Expected**: Server runs on 127.0.0.1:9001.

### 4.6 Shell Completions

```bash
# Generate bash completions
nexus completions bash > /tmp/nexus.bash
cat /tmp/nexus.bash | head -20

# Generate zsh completions
nexus completions zsh > /tmp/nexus.zsh

# Generate fish completions
nexus completions fish > /tmp/nexus.fish
```

**Expected**: Valid shell completion scripts generated.

---

## F02: Backend Registry

### 2.1 Setup Configuration with Static Backend

First, create a configuration with your backend:

```bash
cat > nexus.toml << 'EOF'
[server]
host = "0.0.0.0"
port = 8000

[discovery]
enabled = false

[health_check]
enabled = true
interval_seconds = 30

[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
priority = 1
EOF
```

### 2.2 Start Server and List Backends

```bash
# Start Nexus in background
nexus serve &
SERVER_PID=$!
sleep 3

# List backends via CLI
nexus backends list
```

**Expected**:
```
Backends:
  local-ollama (ollama)
    URL: http://localhost:11434
    Status: Healthy
    Priority: 1
    Models: llama3.2:latest, ...
```

### 2.3 List Backends as JSON

```bash
nexus backends list --json | jq .
```

**Expected**: JSON array of backend objects with all metadata.

### 2.4 Add Backend Dynamically

```bash
# Add a new backend
nexus backends add gpu-server http://192.168.1.100:8000 --type vllm --priority 2

# Verify it appears
nexus backends list
```

**Expected**: New backend "gpu-server" appears in list.

### 2.5 Remove Backend

```bash
# Remove the backend
nexus backends remove gpu-server

# Verify it's gone
nexus backends list
```

**Expected**: Backend "gpu-server" no longer in list.

### 2.6 Backend Persistence

```bash
# Add backend, then restart server
nexus backends add test-backend http://localhost:9999 --type generic

# Stop and restart
kill $SERVER_PID
nexus serve &
SERVER_PID=$!
sleep 3

# Check if static backends are preserved
nexus backends list
```

**Expected**: Static backends from config are present. Dynamically added backends may not persist (depending on implementation).

---

## F03: Health Checker

### 3.1 Check System Health via CLI

```bash
nexus health
```

**Expected**:
```
System Health: Healthy

Backends:
  ✓ local-ollama (Healthy)
    Last check: 2s ago
    Response time: 45ms
```

### 3.2 Health Check JSON Output

```bash
nexus health --json | jq .
```

**Expected**:
```json
{
  "status": "healthy",
  "backends": [
    {
      "id": "local-ollama",
      "status": "healthy",
      "last_check": "2026-02-03T23:00:00Z",
      "response_time_ms": 45
    }
  ]
}
```

### 3.3 Health Endpoint via HTTP

```bash
curl -s http://localhost:8000/health | jq .
```

**Expected**:
```json
{
  "status": "healthy"
}
```

### 3.4 Simulate Backend Failure

```bash
# Stop your Ollama backend temporarily
# (or point to non-existent backend)

# Add unreachable backend
nexus backends add dead-backend http://localhost:99999 --type generic

# Wait for health check cycle
sleep 35

# Check health
nexus health
```

**Expected**:
```
System Health: Degraded

Backends:
  ✓ local-ollama (Healthy)
  ✗ dead-backend (Unhealthy)
    Error: Connection refused
```

### 3.5 Health Check Interval Verification

```bash
# Watch logs for health check activity
RUST_LOG=debug nexus serve 2>&1 | grep -i "health"
```

**Expected**: Health check logs appearing at configured interval (default 30s).

### 3.6 Cleanup

```bash
nexus backends remove dead-backend
```

---

## F01: Core API Gateway

### 1.1 List Models Endpoint

```bash
curl -s http://localhost:8000/v1/models | jq .
```

**Expected**:
```json
{
  "object": "list",
  "data": [
    {
      "id": "llama3.2:latest",
      "object": "model",
      "created": 1706900000,
      "owned_by": "local-ollama"
    }
  ]
}
```

### 1.2 List Models via CLI

```bash
nexus models
```

**Expected**:
```
Available Models:
  llama3.2:latest (local-ollama)
  mistral:7b (local-ollama)
  ...
```

### 1.3 Filter Models by Backend

```bash
nexus models --backend local-ollama
```

**Expected**: Only models from specified backend.

### 1.4 Non-Streaming Chat Completion

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2:latest",
    "messages": [
      {"role": "user", "content": "Say hello in exactly 3 words"}
    ],
    "stream": false
  }' | jq .
```

**Expected**:
```json
{
  "id": "chatcmpl-...",
  "object": "chat.completion",
  "created": 1706900000,
  "model": "llama3.2:latest",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Hello there, friend!"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 15,
    "completion_tokens": 4,
    "total_tokens": 19
  }
}
```

### 1.5 Streaming Chat Completion

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2:latest",
    "messages": [
      {"role": "user", "content": "Count from 1 to 5"}
    ],
    "stream": true
  }'
```

**Expected**: Server-Sent Events format:
```
data: {"id":"chatcmpl-...","object":"chat.completion.chunk","created":1706900000,"model":"llama3.2:latest","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}

data: {"id":"chatcmpl-...","object":"chat.completion.chunk","created":1706900000,"model":"llama3.2:latest","choices":[{"index":0,"delta":{"content":"1"},"finish_reason":null}]}

... more chunks ...

data: {"id":"chatcmpl-...","object":"chat.completion.chunk","created":1706900000,"model":"llama3.2:latest","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]
```

### 1.6 Multi-turn Conversation

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2:latest",
    "messages": [
      {"role": "system", "content": "You are a helpful math tutor."},
      {"role": "user", "content": "What is 2+2?"},
      {"role": "assistant", "content": "2+2 equals 4."},
      {"role": "user", "content": "And if I add 3 more?"}
    ]
  }' | jq '.choices[0].message.content'
```

**Expected**: Response acknowledging context: "7" or similar.

### 1.7 Temperature and Max Tokens

```bash
# Low temperature (deterministic)
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2:latest",
    "messages": [{"role": "user", "content": "What is 1+1?"}],
    "temperature": 0.0,
    "max_tokens": 10
  }' | jq '.choices[0].message.content'
```

**Expected**: Short, deterministic response.

### 1.8 Error Handling - Invalid Model

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "nonexistent-model",
    "messages": [{"role": "user", "content": "Hello"}]
  }' | jq .
```

**Expected**: OpenAI-compatible error:
```json
{
  "error": {
    "message": "Model 'nonexistent-model' not found",
    "type": "invalid_request_error",
    "param": "model",
    "code": "model_not_found"
  }
}
```

### 1.9 Error Handling - Missing Required Fields

```bash
# Missing messages
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "llama3.2:latest"}' | jq .
```

**Expected**: Error about missing `messages` field.

### 1.10 Error Handling - Invalid JSON

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d 'not valid json' | jq .
```

**Expected**: Error about JSON parsing.

### 1.11 CORS Headers (if applicable)

```bash
curl -s -I http://localhost:8000/v1/models \
  -H "Origin: http://example.com" | grep -i "access-control"
```

**Expected**: CORS headers if enabled in config.

### 1.12 Request with Authorization Header

```bash
# Nexus should forward auth headers to backends that need them
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer test-token" \
  -d '{
    "model": "llama3.2:latest",
    "messages": [{"role": "user", "content": "Hello"}]
  }' | jq .
```

**Expected**: Request succeeds (auth may be ignored for local backends).

---

## Integration Test: Full Workflow

This test combines all features in a realistic workflow:

```bash
#!/bin/bash
set -e

echo "=== Nexus Integration Test ==="

# 1. Initialize config
echo "Step 1: Initialize configuration"
nexus config init --output /tmp/nexus-test.toml

# 2. Start server
echo "Step 2: Start server"
nexus serve -c /tmp/nexus-test.toml &
SERVER_PID=$!
sleep 3

# 3. Check health
echo "Step 3: Check health"
nexus health

# 4. List backends
echo "Step 4: List backends"
nexus backends list

# 5. List models
echo "Step 5: List models"
nexus models

# 6. Make a chat completion
echo "Step 6: Test chat completion"
RESPONSE=$(curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2:latest",
    "messages": [{"role": "user", "content": "Say OK"}],
    "max_tokens": 5
  }')
echo "$RESPONSE" | jq '.choices[0].message.content'

# 7. Test streaming
echo "Step 7: Test streaming"
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.2:latest",
    "messages": [{"role": "user", "content": "Count 1 2 3"}],
    "stream": true,
    "max_tokens": 10
  }' | head -5

# Cleanup
echo "Cleaning up..."
kill $SERVER_PID 2>/dev/null || true
rm /tmp/nexus-test.toml

echo "=== All tests passed! ==="
```

---

## Troubleshooting

### Server won't start

```bash
# Check if port is in use
lsof -i :8000

# Try different port
nexus serve --port 8001
```

### Backend shows as Unhealthy

```bash
# Verify backend is running
curl http://localhost:11434/api/tags  # Ollama
curl http://localhost:8000/v1/models  # vLLM

# Check Nexus logs
RUST_LOG=debug nexus serve
```

### No models found

```bash
# Ensure backend has models loaded
ollama list  # For Ollama

# Pull a model if needed
ollama pull llama3.2:latest
```

### Streaming not working

```bash
# Ensure you're not buffering output
curl --no-buffer -s http://localhost:8000/v1/chat/completions ...

# Check backend supports streaming
```

---

## Cleanup

After testing:

```bash
# Stop Nexus server
kill $SERVER_PID

# Remove test config files
rm -f /tmp/nexus-test.toml /tmp/my-nexus.toml /tmp/bad.toml

# Remove shell completion files
rm -f /tmp/nexus.{bash,zsh,fish}
```

---

## Summary

| Feature | Key Tests | Pass Criteria |
|---------|-----------|---------------|
| F04: CLI | Config init, env vars, completions | Commands work, config valid |
| F02: Registry | Add/remove/list backends | Backends tracked correctly |
| F03: Health | Health status, failure detection | Accurate status reporting |
| F01: API | Models list, chat completion, streaming | OpenAI-compatible responses |

For automated testing, run:
```bash
cargo test
```

Current test suite: **224 tests passing**.
