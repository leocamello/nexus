# F07: Model Aliases

**Status**: 🔄 Partially Implemented  
**Priority**: P1  
**Branch**: feature/f07-model-aliases  
**Dependencies**: F06 (Intelligent Router)  
**Implementation**: `src/routing/mod.rs`

## Implementation Gap Analysis

| Requirement | Status | Notes |
|-------------|--------|-------|
| Basic alias resolution | ✅ | Single-level working |
| Alias chaining (max 3 levels) | ❌ | Only single-level |
| Circular alias detection | ❌ | Not implemented |
| DEBUG logging | ❌ | Not implemented |
| Direct match priority | ✅ | Working |
| Config parsing | ✅ | Working |

---

## Overview

### What It Is
A model aliasing system that maps common model names (like "gpt-4") to available local models, enabling drop-in compatibility with tools configured for OpenAI or other providers.

### Goals
1. Allow clients to use familiar model names (gpt-4, claude-3-opus) without changes
2. Provide transparent aliasing that's invisible to clients
3. Support configuration-based alias mapping
4. Enable gradual migration from cloud to local models

### Non-Goals
1. Dynamic alias management via API (use config file)
2. Per-client aliases (all clients share the same aliases)
3. Wildcard or pattern-based aliases
4. Alias chaining beyond 3 levels (practical limit for complexity)

---

## User Stories

### US-01: OpenAI Client Compatibility
**As a** developer using an OpenAI client library  
**I want** to request "gpt-4" and have it routed to a local model  
**So that** I don't need to change my existing code

**Priority**: P0 (Core functionality)

**Acceptance Scenarios**:
- **Given** alias "gpt-4" → "llama3:70b" is configured
- **And** a backend has model "llama3:70b"
- **When** I request model "gpt-4"
- **Then** the request is routed to the backend with "llama3:70b"
- **And** the response shows model "gpt-4" (not "llama3:70b")

### US-02: Direct Match Priority
**As a** system administrator  
**I want** direct model matches to take priority over aliases  
**So that** I can have both the alias and the real model available

**Priority**: P0 (Core functionality)

**Acceptance Scenarios**:
- **Given** alias "gpt-4" → "llama3:70b" is configured
- **And** a backend has actual model "gpt-4"
- **When** I request model "gpt-4"
- **Then** the request is routed to the backend with actual "gpt-4"
- **And** the alias is not used

### US-03: Alias with Fallback
**As a** developer  
**I want** aliases to work with fallback chains  
**So that** I get resilience even when using aliased models

**Priority**: P1 (Enhanced functionality)

**Acceptance Scenarios**:
- **Given** alias "gpt-4" → "llama3:70b" is configured
- **And** fallback chain "llama3:70b" → ["mistral:7b"]
- **And** no backend has "llama3:70b" available
- **When** I request model "gpt-4"
- **Then** the request is routed to a backend with "mistral:7b"
- **And** the alias resolution is logged at DEBUG level

### US-04: Alias Chaining
**As a** system administrator  
**I want** aliases to chain through intermediate aliases  
**So that** I can create layered naming schemes

**Priority**: P1 (Enhanced functionality)

**Acceptance Scenarios**:
- **Given** alias "gpt-4" → "llama-large" is configured
- **And** alias "llama-large" → "llama3:70b" is configured
- **When** I request model "gpt-4"
- **Then** the alias chain is resolved: "gpt-4" → "llama-large" → "llama3:70b"
- **And** the request is routed to "llama3:70b"
- **And** chain is limited to max 3 levels

### US-05: Circular Alias Detection
**As a** system administrator  
**I want** circular aliases to be detected at config load  
**So that** I don't have infinite loops at runtime

**Priority**: P0 (Safety)

**Acceptance Scenarios**:
- **Given** alias "a" → "b" is configured
- **And** alias "b" → "a" is configured
- **When** the config is loaded
- **Then** an error is returned indicating circular alias detected
- **And** the server does not start

---

## Technical Design

### Alias Resolution (with Chaining)

```rust
/// Resolve model aliases with chaining support (max 3 levels)
fn resolve_alias(&self, model: &str) -> Result<String, RoutingError> {
    let mut current = model.to_string();
    let mut visited = HashSet::new();
    let max_depth = 3;
    
    for _ in 0..max_depth {
        if visited.contains(&current) {
            // Should never happen if validated at config load
            return Err(RoutingError::CircularAlias { model: model.to_string() });
        }
        visited.insert(current.clone());
        
        match self.aliases.get(&current) {
            Some(target) => {
                tracing::debug!(
                    from = %current,
                    to = %target,
                    "Resolved alias"
                );
                current = target.clone();
            }
            None => break,
        }
    }
    
    Ok(current)
}
```

### Circular Detection at Config Load

```rust
/// Validate aliases for circular references
pub fn validate_aliases(aliases: &HashMap<String, String>) -> Result<(), ConfigError> {
    for start in aliases.keys() {
        let mut current = start.clone();
        let mut visited = HashSet::new();
        
        while let Some(target) = aliases.get(&current) {
            if visited.contains(target) {
                return Err(ConfigError::CircularAlias {
                    chain: visited.into_iter().collect(),
                });
            }
            visited.insert(current.clone());
            current = target.clone();
        }
    }
    Ok(())
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
│ 1. Resolve alias chain (max 3 levels)                        │
│    "gpt-4" → "llama-large" → "llama3:70b"                   │
│    Each step logged at DEBUG level                           │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 2. Find backends for resolved model                          │
│    registry.get_backends_for_model("llama3:70b")            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 3. If no backends, try fallback chain                        │
│    (See F08: Fallback Chains)                                │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 4. Select best backend using routing strategy                │
└─────────────────────────────────────────────────────────────┘
```

### Data Structure

```rust
pub struct Router {
    registry: Arc<Registry>,
    strategy: RoutingStrategy,
    weights: ScoringWeights,
    
    /// Model aliases (alias → target)
    /// Example: {"gpt-4" → "llama3:70b", "claude-3" → "mistral:7b"}
    aliases: HashMap<String, String>,
    
    fallbacks: HashMap<String, Vec<String>>,
    round_robin_counter: AtomicU64,
}
```

### Design Decision: Alias Chaining with Max 3 Levels

**Decision**: Aliases support chaining up to 3 levels.

**Rationale**:
1. **Flexibility**: Enables layered naming (gpt-4 → llama-large → llama3:70b)
2. **Safety**: Max depth prevents runaway chains
3. **Config validation**: Circular references detected at startup

**Validation**:
- Circular aliases rejected at config load (server won't start)
- Runtime circular detection as safety net
- Chain depth tracked and limited

---

## Configuration

```toml
[routing.aliases]
# OpenAI compatibility
"gpt-4" = "llama3:70b"
"gpt-4-turbo" = "llama3:70b"
"gpt-4o" = "llama3:70b"
"gpt-3.5-turbo" = "mistral:7b"

# Anthropic compatibility
"claude-3-opus" = "qwen2:72b"
"claude-3-sonnet" = "llama3:70b"
"claude-3-haiku" = "mistral:7b"

# Custom aliases
"fast" = "mistral:7b"
"smart" = "llama3:70b"
"vision" = "llava:34b"
```

**Parsing**:
```rust
pub struct RoutingConfig {
    pub strategy: RoutingStrategy,
    pub max_retries: u32,
    pub weights: ScoringWeights,
    pub aliases: HashMap<String, String>,
    pub fallbacks: HashMap<String, Vec<String>>,
}
```

---

## Logging

Alias resolution is logged at DEBUG level:

```
DEBUG routing: Resolved alias from="gpt-4" to="llama-large"
DEBUG routing: Resolved alias from="llama-large" to="llama3:70b"
DEBUG routing: Final model after alias resolution model="llama3:70b" original="gpt-4" chain_depth=2
```

**Log Fields**:
- `from`: Source model name
- `to`: Target model name  
- `original`: What the client requested
- `chain_depth`: Number of alias hops

---

## Edge Cases

| Condition | Behavior |
|-----------|----------|
| No alias configured | Use model name directly |
| Alias target doesn't exist | Try fallback chain, then error |
| Self-referential alias (a→a) | Detected as circular at config load |
| Circular chain (a→b→a) | Rejected at config load |
| Chain exceeds 3 levels | Stop at level 3, use current value |
| Empty alias target | Ignore the alias entry |
| Alias with spaces | Trim and normalize |

---

## Non-Functional Requirements

### Performance
| Metric | Target |
|--------|--------|
| Alias lookup | O(1) HashMap lookup |
| Memory per alias | ~100 bytes |
| Max aliases supported | 10,000+ |

### Configuration
- Aliases loaded at startup from config file
- Requires restart to update aliases
- Environment variable override not supported (use config file)

---

## Testing Strategy

### Unit Tests
1. Basic alias resolution (single level)
2. Alias chaining (2 levels)
3. Alias chaining (3 levels - max)
4. Chain exceeds max depth (stops at 3)
5. No alias (passthrough)
6. Circular detection at config load
7. Self-referential alias detection
8. Case sensitivity

### Integration Tests
1. End-to-end routing with aliases
2. Alias + fallback chain combination
3. Direct match vs alias priority

### Test Coverage
- Implemented in `src/routing/mod.rs` → `alias_and_fallback_tests` module
- Config validation tests in `src/config/` module

---

## Acceptance Criteria Summary

- [x] AC-01: Aliases configured in `[routing.aliases]` section
- [x] AC-02: Transparent to client (response shows requested model)
- [x] AC-03: Alias resolution logged at DEBUG level
- [x] AC-04: Circular alias detection at config load
- [x] AC-05: Max 3 levels of chaining
- [x] AC-06: Direct matches preferred over aliases
- [x] AC-07: Works with fallback chains

---

## Implementation Status

**Partially Implemented in F06 (Intelligent Router)**:
- `src/routing/mod.rs`: Basic `resolve_alias()` function (single-level only)
- `src/config/routing.rs`: `aliases` field parsing

**Still Needed**:
- Alias chaining (max 3 levels)
- Circular alias detection at config load
- DEBUG level logging of alias resolution

**Files to Modify**:
| File | Changes Needed |
|------|----------------|
| `src/routing/mod.rs` | Update `resolve_alias()` for chaining + logging |
| `src/config/routing.rs` | Add `validate_aliases()` function |
| `src/config/mod.rs` | Call validation during config load |
| `src/routing/error.rs` | Add `CircularAlias` error variant |

---

## References

- [F06: Intelligent Router](../006-intelligent-router/spec.md)
- [F08: Fallback Chains](../008-fallback-chains/spec.md)
- [Configuration Guide](../../nexus.example.toml)
