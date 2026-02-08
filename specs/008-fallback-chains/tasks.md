# F08: Fallback Chains - Implementation Tasks

**Feature**: Fallback Chains  
**Plan**: [plan.md](./plan.md)  
**Status**: ✅ All tasks complete (implemented with F06)

---

## Task Overview

| Task | Description | Status | Implemented In |
|------|-------------|--------|----------------|
| T01 | Data structure | ✅ | F06 |
| T02 | Config parsing | ✅ | F06 |
| T03 | Resolution logic | ✅ | F06 |
| T04 | Error types | ✅ | F06 |
| T05 | Unit tests | ✅ | F06 |
| T06 | Integration tests | ✅ | F06 |

---

## T01: Fallback Data Structure ✅

**Status**: Complete (F06)  
**File**: `src/routing/mod.rs`

### Acceptance Criteria
- [x] HashMap<String, Vec<String>> field in Router
- [x] Constructor accepts fallbacks parameter
- [x] Default to empty HashMap

### Implementation
```rust
pub struct Router {
    fallbacks: HashMap<String, Vec<String>>,
    // ...
}

impl Router {
    pub fn with_aliases_and_fallbacks(
        self,
        aliases: HashMap<String, String>,
        fallbacks: HashMap<String, Vec<String>>,
    ) -> Self
}
```

---

## T02: Configuration Parsing ✅

**Status**: Complete (F06)  
**File**: `src/config/routing.rs`

### Acceptance Criteria
- [x] `[routing.fallbacks]` section in TOML
- [x] HashMap<String, Vec<String>> serde parsing
- [x] Default to empty if not specified

### Implementation
```toml
[routing.fallbacks]
"llama3:70b" = ["qwen2:72b", "mistral:7b"]
"gpt-4" = ["llama3:70b", "qwen2:72b"]
```

---

## T03: Resolution Logic ✅

**Status**: Complete (F06)  
**File**: `src/routing/mod.rs`

### Acceptance Criteria
- [x] Linear iteration through fallback list
- [x] Returns first available model
- [x] Logs fallback usage at WARN level
- [x] Single-level only (no recursion)

### Implementation
```rust
fn find_candidates_with_fallback(&self, model: &str) 
    -> Result<(Vec<Arc<Backend>>, bool), RoutingError> 
{
    // Try primary, then iterate fallbacks
    // ...
}
```

---

## T04: Error Types ✅

**Status**: Complete (F06)  
**File**: `src/routing/error.rs`

### Acceptance Criteria
- [x] FallbackChainExhausted error variant
- [x] Includes original model
- [x] Includes list of tried models
- [x] Clear error message

### Implementation
```rust
#[derive(Debug, thiserror::Error)]
pub enum RoutingError {
    #[error("Fallback chain exhausted for model '{model}'. Tried: {tried:?}")]
    FallbackChainExhausted {
        model: String,
        tried: Vec<String>,
    },
}
```

---

## T05: Unit Tests ✅

**Status**: Complete (F06)  
**File**: `src/routing/mod.rs`

### Acceptance Criteria
- [x] Fallback to first available test
- [x] Skip unavailable, use second test
- [x] All fallbacks exhausted test
- [x] No fallback configured test

### Test Coverage
- `alias_and_fallback_tests` module
- `uses_fallback_when_primary_unavailable`
- `fallback_chain_exhausted`

---

## T06: Integration Tests ✅

**Status**: Complete (F06)  
**File**: `tests/routing_integration.rs`

### Acceptance Criteria
- [x] End-to-end routing with fallback
- [x] Verify correct backend selection
- [x] Verify WARN logging

### Test
- `test_routing_with_fallbacks`

---

## Summary

All tasks were completed as part of F06 (Intelligent Router). This tasks.md documents what was implemented for traceability.

### Code Locations
| Feature | File | Line |
|---------|------|------|
| Router.fallbacks | `src/routing/mod.rs` | 38 |
| find_candidates_with_fallback() | `src/routing/mod.rs` | 120+ |
| FallbackChainExhausted | `src/routing/error.rs` | 20+ |
| RoutingConfig.fallbacks | `src/config/routing.rs` | 26 |
| Unit tests | `src/routing/mod.rs` | 850+ |
| Integration tests | `tests/routing_integration.rs` | 80+ |

### Test Commands
```bash
# Run fallback-specific tests
cargo test fallback

# Run all routing tests
cargo test routing::
```

---

## Outstanding Work

### X-Nexus-Fallback-Model Header
**Status**: Not implemented  
**Priority**: P2 (Future enhancement)

Would add HTTP response header indicating when fallback was used:
```
X-Nexus-Fallback-Model: qwen2:72b
X-Nexus-Original-Model: llama3:70b
```

This is documented as a future enhancement in the spec.
