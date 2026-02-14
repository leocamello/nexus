# Quickstart: Model Aliases

**Feature**: F07 Model Aliases  
**Status**: ✅ Implemented  
**Prerequisites**: Rust 1.87+, Nexus codebase cloned, at least one backend with models

---

## Overview

Model aliases let you map friendly or standard model names to your actual local model identifiers. For example, you can map `gpt-4` to `llama3:70b` so that any client sending `model: "gpt-4"` is transparently routed to your local Llama model. Aliases support chaining (up to 3 levels) and include circular reference detection at startup.

---

## Project Structure

```
nexus/
├── src/
│   ├── routing/
│   │   └── mod.rs          # Router.resolve_alias() — chain resolution (max 3 levels)
│   ├── config/
│   │   └── routing.rs      # RoutingConfig.aliases HashMap, validate_aliases()
│   └── api/
│       └── completions.rs  # Uses resolved model name for backend selection
├── nexus.example.toml      # Example alias config
└── tests/
    └── integration/        # End-to-end alias tests
```

---

## Configuration

### Basic Aliases

In `nexus.toml`:

```toml
[routing.aliases]
"gpt-4" = "llama3:70b"
"gpt-4-turbo" = "llama3:70b"
"gpt-3.5-turbo" = "mistral:7b"
"claude-3-sonnet" = "qwen2:72b"
```

Now any request using `model: "gpt-4"` is silently routed to `llama3:70b`.

### Chained Aliases (Max 3 Levels)

```toml
[routing.aliases]
"best" = "gpt-4"              # Level 1: "best" → "gpt-4"
"gpt-4" = "llama3:70b"        # Level 2: "gpt-4" → "llama3:70b"
```

Request for `best` → resolves to `gpt-4` → resolves to `llama3:70b`.

### Three-Level Chain

```toml
[routing.aliases]
"default" = "best"             # Level 1
"best" = "gpt-4"              # Level 2
"gpt-4" = "llama3:70b"        # Level 3 (maximum depth)
```

### Full Config Example

```toml
[server]
host = "0.0.0.0"
port = 8000

[routing]
strategy = "smart"

[routing.aliases]
# OpenAI compatibility
"gpt-4" = "llama3:70b"
"gpt-4-turbo" = "llama3:70b"
"gpt-3.5-turbo" = "mistral:7b"
# Convenience names
"best" = "llama3:70b"
"fast" = "mistral:7b"
"vision" = "llava:13b"

[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
priority = 1
```

---

## Usage

### 1. Configure Aliases and Start Nexus

```bash
# Edit your nexus.toml with aliases (see above), then:
RUST_LOG=nexus::routing=debug cargo run -- serve
```

### 2. Send a Request Using an Alias

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4",
    "messages": [{"role": "user", "content": "Hello, who are you?"}]
  }'
```

With debug logging, you'll see alias resolution:

```
DEBUG nexus::routing: Resolved alias  from="gpt-4" to="llama3:70b" depth=1
DEBUG nexus::routing: Alias resolution complete  original="gpt-4" resolved="llama3:70b" chain_depth=1
```

### 3. Test Chained Aliases

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "best",
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

With `"best" → "gpt-4" → "llama3:70b"` chain:

```
DEBUG nexus::routing: Resolved alias  from="best" to="gpt-4" depth=1
DEBUG nexus::routing: Resolved alias  from="gpt-4" to="llama3:70b" depth=2
DEBUG nexus::routing: Alias resolution complete  original="best" resolved="llama3:70b" chain_depth=2
```

### 4. Request With Non-Aliased Model

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:70b",
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

No alias resolution occurs — the model name is used directly.

---

## Manual Testing

### Test 1: Basic Alias Resolution

1. Config:
   ```toml
   [routing.aliases]
   "gpt-4" = "llama3:70b"
   ```

2. Request:
   ```bash
   curl -s http://localhost:8000/v1/chat/completions \
     -H "Content-Type: application/json" \
     -d '{"model": "gpt-4", "messages": [{"role": "user", "content": "Hi"}]}'
   ```

3. Check debug logs.

**Expected**: Log shows `Resolved alias from="gpt-4" to="llama3:70b"`, request succeeds if `llama3:70b` exists.

✅ Pass if: Request routed to `llama3:70b`  
❌ Fail if: 404 error for `gpt-4`

### Test 2: Two-Level Chain Resolution

1. Config:
   ```toml
   [routing.aliases]
   "best" = "gpt-4"
   "gpt-4" = "llama3:70b"
   ```

2. Request:
   ```bash
   curl -s http://localhost:8000/v1/chat/completions \
     -H "Content-Type: application/json" \
     -d '{"model": "best", "messages": [{"role": "user", "content": "Hi"}]}'
   ```

**Expected**: Log shows chain_depth=2, request resolves to `llama3:70b`.

✅ Pass if: Two alias hops visible in logs  
❌ Fail if: Resolves only to `gpt-4` (chain not followed)

### Test 3: Three-Level Chain (Maximum Depth)

1. Config:
   ```toml
   [routing.aliases]
   "default" = "best"
   "best" = "gpt-4"
   "gpt-4" = "llama3:70b"
   ```

2. Request with `"model": "default"`.

**Expected**: Resolves through 3 levels to `llama3:70b`.

✅ Pass if: chain_depth=3  
❌ Fail if: Chain stops at depth 2

### Test 4: Chain Depth Exceeds Maximum (Silently Stops)

1. Config:
   ```toml
   [routing.aliases]
   "a" = "b"
   "b" = "c"
   "c" = "d"
   "d" = "llama3:70b"   # This 4th level won't be resolved
   ```

2. Request with `"model": "a"`.

**Expected**: Resolves to `d` (stops at depth 3), NOT `llama3:70b`. If `d` is not a real model, returns 404.

✅ Pass if: Resolution stops at depth 3  
❌ Fail if: Resolves all the way to `llama3:70b`

### Test 5: Circular Alias Detection at Startup

1. Config:
   ```toml
   [routing.aliases]
   "a" = "b"
   "b" = "a"
   ```

2. Start Nexus:
   ```bash
   cargo run -- serve
   ```

**Expected**: Startup error about circular alias detected.

✅ Pass if: Startup fails with `CircularAlias` error  
❌ Fail if: Nexus starts and loops forever

### Test 6: Self-Referencing Alias

```toml
[routing.aliases]
"a" = "a"
```

**Expected**: Startup fails with circular alias error (`start: "a"`, `cycle: "a"`).

✅ Pass if: Error caught at config validation  
❌ Fail if: Nexus starts

### Test 7: Non-Aliased Model Passes Through

1. Config:
   ```toml
   [routing.aliases]
   "gpt-4" = "llama3:70b"
   ```

2. Request with `"model": "mistral:7b"` (not aliased).

**Expected**: No alias resolution logged, request routes to `mistral:7b` directly.

✅ Pass if: No alias debug logs for this request  
❌ Fail if: Error or unexpected resolution

### Test 8: Alias to Nonexistent Model Returns 404

```toml
[routing.aliases]
"gpt-4" = "nonexistent-model"
```

```bash
curl -s http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "gpt-4", "messages": [{"role": "user", "content": "Hi"}]}'
```

**Expected**: Alias resolves, but then 404 because `nonexistent-model` doesn't exist in any backend.

✅ Pass if: 404 error for `nonexistent-model`  
❌ Fail if: 404 for `gpt-4` (alias not resolved)

### Test 9: Run Unit Tests

```bash
# Alias-related tests
cargo test config::routing::tests::

# All routing tests (includes alias resolution)
cargo test routing::
```

**Expected**:
```
test config::routing::tests::validates_circular_alias_direct ... ok
test config::routing::tests::validates_circular_alias_indirect ... ok
test config::routing::tests::validates_circular_alias_three_way ... ok
test config::routing::tests::validates_non_circular_aliases ... ok
test config::routing::tests::validates_empty_aliases ... ok
test config::routing::tests::validates_chained_aliases_no_cycle ... ok
...
```

---

## How It Works

### Resolution Algorithm

```
resolve_alias(model):
    current = model
    depth = 0
    while depth < 3:
        if current in aliases:
            current = aliases[current]
            depth += 1
        else:
            break
    return current
```

### Validation at Startup

Before the router is created, `validate_aliases()` in `src/config/routing.rs` traverses all alias chains and rejects configurations with circular references:

```
validate_aliases(aliases):
    for each start_key in aliases:
        visited = {start_key}
        current = start_key
        while current in aliases:
            target = aliases[current]
            if target in visited:
                ERROR: CircularAlias(start, target)
            visited.add(target)
            current = target
    OK
```

---

## Debugging Tips

### Alias Not Resolving

1. Enable debug logging:
   ```bash
   RUST_LOG=nexus::routing=debug cargo run -- serve
   ```

2. Check that the alias key matches exactly (case-sensitive):
   ```toml
   # These are different:
   "GPT-4" = "llama3:70b"    # Only matches model "GPT-4"
   "gpt-4" = "llama3:70b"    # Only matches model "gpt-4"
   ```

3. Verify the alias is in the `[routing.aliases]` section (not `[routing]`).

### Alias Resolves But Request Fails

The alias resolves the model name, but the target model must exist on a healthy backend:

```bash
# Check available models
curl -s http://localhost:8000/v1/models | jq '.data[].id'
```

### Circular Alias Error at Startup

The error message includes the chain:
```
ConfigError: Circular alias detected: start="a", cycle="b"
```

Fix by removing or breaking the cycle in `[routing.aliases]`.

---

## References

- **Feature Spec**: `specs/007-model-aliases/spec.md`
- **Data Model**: `specs/007-model-aliases/data-model.md`
- **Implementation Walkthrough**: `specs/007-model-aliases/walkthrough.md`
- **Alias Validation**: `src/config/routing.rs` — `validate_aliases()`
- **Alias Resolution**: `src/routing/mod.rs` — `Router::resolve_alias()`
