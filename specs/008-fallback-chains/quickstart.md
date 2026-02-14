# Quickstart: Fallback Chains

**Feature**: F08 Fallback Chains  
**Status**: ✅ Implemented  
**Prerequisites**: Rust 1.75+, Nexus codebase cloned, at least one backend with models

---

## Overview

Fallback chains define alternative models to try when the primary model is unavailable (unhealthy, no capacity, or missing). When a fallback model is used, Nexus adds an `x-nexus-fallback-model` response header so the client knows which model actually served the request. This enables graceful degradation without client-side logic.

---

## Project Structure

```
nexus/
├── src/
│   ├── routing/
│   │   ├── mod.rs          # Router.select_backend() — fallback chain traversal
│   │   └── error.rs        # FallbackChainExhausted error
│   ├── config/
│   │   └── routing.rs      # RoutingConfig.fallbacks HashMap<String, Vec<String>>
│   └── api/
│       └── completions.rs  # Adds x-nexus-fallback-model header when fallback used
├── nexus.example.toml      # Example fallback config
└── tests/
    └── integration/        # End-to-end fallback tests
```

---

## Configuration

### Basic Fallback Chain

In `nexus.toml`:

```toml
[routing.fallbacks]
"llama3:70b" = ["qwen2:72b", "mixtral:8x7b"]
```

When `llama3:70b` is requested but unavailable:
1. Try `qwen2:72b`
2. If also unavailable, try `mixtral:8x7b`
3. If all fail, return error

### Multiple Fallback Chains

```toml
[routing.fallbacks]
"llama3:70b" = ["qwen2:72b", "mixtral:8x7b"]
"llava:13b" = ["llava:7b"]
"codellama:34b" = ["codellama:13b", "codellama:7b"]
```

### Fallbacks with Aliases (Combined)

```toml
[routing.aliases]
"gpt-4" = "llama3:70b"

[routing.fallbacks]
"llama3:70b" = ["qwen2:72b", "mixtral:8x7b"]
```

Request flow: `gpt-4` → (alias) → `llama3:70b` → (fallback) → `qwen2:72b` → `mixtral:8x7b`

### Full Config Example

```toml
[server]
host = "0.0.0.0"
port = 8000

[routing]
strategy = "smart"
max_retries = 2

[routing.aliases]
"gpt-4" = "llama3:70b"
"gpt-3.5-turbo" = "mistral:7b"

[routing.fallbacks]
"llama3:70b" = ["qwen2:72b", "mixtral:8x7b"]
"mistral:7b" = ["phi3:mini"]

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

---

## Usage

### 1. Configure Fallbacks and Start Nexus

```bash
RUST_LOG=nexus::routing=debug cargo run -- serve
```

### 2. Send a Request for the Primary Model

```bash
curl -sv http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:70b",
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

If `llama3:70b` is available, it is used normally (no fallback header).

### 3. Trigger a Fallback

When `llama3:70b` is unavailable (backend down, model not loaded):

```bash
curl -sv http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:70b",
    "messages": [{"role": "user", "content": "Hello"}]
  }' 2>&1 | grep -i x-nexus
```

**Expected response headers**:
```
< x-nexus-fallback-model: qwen2:72b
```

**Expected debug log**:
```
WARN  nexus::routing: Using fallback model  requested_model="llama3:70b" fallback_model="qwen2:72b" backend="gpu-server"
```

### 4. Check Fallback Header Programmatically

```bash
# Extract just the fallback header
FALLBACK=$(curl -s -D - http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "llama3:70b", "messages": [{"role": "user", "content": "Hi"}]}' \
  -o /dev/null 2>/dev/null | grep -i x-nexus-fallback-model | tr -d '\r')

if [ -n "$FALLBACK" ]; then
  echo "Fallback was used: $FALLBACK"
else
  echo "Primary model served the request"
fi
```

### 5. Alias + Fallback Combined Flow

```bash
# "gpt-4" → alias to "llama3:70b" → fallback to "qwen2:72b"
curl -sv http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4",
    "messages": [{"role": "user", "content": "Hello"}]
  }' 2>&1 | grep -i x-nexus
```

**Expected debug log sequence**:
```
DEBUG nexus::routing: Resolved alias  from="gpt-4" to="llama3:70b" depth=1
WARN  nexus::routing: Using fallback model  requested_model="llama3:70b" fallback_model="qwen2:72b"
```

---

## Manual Testing

### Test 1: Primary Model Available (No Fallback)

1. Ensure `llama3:70b` is available on a healthy backend.

2. Request:
   ```bash
   curl -sv http://localhost:8000/v1/chat/completions \
     -H "Content-Type: application/json" \
     -d '{"model": "llama3:70b", "messages": [{"role": "user", "content": "Hi"}]}' \
     2>&1 | grep -i x-nexus-fallback
   ```

**Expected**: No `x-nexus-fallback-model` header in response.

✅ Pass if: No fallback header present  
❌ Fail if: Fallback header present when primary is available

### Test 2: First Fallback Used

1. Configure:
   ```toml
   [routing.fallbacks]
   "llama3:70b" = ["qwen2:72b", "mixtral:8x7b"]
   ```

2. Make `llama3:70b` unavailable (stop its backend or ensure no backend has it).

3. Ensure `qwen2:72b` is available.

4. Request:
   ```bash
   curl -sv http://localhost:8000/v1/chat/completions \
     -H "Content-Type: application/json" \
     -d '{"model": "llama3:70b", "messages": [{"role": "user", "content": "Hi"}]}' \
     2>&1 | grep -i x-nexus-fallback
   ```

**Expected**:
```
< x-nexus-fallback-model: qwen2:72b
```

✅ Pass if: First fallback model used and header set  
❌ Fail if: Skips to second fallback or returns error

### Test 3: Second Fallback Used

1. Make both `llama3:70b` and `qwen2:72b` unavailable.
2. Ensure `mixtral:8x7b` is available.

3. Request with `"model": "llama3:70b"`.

**Expected**:
```
< x-nexus-fallback-model: mixtral:8x7b
```

✅ Pass if: Second fallback used  
❌ Fail if: Error returned or first fallback used

### Test 4: All Fallbacks Exhausted

1. Make `llama3:70b`, `qwen2:72b`, AND `mixtral:8x7b` all unavailable.

2. Request:
   ```bash
   curl -s http://localhost:8000/v1/chat/completions \
     -H "Content-Type: application/json" \
     -d '{"model": "llama3:70b", "messages": [{"role": "user", "content": "Hi"}]}'
   ```

**Expected**: 404 error (all models in chain unavailable):
```json
{
  "error": {
    "message": "Model 'llama3:70b' not found...",
    "type": "invalid_request_error",
    "code": "model_not_found"
  }
}
```

✅ Pass if: 404 with model_not_found after trying entire chain  
❌ Fail if: 500 error or partial response

### Test 5: Fallback with Streaming

```bash
curl -sN http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:70b",
    "stream": true,
    "messages": [{"role": "user", "content": "Hi"}]
  }' -D /tmp/nexus-headers.txt

# Check headers
grep -i x-nexus-fallback /tmp/nexus-headers.txt
```

**Expected**: `x-nexus-fallback-model` header is present in the SSE response headers (before the stream body).

✅ Pass if: Fallback header in streaming response  
❌ Fail if: Header missing in streaming mode

### Test 6: Fallback Respects Capability Requirements

1. Configure:
   ```toml
   [routing.fallbacks]
   "llava:13b" = ["llava:7b", "llama3:8b"]
   ```

2. Send a vision request for `llava:13b`:
   ```bash
   curl -s http://localhost:8000/v1/chat/completions \
     -H "Content-Type: application/json" \
     -d '{
       "model": "llava:13b",
       "messages": [{
         "role": "user",
         "content": [
           {"type": "text", "text": "Describe this image"},
           {"type": "image_url", "image_url": {"url": "https://example.com/photo.jpg"}}
         ]
       }]
     }'
   ```

**Expected**: Fallback to `llava:7b` (vision-capable), NOT `llama3:8b` (no vision). The capability filter applies to fallback models too.

✅ Pass if: Only vision-capable fallbacks are considered  
❌ Fail if: Falls back to non-vision model

### Test 7: Model Without Fallback Chain

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "phi3:mini", "messages": [{"role": "user", "content": "Hi"}]}'
```

If `phi3:mini` is unavailable and has no fallback chain configured:

**Expected**: Standard error (NoHealthyBackend or ModelNotFound) — no fallback attempted.

✅ Pass if: No fallback log entries for this model  
❌ Fail if: Unexpected fallback behavior

### Test 8: Alias Then Fallback

1. Config:
   ```toml
   [routing.aliases]
   "gpt-4" = "llama3:70b"

   [routing.fallbacks]
   "llama3:70b" = ["qwen2:72b"]
   ```

2. Make `llama3:70b` unavailable, `qwen2:72b` available.

3. Request:
   ```bash
   curl -sv http://localhost:8000/v1/chat/completions \
     -H "Content-Type: application/json" \
     -d '{"model": "gpt-4", "messages": [{"role": "user", "content": "Hi"}]}' \
     2>&1 | grep -i x-nexus
   ```

**Expected**: Alias resolves first (`gpt-4` → `llama3:70b`), then fallback activates (`llama3:70b` → `qwen2:72b`), and header shows:
```
< x-nexus-fallback-model: qwen2:72b
```

✅ Pass if: Both alias resolution and fallback visible in logs  
❌ Fail if: Fallback not triggered (lookup uses alias name instead of resolved name)

### Test 9: Run Unit Tests

```bash
# Routing tests (includes fallback logic)
cargo test routing::

# Config validation tests
cargo test config::routing::tests::
```

**Expected**:
```
test routing::tests::routing_strategy_default_is_smart ... ok
...
```

---

## How It Works

### Fallback Chain Flow

```
select_backend(requirements):
    model = resolve_alias(requirements.model)       # Step 1: Alias resolution

    candidates = filter_candidates(model)            # Step 2: Try primary
    if candidates not empty:
        return select_by_strategy(candidates)        # No fallback needed

    for fallback_model in fallbacks[model]:           # Step 3: Try each fallback
        candidates = filter_candidates(fallback_model)
        if candidates not empty:
            result = select_by_strategy(candidates)
            result.fallback_used = true
            result.actual_model = fallback_model
            return result

    return FallbackChainExhausted                     # Step 4: All failed
```

### Response Header

When a fallback is used, the response includes:

```
x-nexus-fallback-model: <actual-model-name>
```

This header appears in both non-streaming and streaming (SSE) responses. Clients can check for this header to know that a different model than requested was used.

---

## Debugging Tips

### Fallback Not Triggering

1. Enable debug logging:
   ```bash
   RUST_LOG=nexus::routing=debug cargo run -- serve
   ```

2. Verify fallback chain is in `[routing.fallbacks]` section:
   ```toml
   # Correct:
   [routing.fallbacks]
   "llama3:70b" = ["qwen2:72b"]

   # Wrong (this is an alias, not a fallback):
   [routing.aliases]
   "llama3:70b" = "qwen2:72b"
   ```

3. Ensure the primary model key matches exactly (case-sensitive):
   ```toml
   "llama3:70b" = ["qwen2:72b"]   # Only matches "llama3:70b"
   ```

### Fallback Header Missing

1. Use `-v` (verbose) with curl to see response headers:
   ```bash
   curl -sv http://localhost:8000/v1/chat/completions ...
   ```

2. The header is only set when a fallback model is actually used. If the primary model is available, no header is added.

3. Check that the fallback model name is a valid HTTP header value (no special characters).

### Fallback Chain Exhausted Error

If you see `FallbackChainExhausted`:
1. Check that at least one model in the chain is available:
   ```bash
   curl -s http://localhost:8000/v1/models | jq '.data[].id'
   ```

2. Verify backends hosting fallback models are healthy:
   ```bash
   curl -s http://localhost:8000/health | jq .
   ```

### Fallback With Aliases Not Working

Fallback chains are keyed by the **resolved** model name, not the alias:

```toml
# This works: alias resolves "gpt-4" to "llama3:70b", fallback chain is on "llama3:70b"
[routing.aliases]
"gpt-4" = "llama3:70b"

[routing.fallbacks]
"llama3:70b" = ["qwen2:72b"]

# This does NOT work: fallback chain keyed on alias name
[routing.fallbacks]
"gpt-4" = ["qwen2:72b"]     # Never triggered! Alias is resolved before fallback lookup
```

---

## References

- **Feature Spec**: `specs/008-fallback-chains/spec.md`
- **Data Model**: `specs/008-fallback-chains/data-model.md`
- **Implementation Walkthrough**: `specs/008-fallback-chains/walkthrough.md`
- **Fallback Logic**: `src/routing/mod.rs` — `Router::select_backend()` and `Router::get_fallbacks()`
- **Header Constant**: `src/api/completions.rs` — `FALLBACK_HEADER = "x-nexus-fallback-model"`
