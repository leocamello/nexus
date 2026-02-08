# F08: Fallback Chains - Implementation Tasks

**Feature**: Fallback Chains  
**Plan**: [plan.md](./plan.md)  
**Status**: 🔄 In Progress

---

## Task Overview

| Task | Description | Status | Priority |
|------|-------------|--------|----------|
| T01 | Fallback data structure | ✅ (F06) | - |
| T02 | Config parsing | ✅ (F06) | - |
| T03 | Resolution logic | ✅ (F06) | - |
| T04 | Error types | ✅ (F06) | - |
| T05 | Unit tests (fallback) | ✅ (F06) | - |
| T06 | Integration tests (fallback) | ✅ (F06) | - |
| T07 | RoutingResult struct | ⬜ | P0 |
| T08 | X-Nexus-Fallback-Model header | ⬜ | P0 |
| T09 | Header unit tests | ⬜ | P0 |
| T10 | Header integration tests | ⬜ | P0 |

---

## Previously Completed Tasks (F06)

### T01-T06: Core Fallback Functionality ✅

All core fallback functionality was implemented in F06:
- Fallback chains in Router struct
- Config parsing for `[routing.fallbacks]`
- Linear iteration through fallback list
- `FallbackChainExhausted` error type
- WARN level logging
- Unit and integration tests

---

## New Tasks (F08)

## T07: RoutingResult Struct ⬜

**Status**: Not Started  
**File**: `src/routing/mod.rs`

### Tests to Write First (TDD Red Phase)
```rust
#[test]
fn routing_result_contains_fallback_info() {
    // Given router with fallback "primary" → ["fallback"]
    // And only "fallback" is available
    // When select_backend("primary")
    // Then result.fallback_used == true
    // And result.actual_model == "fallback"
}

#[test]
fn routing_result_no_fallback_when_primary_used() {
    // Given router with fallback "primary" → ["fallback"]
    // And "primary" is available
    // When select_backend("primary")
    // Then result.fallback_used == false
    // And result.actual_model == "primary"
}
```

### Verify Tests Fail First
1. Write tests above
2. Run `cargo test routing_result` - must see FAILURES
3. Only then proceed to implementation

### Implementation (TDD Green Phase)
```rust
/// Result of a successful routing decision
pub struct RoutingResult {
    /// The selected backend
    pub backend: Arc<Backend>,
    /// The actual model name used (may differ if fallback)
    pub actual_model: String,
    /// True if a fallback model was used
    pub fallback_used: bool,
}

impl Router {
    /// Select the best backend, returning routing metadata
    pub fn select_backend(
        &self,
        requirements: &RequestRequirements,
    ) -> Result<RoutingResult, RoutingError> {
        // ... existing logic ...
        // Return RoutingResult with fallback_used flag
    }
}
```

### Acceptance Criteria
- [ ] RoutingResult struct with backend, actual_model, fallback_used fields
- [ ] select_backend returns RoutingResult instead of Arc<Backend>
- [ ] fallback_used is true when fallback model used
- [ ] actual_model contains the model that was actually selected

---

## T08: X-Nexus-Fallback-Model Header ⬜

**Status**: Not Started  
**File**: `src/api/chat.rs`

### Tests to Write First (TDD Red Phase)
```rust
#[tokio::test]
async fn response_includes_fallback_header_when_fallback_used() {
    // Given router with fallback config
    // And primary model unavailable
    // When POST /v1/chat/completions
    // Then response has X-Nexus-Fallback-Model header
    // And header value is the fallback model name
}

#[tokio::test]
async fn response_no_fallback_header_when_primary_used() {
    // Given router with fallback config
    // And primary model available
    // When POST /v1/chat/completions
    // Then response does NOT have X-Nexus-Fallback-Model header
}
```

### Verify Tests Fail First
1. Write tests above
2. Run `cargo test fallback_header` - must see FAILURES
3. Only then proceed to implementation

### Implementation (TDD Green Phase)
```rust
// In src/api/chat.rs or similar
pub const FALLBACK_HEADER: &str = "x-nexus-fallback-model";

// After routing and proxying response:
if routing_result.fallback_used {
    response.headers_mut().insert(
        HeaderName::from_static(FALLBACK_HEADER),
        HeaderValue::from_str(&routing_result.actual_model)?,
    );
}
```

### Acceptance Criteria
- [ ] X-Nexus-Fallback-Model header added when fallback used
- [ ] Header contains actual model name
- [ ] No header when primary model used
- [ ] Header is lowercase (HTTP/2 compliant)

---

## T09: Header Unit Tests ⬜

**Status**: Not Started  
**File**: `src/routing/mod.rs`, `src/api/`

### Tests to Add
- [ ] `routing_result_contains_fallback_info`
- [ ] `routing_result_no_fallback_when_primary_used`
- [ ] `routing_result_with_alias_and_fallback`

### Acceptance Criteria
- [ ] All RoutingResult tests pass
- [ ] Edge cases covered (alias + fallback, no fallback configured)

---

## T10: Header Integration Tests ⬜

**Status**: Not Started  
**File**: `tests/api_integration.rs` or `tests/routing_integration.rs`

### Tests to Add
- [ ] `response_includes_fallback_header_when_fallback_used`
- [ ] `response_no_fallback_header_when_primary_used`
- [ ] `streaming_response_includes_fallback_header`

### Acceptance Criteria
- [ ] Integration tests verify header in HTTP response
- [ ] Both streaming and non-streaming responses tested

---

## Summary

### Previously Completed (F06)
- Core fallback chain functionality
- Config parsing
- Error handling
- WARN logging
- Basic tests

### New Work (F08)
- RoutingResult struct to carry fallback metadata
- X-Nexus-Fallback-Model response header
- Additional tests for header functionality

### Test Commands
```bash
# Run all fallback tests
cargo test fallback

# Run routing result tests
cargo test routing_result

# Run header tests
cargo test fallback_header
```
