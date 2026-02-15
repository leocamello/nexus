# Implementation Plan: Control Plane Reconciler Pipeline

**Branch**: `014-control-plane` | **Date**: 2024-02-15 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/014-control-plane/spec.md`

## Summary

Replace the imperative 1615-line `Router::select_backend()` function with a pipeline of independent Reconcilers that annotate shared routing state. This enables Privacy Zones (F13) and Budget Management (F14) features without O(n²) feature interaction complexity.

**Technical Approach**: Use Chain of Responsibility pattern with sequential pipeline execution. Each Reconciler independently filters candidates and adds policy annotations to a shared RoutingIntent state. The pipeline achieves <1ms routing decisions through synchronous CPU-bound operations and zero-copy Arc<Backend> references.

## Technical Context

**Language/Version**: Rust 1.87 (stable toolchain)  
**Primary Dependencies**: async-trait 0.1, tokio 1.x (full features), dashmap 6, thiserror 1  
**Storage**: In-memory only (all state in DashMap/Arc, no persistence)  
**Testing**: cargo test (unit, integration), criterion (benchmarks), proptest (property-based)  
**Target Platform**: Linux/macOS/Windows servers (cross-platform binary)  
**Project Type**: Single project (CLI + library crate)  
**Performance Goals**: <1ms routing decision (constitutional requirement), <500μs pipeline execution target  
**Constraints**: <5ms total request overhead, <50MB baseline memory, sub-millisecond backend selection  
**Scale/Scope**: 100+ backends per instance, 1000+ concurrent requests, 4 core reconcilers (Privacy, Budget, Capability, Selection)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Simplicity Gate
- [x] Using ≤3 main modules for initial implementation?
  - **YES**: `src/control/` (new module for pipeline), `src/routing/` (modified for integration), existing modules unchanged
- [x] No speculative "might need" features?
  - **YES**: Only implementing features from spec (Privacy, Budget, Capability). No speculative circuit breakers, dynamic config, etc.
- [x] No premature optimization?
  - **YES**: Simple sequential pipeline, no parallel execution, no caching layers
- [x] Start with simplest approach that could work?
  - **YES**: Chain of Responsibility with mutable reference - simplest pattern that achieves goals

### Anti-Abstraction Gate
- [x] Using Axum/Tokio/reqwest directly (no wrapper layers)?
  - **YES**: Pipeline uses tokio directly, no custom async frameworks
- [x] Single representation for each data type?
  - **YES**: One RoutingIntent, one RoutingDecision, one Reconciler trait
- [x] No "framework on top of framework" patterns?
  - **YES**: Pipeline is simple Vec<Box<dyn Reconciler>>, no meta-frameworks
- [x] Abstractions justified by actual (not theoretical) needs?
  - **YES**: Reconciler trait needed for polymorphism (4 concrete types), RoutingIntent needed for shared state

### Integration-First Gate
- [x] API contracts defined before implementation?
  - **YES**: See `contracts/reconciler-trait.md` and `contracts/router-integration.md`
- [x] Integration tests planned with real/mock backends?
  - **YES**: Tests use real Backend structs from registry, mock agents where needed
- [x] End-to-end flow testable?
  - **YES**: Router::select_backend() → Pipeline → RoutingResult (existing API unchanged)

### Performance Gate
- [x] Routing decision target: < 1ms?
  - **YES**: Target <500μs pipeline, <1ms total (constitutional requirement)
- [x] Total overhead target: < 5ms?
  - **YES**: Pipeline adds <500μs, other overhead unchanged
- [x] Memory baseline target: < 50MB?
  - **YES**: Pipeline adds ~5KB per request (RoutingIntent + annotations), well under budget

**All Gates: PASS** ✅

**Post-Design Re-Check** (after Phase 1):
- [x] Data model adds 3 core types (RoutingIntent, RoutingAnnotations, RoutingDecision) - justified by pattern
- [x] 4 concrete reconcilers (Privacy, Budget, Capability, Selection) - maps 1:1 to spec requirements
- [x] No additional abstractions introduced beyond initial design
- [x] Performance budget maintained: <500μs per reconciler measured in research

**Final Assessment**: All gates pass with no violations requiring justification.

## Project Structure

### Documentation (this feature)

```text
specs/014-control-plane/
├── plan.md              # This file (implementation plan)
├── spec.md              # Feature specification (input)
├── research.md          # Phase 0: Architecture research
├── data-model.md        # Phase 1: Entity and type definitions
├── quickstart.md        # Phase 1: Developer guide
├── contracts/           # Phase 1: API contracts
│   ├── reconciler-trait.md    # Reconciler trait contract
│   └── router-integration.md  # Router API integration contract
└── tasks.md             # Phase 2: Implementation tasks (not yet created)
```

### Source Code (repository root)

```text
src/
├── control/                # NEW: Control plane reconciler pipeline
│   ├── mod.rs             # Pipeline orchestration
│   ├── reconciler.rs      # Reconciler trait + error types
│   ├── intent.rs          # RoutingIntent + RoutingAnnotations
│   ├── decision.rs        # RoutingDecision
│   ├── privacy.rs         # PrivacyReconciler
│   ├── budget.rs          # BudgetReconciler
│   ├── capability.rs      # CapabilityReconciler
│   └── selection.rs       # SelectionReconciler
│
├── routing/               # MODIFIED: Integration with pipeline
│   ├── mod.rs             # Router (integrate pipeline)
│   ├── requirements.rs    # RequestRequirements (extend for policies)
│   ├── scoring.rs         # Unchanged
│   ├── strategies.rs      # Unchanged
│   └── error.rs           # Add ReconcileError mapping
│
├── agent/                 # UNCHANGED: Already has PrivacyZone
│   ├── types.rs           # AgentProfile with privacy_zone field
│   └── ...
│
├── registry/              # UNCHANGED: Backend storage
├── api/                   # UNCHANGED: HTTP endpoints
├── cli/                   # UNCHANGED: CLI commands
└── lib.rs                 # Add pub mod control

tests/
├── control/               # NEW: Pipeline tests
│   ├── privacy_tests.rs
│   ├── budget_tests.rs
│   ├── capability_tests.rs
│   ├── selection_tests.rs
│   └── integration_tests.rs
│
├── routing/               # EXISTING: Router tests (must all pass)
│   └── router_tests.rs
│
└── integration/           # EXISTING: End-to-end tests
    └── routing_e2e.rs

benches/
└── routing.rs             # ADD: Pipeline benchmark
```

**Structure Decision**: 

This is a **single project** (Rust library + binary crate) following the existing Nexus architecture. The implementation adds one new top-level module (`src/control/`) containing the reconciler pipeline, and modifies the existing `src/routing/` module for integration.

**Key Directories**:
- `src/control/`: New module for all reconciler pipeline code (trait, intent, reconcilers)
- `src/routing/`: Modified to integrate pipeline into Router::select_backend()
- `tests/control/`: New test directory for pipeline-specific tests
- `benches/routing.rs`: Extended to benchmark pipeline performance

**Integration Points**:
1. Router::select_backend() wraps pipeline execution
2. RequestRequirements extended with optional policy fields (privacy_zone, budget_limit, min_tier)
3. AgentProfile.privacy_zone (already exists) used by PrivacyReconciler
4. Existing RoutingResult unchanged (backward compatibility)

**Module Structure Rationale**:
- Keeps all reconciler code together in `src/control/`
- Clear separation between policy evaluation (control) and routing orchestration (routing)
- Enables independent testing of reconcilers vs router integration
- Maintains existing code organization (no large refactors)

## Complexity Tracking

**No violations** - All Constitution Gates passed without requiring justification.

The implementation introduces controlled complexity (new `control` module, Reconciler trait) but this is justified by:
1. **Actual need**: Replacing 1615-line imperative function that has O(n²) feature interaction
2. **Spec requirements**: Privacy, Budget, Capability policies require independent evaluation
3. **Simplicity win**: 4 independent 50-100 line reconcilers vs 1615-line monolith
4. **Constitutional alignment**: Single representation per type, no framework layers, async-trait directly

**Metrics**:
- **Before**: 1615 lines imperative Router::select_backend()
- **After**: ~200 lines pipeline infrastructure + 4x 50-100 line reconcilers = ~600 total lines
- **Net reduction**: ~1000 lines of code (-62%)
- **Cyclomatic complexity**: Down from O(n²) feature interaction to O(n) sequential pipeline

---

## Implementation Phases

### Phase 0: Research ✅ COMPLETE

**Artifacts Created**:
- [research.md](./research.md) - Architectural patterns, decisions, alternatives

**Key Decisions**:
1. Chain of Responsibility pattern with sequential pipeline
2. Annotation-based shared state (RoutingIntent)
3. Configurable fail-open vs fail-closed per reconciler
4. Synchronous CPU-bound reconcilers (async only for future I/O)
5. Wrapper for API compatibility (Router::select_backend unchanged)

### Phase 1: Design & Contracts ✅ COMPLETE

**Artifacts Created**:
- [data-model.md](./data-model.md) - Complete type definitions
- [contracts/reconciler-trait.md](./contracts/reconciler-trait.md) - Reconciler contract
- [contracts/router-integration.md](./contracts/router-integration.md) - Router API contract
- [quickstart.md](./quickstart.md) - Developer guide

**Types Designed**:
- **Core**: RoutingIntent, RoutingAnnotations, RoutingDecision (3 types)
- **Policy**: PrivacyConstraint, PrivacyViolation, BudgetStatus, BudgetViolation, CapabilityMismatch (5 types)
- **Infrastructure**: Reconciler trait, ReconcilerPipeline, ReconcileError, ReconcileErrorPolicy (4 types)
- **Concrete**: PrivacyReconciler, BudgetReconciler, CapabilityReconciler, SelectionReconciler (4 implementations)

### Phase 2: Task Generation (Next Step)

**Command**: `copilot task "Generate tasks for Control Plane Reconciler Pipeline"` or use `/speckit.tasks`

**Expected Artifacts**:
- `tasks.md` - Dependency-ordered implementation tasks

**Task Categories** (preview):
1. **Infrastructure**: Reconciler trait, ReconcilerPipeline, RoutingIntent types
2. **Core Reconcilers**: Privacy, Budget, Capability, Selection implementations
3. **Integration**: Router modifications, error mapping, API compatibility
4. **Testing**: Unit tests (per reconciler), integration tests (pipeline), benchmarks
5. **Documentation**: Code comments, README updates, migration guide

---

## Testing Strategy

### Unit Tests (Per Reconciler)

**Target**: 100% code coverage for reconciler logic

**Test Structure**:
```rust
#[tokio::test]
async fn privacy_reconciler_filters_cloud_backends() {
    // Setup
    let reconciler = PrivacyReconciler::new(PrivacyConstraint::Restricted);
    let mut intent = test_intent_with_mixed_backends();
    
    // Execute
    reconciler.reconcile(&mut intent).await.unwrap();
    
    // Assert
    assert_eq!(intent.candidate_backends.len(), 1); // Only local
    assert_eq!(intent.annotations.privacy_excluded.len(), 1); // Cloud excluded
}
```

**Coverage**:
- ✅ Filtering logic (backends kept/removed)
- ✅ Annotation correctness (all fields set)
- ✅ Error conditions (no candidates, invalid input)
- ✅ Trace messages (observability)

### Integration Tests (Pipeline)

**Target**: Verify sequential execution and policy interaction

**Test Structure**:
```rust
#[tokio::test]
async fn pipeline_privacy_then_budget_then_selection() {
    let pipeline = build_test_pipeline();
    let mut intent = test_intent();
    
    pipeline.execute(&mut intent).await.unwrap();
    
    // Verify privacy filtered first
    assert!(intent.annotations.privacy_constraints.is_some());
    
    // Then budget annotated
    assert!(intent.annotations.estimated_cost.is_some());
    
    // Finally selection made
    assert!(intent.decision.is_some());
}
```

**Coverage**:
- ✅ Sequential execution (order matters)
- ✅ Error handling (fail-open vs fail-closed)
- ✅ Annotation accumulation
- ✅ End-to-end routing decision

### Benchmarks

**Target**: <1ms routing decision (constitutional requirement)

**Benchmark Structure**:
```rust
fn bench_pipeline_select_backend(c: &mut Criterion) {
    let router = setup_router_with_pipeline();
    let requirements = test_requirements();
    
    c.bench_function("select_backend_with_pipeline", |b| {
        b.iter(|| router.select_backend(black_box(&requirements)))
    });
}
```

**Targets**:
- Privacy filtering: <50μs
- Budget annotation: <100μs
- Capability filtering: <50μs
- Selection: <200μs
- **Total: <500μs** (well under 1ms limit)

### Regression Tests

**Target**: All existing routing tests pass unchanged

**Coverage**:
- ✅ `tests/routing/router_tests.rs` - All existing tests pass
- ✅ `benches/routing.rs` - No performance regression
- ✅ Integration tests - Router behavior identical

---

## Migration Path

### Step 1: Infrastructure (Week 1)
- Implement Reconciler trait and ReconcilerPipeline
- Implement RoutingIntent and annotation types
- Add to Router (behind feature flag)
- Tests: Unit tests for pipeline execution

### Step 2: Core Reconcilers (Week 1-2)
- Implement PrivacyReconciler
- Implement SelectionReconciler
- Tests: Unit tests per reconciler

### Step 3: Policy Reconcilers (Week 2)
- Implement BudgetReconciler (in-memory estimation)
- Implement CapabilityReconciler
- Tests: Unit tests + integration tests

### Step 4: Router Integration (Week 2-3)
- Integrate pipeline into Router::select_backend()
- Add error mapping (ReconcileError → RoutingError)
- Tests: All existing tests pass

### Step 5: Observability (Week 3)
- Add trace logging
- Add detailed error messages with exclusion reasons
- Tests: Verify trace output

### Step 6: Cleanup (Week 3)
- Remove feature flag (or keep for gradual rollout)
- Update documentation
- Final benchmarks

---

## Performance Budget

| Component | Budget | Justification |
|-----------|--------|---------------|
| PrivacyReconciler | 50μs | Simple filtering, no I/O |
| BudgetReconciler | 100μs | Token math, no external service (Phase 1) |
| CapabilityReconciler | 50μs | Simple capability matching |
| SelectionReconciler | 200μs | Scoring algorithm (existing code) |
| Pipeline overhead | 100μs | Iteration, error handling |
| **Total** | **500μs** | **50% of 1ms constitutional limit** |

**Slack**: 500μs remaining for future enhancements (external budget service, circuit breakers, etc.)

---

## Dependencies

### Existing (No New Dependencies)
- `async-trait = "0.1"` - Already in Cargo.toml for InferenceAgent trait
- `tokio = { version = "1", features = ["full"] }` - Already in Cargo.toml
- `thiserror = "1"` - Already in Cargo.toml for error types
- `arc` and `hashmap` - Rust std library

### Optional (Future Enhancements)
- None for Phase 1 implementation

---

## Documentation Deliverables

### Generated by This Plan
- ✅ [research.md](./research.md) - Architecture research and decisions
- ✅ [data-model.md](./data-model.md) - Complete type reference
- ✅ [contracts/reconciler-trait.md](./contracts/reconciler-trait.md) - Reconciler contract
- ✅ [contracts/router-integration.md](./contracts/router-integration.md) - Router API contract
- ✅ [quickstart.md](./quickstart.md) - Developer guide for using/extending pipeline

### To Be Generated by Implementation
- Code documentation (doc comments)
- README updates (mention new control plane module)
- CHANGELOG entry
- Migration guide (if needed for external users)

---

## Success Criteria

### From Spec (Measurable Outcomes)

- **SC-001**: Privacy-constrained requests never routed to wrong zone (100% enforcement)
  - **Verification**: Integration tests with mixed local/cloud backends
  
- **SC-002**: Budget hard limits prevent spending overruns (100% accuracy)
  - **Verification**: Unit tests for BudgetReconciler filtering logic
  
- **SC-003**: Capability tier requirements prevent silent downgrades (100% accuracy)
  - **Verification**: Unit tests for CapabilityReconciler filtering
  
- **SC-004**: Policy evaluation completes in <1ms 99.9% of time
  - **Verification**: Benchmark measurements, target <500μs
  
- **SC-005**: Rejected requests provide actionable error messages
  - **Verification**: Integration tests verify exclusion reasons populated
  
- **SC-006**: All existing routing tests pass (backward compatibility)
  - **Verification**: Run existing test suite without modification
  
- **SC-007**: New policies addable via configuration (no code changes)
  - **Verification**: Demonstrate custom reconciler addition in quickstart

### Additional Criteria

- **Code Quality**: All reconcilers <100 lines, clear single responsibility
- **Test Coverage**: >90% line coverage for control module
- **Performance**: No regression in routing.rs benchmarks
- **Documentation**: Quickstart enables custom reconciler implementation

---

## Risk Assessment

### Risk 1: Performance Regression
**Likelihood**: Low  
**Impact**: High (constitutional violation)  
**Mitigation**: Comprehensive benchmarking before merge, continuous monitoring  
**Contingency**: Feature flag to revert to imperative path

### Risk 2: Complex Policy Interactions
**Likelihood**: Medium  
**Impact**: Medium (bugs in filtering logic)  
**Mitigation**: Independent reconciler testing, property-based tests  
**Contingency**: Clear trace logging for debugging

### Risk 3: Backward Compatibility Break
**Likelihood**: Low  
**Impact**: High (breaks existing deployments)  
**Mitigation**: All existing tests must pass, API unchanged  
**Contingency**: Revert commit if any test fails

---

## Open Questions

**None** - All questions resolved during research phase.

---

## Related Documents

- [Feature Specification](./spec.md) - User stories, requirements, success criteria
- [Research](./research.md) - Architecture decisions and alternatives
- [Data Model](./data-model.md) - Complete type definitions
- [Reconciler Contract](./contracts/reconciler-trait.md) - Trait implementation guide
- [Router Integration](./contracts/router-integration.md) - API integration contract
- [Quickstart Guide](./quickstart.md) - Developer usage guide
- [Nexus Constitution](../../.specify/memory/constitution.md) - Performance standards

---

**Plan Status**: ✅ COMPLETE  
**Ready for**: Task Generation (`/speckit.tasks`)  
**Estimated Implementation Time**: 2-3 weeks  
**Date Completed**: 2024-02-15
