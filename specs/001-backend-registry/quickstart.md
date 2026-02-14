# Quickstart: Backend Registry

**Feature**: F01 Backend Registry  
**Status**: ✅ Implemented  
**Prerequisites**: Rust 1.87+, Nexus codebase cloned, optionally an Ollama instance running

---

## Overview

The Backend Registry is Nexus's source of truth for all LLM backends. It tracks backend URLs, types, health status, available models, and real-time metrics (pending requests, latency EMA). Backends can be added via config file, CLI, or mDNS auto-discovery.

This guide shows how to configure backends, query the registry, and verify model availability.

---

## Development Setup

### 1. Build Nexus

```bash
cargo build
```

### 2. Create a Config File

```bash
cargo run -- config init
```

This generates `nexus.toml` from the default template.

### 3. Start a Local Backend (Optional)

If you have Ollama installed:

```bash
ollama serve                      # Start Ollama on port 11434
ollama pull mistral:7b            # Pull a model for testing
```

---

## Project Structure

```
nexus/
├── src/
│   ├── registry/
│   │   ├── mod.rs              # Registry struct — DashMap storage, model-to-backend index
│   │   ├── backend.rs          # Backend, Model, BackendType, BackendStatus, DiscoverySource
│   │   ├── error.rs            # RegistryError types
│   │   └── tests.rs            # Unit tests for registry logic
│   ├── config/
│   │   ├── mod.rs              # NexusConfig loading & validation
│   │   └── backend.rs          # BackendConfig (name, url, type, priority)
│   └── cli/
│       ├── backends.rs         # `nexus backends` subcommands
│       ├── models.rs           # `nexus models` subcommand
│       └── output.rs           # Table & JSON formatters
├── nexus.example.toml          # Annotated example configuration
└── tests/                      # Integration tests
```

---

## Key Types

| Type | Location | Description |
|------|----------|-------------|
| `Registry` | `registry/mod.rs` | DashMap-based concurrent storage with model-to-backend index |
| `Backend` | `registry/backend.rs` | Thread-safe entry with atomic counters (pending, total, latency EMA) |
| `Model` | `registry/backend.rs` | id, name, context_length, vision, tools, json_mode, max_output_tokens |
| `BackendType` | `registry/backend.rs` | Ollama, VLLM, LlamaCpp, Exo, OpenAI, LMStudio, Generic |
| `BackendStatus` | `registry/backend.rs` | Healthy, Unhealthy, Unknown, Draining |
| `DiscoverySource` | `registry/backend.rs` | Static (config), MDNS (auto), Manual (CLI) |
| `BackendView` | `registry/backend.rs` | Serializable snapshot — no atomics, used for API/CLI output |

---

## Usage Guide

### Step 1: Add Backends via Config File

Edit `nexus.toml` to define static backends:

```toml
[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
priority = 1

[[backends]]
name = "gpu-server"
url = "http://192.168.1.100:8000"
type = "vllm"
priority = 3

[[backends]]
name = "lmstudio"
url = "http://localhost:1234"
type = "lmstudio"
priority = 2
```

**Priority**: Lower number = higher priority (1 is highest). Default is 50.

**Supported types**: `ollama`, `vllm`, `llamacpp`, `exo`, `openai`, `lmstudio`, `generic`

Start the server to load them:

```bash
cargo run -- serve -c nexus.toml
```

### Step 2: Add Backends via CLI

Add a backend to a running Nexus instance:

```bash
# Auto-detect backend type from URL
cargo run -- backends add http://localhost:11434

# Explicit type and name
cargo run -- backends add http://192.168.1.100:8000 \
  --name gpu-server \
  --backend-type vllm \
  --priority 3

# Add an OpenAI-compatible backend
cargo run -- backends add http://localhost:1234 \
  --name lmstudio \
  --backend-type lmstudio
```

### Step 3: List Registered Backends

```bash
# Table format (default)
cargo run -- backends list

# Expected output:
# ┌──────────────┬─────────────────────────────┬─────────┬────────┬────────┬─────────┐
# │ Name         │ URL                         │ Type    │ Status │ Models │ Latency │
# ├──────────────┼─────────────────────────────┼─────────┼────────┼────────┼─────────┤
# │ local-ollama │ http://localhost:11434       │ ollama  │ ✓      │ 3      │ 12ms    │
# │ gpu-server   │ http://192.168.1.100:8000   │ vllm    │ ✗      │ 0      │ N/A     │
# │ lmstudio     │ http://localhost:1234        │ lmstudio│ ?      │ 0      │ N/A     │
# └──────────────┴─────────────────────────────┴─────────┴────────┴────────┴─────────┘

# JSON format
cargo run -- backends list --json

# Expected output:
# {
#   "backends": [
#     {
#       "id": "local-ollama",
#       "name": "local-ollama",
#       "url": "http://localhost:11434",
#       "backend_type": "ollama",
#       "status": "healthy",
#       "models": [...],
#       "priority": 1,
#       "pending_requests": 0,
#       "total_requests": 42,
#       "avg_latency_ms": 12
#     }
#   ]
# }

# Filter by status
cargo run -- backends list --status healthy
cargo run -- backends list --status unhealthy
```

### Step 4: List Available Models

```bash
# Table format
cargo run -- models

# Expected output:
# ┌───────────────────┬──────────────────┬─────────────────┐
# │ Model             │ Backends         │ Context Length   │
# ├───────────────────┼──────────────────┼─────────────────┤
# │ mistral:7b        │ local-ollama     │ 4096            │
# │ llama3:70b        │ gpu-server       │ 8192            │
# └───────────────────┴──────────────────┴─────────────────┘

# JSON format
cargo run -- models --json

# Filter by backend
cargo run -- models --backend local-ollama
```

### Step 5: Remove a Backend

```bash
cargo run -- backends remove gpu-server
```

### Step 6: Query via REST API

While the server is running, query the registry through the API:

```bash
# List all models (OpenAI-compatible)
curl -s http://localhost:8000/v1/models | jq .

# Expected:
# {
#   "object": "list",
#   "data": [
#     {
#       "id": "mistral:7b",
#       "object": "model",
#       "created": 1699999999,
#       "owned_by": "nexus",
#       "context_length": 4096,
#       "capabilities": {
#         "vision": false,
#         "tools": true,
#         "json_mode": true
#       }
#     }
#   ]
# }

# Check overall health (includes backend/model counts)
curl -s http://localhost:8000/health | jq .

# Expected:
# {
#   "status": "healthy",
#   "uptime_seconds": 120,
#   "backends": {
#     "total": 2,
#     "healthy": 1,
#     "unhealthy": 1
#   },
#   "models": 3
# }
```

---

## Manual Testing

### Test 1: Empty Registry

Start Nexus with no backends configured:

```bash
# Create minimal config with no backends
cat > /tmp/nexus-empty.toml << 'EOF'
[server]
port = 8001

[discovery]
enabled = false

[health_check]
enabled = false
EOF

cargo run -- serve -c /tmp/nexus-empty.toml
```

In another terminal:

```bash
# Should return empty list
curl -s http://localhost:8001/v1/models | jq .
# Expected: { "object": "list", "data": [] }

# Health shows 0 backends
curl -s http://localhost:8001/health | jq .
# Expected: { "status": "healthy", "uptime_seconds": ..., "backends": { "total": 0, "healthy": 0, "unhealthy": 0 }, "models": 0 }

cargo run -- backends list -c /tmp/nexus-empty.toml
# Expected: empty table
```

### Test 2: Add Backend and Verify Models Appear

```bash
# Start with Ollama running, then add it
cargo run -- backends add http://localhost:11434 -c /tmp/nexus-empty.toml

# Verify models populated after health check
cargo run -- models -c /tmp/nexus-empty.toml
```

### Test 3: Backend with Unreachable URL

```bash
# Add a backend that doesn't exist
cargo run -- backends add http://192.168.99.99:11434 \
  --name unreachable \
  --backend-type ollama

# List shows it as unhealthy
cargo run -- backends list --status unhealthy
```

### Test 4: Multiple Backends, Same Model

If two backends serve the same model, the registry indexes both:

```bash
cat > /tmp/nexus-multi.toml << 'EOF'
[server]
port = 8002

[discovery]
enabled = false

[[backends]]
name = "ollama-1"
url = "http://machine1:11434"
type = "ollama"
priority = 1

[[backends]]
name = "ollama-2"
url = "http://machine2:11434"
type = "ollama"
priority = 2
EOF

cargo run -- serve -c /tmp/nexus-multi.toml
```

```bash
# Both backends listed for shared models
cargo run -- models --json -c /tmp/nexus-multi.toml
```

### Test 5: Backend Status Icons

Verify the status icons in CLI output:

| Icon | Status | Meaning |
|------|--------|---------|
| ✓ | Healthy | Backend is reachable and serving models |
| ✗ | Unhealthy | Backend failed health checks |
| ? | Unknown | Backend hasn't been checked yet |
| ~ | Draining | Backend is being gracefully removed |

### Test 6: Run Unit Tests

```bash
# All registry tests
cargo test registry::

# Specific test modules
cargo test registry::tests::
```

---

## Debugging Tips

### Backend Not Appearing

1. Check the config file syntax:
   ```bash
   cargo run -- serve -c nexus.toml 2>&1 | head -20
   ```
   Look for TOML parse errors in startup output.

2. Verify the backend URL is reachable:
   ```bash
   curl -s http://localhost:11434/api/tags  # Ollama
   curl -s http://localhost:8000/v1/models  # vLLM/OpenAI
   curl -s http://localhost:8080/health     # llama.cpp
   ```

3. Check that the backend type matches the actual server (wrong type = wrong health endpoint).

### Models Not Populating

1. Models are populated by the health checker — wait for at least one health check cycle (default: 30s).
2. Run with debug logging to see health check results:
   ```bash
   RUST_LOG=debug cargo run -- serve
   ```
3. Check for parse errors — the health checker extracts models from the health check response.

### Duplicate Backend Error

The registry rejects backends with duplicate IDs/names. Use a different `--name` or remove the existing one first:

```bash
cargo run -- backends remove existing-name
cargo run -- backends add http://new-url --name existing-name
```

---

## Code Style

- `Registry` uses `DashMap` for lock-free concurrent access — no mutexes
- `Backend` uses `AtomicU32`/`AtomicU64` for pending requests, total requests, latency EMA
- `BackendView` is the serializable form — always convert via `From<&Backend>` for output
- Model-to-backend mapping is maintained as a secondary index in the registry
- All public methods on `Registry` are `&self` (no `&mut self`) — concurrency-safe by design

---

## References

- **Feature Spec**: `specs/001-backend-registry/spec.md`
- **Data Model**: `specs/001-backend-registry/data-model.md`
- **Implementation Walkthrough**: `specs/001-backend-registry/walkthrough.md`
- **Example Config**: `nexus.example.toml`
- **DashMap Docs**: https://docs.rs/dashmap/latest/dashmap/
