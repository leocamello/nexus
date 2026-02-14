# Quickstart: Intelligent Router

**Feature**: F06 Intelligent Router  
**Status**: ✅ Implemented  
**Prerequisites**: Rust 1.75+, Nexus codebase cloned, at least one backend configured

---

## Overview

The intelligent router selects the best backend for each request based on model availability, backend health, capability requirements (vision, tools, JSON mode), and a configurable scoring system. This guide shows how to configure routing strategies, adjust scoring weights, and test capability-aware routing.

---

## Project Structure

```
nexus/
├── src/
│   ├── routing/
│   │   ├── mod.rs            # Router: alias resolution, fallback chains, backend selection
│   │   ├── strategies.rs     # RoutingStrategy enum (Smart, RoundRobin, PriorityOnly, Random)
│   │   ├── scoring.rs        # score_backend() function, ScoringWeights
│   │   ├── requirements.rs   # RequestRequirements extraction from request payload
│   │   └── error.rs          # RoutingError types (ModelNotFound, NoHealthyBackend, etc.)
│   ├── config/
│   │   └── routing.rs        # RoutingConfig, RoutingWeights, strategy deserialization
│   └── api/
│       └── completions.rs    # Uses Router.select_backend() for each request
├── nexus.example.toml        # Example routing config
└── tests/
    └── integration/          # End-to-end routing tests
```

---

## Configuration

### Routing Strategies

In `nexus.toml`:

```toml
[routing]
# Available strategies: smart | round_robin | priority_only | random
strategy = "smart"
max_retries = 2
```

| Strategy | Behavior | Best For |
|---|---|---|
| `smart` (default) | Scores by priority, load, latency; picks highest | Production — balanced decisions |
| `round_robin` | Rotates through healthy backends | Even distribution across identical backends |
| `priority_only` | Always picks lowest priority number | Preferred backend with cheap fallback |
| `random` | Random selection from healthy candidates | Testing, chaos engineering |

### Scoring Weights (Smart Strategy Only)

```toml
[routing.weights]
priority = 50    # Weight for backend priority (lower number = preferred)
load = 30        # Weight for current pending requests (fewer = better)
latency = 20     # Weight for average latency EMA (lower = better)
```

**Weights must sum to 100.** The score formula:

```
score = (priority_score × priority_weight + load_score × load_weight + latency_score × latency_weight) / 100
```

Where each sub-score ranges 0–100 (higher is better).

### Backend Priorities

```toml
[[backends]]
name = "fast-gpu"
url = "http://192.168.1.100:11434"
type = "ollama"
priority = 1      # Lower = more preferred

[[backends]]
name = "slow-cpu"
url = "http://192.168.1.200:11434"
type = "ollama"
priority = 10     # Higher = less preferred
```

### Latency-Focused Config Example

```toml
[routing]
strategy = "smart"

[routing.weights]
priority = 20
load = 30
latency = 50      # Heavily favor low-latency backends
```

### Load-Balancing Config Example

```toml
[routing]
strategy = "smart"

[routing.weights]
priority = 10
load = 70          # Heavily favor least-loaded backends
latency = 20
```

---

## Usage

### 1. Start Nexus with Multiple Backends

```toml
# nexus.toml
[routing]
strategy = "smart"

[routing.weights]
priority = 50
load = 30
latency = 20

[[backends]]
name = "gpu-server"
url = "http://192.168.1.100:11434"
type = "ollama"
priority = 1

[[backends]]
name = "cpu-server"
url = "http://192.168.1.200:11434"
type = "ollama"
priority = 5
```

```bash
RUST_LOG=nexus::routing=debug cargo run -- serve
```

### 2. Send a Request and Observe Routing

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:8b",
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

With `RUST_LOG=nexus::routing=debug`, you'll see the routing decision in logs:

```
DEBUG nexus::routing: Selected backend  route_reason="highest_score:gpu-server:98"
```

### 3. Send a Vision Request

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llava:13b",
    "messages": [{
      "role": "user",
      "content": [
        {"type": "text", "text": "What is in this image?"},
        {"type": "image_url", "image_url": {"url": "https://example.com/photo.jpg"}}
      ]
    }]
  }'
```

The router detects `image_url` in the request payload and only considers backends where `llava:13b` has `supports_vision = true`.

### 4. Send a Tools/Function-Calling Request

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:8b",
    "messages": [{"role": "user", "content": "What is the weather in NYC?"}],
    "tools": [{
      "type": "function",
      "function": {
        "name": "get_weather",
        "parameters": {"type": "object", "properties": {"city": {"type": "string"}}}
      }
    }]
  }'
```

The router only routes to backends where the model has `supports_tools = true`.

### 5. Send a JSON Mode Request

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:8b",
    "messages": [{"role": "user", "content": "Return a JSON object with name and age"}],
    "response_format": {"type": "json_object"}
  }'
```

The router only routes to backends where the model has `supports_json_mode = true`.

---

## Manual Testing

### Test 1: Smart Routing Selects Highest Score

1. Configure two backends with different priorities:
   ```toml
   [[backends]]
   name = "primary"
   url = "http://localhost:11434"
   type = "ollama"
   priority = 1

   [[backends]]
   name = "secondary"
   url = "http://localhost:11435"
   type = "ollama"
   priority = 10
   ```

2. Start Nexus:
   ```bash
   RUST_LOG=nexus::routing=debug cargo run -- serve
   ```

3. Send a request:
   ```bash
   curl -s http://localhost:8000/v1/chat/completions \
     -H "Content-Type: application/json" \
     -d '{"model": "llama3:8b", "messages": [{"role": "user", "content": "Hi"}]}'
   ```

4. Check logs for routing decision.

**Expected**: `route_reason` shows `highest_score:primary:...` (primary has higher score due to lower priority number).

✅ Pass if: Primary backend selected  
❌ Fail if: Secondary selected despite lower priority

### Test 2: Round-Robin Distribution

1. Set strategy to `round_robin`:
   ```toml
   [routing]
   strategy = "round_robin"
   ```

2. Send 4 requests:
   ```bash
   for i in 1 2 3 4; do
     curl -s http://localhost:8000/v1/chat/completions \
       -H "Content-Type: application/json" \
       -d '{"model": "llama3:8b", "messages": [{"role": "user", "content": "Hi"}]}' &
   done
   wait
   ```

3. Check logs.

**Expected**: Requests alternate between backends (`round_robin:index_0`, `round_robin:index_1`, ...).

✅ Pass if: Requests distributed across backends  
❌ Fail if: All requests go to same backend

### Test 3: Only Healthy Backends Selected

1. Configure two backends, ensure one is down.

2. Send a request:
   ```bash
   curl -s http://localhost:8000/v1/chat/completions \
     -H "Content-Type: application/json" \
     -d '{"model": "llama3:8b", "messages": [{"role": "user", "content": "Hi"}]}'
   ```

**Expected**: Only the healthy backend is selected. Log shows `only_healthy_backend`.

✅ Pass if: Unhealthy backend never selected  
❌ Fail if: Request fails or routes to unhealthy backend

### Test 4: Capability Mismatch Returns Error

Send a vision request for a non-vision model (when the model doesn't support vision on any backend):

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:8b",
    "messages": [{
      "role": "user",
      "content": [
        {"type": "text", "text": "Describe this"},
        {"type": "image_url", "image_url": {"url": "https://example.com/photo.jpg"}}
      ]
    }]
  }'
```

**Expected**: 400 error with capability mismatch message if no vision-capable backend has the model.

✅ Pass if: Clear error about missing vision capability  
❌ Fail if: Request silently routes to non-vision backend

### Test 5: Model Not Found Returns 404

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "nonexistent-model",
    "messages": [{"role": "user", "content": "Hi"}]
  }'
```

**Expected**:
```json
{
  "error": {
    "message": "Model 'nonexistent-model' not found",
    "type": "invalid_request_error",
    "code": "model_not_found"
  }
}
```

✅ Pass if: 404 with OpenAI-compatible error format  
❌ Fail if: 500 or non-standard error format

### Test 6: No Healthy Backend Returns 503

When model exists but all backends hosting it are unhealthy:

**Expected**:
```json
{
  "error": {
    "message": "No healthy backend available for model 'llama3:8b'",
    "type": "server_error",
    "code": "service_unavailable"
  }
}
```

✅ Pass if: 503 with actionable error message  
❌ Fail if: 404 or silent failure

### Test 7: Run Unit Tests

```bash
# All routing tests
cargo test routing::

# Specific test modules
cargo test routing::tests::                # Strategy parsing, display
cargo test routing::filter_tests::         # Candidate filtering
cargo test routing::scoring::tests::       # Score calculation
cargo test routing::requirements::tests::  # Request requirement extraction
cargo test config::routing::tests::        # Config deserialization
```

**Expected**:
```
test routing::tests::routing_strategy_default_is_smart ... ok
test routing::tests::routing_strategy_from_str ... ok
test routing::scoring::tests::score_with_default_weights ... ok
test routing::scoring::tests::score_prioritizes_low_priority ... ok
test routing::scoring::tests::score_prioritizes_low_load ... ok
test routing::scoring::tests::score_prioritizes_low_latency ... ok
test routing::requirements::tests::detects_vision_requirement ... ok
test routing::requirements::tests::detects_tools_requirement ... ok
test routing::requirements::tests::detects_json_mode_requirement ... ok
...
```

### Test 8: Invalid Weights Rejected

Weights that don't sum to 100 should be caught at startup:

```toml
[routing.weights]
priority = 50
load = 50
latency = 50
```

**Expected**: Startup error or validation warning about weights not summing to 100.

---

## Scoring Deep Dive

### How Scores Are Calculated

```
priority_score = 100 - min(priority, 100)       # priority 1 → score 99
load_score     = 100 - min(pending_requests, 100) # 0 pending → score 100
latency_score  = 100 - min(avg_latency_ms/10, 100) # 50ms → score 95

total = (priority_score × 50 + load_score × 30 + latency_score × 20) / 100
```

### Example Calculations

| Backend | Priority | Pending | Latency (ms) | Score (default weights) |
|---|---|---|---|---|
| gpu-server | 1 | 0 | 50 | (99×50 + 100×30 + 95×20) / 100 = **98** |
| cpu-server | 5 | 3 | 200 | (95×50 + 97×30 + 80×20) / 100 = **92** |
| overloaded | 1 | 50 | 500 | (99×50 + 50×30 + 50×20) / 100 = **74** |

---

## Debugging Tips

### Routing Decisions Not Visible

Enable debug logging for the routing module:
```bash
RUST_LOG=nexus::routing=debug cargo run -- serve
```

### Unexpected Backend Selected

1. Check backend health status:
   ```bash
   curl -s http://localhost:8000/health | jq .
   ```

2. Verify model exists on expected backend:
   ```bash
   curl -s http://localhost:8000/v1/models | jq '.data[] | {id, owned_by}'
   ```

3. Review scoring weights — a high `load` weight will prefer empty backends over high-priority ones.

### All Requests Go to One Backend

- If using `smart`: The highest-priority backend with lowest load/latency always wins. Increase `load` weight to spread requests.
- If using `round_robin`: Verify multiple backends are healthy and have the requested model.
- If using `priority_only`: This is expected — it always picks the lowest priority number.

### Capability Filtering Too Aggressive

Model capabilities (vision, tools, json_mode) are discovered by the health checker. If a backend reports a model without capabilities:
1. Check the backend's model metadata response
2. Verify the health checker is populating capability flags correctly
3. Check with `cargo run -- models list` to see capability flags

---

## References

- **Feature Spec**: `specs/006-intelligent-router/spec.md`
- **Data Model**: `specs/006-intelligent-router/data-model.md`
- **Implementation Walkthrough**: `specs/006-intelligent-router/walkthrough.md`
- **Scoring Algorithm**: `src/routing/scoring.rs`
- **Capability Requirements**: `src/routing/requirements.rs`
