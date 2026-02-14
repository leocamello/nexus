# Routing Headers & Scoring Contract

This document defines the `X-Nexus-*` response headers, the scoring algorithm specification, and the routing error types with their HTTP status codes.

## Response Headers

Nexus adds metadata headers to proxied responses. These headers never modify the JSON response body (OpenAI compatibility principle).

### Currently Implemented Headers

| Header | When Set | Value Format | Description |
|---|---|---|---|
| `X-Nexus-Fallback-Model` | Only when fallback used | Model name string | The actual model that served the request (see F08 contract) |

### Planned Headers (Not Yet Implemented)

| Header | Planned Version | Value Format | Description |
|---|---|---|---|
| `X-Nexus-Backend` | v0.3 | Backend name | Which backend served the request |
| `X-Nexus-Backend-Type` | v0.3 | `local` \| `cloud` | Backend location type |
| `X-Nexus-Route-Reason` | v0.3 | See route reason format | Why this backend was chosen |
| `X-Nexus-Cost-Estimated` | v0.3 | `$0.0042` | Estimated cost (cloud backends only) |
| `X-Nexus-Privacy-Zone` | v0.3 | `restricted` \| `open` | Privacy zone of the backend used |

---

## RoutingResult

The `Router::select_backend()` method returns a `RoutingResult` on success:

```rust
pub struct RoutingResult {
    pub backend: Arc<Backend>,
    pub actual_model: String,
    pub fallback_used: bool,
    pub route_reason: String,
}
```

| Field | Type | Description |
|---|---|---|
| `backend` | `Arc<Backend>` | The selected backend instance |
| `actual_model` | `String` | Model name after alias resolution and fallback (may differ from requested) |
| `fallback_used` | `bool` | `true` if a fallback model was used instead of the primary |
| `route_reason` | `String` | Human-readable explanation of the routing decision |

---

## Route Reason Format

The `route_reason` field follows a structured string format depending on the routing strategy and context.

### Primary Model (No Fallback)

| Strategy | Format | Example |
|---|---|---|
| Smart (single candidate) | `only_healthy_backend` | `only_healthy_backend` |
| Smart (multiple candidates) | `highest_score:{backend_name}:{score}` | `highest_score:gpu-node-1:95.00` |
| RoundRobin (single) | `only_healthy_backend` | `only_healthy_backend` |
| RoundRobin (multiple) | `round_robin:index_{n}` | `round_robin:index_3` |
| PriorityOnly (single) | `only_healthy_backend` | `only_healthy_backend` |
| PriorityOnly (multiple) | `priority:{backend_name}:{priority}` | `priority:gpu-node-1:1` |
| Random (single) | `only_healthy_backend` | `only_healthy_backend` |
| Random (multiple) | `random:{backend_name}` | `random:gpu-node-1` |

### Fallback Model

When a fallback model is used, the route reason is prefixed with `fallback:{original_model}:`:

| Strategy | Format | Example |
|---|---|---|
| Smart | `fallback:{model}:highest_score:{score}` | `fallback:llama3:70b:highest_score:92.00` |
| RoundRobin | `fallback:{model}:round_robin:index_{n}` | `fallback:llama3:70b:round_robin:index_0` |
| PriorityOnly | `fallback:{model}:priority:{priority}` | `fallback:llama3:70b:priority:1` |
| Random | `fallback:{model}:random` | `fallback:llama3:70b:random` |

---

## Scoring Algorithm

Used by the **Smart** routing strategy (default). Selects the backend with the highest composite score.

### Inputs

| Input | Source | Type | Range |
|---|---|---|---|
| `priority` | `Backend.priority` | `u32` | 0–100 (lower = more preferred) |
| `pending_requests` | `Backend.pending_requests` (atomic) | `u32` | 0+ (clamped to 100) |
| `avg_latency_ms` | `Backend.avg_latency_ms` (atomic EMA) | `u32` | 0+ (divided by 10, clamped to 100) |

### Weights

Three configurable weights that **must sum to 100**:

| Weight | Default | Config Key |
|---|---|---|
| `priority` | 50 | `[routing].priority_weight` |
| `load` | 30 | `[routing].load_weight` |
| `latency` | 20 | `[routing].latency_weight` |

### Formula

```
priority_score = 100 - min(priority, 100)
load_score     = 100 - min(pending_requests, 100)
latency_score  = 100 - min(avg_latency_ms / 10, 100)

score = (priority_score × priority_weight
       + load_score     × load_weight
       + latency_score  × latency_weight) / 100
```

### Output

- **Type**: `u32`
- **Range**: 0–100 (higher is better)
- **Best possible**: 100 (priority=0, pending=0, latency=0ms)
- **Worst possible**: 0 (priority≥100, pending≥100, latency≥1000ms)

### Scaling Reference

| Latency (ms) | Latency Score |
|---|---|
| 0 | 100 |
| 100 | 90 |
| 500 | 50 |
| 1000+ | 0 |

### Example Calculation

Backend with priority=1, pending=0, latency=50ms, default weights:

```
priority_score = 100 - 1 = 99
load_score     = 100 - 0 = 100
latency_score  = 100 - (50/10) = 95

score = (99×50 + 100×30 + 95×20) / 100
      = (4950 + 3000 + 1900) / 100
      = 98
```

---

## Routing Strategies

| Strategy | Config Value | Selection Method |
|---|---|---|
| Smart | `smart` | Highest `score_backend()` score |
| RoundRobin | `round_robin` | Atomic counter mod candidate count |
| PriorityOnly | `priority_only` | Lowest `backend.priority` value |
| Random | `random` | Hash-based random index |

---

## Candidate Filtering

Before scoring, backends are filtered through three stages:

1. **Model match**: Only backends that have the requested model (after alias resolution)
2. **Health check**: Only backends with `status == Healthy`
3. **Capability match**: Only backends where the model supports required capabilities:
   - `needs_vision` → `model.supports_vision`
   - `needs_tools` → `model.supports_tools`
   - `needs_json_mode` → `model.supports_json_mode`
   - `estimated_tokens ≤ model.context_length`

---

## Request Requirements Extraction

Requirements are extracted from the incoming `ChatCompletionRequest`:

| Requirement | Detection Method |
|---|---|
| `needs_vision` | Any message content part with `type == "image_url"` |
| `needs_tools` | `request.extra` contains `"tools"` key |
| `needs_json_mode` | `request.extra.response_format.type == "json_object"` |
| `estimated_tokens` | Sum of message text lengths ÷ 4 (rough char-to-token ratio) |

---

## Alias Resolution

Model aliases are resolved before routing, with a maximum chain depth of **3 levels**:

```
"gpt" → "llama3:70b" → (no further alias) → route "llama3:70b"
```

If an alias chain exceeds 3 levels, resolution stops at the current value.

---

## Error Types and HTTP Status Codes

| RoutingError | HTTP Status | Error Code | Error Message Format |
|---|---|---|---|
| `ModelNotFound` | `404 Not Found` | `model_not_found` | `Model '{model}' not found. Available models: [...]` |
| `FallbackChainExhausted` | `404 Not Found` | `model_not_found` | `Model '{chain[0]}' not found. Available models: [...]` |
| `NoHealthyBackend` | `503 Service Unavailable` | `service_unavailable` | `No healthy backend available for model '{model}'` |
| `CapabilityMismatch` | `400 Bad Request` | `invalid_request_error` | `Model '{model}' lacks required capabilities: ["{cap}", ...]` |

### Error Response Format (OpenAI-Compatible)

```json
{
  "error": {
    "message": "Model 'gpt-5' not found. Available models: llama3:8b, mistral:7b",
    "type": "invalid_request_error",
    "code": "model_not_found"
  }
}
```

```json
{
  "error": {
    "message": "No healthy backend available for model 'llama3:70b'",
    "type": "server_error",
    "code": "service_unavailable"
  }
}
```

```json
{
  "error": {
    "message": "Model 'llama3:8b' lacks required capabilities: [\"vision\"]",
    "type": "invalid_request_error",
    "code": null
  }
}
```

---

## Implementation Notes

- **Routing decision target**: < 1ms (per constitution latency budget)
- **Atomic reads**: All scoring inputs use lock-free `AtomicU32`/`AtomicU64` with `Relaxed` ordering
- **Latency EMA**: `new_avg = (sample + 4 × old_avg) / 5` (α=0.2), stored as `AtomicU32`
- **Round-robin counter**: Global `AtomicU64`, index = `counter % candidates.len()`
