# Data Model: Fallback Chains (F08)

**Date**: 2025-01-10  
**Phase**: Phase 1 - Design & Contracts

This document defines the data entities and their relationships for the Fallback Chains feature.

## Core Entities

### 1. Fallback Map

**Purpose**: A mapping from model names to ordered lists of alternative models, providing automatic failover when the primary model is unavailable.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `fallbacks` | `HashMap<String, Vec<String>>` | Stored on `Router`; keys are model names, values are ordered fallback lists |

**Entry Constraints**:

| Constraint | Rule |
|------------|------|
| Key uniqueness | HashMap enforces; one fallback chain per model |
| Ordering | Vec preserves insertion order; first fallback = highest priority |
| Chain depth | Single-level only (a fallback's own fallbacks are NOT followed) |
| Empty chains | Treated as no fallback configured |
| Self-reference | Not explicitly prevented but harmless (model already tried as primary) |

**Responsibilities**:
- Define ordered fallback preferences for models
- Enable graceful degradation when primary models are unavailable
- Provide clear chain information for error messages

**Lifecycle**: Loaded from `[routing.fallbacks]` config section at startup. Immutable after `Router` construction. Requires server restart to modify.

**Thread Safety**: Immutable `HashMap` on `Router`; safe for concurrent reads without synchronization.

---

### 2. Fallback Resolution Logic

**Purpose**: Iterates through fallback candidates when the primary model has no healthy, capable backends.

**Access**:
```rust
fn get_fallbacks(&self, model: &str) -> Vec<String>
```

**Behavior within `select_backend()`**:

| Step | Action | Outcome |
|------|--------|---------|
| 1 | Get candidates for primary model | If found → use them (no fallback) |
| 2 | Look up `fallbacks[model]` | If missing → `ModelNotFound` or `NoHealthyBackend` |
| 3 | For each fallback in order | Call `filter_candidates(fallback, requirements)` |
| 4 | First fallback with candidates | Log WARN, return `RoutingResult` with `fallback_used=true` |
| 5 | All fallbacks exhausted | Return `FallbackChainExhausted` error |

**Filtering**: Each fallback model goes through the same health and capability filters as the primary model. A fallback is skipped if:
- No backend has that model
- All backends with that model are unhealthy
- No backend meets capability requirements (vision, tools, JSON mode, context length)

**Lifecycle**: Executed per-request within `select_backend()`. Fallback iteration is lazy — stops at first successful match.

**Thread Safety**: Reads from immutable `HashMap` and `Registry` (DashMap); no mutation needed.

---

### 3. RoutingResult (Fallback Metadata)

**Purpose**: Extends the routing result with fallback information for downstream processing (headers, logging).

**Fallback-Specific Attributes**:

| Attribute | Type | Value When Fallback Used |
|-----------|------|--------------------------|
| `actual_model` | `String` | The fallback model name that was actually routed to |
| `fallback_used` | `bool` | `true` |
| `route_reason` | `String` | `"fallback:{primary_model}:{strategy_reason}"` |

**Consumed by**:
- `src/api/completions.rs`: Adds `X-Nexus-Fallback-Model` header when `fallback_used == true`
- Response body: Model field shows the **requested** model (transparent to client)

---

### 4. RoutingError::FallbackChainExhausted

**Purpose**: Error indicating all models in a fallback chain were attempted but none had available backends.

**Attributes**:

| Attribute | Type | Constraints |
|-----------|------|-------------|
| `chain` | `Vec<String>` | Ordered list: `[primary, fallback1, fallback2, ...]` |

**Error Message**: `"All backends in fallback chain unavailable: [\"primary\", \"fallback1\", \"fallback2\"]"`

**HTTP Response**: 503 Service Unavailable

**Construction**: Built in `select_backend()` by prepending the primary model to the fallback list:
```rust
let mut chain = vec![model.clone()];
chain.extend(fallbacks);
Err(RoutingError::FallbackChainExhausted { chain })
```

**Lifecycle**: Created on routing failure, propagated to API handler for error response.

---

### 5. X-Nexus-Fallback-Model Header

**Purpose**: HTTP response header indicating that a fallback model was used instead of the requested model.

**Attributes**:

| Attribute | Value |
|-----------|-------|
| Header name | `X-Nexus-Fallback-Model` |
| Header value | The actual model name used (fallback model) |
| Present when | `RoutingResult.fallback_used == true` |
| Absent when | Primary model used, or alias-only resolution |

**Behavior**:

| Scenario | Header | Response Body `model` |
|----------|--------|----------------------|
| Primary model used | Not present | Requested model |
| Fallback used | `X-Nexus-Fallback-Model: qwen2:72b` | Requested model |
| Alias resolved, no fallback | Not present | Requested model |
| Alias + fallback | `X-Nexus-Fallback-Model: mistral:7b` | Requested model |

**Implementation**: Added in `src/api/completions.rs` after routing decision, not in the Router itself. The API handler checks `routing_result.fallback_used` and appends the header.

---

## Entity Relationships

```
┌──────────────────────────────┐
│     Config File (TOML)       │
│                              │
│  [routing.fallbacks]         │
│  "llama3:70b" = [            │
│    "qwen2:72b",              │
│    "mistral:7b"              │
│  ]                           │
└──────────────────────────────┘
            │
            │ parsed into
            ▼
┌──────────────────────────────┐
│  Router.fallbacks            │
│  HashMap<String, Vec<String>>│
│                              │
│  ┌───────────┬─────────────┐ │
│  │ Key       │ Value       │ │
│  ├───────────┼─────────────┤ │
│  │llama3:70b │[qwen2:72b,  │ │
│  │           │ mistral:7b] │ │
│  └───────────┴─────────────┘ │
└──────────────────────────────┘
            │
            │ used by select_backend()
            ▼
┌──────────────────────────────────────────────────┐
│                select_backend()                   │
│                                                  │
│  1. resolve_alias("gpt-4") → "llama3:70b"       │
│  2. filter_candidates("llama3:70b") → empty      │
│  3. get_fallbacks("llama3:70b")                  │
│     → ["qwen2:72b", "mistral:7b"]               │
│  4. Try "qwen2:72b":                             │
│     filter_candidates → found!                   │
│     → Log WARN, return RoutingResult             │
│       (fallback_used=true,                       │
│        actual_model="qwen2:72b")                 │
└──────────────────────────────────────────────────┘
            │
            │ RoutingResult
            ▼
┌──────────────────────────────────────────────────┐
│            API Handler (completions.rs)           │
│                                                  │
│  if routing_result.fallback_used:                │
│    response.header("X-Nexus-Fallback-Model",    │
│                     routing_result.actual_model)  │
│  response.body.model = requested_model           │
│    (transparent to client)                       │
└──────────────────────────────────────────────────┘
```

---

## State Transitions

### Fallback Chain Traversal

```
Primary model ("llama3:70b")
    ↓
filter_candidates("llama3:70b")
    ↓
┌──────────────────────┐
│ Candidates found?    │
│                      │
│ Yes → Route to best  │──▶ RoutingResult (fallback_used=false)
│       candidate      │
│                      │
│ No  → Check fallback │
│       chain          │
└──────────────────────┘
    ↓
get_fallbacks("llama3:70b") → ["qwen2:72b", "mistral:7b"]
    ↓
┌──────────────────────┐
│ Try "qwen2:72b"      │
│ filter_candidates()  │
│                      │
│ Found → WARN log     │──▶ RoutingResult (fallback_used=true,
│         Route to it  │     actual_model="qwen2:72b")
│                      │
│ Empty → Try next     │
└──────────────────────┘
    ↓
┌──────────────────────┐
│ Try "mistral:7b"     │
│ filter_candidates()  │
│                      │
│ Found → WARN log     │──▶ RoutingResult (fallback_used=true,
│         Route to it  │     actual_model="mistral:7b")
│                      │
│ Empty → Exhausted    │──▶ FallbackChainExhausted
└──────────────────────┘     chain=["llama3:70b", "qwen2:72b", "mistral:7b"]
```

### Error Decision Tree

```
select_backend() fails:
    ↓
┌──────────────────────────────┐
│ Fallback chain configured?   │
│                              │
│ Yes, all exhausted           │──▶ FallbackChainExhausted (503)
│                              │    chain includes primary + all fallbacks
│ No chain configured          │
│   ↓                          │
│   Model exists in registry?  │
│   (any backend, any health)  │
│                              │
│   Yes → NoHealthyBackend     │──▶ 503, model exists but all backends down
│   No  → ModelNotFound        │──▶ 404, model never registered
└──────────────────────────────┘
```

---

## Validation & Constraints

### Single-Level Fallback Design

**Rule**: Fallbacks do not follow a fallback model's own fallback chain.

**Rationale**:
1. **Predictability**: Users know exactly which models may serve their request
2. **Performance**: O(n) where n = chain length, no graph traversal
3. **Control**: All acceptable alternatives explicitly listed

**Example**:
```toml
[routing.fallbacks]
"primary" = ["fallback1", "fallback2"]
"fallback1" = ["alternate"]  # NOT followed when routing "primary"
```
Request for `"primary"`: tries `"primary"` → `"fallback1"` → `"fallback2"` → error.
`"alternate"` is never considered.

### Fallback + Capability Filtering

**Rule**: Each fallback model passes through the same capability filters as the primary model. A fallback is skipped (not an error) if it doesn't meet capability requirements.

**Implementation**: `filter_candidates(fallback_model, requirements)` is called for each fallback, applying the same health, vision, tools, JSON mode, and context length checks.

### Fallback + Alias Interaction

**Rule**: Alias resolution happens **before** fallback lookup. Fallback chains are keyed by the **resolved** model name, not the alias.

**Example**:
- Alias: `"gpt-4"` → `"llama3:70b"`
- Fallback: `"llama3:70b"` → `["mistral:7b"]`
- Request for `"gpt-4"`: resolves to `"llama3:70b"`, then fallback chain `["mistral:7b"]` applies.

### Logging

**Rule**: Fallback usage logged at WARN level with structured fields.

**Implementation**:
```rust
tracing::warn!(
    requested_model = %model,
    fallback_model = %fallback_model,
    backend = %selected.name,
    "Using fallback model"
);
```

---

## Thread Safety

**Requirement**: Fallback resolution must be lock-free and safe for concurrent access.

**Implementation**:
- `fallbacks` is an immutable `HashMap<String, Vec<String>>` on the `Router` struct
- `get_fallbacks()` performs a single `HashMap::get()` call followed by `cloned()`
- Registry access uses `DashMap` (concurrent HashMap) for candidate lookup
- No `Mutex`, `RwLock`, or atomic operations in the fallback path
- Multiple threads can traverse fallback chains concurrently

---

## Performance Characteristics

| Operation | Target Latency | Implementation |
|-----------|----------------|----------------|
| Fallback chain lookup | < 50ns | `HashMap::get()` + `Vec::clone()` |
| Per-fallback candidate check | < 15µs | Registry DashMap lookup + health/capability filter |
| Full chain traversal (3 fallbacks) | < 50µs | 3 × candidate check (lazy, stops at first match) |
| Chain exhaustion (worst case) | < 100µs | All fallbacks checked + error construction |
| RoutingResult construction | < 500ns | Backend snapshot + string allocation |
| Header construction | < 100ns | String copy for header value |

**Total Fallback Overhead Per Request**: < 100µs (well within 1ms routing budget).

**Memory**:
- Per fallback entry: ~200 bytes (String key + Vec<String> with ~3 entries)
- FallbackChainExhausted error: ~300 bytes (Vec<String> with chain)
- X-Nexus-Fallback-Model header: ~50 bytes

---

## Testing Strategy

### Unit Tests

1. **Fallback to first available**: Primary unavailable, first fallback has healthy backends
2. **Skip unavailable, use second**: First fallback also unavailable, second fallback works
3. **All fallbacks exhausted**: Return `FallbackChainExhausted` with full chain in error
4. **No fallback configured**: Return `ModelNotFound` or `NoHealthyBackend` (no chain error)
5. **Empty fallback chain**: Treated as no fallback configured
6. **Fallback with capability filtering**: Vision-requiring request skips non-vision fallback models

### Integration Tests

1. **End-to-end fallback routing**: Request through API, primary down, fallback used
2. **Combined alias + fallback**: Alias resolves, resolved model has fallback chain
3. **X-Nexus-Fallback-Model header**: Verify header present when fallback used, absent otherwise
4. **Response model transparency**: Response body shows requested model, not fallback

### Test Coverage

- Unit tests in `src/routing/mod.rs` → `alias_and_fallback_tests` module
- Tests: `uses_fallback_when_primary_unavailable`, `fallback_chain_exhausted`, `fallback_skips_unhealthy`
- Integration tests in `tests/fallback_header_integration.rs`

---

## Fallback vs Retry

| Concept | Trigger | Scope | Level |
|---------|---------|-------|-------|
| **Retry** | Request failure (timeout, 5xx) | Same model, different backend | Within model |
| **Fallback** | Model completely unavailable | Different model entirely | Across models |

**Ordering**: Retries are attempted within a model before fallback to a different model. Fallback is a last resort when no backend can serve the requested model at all.

---

## Future Extensions

### Not in Scope

1. **Multi-level fallback chaining**: A fallback's own fallbacks are not followed
2. **Dynamic fallback discovery**: Fallbacks are statically configured
3. **Quality-aware fallback**: No automatic quality matching between primary and fallback
4. **Cross-capability fallback**: Fallbacks are subject to same capability filters
5. **Fallback metrics**: Tracked by F09 (Request Metrics), not by the fallback system itself
