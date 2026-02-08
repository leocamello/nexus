# F07: Model Aliases - Implementation Tasks

**Feature**: Model Aliases  
**Plan**: [plan.md](./plan.md)  
**Status**: 🔄 In Progress

---

## Task Overview

| Task | Description | Status | Priority |
|------|-------------|--------|----------|
| T01 | Alias chaining implementation | ⬜ | P0 |
| T02 | Circular alias detection | ⬜ | P0 |
| T03 | DEBUG logging | ⬜ | P1 |
| T04 | Unit tests for chaining | ⬜ | P0 |
| T05 | Unit tests for circular detection | ⬜ | P0 |
| T06 | Integration tests | ⬜ | P1 |

---

## T01: Alias Chaining Implementation ⬜

**Status**: Not Started  
**File**: `src/routing/mod.rs`

### Tests to Write First (TDD Red Phase)
```rust
#[test]
fn alias_chain_two_levels() {
    // Given aliases: "gpt-4" → "llama-large", "llama-large" → "llama3:70b"
    // When resolving "gpt-4"
    // Then returns "llama3:70b"
}

#[test]
fn alias_chain_three_levels() {
    // Given aliases: "a" → "b", "b" → "c", "c" → "d"
    // When resolving "a"
    // Then returns "d"
}

#[test]
fn alias_chain_stops_at_max_depth() {
    // Given aliases: "a" → "b", "b" → "c", "c" → "d", "d" → "e"
    // When resolving "a" (4 levels)
    // Then returns "d" (stops at 3)
}
```

### Verify Tests Fail First
1. Write tests above
2. Run `cargo test alias_chain` - must see FAILURES
3. Only then proceed to implementation

### Implementation (TDD Green Phase)
After tests fail, implement:
```rust
fn resolve_alias(&self, model: &str) -> String {
    let mut current = model.to_string();
    let mut depth = 0;
    const MAX_DEPTH: usize = 3;
    
    while depth < MAX_DEPTH {
        match self.aliases.get(&current) {
            Some(target) => {
                current = target.clone();
                depth += 1;
            }
            None => break,
        }
    }
    
    current
}
```

### Acceptance Criteria
- [ ] Resolves single-level aliases (existing behavior)
- [ ] Resolves 2-level chains
- [ ] Resolves 3-level chains
- [ ] Stops at max 3 levels

---

## T02: Circular Alias Detection ⬜

**Status**: Not Started  
**Files**: `src/config/routing.rs`, `src/routing/error.rs`

### Tests to Write First (TDD Red Phase)
```rust
#[test]
fn validates_circular_alias_direct() {
    // Given aliases: "a" → "a"
    // When validating
    // Then returns CircularAlias error
}

#[test]
fn validates_circular_alias_indirect() {
    // Given aliases: "a" → "b", "b" → "a"
    // When validating
    // Then returns CircularAlias error
}

#[test]
fn validates_circular_alias_three_way() {
    // Given aliases: "a" → "b", "b" → "c", "c" → "a"
    // When validating
    // Then returns CircularAlias error
}

#[test]
fn validates_non_circular_aliases() {
    // Given aliases: "a" → "b", "c" → "d"
    // When validating
    // Then returns Ok
}
```

### Verify Tests Fail First
1. Write tests above
2. Run `cargo test validates_circular` - must see FAILURES
3. Only then proceed to implementation

### Implementation (TDD Green Phase)
After tests fail, implement:
```rust
// In src/config/routing.rs
pub fn validate_aliases(aliases: &HashMap<String, String>) -> Result<(), ConfigError> {
    for start in aliases.keys() {
        let mut current = start;
        let mut visited = HashSet::new();
        visited.insert(start);
        
        while let Some(target) = aliases.get(current) {
            if visited.contains(target) {
                return Err(ConfigError::CircularAlias {
                    start: start.clone(),
                    cycle: target.clone(),
                });
            }
            visited.insert(target);
            current = target;
        }
    }
    Ok(())
}
```

### Error Type (in src/routing/error.rs or src/config/mod.rs)
```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Circular alias detected: '{start}' eventually points back to '{cycle}'")]
    CircularAlias { start: String, cycle: String },
}
```

### Acceptance Criteria
- [ ] Detects self-referential aliases (a→a)
- [ ] Detects 2-way circular aliases (a→b→a)
- [ ] Detects multi-way circular aliases
- [ ] Returns Ok for valid aliases
- [ ] Called during config load

---

## T03: DEBUG Logging ⬜

**Status**: Not Started  
**File**: `src/routing/mod.rs`

### Tests to Write First (TDD Red Phase)
```rust
#[test]
fn logs_alias_resolution_at_debug() {
    // Use tracing-test or capture logs
    // Given alias "gpt-4" → "llama3:70b"
    // When resolving "gpt-4"
    // Then DEBUG log emitted with from/to fields
}
```

### Verify Tests Fail First
1. Write test above (may need tracing-test crate)
2. Run test - must see FAILURE
3. Only then proceed to implementation

### Implementation (TDD Green Phase)
After test fails, implement:
```rust
fn resolve_alias(&self, model: &str) -> String {
    let mut current = model.to_string();
    let mut depth = 0;
    const MAX_DEPTH: usize = 3;
    
    while depth < MAX_DEPTH {
        match self.aliases.get(&current) {
            Some(target) => {
                tracing::debug!(
                    from = %current,
                    to = %target,
                    depth = depth + 1,
                    "Resolved alias"
                );
                current = target.clone();
                depth += 1;
            }
            None => break,
        }
    }
    
    if depth > 0 {
        tracing::debug!(
            original = %model,
            resolved = %current,
            chain_depth = depth,
            "Alias resolution complete"
        );
    }
    
    current
}
```

### Acceptance Criteria
- [ ] Each alias hop logged at DEBUG
- [ ] Final resolution logged with chain depth
- [ ] No logging when no alias used

---

## T04: Unit Tests for Chaining ⬜

**Status**: Not Started  
**File**: `src/routing/mod.rs`

### Tests to Add
- [ ] `alias_chain_two_levels`
- [ ] `alias_chain_three_levels`
- [ ] `alias_chain_stops_at_max_depth`
- [ ] `alias_preserves_existing_single_level_behavior`

### Acceptance Criteria
- [ ] All chaining tests pass
- [ ] Existing alias tests still pass
- [ ] Edge cases covered

---

## T05: Unit Tests for Circular Detection ⬜

**Status**: Not Started  
**File**: `src/config/routing.rs`

### Tests to Add
- [ ] `validates_circular_alias_direct`
- [ ] `validates_circular_alias_indirect`
- [ ] `validates_circular_alias_three_way`
- [ ] `validates_non_circular_aliases`
- [ ] `validates_empty_aliases`

### Acceptance Criteria
- [ ] All circular detection tests pass
- [ ] Error messages are clear
- [ ] Validation integrated with config load

---

## T06: Integration Tests ⬜

**Status**: Not Started  
**File**: `tests/routing_integration.rs`

### Tests to Add
- [ ] `routing_with_chained_aliases` - Full E2E with 2-level chain
- [ ] `routing_rejects_circular_config` - Config with circular fails

### Acceptance Criteria
- [ ] Integration tests pass
- [ ] Full request flow works with chained aliases

---

## Summary

### Remaining Work
- 3 implementation tasks (T01-T03)
- 3 testing tasks (T04-T06)

### Test Commands
```bash
# Run all routing tests
cargo test routing::

# Run alias-specific tests
cargo test alias

# Run config validation tests
cargo test config::routing
```
