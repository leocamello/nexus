# F08: Fallback Chains - Implementation Tasks

**Feature**: Fallback Chains  
**Plan**: [plan.md](./plan.md)  
**Status**: 🔄 In Progress

---

## TDD Enforcement Protocol (Constitution-Mandated)

Before writing ANY implementation code for T07-T10:

1. **RED Phase Checkpoint**:
   - [ ] All tests written and added to appropriate test files
   - [ ] Tests executed: `cargo test <feature>` 
   - [ ] Failures confirmed (output shows expected errors)
   - [ ] Run: `cargo test <test_name> 2>&1 | grep -E '(FAILED|error)' | head -20`

2. **Implementation Gate**:
   - Cannot proceed to "Implementation" section until RED phase confirmed
   - If tests pass on first run, tests are INVALID (rewrite tests)

3. **GREEN Phase Checkpoint**:
   - [ ] Implementation written
   - [ ] Tests executed: `cargo test <feature>`
   - [ ] All tests PASS
   - [ ] No test code modified during GREEN phase

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

### Step 1: Write Tests (TDD Red Phase)
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

### Step 2: Verify Tests Fail (RED Phase - MANDATORY)
1. Write tests above
2. Run: `cargo test routing_result 2>&1 | grep -E '(FAILED|error)' | head -20`
3. **STOP**: Do NOT proceed if tests pass
4. Expected: Compilation errors (RoutingResult doesn't exist yet)
5. Only then proceed to implementation

### Step 3: Implementation (TDD Green Phase)
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

### Step 4: Verify Tests Pass (GREEN Phase)
Run: `cargo test routing_result`
Expected: All tests PASS

### Acceptance Criteria
- [ ] RoutingResult struct with backend, actual_model, fallback_used fields (`cargo build`)
- [ ] select_backend returns RoutingResult instead of Arc<Backend>
- [ ] Test `routing_result_contains_fallback_info` passes
- [ ] Test `routing_result_no_fallback_when_primary_used` passes
- [ ] All routing_result tests pass (`cargo test routing_result`)

---

## T08: X-Nexus-Fallback-Model Header ⬜

**Status**: Not Started  
**File**: `src/api/chat.rs`  
**Satisfies**: AC-06

### Step 1: Write Tests (TDD Red Phase)
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

### Step 2: Verify Tests Fail (RED Phase - MANDATORY)
1. Write tests above
2. Run: `cargo test fallback_header 2>&1 | grep -E '(FAILED|error)' | head -20`
3. **STOP**: Do NOT proceed if tests pass
4. Expected: Compilation errors or test failures
5. Only then proceed to implementation

### Step 3: Implementation (TDD Green Phase)
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

### Step 4: Verify Tests Pass (GREEN Phase)
Run: `cargo test fallback_header`
Expected: All tests PASS

### Acceptance Criteria
- [ ] AC-06 satisfied: X-Nexus-Fallback-Model header present when fallback used
- [ ] AC-06 satisfied: Header absent when primary model used
- [ ] Header contains actual model name
- [ ] Header is lowercase (HTTP/2 compliant)

---

## T09: Header Unit Tests ⬜

**Status**: Not Started  
**File**: `src/routing/mod.rs`, `src/api/`

### Step 1: Write Tests (TDD Red Phase)
```rust
// In src/routing/mod.rs test module
#[test]
fn routing_result_with_alias_and_fallback() {
    // Given alias "alias" → "primary"
    // And fallback "primary" → ["fallback"]
    // And only "fallback" is available
    // When select_backend("alias")
    // Then result.fallback_used == true
    // And result.actual_model == "fallback"
}
```

### Step 2: Verify Tests Fail (RED Phase)
Run: `cargo test routing_result_with_alias 2>&1 | grep -E '(FAILED|error)' | head -20`

### Step 3: Implementation
This test should pass after T07 implementation if designed correctly.

### Acceptance Criteria
- [ ] Test `routing_result_contains_fallback_info` passes
- [ ] Test `routing_result_no_fallback_when_primary_used` passes
- [ ] Test `routing_result_with_alias_and_fallback` passes
- [ ] Edge cases covered (alias + fallback, no fallback configured)

---

## T10: Header Integration Tests ⬜

**Status**: Not Started  
**File**: `tests/api_integration.rs` or `tests/routing_integration.rs`

### Step 1: Write Tests (TDD Red Phase)
```rust
#[tokio::test]
async fn api_response_includes_fallback_header() {
    // Full HTTP test:
    // 1. Set up mock backend with only fallback model
    // 2. Configure fallback "primary" → ["fallback"]
    // 3. POST /v1/chat/completions with model="primary"
    // 4. Assert X-Nexus-Fallback-Model: fallback header present
}

#[tokio::test]
async fn streaming_response_includes_fallback_header() {
    // Same as above but with stream: true
}
```

### Step 2: Verify Tests Fail (RED Phase)
Run: `cargo test api_response_includes_fallback 2>&1 | grep -E '(FAILED|error)' | head -20`

### Acceptance Criteria
- [ ] Test `api_response_includes_fallback_header` passes
- [ ] Test `response_no_fallback_header_when_primary_used` passes
- [ ] Test `streaming_response_includes_fallback_header` passes
- [ ] Integration tests verify header in HTTP response

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
