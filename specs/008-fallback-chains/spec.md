# F08: Fallback Chains

**Status**: ✅ Implemented (as part of F06)  
**Priority**: P1  
**Branch**: N/A (merged with F06)  
**Dependencies**: F06 (Intelligent Router)  
**Implementation**: `src/routing/mod.rs`

---

## Overview

### What It Is
A fallback chain system that automatically routes requests to alternative models when the primary model is unavailable, maintaining service availability.

### Goals
1. Maintain service availability when preferred models are unavailable
2. Provide graceful degradation to alternative models
3. Enable transparent fallback without client awareness
4. Support ordered fallback preferences

### Non-Goals
1. Automatic quality matching (user must configure appropriate fallbacks)
2. Dynamic fallback discovery (use static configuration)
3. Cross-capability fallbacks (won't fallback to model without needed features)
4. Multi-level chaining (doesn't follow fallback's fallbacks)

---

## User Stories

### US-01: Primary Model Unavailable
**As a** developer  
**I want** my requests to fallback to alternative models automatically  
**So that** my application remains available when preferred models are down

**Priority**: P0 (Core functionality)

**Acceptance Scenarios**:
- **Given** fallback "llama3:70b" → ["qwen2:72b", "mistral:7b"]
- **And** no backend has healthy "llama3:70b"
- **And** a backend has healthy "qwen2:72b"
- **When** I request model "llama3:70b"
- **Then** the request is routed to the backend with "qwen2:72b"
- **And** WARN log shows fallback occurred

### US-02: Multiple Fallbacks
**As a** system administrator  
**I want** to configure ordered lists of fallback models  
**So that** there's a clear priority order for alternatives

**Priority**: P0 (Core functionality)

**Acceptance Scenarios**:
- **Given** fallback "gpt-4" → ["llama3:70b", "qwen2:72b", "mistral:7b"]
- **And** "gpt-4" and "llama3:70b" are unavailable
- **And** "qwen2:72b" is available
- **When** I request model "gpt-4"
- **Then** the request is routed to "qwen2:72b"
- **And** "mistral:7b" is not considered (higher priority fallback succeeded)

### US-03: Fallback Exhausted
**As a** developer  
**I want** a clear error when no fallback is available  
**So that** I can handle the failure appropriately

**Priority**: P0 (Core functionality)

**Acceptance Scenarios**:
- **Given** fallback "special:model" → ["alternative"]
- **And** both "special:model" and "alternative" are unavailable
- **When** I request model "special:model"
- **Then** I receive a 503 Service Unavailable error
- **And** the error indicates all fallbacks were exhausted

---

## Technical Design

### Fallback Resolution

```rust
/// Find backends, falling back through configured fallback chains
fn find_candidates_with_fallback(&self, model: &str) 
    -> Result<(Vec<Arc<Backend>>, bool), RoutingError> 
{
    // 1. Try primary model
    let candidates = self.registry.get_backends_for_model(model);
    if !candidates.is_empty() {
        return Ok((candidates, false));
    }
    
    // 2. Try fallback chain if configured
    if let Some(fallbacks) = self.fallbacks.get(model) {
        for fallback in fallbacks {
            let candidates = self.registry.get_backends_for_model(fallback);
            if !candidates.is_empty() {
                tracing::warn!(
                    original = model,
                    fallback = %fallback,
                    "Using fallback model"
                );
                return Ok((candidates, true)); // true = fallback used
            }
        }
        
        // All fallbacks exhausted
        return Err(RoutingError::FallbackChainExhausted {
            model: model.to_string(),
            tried: fallbacks.clone(),
        });
    }
    
    // 3. No fallback configured
    Err(RoutingError::ModelNotFound(model.to_string()))
}
```

**Resolution Flow**:
```
┌─────────────────────────────────────────────────────────────┐
│                    select_backend(request)                   │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 1. Resolve aliases                                           │
│    "gpt-4" → "llama3:70b"                                   │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 2. Find backends for resolved model                          │
│    registry.get_backends_for_model("llama3:70b")            │
│    → Found? Continue to step 4                               │
│    → Empty? Continue to step 3                               │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 3. Try fallback chain (if configured)                        │
│    fallbacks["llama3:70b"] = ["qwen2:72b", "mistral:7b"]    │
│    Try each in order until backends found                    │
│    → Found? Log WARN, continue to step 4                     │
│    → All exhausted? Return FallbackChainExhausted error     │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 4. Filter by health and capabilities                         │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 5. Score and select best candidate                           │
└─────────────────────────────────────────────────────────────┘
```

### Data Structure

```rust
pub struct Router {
    registry: Arc<Registry>,
    strategy: RoutingStrategy,
    weights: ScoringWeights,
    aliases: HashMap<String, String>,
    
    /// Fallback chains (model → list of fallbacks in priority order)
    /// Example: {"llama3:70b" → ["qwen2:72b", "mistral:7b"]}
    fallbacks: HashMap<String, Vec<String>>,
    
    round_robin_counter: AtomicU64,
}
```

### Design Decision: Single-Level Fallbacks

**Decision**: Fallbacks are single-level only (don't follow a fallback's fallbacks).

**Rationale**:
1. **Predictability**: Clear understanding of what models might be used
2. **Simplicity**: Avoids complex graph traversal
3. **Performance**: O(n) where n = fallback chain length
4. **Control**: User explicitly defines all acceptable alternatives

**Example**:
```toml
[routing.fallbacks]
"primary" = ["fallback1", "fallback2"]
"fallback1" = ["alternate"]  # This chain is NOT followed from "primary"
```

When requesting "primary":
- Tries "primary"
- Tries "fallback1" (NOT "alternate")
- Tries "fallback2"
- Errors if all unavailable

---

## Configuration

```toml
[routing.fallbacks]
# Large models fallback to smaller
"llama3:70b" = ["qwen2:72b", "mixtral:8x7b", "llama3:8b"]

# Alias compatibility
"gpt-4" = ["llama3:70b", "qwen2:72b", "mistral:7b"]
"claude-3-opus" = ["llama3:70b", "mixtral:8x7b"]

# Vision models
"llava:34b" = ["llava:13b", "llava:7b"]
```

**Parsing**:
```rust
pub struct RoutingConfig {
    // ... other fields ...
    
    #[serde(default)]
    pub fallbacks: HashMap<String, Vec<String>>,
}
```

---

## Error Handling

### FallbackChainExhausted

```rust
#[derive(Debug, thiserror::Error)]
pub enum RoutingError {
    #[error("Fallback chain exhausted for model '{model}'. Tried: {tried:?}")]
    FallbackChainExhausted {
        model: String,
        tried: Vec<String>,
    },
    // ... other variants ...
}
```

**HTTP Response**:
```json
{
    "error": {
        "message": "Fallback chain exhausted for model 'llama3:70b'. Tried: [\"qwen2:72b\", \"mistral:7b\"]",
        "type": "service_unavailable",
        "code": 503
    }
}
```

---

## Logging

Fallback usage is logged at WARN level:

```
WARN routing: Using fallback model original="llama3:70b" fallback="qwen2:72b"
WARN routing: Fallback chain exhausted model="llama3:70b" tried=["qwen2:72b", "mistral:7b"]
```

**Log Fields**:
- `original_model`: What the client requested
- `fallback_model`: What we're actually using
- `tried`: List of models attempted

---

## Fallback vs Retry

| Concept | Trigger | Scope | Automatic |
|---------|---------|-------|-----------|
| **Retry** | Request failure (timeout, 5xx) | Same model, different backend | Yes (max_retries) |
| **Fallback** | Model completely unavailable | Different model | Yes (if configured) |

**Example**:
1. Request for "llama3:70b"
2. Backend A has "llama3:70b", request fails → **Retry** on Backend B
3. Backend B also fails, no more backends → **Fallback** to "qwen2:72b"

---

## Edge Cases

| Condition | Behavior |
|-----------|----------|
| No fallback configured | Return ModelNotFound error |
| Fallback chain empty | Treat as no fallback |
| Fallback model also has fallback | NOT followed (single-level) |
| All fallbacks unhealthy | Return FallbackChainExhausted |
| Circular fallback (a→b→a) | Not possible (single-level) |

---

## Response Handling

### Current Implementation
- Response model field shows the **requested** model (not fallback)
- Transparent to client

### Future Enhancement (Not Implemented)
```
X-Nexus-Fallback-Model: qwen2:72b
X-Nexus-Original-Model: llama3:70b
```

This header feature is documented in the acceptance criteria but not yet implemented. It would be added in a future enhancement.

---

## Testing Strategy

### Unit Tests
1. Fallback to first available
2. Skip unavailable, use second
3. All fallbacks exhausted
4. No fallback configured
5. Empty fallback chain

### Integration Tests
1. End-to-end fallback routing
2. Combined alias + fallback
3. Fallback with capability filtering

### Test Coverage
- Implemented in `src/routing/mod.rs` → `alias_and_fallback_tests` module
- Tests: `uses_fallback_when_primary_unavailable`, `fallback_chain_exhausted`

---

## Acceptance Criteria Summary

- [x] AC-01: Fallback chains configurable in `[routing.fallbacks]`
- [x] AC-02: Tries each fallback in order
- [x] AC-03: Logs fallback usage at WARN level
- [x] AC-04: Returns 503 if all fallbacks exhausted (FallbackChainExhausted error)
- [x] AC-05: Response model field shows requested model
- [ ] AC-06: X-Nexus-Fallback-Model header (future enhancement)

---

## Implementation Status

**Implemented in F06 (Intelligent Router)**:
- `src/routing/mod.rs`: `find_candidates_with_fallback()` function
- `src/routing/error.rs`: `FallbackChainExhausted` error variant
- `src/config/routing.rs`: `fallbacks` field parsing

**Files**:
| File | Function |
|------|----------|
| `src/routing/mod.rs` | `find_candidates_with_fallback()`, `with_aliases_and_fallbacks()` |
| `src/routing/error.rs` | `RoutingError::FallbackChainExhausted` |
| `src/config/routing.rs` | `RoutingConfig.fallbacks` |

---

## References

- [F06: Intelligent Router](../006-intelligent-router/spec.md)
- [F07: Model Aliases](../007-model-aliases/spec.md)
- [Configuration Guide](../../nexus.example.toml)
