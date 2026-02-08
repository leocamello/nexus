# F07: Model Aliases - Technical Plan

**Feature**: Model Aliases  
**Spec**: [spec.md](./spec.md)  
**Created**: 2026-02-08  
**Status**: 🔄 Partially Implemented

---

## Constitution Check

### Simplicity Gate ✅
- [x] Using ≤3 main modules? **Yes**: Part of routing module only
- [x] No speculative features? **Yes**: Chaining requested by user
- [x] No premature optimization? **Yes**: Simple loop with max depth
- [x] Simplest approach? **Yes**: Iterative resolution with visited set

### Anti-Abstraction Gate ✅
- [x] No wrapper layers? **Yes**: Direct HashMap in Router struct
- [x] Single representation? **Yes**: `HashMap<String, String>`
- [x] No framework-on-framework? **Yes**: Plain Rust
- [x] Abstractions justified? **Yes**: No unnecessary abstractions

### Integration-First Gate ✅
- [x] API contracts defined? **Yes**: Transparent to clients
- [x] Integration tests planned? **Yes**: Alias routing tests
- [x] End-to-end testable? **Yes**: Can test full alias flow

### Performance Gate ✅
- [x] Routing decision < 1ms? **Yes**: Max 3 HashMap lookups
- [x] Total overhead < 5ms? **Yes**: O(1) per level, max 3 levels
- [x] Memory baseline < 50MB? **Yes**: ~100 bytes per alias

---

## Gap Analysis

| Component | Status | Work Needed |
|-----------|--------|-------------|
| Basic alias resolution | ✅ | None |
| Config parsing | ✅ | Add validation |
| Alias chaining | ❌ | Implement loop with max depth |
| Circular detection | ❌ | Add config validation |
| DEBUG logging | ❌ | Add tracing calls |
| Unit tests | 🔄 | Add chaining + circular tests |

---

## Technical Approach

### Changes to `src/routing/mod.rs`

1. Update `resolve_alias()` to support chaining:
   - Add visited set to track chain
   - Loop up to 3 times
   - Add DEBUG logging at each step
   - Return Result to handle errors

2. Add tracing for alias resolution

### Changes to `src/config/routing.rs`

1. Add `validate_aliases()` function:
   - Check for circular references
   - Return ConfigError on circular detection

### Changes to `src/config/mod.rs`

1. Call `validate_aliases()` during config load
2. Return error if validation fails

### Changes to `src/routing/error.rs`

1. Add `CircularAlias` error variant (runtime safety net)

---

## Test Plan

### Unit Tests to Add
| Test | Description |
|------|-------------|
| `alias_chain_two_levels` | a→b→c resolves to c |
| `alias_chain_three_levels` | a→b→c→d resolves to d |
| `alias_chain_exceeds_max` | a→b→c→d→e stops at d |
| `circular_alias_config_error` | a→b→a fails at config |
| `self_referential_alias_error` | a→a fails at config |
| `alias_resolution_logged` | DEBUG logs emitted |

### Integration Tests to Add
| Test | Description |
|------|-------------|
| `routing_with_chained_aliases` | Full E2E with 2-level chain |

---

## Estimated Effort

| Task | Estimate |
|------|----------|
| T01: Update resolve_alias() | 30 min |
| T02: Add circular validation | 30 min |
| T03: Add DEBUG logging | 15 min |
| T04: Unit tests | 45 min |
| T05: Integration tests | 30 min |
| T06: Documentation | 15 min |
| **Total** | ~2.5 hours |

---

## Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| Breaking existing behavior | High | Comprehensive tests before changes |
| Performance regression | Low | Max 3 lookups is bounded |
| Config migration | Low | No config format changes |
