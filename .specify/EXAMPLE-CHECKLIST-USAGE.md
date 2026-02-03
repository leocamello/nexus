# Example: Using Both Checklists Together

This document demonstrates how to use both the Requirements Quality Checklist and Implementation Verification Checklist throughout a feature's lifecycle.

## Example Feature: F06 - Intelligent Router

Let's walk through how both checklists are used for the Intelligent Router feature.

---

## Phase 1: Spec Writing (Week 1)

### Day 1-2: Initial Draft

**Author**: Alice writes the initial spec for the Intelligent Router feature.

**Reference**: `.specify/checklists/requirements-quality.md` (open in split window)

**Actions**:
- Drafts spec.md with Overview, User Stories, Requirements
- Checks against requirements quality checklist while writing
- Ensures Constitution Gates are addressed
- Documents performance targets referencing Constitution latency budget

**Checklist Usage** (Requirements Quality):
```markdown
[x] CHK001: Is Simplicity Gate explicitly checked? → YES, in plan.md §Constitution Check
[x] CHK006: Is Zero Configuration principle addressed? → YES, router uses sensible defaults
[ ] CHK078: Are all FR-XXX requirements numbered sequentially? → FIX: FR-003 missing
[x] CHK123: Are performance targets quantified? → YES, routing decision < 1ms
```

**Outcome**: Draft spec with most quality items addressed, a few gaps identified.

---

### Day 3: Self-Review

**Author**: Alice runs through requirements quality checklist systematically.

**Command**:
```bash
# Copy checklist to feature directory for tracking
cp .specify/checklists/requirements-quality.md \
   specs/006-intelligent-router/review-checklist.md

# Edit and check off items
vim specs/006-intelligent-router/review-checklist.md
```

**Findings**:
```markdown
## Self-Review Findings (using requirements-quality.md)

### Items Needing Work:
- [ ] CHK078: FR numbering has gaps (FR-001, FR-002, FR-005) - need to add FR-003, FR-004
- [ ] CHK089: User Story 3 missing Given/When/Then scenarios
- [ ] CHK145: Edge case for "all backends have equal scores" not documented
- [ ] CHK174: Property-based test approach not documented for scoring logic

### Items Already Met:
- [x] CHK006-CHK012: All 7 Constitution principles addressed
- [x] CHK123-CHK128: All NFRs quantified with specific metrics
- [x] CHK089-CHK092: Most user stories have proper acceptance scenarios
```

**Actions**:
- Fixes identified gaps
- Re-checks checklist
- Moves to review phase

**Outcome**: Spec ready for team review.

---

### Day 4: Team Review

**Reviewer**: Bob reviews the spec using requirements quality checklist.

**Process**:
```bash
# Bob clones repo and checks out spec branch
git checkout 006-intelligent-router

# Opens spec and checklist side-by-side
code specs/006-intelligent-router/spec.md
code specs/006-intelligent-router/review-checklist.md
```

**Review Comments**:
```markdown
## Review Feedback (Bob)

### Critical Issues (must fix before approval):
- ❌ CHK015: Performance Gate not fully addressed
  - Routing decision < 1ms specified, but how will it be measured? Need benchmark or tracing approach
  
- ❌ CHK130: Concurrency requirements vague
  - "Handle concurrent requests" is not quantified. How many? 100? 1000?

- ❌ CHK145: Edge case missing
  - What happens when all backends have equal scores? Spec says "first available" but that's ambiguous
  - Suggest: "Deterministic tie-breaking: sort by backend ID alphabetically"

### Minor Issues (good to fix):
- ⚠️ CHK174: Test strategy could be more detailed
  - Property-based testing mentioned but no specific properties documented
  - Suggest: Add proptest properties like "score(backend) is deterministic" and "score(backend1) > score(backend2) ⇒ router prefers backend1"

### Strengths:
- ✅ CHK001-CHK005: All Constitution Gates explicitly checked
- ✅ CHK089-CHK092: User stories well-structured with Given/When/Then
- ✅ CHK067-CHK072: Data structures clearly defined with Rust types
```

**Actions**:
- Alice addresses critical issues
- Spec updated and re-reviewed
- Bob approves spec

**Outcome**: Spec approved, ready for implementation.

---

## Phase 2: Implementation Planning (Week 2, Day 1)

### Create Implementation Verification Checklist

**Author**: Alice prepares for implementation.

**Actions**:
```bash
# Copy implementation verification template
cp .specify/templates/implementation-verification.md \
   specs/006-intelligent-router/verification.md

# Customize for this feature
vim specs/006-intelligent-router/verification.md
```

**Customization**:
```markdown
## Feature-Specific Verification Items

### Routing Logic Verification
- [ ] VER-300: Scoring function considers priority first (weight 50)
- [ ] VER-301: Scoring function considers load second (weight 30)
- [ ] VER-302: Scoring function considers latency third (weight 20)
- [ ] VER-303: Tie-breaking uses alphabetical backend ID ordering
- [ ] VER-304: Routing decision completes in < 1ms (measured with criterion benchmark)

### Property-Based Tests Verification
- [ ] VER-305: Property test: score() is deterministic (same inputs → same output)
- [ ] VER-306: Property test: score(A) > score(B) ⇒ router selects A over B
- [ ] VER-307: Property test: all weights ≥ 0 and sum to 100
- [ ] VER-308: Property test: no backend is selected if all are unhealthy

### Concurrent Access Verification
- [ ] VER-309: Concurrent route selections don't race (100+ threads)
- [ ] VER-310: Backend scoring remains consistent under concurrent updates
- [ ] VER-311: Pending request counters stay accurate under concurrent increment/decrement
```

**Outcome**: Customized verification checklist ready to guide implementation.

---

## Phase 3: Test-Driven Development (Week 2-3)

### Day 1: Write Tests (RED phase)

**Author**: Alice writes tests first (TDD).

**Actions**:
```bash
# Create test file
touch src/routing/tests.rs

# Write tests that capture spec requirements
vim src/routing/tests.rs
```

**Test Examples**:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // VER-014: All public functions have unit tests
    #[test]
    fn test_score_backend_priority_weight() {
        // Tests CHK123 (NFR-001): Routing considers priority first
        let backend1 = create_backend("b1", priority: 10, load: 5, latency: 50);
        let backend2 = create_backend("b2", priority: 20, load: 2, latency: 30);
        
        let score1 = score(&backend1);
        let score2 = score(&backend2);
        
        assert!(score1 > score2, "Lower priority should score higher");
    }

    // VER-016: Property-based tests for complex logic
    proptest! {
        // VER-305: Property test - score is deterministic
        #[test]
        fn score_is_deterministic(
            priority in 1i32..100,
            load in 0u32..1000,
            latency in 1u32..5000
        ) {
            let backend = create_backend("test", priority, load, latency);
            let score1 = score(&backend);
            let score2 = score(&backend);
            prop_assert_eq!(score1, score2);
        }
        
        // VER-306: Property test - higher score means better backend
        #[test]
        fn higher_score_preferred(
            backends in prop::collection::vec(backend_strategy(), 2..10)
        ) {
            let mut sorted = backends.clone();
            sorted.sort_by_key(|b| score(b));
            
            let selected = select_backend(&backends).unwrap();
            prop_assert_eq!(selected.id, sorted.last().unwrap().id);
        }
    }
    
    // VER-025: Concurrent access tests
    #[tokio::test]
    async fn test_concurrent_route_selections() {
        // Tests CHK130 (NFR-002): Handle 100+ concurrent requests
        let router = Router::new(registry);
        let handles: Vec<_> = (0..100)
            .map(|_| {
                let router = router.clone();
                tokio::spawn(async move {
                    router.select_backend("llama3").await
                })
            })
            .collect();
        
        for handle in handles {
            assert!(handle.await.is_ok());
        }
    }
}
```

**Verification Checklist Update**:
```markdown
- [x] VER-014: All public functions have unit tests
- [x] VER-016: Property-based tests exist for scoring logic
- [x] VER-025: Concurrent access stress tests written
- [x] VER-305: Property test for deterministic scoring written
- [x] VER-306: Property test for score ordering written
- [ ] VER-011: Tests fail (RED phase) - TODO: run cargo test
```

**Actions**:
```bash
# Run tests - they should FAIL (RED phase)
cargo test

# VER-011: Confirm RED phase
# Expected: All new tests fail (no implementation yet)
```

**Outcome**: Tests written and failing (RED phase confirmed).

---

### Day 2-4: Implement Feature (GREEN phase)

**Author**: Alice implements the routing logic to make tests pass.

**Actions**:
```bash
# Implement scoring function
vim src/routing/mod.rs

# Implement backend selection
vim src/routing/selector.rs

# Run tests frequently
cargo test
```

**Implementation**:
```rust
pub fn score(backend: &Backend) -> u32 {
    // CHK123 (NFR-001): Priority weight 50, load weight 30, latency weight 20
    let priority_score = (100 - backend.priority as u32) * 50;
    let load_score = (1000 - backend.pending_requests.load(Ordering::Relaxed)) * 30;
    let latency_score = (5000 - backend.avg_latency_ms.load(Ordering::Relaxed)) * 20;
    
    priority_score + load_score + latency_score
}

pub fn select_backend(backends: &[Backend]) -> Option<&Backend> {
    backends
        .iter()
        .filter(|b| b.status == BackendStatus::Healthy)
        .max_by_key(|b| (score(b), &b.id)) // CHK145: Tie-breaking by ID
}
```

**Verification Checklist Update**:
```markdown
- [x] VER-011: Tests pass (GREEN phase achieved)
- [x] VER-300: Scoring considers priority first (implemented with weight 50)
- [x] VER-301: Scoring considers load second (implemented with weight 30)
- [x] VER-302: Scoring considers latency third (implemented with weight 20)
- [x] VER-303: Tie-breaking uses alphabetical backend ID
- [ ] VER-304: Routing decision < 1ms (TODO: add benchmark)
```

**Outcome**: Tests pass (GREEN phase).

---

### Day 5: Refactor & Optimize (REFACTOR phase)

**Author**: Alice refactors while keeping tests green.

**Actions**:
```bash
# Add performance benchmark
vim benches/routing_bench.rs

# Run benchmark
cargo bench

# Refactor if needed to meet < 1ms target
vim src/routing/mod.rs

# Ensure tests still pass
cargo test
```

**Benchmark Results**:
```
routing/select_backend
                        time:   [450.23 ns 452.87 ns 455.71 ns]
```

**Verification Checklist Update**:
```markdown
- [x] VER-304: Routing decision < 1ms (measured: ~450 ns ≈ 0.00045 ms ✅)
- [x] VER-040: Performance Gate met (routing decision < 1ms)
```

**Outcome**: Tests still pass, performance target met.

---

## Phase 4: Pre-PR Verification (Week 3, Day 6)

### Run Full Verification Checklist

**Author**: Alice runs through complete implementation verification checklist.

**Command**:
```bash
# Check verification checklist
vim specs/006-intelligent-router/verification.md
```

**Systematic Review**:

```markdown
## Section 1: Acceptance Criteria Verification
- [x] VER-001: All AC in tasks.md checked
- [x] VER-002: Each AC has passing test
- [x] VER-003: No AC skipped
- [x] VER-004: All user stories implemented

## Section 2: TDD Compliance
- [x] VER-009: Git history shows tests before implementation
  → Confirmed: git log shows test commit before impl commit
- [x] VER-010: Initial commits show RED phase
  → Confirmed: first test run failed as expected
- [x] VER-011: Subsequent commits show GREEN phase
  → Confirmed: second test run passed
- [x] VER-014: All public functions have unit tests
- [x] VER-016: Property-based tests exist

## Section 3: Constitutional Compliance
- [x] VER-026: ≤3 main modules (routing/mod.rs, routing/selector.rs - only 2)
- [x] VER-030: Axum used directly (no custom router wrapper)
- [x] VER-036: API contracts match spec
- [x] VER-040: Routing decision < 1ms (450 ns measured ✅)

## Section 4: Code Quality
- [x] VER-045: cargo build - 0 errors, 0 warnings
- [x] VER-046: cargo clippy - 0 warnings
- [x] VER-047: cargo fmt --check - pass
- [x] VER-051: All public items have doc comments
- [x] VER-057: No println! (all logging via tracing)

## Section 5: Functional Correctness
- [x] VER-063: All FR-XXX requirements implemented
- [x] VER-067: All user stories implemented
- [x] VER-071: All API endpoints work (router integrated with API gateway)

## Section 6: NFR Verification
- [x] VER-076: Latency targets met (< 1ms ✅)
- [x] VER-080: Shared state uses proper sync (DashMap, atomics)
- [x] VER-082: Concurrent stress tests pass (100+ threads)

## Section 7: Edge Cases
- [x] VER-094: All edge cases from spec implemented
  → Including "all backends equal scores" → alphabetical tie-breaking
- [x] VER-101: Empty inputs handled (no backends → None returned)

## Section 8: Integration
- [x] VER-113: Backend registration works
- [x] VER-117: Backend selection logic correct
- [x] VER-118: Retry logic works

## Section 9: Manual Testing
- [x] VER-165: Zero-config startup works
- [x] VER-169: Chat completion uses intelligent routing

## Final Checklist
- [x] VER-193: All AC checked in tasks.md
- [x] VER-194: cargo test passes
- [x] VER-195: cargo clippy passes
- [x] VER-196: cargo fmt passes
- [x] VER-200: Feature ready for merge

## Constitutional Compliance Final Check
- [x] VER-201: Zero Configuration - works with defaults ✅
- [x] VER-202: Single Binary - no new dependencies ✅
- [x] VER-203: OpenAI-Compatible - API unchanged ✅
- [x] VER-204: Backend Agnostic - routing works for all backend types ✅
- [x] VER-205: Intelligent Routing - considers capabilities first ✅
- [x] VER-206: Resilient - handles backend failures gracefully ✅
- [x] VER-207: Local-First - no external calls ✅
```

**Issues Found**:
```markdown
### Issues Found During Verification:
- [ ] VER-051: Missing doc comment on `score_latency()` helper function
  → FIX: Added doc comment
  
- [ ] VER-148: Spec status not updated to "✅ Implemented"
  → FIX: Updated spec.md header
```

**Actions**:
- Fixes issues found
- Re-checks verification checklist
- All items now checked

**Outcome**: Implementation verified and ready for PR.

---

## Phase 5: PR Creation & Code Review (Week 4, Day 1)

### PR Description

**Author**: Alice creates PR with checklist summary.

```markdown
## PR: Intelligent Router (F06)

Implements intelligent backend selection based on priority, load, and latency.

### Spec
- **Spec**: `specs/006-intelligent-router/spec.md`
- **Spec Status**: ✅ Approved (reviewed by @bob)

### Verification Completed
- ✅ Requirements Quality Checklist: All items addressed during spec phase
- ✅ Implementation Verification Checklist: All 210 items checked (see `specs/006-intelligent-router/verification.md`)

### TDD Evidence
- Tests written first: commit `abc123` (tests)
- Implementation: commit `def456` (routing logic)
- RED → GREEN workflow followed

### Constitutional Compliance
- [x] Simplicity Gate: 2 modules (routing/mod.rs, routing/selector.rs)
- [x] Performance Gate: Routing decision 450 ns (< 1ms target)
- [x] Integration-First Gate: Integration tests pass
- [x] Anti-Abstraction Gate: Direct framework usage

### Testing
- 42 unit tests pass
- 8 property-based tests pass (proptest)
- 5 integration tests pass
- Concurrent stress test passes (100 threads)

### Performance
- Routing decision: 450 ns (0.00045 ms) ✅ < 1ms target
- Memory overhead: 8 KB per backend ✅ < 10 KB target

### Manual Testing
- ✅ Zero-config startup works
- ✅ Intelligent routing selects best backend
- ✅ Tie-breaking is deterministic
- ✅ Concurrent requests handled correctly

Closes #42, #43, #44
```

---

### Code Review

**Reviewer**: Bob reviews the PR using verification checklist.

**Process**:
```bash
# Bob checks out PR branch
gh pr checkout 42

# Opens verification checklist
cat specs/006-intelligent-router/verification.md
```

**Review Comments**:
```markdown
## Code Review (Bob)

### Verification Checklist Spot Checks:
- ✅ VER-009: Confirmed TDD workflow (git log shows tests first)
- ✅ VER-040: Confirmed routing < 1ms (benchmark results in PR)
- ✅ VER-046: Ran clippy locally - 0 warnings ✅
- ✅ VER-082: Ran concurrent stress test - pass ✅

### Code Quality Observations:
- ✅ Clean, readable code
- ✅ Excellent doc comments with examples
- ✅ Property-based tests are thorough
- ✅ No code smells (no unwrap, no println, proper error handling)

### Manual Testing:
- ✅ Pulled branch and ran locally
- ✅ Tested with 3 backends (Ollama, vLLM, llama.cpp)
- ✅ Confirmed intelligent routing prefers lower priority backend
- ✅ Confirmed tie-breaking works (backends with equal scores)

### Recommendation: ✅ APPROVE

All verification items checked, tests pass, Constitutional compliance verified.
```

**Outcome**: PR approved and merged.

---

## Phase 6: Post-Merge (Week 4, Day 2)

### Archive Checklists

**Actions**:
```bash
# Update spec status
vim specs/006-intelligent-router/spec.md
# Change: Status: ✅ Implemented
# Add: PR: [#42](https://github.com/leocamello/nexus/pull/42)

# Archive verification checklist (for future reference)
git add specs/006-intelligent-router/verification.md
git commit -m "docs: archive verification checklist for F06"
```

### Update Checklists (Continuous Improvement)

**Team Retrospective**:

```markdown
## Retrospective: F06 Intelligent Router

### What the Requirements Quality Checklist Caught:
- ✅ Missing FR numbering (CHK078) - caught during self-review
- ✅ Edge case gap (CHK145) - caught during team review
- ✅ Property test documentation gap (CHK174) - caught during review

### What the Implementation Verification Checklist Caught:
- ✅ Missing doc comment (VER-051) - caught during pre-PR check
- ✅ Spec status not updated (VER-148) - caught during pre-PR check

### Improvements for Next Feature:
1. Add to Requirements Quality Checklist:
   - CHK-NEW: "Are property test invariants explicitly listed (not just 'use proptest')?"
   
2. Add to Implementation Verification Checklist:
   - VER-NEW: "Are benchmark results committed to repo (not just mentioned in PR)?"
   
3. Process Improvement:
   - Run requirements checklist during spec writing (not just at end)
   - Run verification checklist incrementally (not all at end)
```

**Actions**:
```bash
# Update requirements quality checklist
vim .specify/checklists/requirements-quality.md
# Add CHK-NEW item

# Update implementation verification template
vim .specify/templates/implementation-verification.md
# Add VER-NEW item

# Commit improvements
git add .specify/
git commit -m "docs: improve checklists based on F06 retrospective"
```

**Outcome**: Checklists improved for next feature.

---

## Summary: Checklist Usage Lifecycle

```
┌─────────────────────────────────────────────────────────┐
│                     Spec Phase                          │
├─────────────────────────────────────────────────────────┤
│ Day 1-2: Write spec (reference requirements checklist) │
│ Day 3:   Self-review (check off requirements items)    │
│ Day 4:   Team review (reviewer uses requirements list) │
│ Outcome: Spec approved ✅                               │
└─────────────────────────────────────────────────────────┘
                             ↓
┌─────────────────────────────────────────────────────────┐
│                 Implementation Phase                    │
├─────────────────────────────────────────────────────────┤
│ Day 1:   Copy verification template, customize         │
│ Day 2:   Write tests (RED) - check TDD items          │
│ Day 3-5: Implement (GREEN) - check feature items      │
│ Day 6:   Refactor - verify performance items           │
│ Day 7:   Pre-PR verification - check ALL items         │
│ Outcome: Implementation verified ✅                     │
└─────────────────────────────────────────────────────────┘
                             ↓
┌─────────────────────────────────────────────────────────┐
│                     Review Phase                        │
├─────────────────────────────────────────────────────────┤
│ Day 1:   Create PR with checklist summary             │
│ Day 2:   Code review (spot-check verification items)  │
│ Outcome: PR approved and merged ✅                     │
└─────────────────────────────────────────────────────────┘
                             ↓
┌─────────────────────────────────────────────────────────┐
│                   Post-Merge Phase                      │
├─────────────────────────────────────────────────────────┤
│ - Archive checklists                                   │
│ - Retrospective (what did checklists catch?)          │
│ - Update checklists for next feature                  │
│ Outcome: Continuous improvement ✅                     │
└─────────────────────────────────────────────────────────┘
```

---

## Key Takeaways

### Requirements Quality Checklist:
- ✅ Use during spec writing (not just at end)
- ✅ Self-review before team review
- ✅ Catches structural and content gaps early
- ✅ Ensures Constitutional alignment before coding

### Implementation Verification Checklist:
- ✅ Copy and customize at start of implementation
- ✅ Track progress incrementally (not all at end)
- ✅ Comprehensive pre-PR check catches issues
- ✅ Provides confidence that nothing is missed

### Both Checklists Together:
- ✅ Complementary: Spec quality → Implementation correctness
- ✅ Sequential: Requirements first, then verification
- ✅ Living documents: Update after each feature
- ✅ Evidence-based: Document what was checked and verified

---

**Version**: 1.0.0  
**Last Updated**: 2026-02-03  
**Next Review**: After F07 completion
