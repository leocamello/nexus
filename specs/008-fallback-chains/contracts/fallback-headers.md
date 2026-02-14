# Fallback Headers Contract

This document defines the `X-Nexus-Fallback-Model` response header — when it's set, what values it contains, and how clients should interpret it.

## Header Definition

| Property | Value |
|---|---|
| **Header Name** | `X-Nexus-Fallback-Model` |
| **Constant** | `FALLBACK_HEADER = "x-nexus-fallback-model"` (lowercase for HTTP/2 compatibility) |
| **Location** | HTTP response headers (both streaming and non-streaming) |
| **Presence** | Conditional — only when a fallback model was used |
| **Value** | The actual model name that served the request |

---

## When the Header Is Set

The header is **only** added when `RoutingResult.fallback_used == true`. This occurs when:

1. The requested model (after alias resolution) has no healthy, capable backends
2. A fallback chain is configured for that model
3. A model in the fallback chain has at least one healthy, capable backend

The header is **not** set when:

- The primary model (or its alias target) is available and used
- No fallback chain is configured
- The request fails entirely (error response returned instead)

---

## Header Value

The value is the model name from the fallback chain that actually served the request.

### Examples

| Requested Model | Alias Target | Fallback Chain | Served By | Header Value |
|---|---|---|---|---|
| `llama3:70b` | — | `[qwen2:72b, mistral:7b]` | `qwen2:72b` | `qwen2:72b` |
| `llama3:70b` | — | `[qwen2:72b, mistral:7b]` | `mistral:7b` | `mistral:7b` |
| `best` | `llama3:70b` | `[qwen2:72b]` | `qwen2:72b` | `qwen2:72b` |
| `llama3:70b` | — | `[]` (none) | `llama3:70b` | _(not set)_ |
| `llama3:8b` | — | — | `llama3:8b` | _(not set)_ |

---

## Client Interpretation

### Detecting Fallbacks

```python
response = client.post("/v1/chat/completions", json=payload)
fallback_model = response.headers.get("X-Nexus-Fallback-Model")

if fallback_model:
    print(f"Note: Request was served by fallback model '{fallback_model}' "
          f"instead of '{payload['model']}'")
```

### Key Rules for Clients

1. **Header absent** = the requested model (or its alias target) served the request
2. **Header present** = a different model from the fallback chain served the request
3. The JSON response body's `model` field reflects the backend's own response and is **not modified** by Nexus (OpenAI compatibility)
4. The header value is always a concrete model name (never an alias)

---

## Chain Traversal Algorithm

```
select_backend(requirements):
    model = resolve_alias(requirements.model)     # Max 3 levels

    # 1. Try primary model
    candidates = filter_candidates(model, requirements)
    if candidates not empty:
        return select_by_strategy(candidates)      # fallback_used = false

    # 2. Try fallback chain in order
    fallbacks = get_fallbacks(model)               # From config [routing.fallbacks]
    for fallback_model in fallbacks:
        candidates = filter_candidates(fallback_model, requirements)
        if candidates not empty:
            return select_by_strategy(candidates)  # fallback_used = true
                                                   # actual_model = fallback_model

    # 3. All attempts failed
    if fallbacks not empty:
        return FallbackChainExhausted              # 404
    else if model_exists_anywhere:
        return NoHealthyBackend                    # 503
    else:
        return ModelNotFound                       # 404
```

### Filter Criteria (Each Step)

Each model in the chain is filtered identically:

1. Backend has the model registered
2. Backend status is `Healthy`
3. Model capabilities match request requirements (vision, tools, JSON mode, context length)

---

## Exhaustion Error Format

When all models in the fallback chain are unavailable, Nexus returns:

**Status**: `404 Not Found`  
**Content-Type**: `application/json`

```json
{
  "error": {
    "message": "Model 'llama3:70b' not found. Available models: mistral:7b, phi-3:mini",
    "type": "invalid_request_error",
    "code": "model_not_found"
  }
}
```

The error message references the first model in the chain (the originally requested model after alias resolution). Available models are listed to help the client choose an alternative.

**Note**: The error uses the `ModelNotFound` and `model_not_found` code (not a custom "fallback exhausted" code) for OpenAI client compatibility. The internal `RoutingError::FallbackChainExhausted` variant carries the full chain for logging:

```rust
RoutingError::FallbackChainExhausted {
    chain: vec!["llama3:70b", "qwen2:72b", "mistral:7b"]
}
```

---

## Fallback Chain Configuration

```toml
[routing.fallbacks]
"llama3:70b" = ["qwen2:72b", "mistral:7b"]
"gpt-4" = ["llama3:70b", "llama3:8b"]
```

- Chains are ordered — first available model wins
- Chains are not recursive (a fallback model's own chain is not consulted)
- Combined with aliases: alias resolution happens **before** fallback lookup

### Alias + Fallback Interaction

```toml
[routing.aliases]
"best" = "llama3:70b"

[routing.fallbacks]
"llama3:70b" = ["qwen2:72b"]
```

Request for `best`:
1. Resolve alias: `best` → `llama3:70b`
2. Try `llama3:70b` → no healthy backend
3. Try fallback `qwen2:72b` → available
4. Response header: `X-Nexus-Fallback-Model: qwen2:72b`

---

## Streaming Behavior

The `X-Nexus-Fallback-Model` header is included in the initial HTTP response headers for SSE (Server-Sent Events) streaming responses. Since HTTP headers are sent before the body, the fallback information is available to clients immediately — before any chunks arrive.

```http
HTTP/1.1 200 OK
Content-Type: text/event-stream
X-Nexus-Fallback-Model: qwen2:72b

data: {"id":"chatcmpl-123","object":"chat.completion.chunk",...}

data: [DONE]
```

---

## Metrics

When a fallback is used, Nexus records:

```
nexus_fallbacks_total{from_model="llama3:70b", to_model="qwen2:72b"} 1
```

This counter increments each time a fallback model serves a request, labeled with both the originally requested model and the actual model used.

---

## Implementation Notes

### Header Injection

The header is added in `src/api/completions.rs` at two points:

1. **Non-streaming path**: After receiving the complete response from the backend
2. **Streaming path**: Before returning the SSE response

```rust
if fallback_used {
    if let Ok(header_value) = HeaderValue::from_str(&actual_model) {
        resp.headers_mut()
            .insert(HeaderName::from_static(FALLBACK_HEADER), header_value);
    }
}
```

### Safety

- `HeaderValue::from_str()` is checked — if the model name contains invalid header characters, the header is silently omitted rather than causing a panic
- The header name uses `from_static` for zero-allocation on the hot path
- Memory overhead: ~50 bytes per response when present
