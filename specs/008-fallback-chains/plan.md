# F08: Fallback Chains - Technical Plan

**Feature**: Fallback Chains  
**Spec**: [spec.md](./spec.md)  
**Created**: 2026-02-08  
**Status**: 🔄 Partially Implemented

---

## Constitution Check

### Simplicity Gate ✅
- [x] Using ≤3 main modules? **Yes**: routing + api modules
- [x] No speculative features? **Yes**: Only what's in AC
- [x] No premature optimization? **Yes**: Simple header addition
- [x] Simplest approach? **Yes**: Add header in response

### Anti-Abstraction Gate ✅
- [x] No wrapper layers? **Yes**: Direct header manipulation
- [x] Single representation? **Yes**: String header value
- [x] No framework-on-framework? **Yes**: Plain axum
- [x] Abstractions justified? **Yes**: RoutingResult carries metadata

### Integration-First Gate ✅
- [x] API contracts defined? **Yes**: X-Nexus-Fallback-Model header
- [x] Integration tests planned? **Yes**: Header verification tests
- [x] End-to-end testable? **Yes**: Can test full flow with header

### Performance Gate ✅
- [x] Routing decision < 1ms? **Yes**: Unchanged
- [x] Total overhead < 5ms? **Yes**: Header addition is negligible
- [x] Memory baseline < 50MB? **Yes**: One string per response

---

## Gap Analysis

| Component | Status | Work Needed |
|-----------|--------|-------------|
| Fallback chain iteration | ✅ | None |
| FallbackChainExhausted error | ✅ | None |
| WARN logging | ✅ | None |
| Config parsing | ✅ | None |
| X-Nexus-Fallback-Model header | ❌ | Implement |

---

## Technical Approach

### Existing Implementation (F06)
- `HashMap<String, Vec<String>>` for fallback chains
- Linear iteration through fallback list
- `FallbackChainExhausted` error type
- WARN level logging for fallback usage

### New Implementation (F08)

#### 1. Router Returns Fallback Metadata

The router needs to return not just the backend, but also whether a fallback was used:

```rust
/// Result of a routing decision
pub struct RoutingResult {
    /// Selected backend
    pub backend: Arc<Backend>,
    /// The actual model being used (may differ from requested if fallback)
    pub actual_model: String,
    /// Whether a fallback was used
    pub fallback_used: bool,
}
```

#### 2. API Layer Adds Header

In `src/api/chat.rs`, after proxying the response:

```rust
if routing_result.fallback_used {
    response.headers_mut().insert(
        HeaderName::from_static("x-nexus-fallback-model"),
        HeaderValue::from_str(&routing_result.actual_model)?,
    );
}
```

---

## Data Structures

### New: RoutingResult

```rust
/// Result of a successful routing decision
pub struct RoutingResult {
    /// The selected backend
    pub backend: Arc<Backend>,
    /// The actual model name (after alias resolution and fallback)
    pub actual_model: String,
    /// True if a fallback model was used
    pub fallback_used: bool,
}
```

### Existing (unchanged)

```rust
// Router fallbacks field
fallbacks: HashMap<String, Vec<String>>

// Error type
RoutingError::FallbackChainExhausted { model, tried }
```

---

## Implementation Files

| File | Changes | Status |
|------|---------|--------|
| `src/routing/mod.rs` | Return `RoutingResult` instead of `Arc<Backend>` | ⬜ |
| `src/api/chat.rs` | Add X-Nexus-Fallback-Model header | ⬜ |
| `src/api/mod.rs` | May need header constant | ⬜ |
| `tests/routing_integration.rs` | Test header presence | ⬜ |

---

## Test Plan

### Unit Tests to Add
| Test | Description |
|------|-------------|
| `routing_result_includes_fallback_info` | RoutingResult has correct fields |
| `no_fallback_used_flag_is_false` | Primary model used |

### Integration Tests to Add
| Test | Description |
|------|-------------|
| `response_has_fallback_header_when_fallback_used` | Header present |
| `response_no_fallback_header_when_primary_used` | Header absent |

---

## Complexity Assessment

| Component | Lines | Complexity | Notes |
|-----------|-------|------------|-------|
| RoutingResult struct | 10 | Low | Simple data struct |
| Router signature change | 20 | Low | Return type change |
| API header addition | 10 | Low | Conditional header |
| Tests | 50 | Low | Header checking |
| **Total** | ~90 | **Low** | Minimal footprint |

---

## Risks & Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking Router API | Medium | Medium | Update all callers |
| Header not forwarded by proxy | Low | Low | Document behavior |
| Performance overhead | Low | Low | Negligible string copy |

---

## Estimated Effort

| Task | Estimate |
|------|----------|
| T01: RoutingResult struct | 15 min |
| T02: Update Router.select_backend | 30 min |
| T03: Add header in API layer | 20 min |
| T04: Unit tests | 30 min |
| T05: Integration tests | 30 min |
| **Total** | ~2 hours |
