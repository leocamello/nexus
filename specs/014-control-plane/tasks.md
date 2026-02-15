# Tasks: Control Plane Reconciler Pipeline

**Feature**: Control Plane — Reconciler Pipeline (RFC-001 Phase 2)  
**Branch**: `feat/control-plane-phase-2`  
**Input**: Design documents from `/home/lhnascimento/Projects/nexus/specs/014-control-plane/`  
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: Tests are NOT explicitly requested in spec.md, so only implementation tasks are included. Test scenarios from spec.md can be used for manual validation.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story. The pipeline architecture enables gradual rollout: start with basic infrastructure, then add privacy filtering (US1), budget tracking (US2), capability tiers (US3), actionable errors (US4), and extensibility (US5).

## Format: `- [ ] [ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Infrastructure Foundation)

**Purpose**: Create the reconciler pipeline infrastructure and core types

- [X] T001 Create src/control/ module directory structure
- [X] T002 Add pub mod control to src/lib.rs to expose the new module
- [X] T003 [P] Create src/control/mod.rs with module declarations (reconciler, intent, decision, pipeline)
- [X] T004 [P] Add async-trait dependency to Cargo.toml (already present, verify version 0.1)

---

## Phase 2: Foundational (Core Pipeline Infrastructure)

**Purpose**: Core types and traits that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T005 [P] Create Reconciler trait in src/control/reconciler.rs with reconcile(&mut RoutingIntent) method
- [X] T006 [P] Create ReconcileError enum in src/control/reconciler.rs with variants (NoCandidates, PrivacyViolation, BudgetServiceError, CapabilityUnavailable, SelectionFailed, Internal)
- [X] T007 [P] Create ReconcileErrorPolicy enum in src/control/reconciler.rs (FailOpen, FailClosed)
- [X] T008 [P] Create RoutingIntent struct in src/control/intent.rs with fields (request_requirements, candidate_backends, annotations, decision)
- [X] T009 [P] Create RoutingAnnotations struct in src/control/intent.rs with all optional policy fields
- [X] T010 [P] Create RoutingDecision struct in src/control/decision.rs with fields (backend, reason, score)
- [X] T011 Create ReconcilerPipeline struct in src/control/mod.rs with reconcilers: Vec&lt;Arc&lt;dyn Reconciler&gt;&gt;
- [X] T012 Implement ReconcilerPipeline::execute() method in src/control/mod.rs with error handling per reconciler policy
- [X] T013 Add From&lt;ReconcileError&gt; for RoutingError in src/routing/error.rs to map pipeline errors
- [X] T014 Extend RequestRequirements in src/routing/requirements.rs with optional fields (privacy_zone, budget_limit, min_capability_tier)

**Checkpoint**: Foundation ready - reconciler pipeline infrastructure is complete

---

## Phase 3: User Story 1 - Consistent Privacy-Aware Routing (Priority: P1) 🎯 MVP

**Goal**: Automatically route requests based on privacy zone constraints, preventing accidental routing to non-compliant backends

**Independent Test**: 
1. Configure backends with different privacy zones (local vs. cloud)
2. Submit request with restricted privacy constraint
3. Verify only local backends are candidates
4. Verify cloud backends have PrivacyViolation annotations
5. Verify request succeeds with local backend

### Implementation for User Story 1

- [X] T015 [P] [US1] Create PrivacyConstraint enum in src/control/privacy.rs (Unrestricted, Restricted, Zone)
- [X] T016 [P] [US1] Create PrivacyViolation struct in src/control/privacy.rs with fields (backend_zone, required_constraint, message)
- [X] T017 [US1] Implement PrivacyConstraint::allows_backend() method in src/control/privacy.rs
- [X] T018 [US1] Create PrivacyReconciler struct in src/control/privacy.rs with default_constraint field
- [X] T019 [US1] Implement Reconciler trait for PrivacyReconciler in src/control/privacy.rs with FailClosed error policy
- [X] T020 [US1] Implement privacy filtering logic in PrivacyReconciler::reconcile() to filter candidate_backends by privacy zone
- [X] T021 [US1] Add privacy exclusion annotations to RoutingIntent in PrivacyReconciler (privacy_excluded HashMap)
- [X] T022 [US1] Add trace logging for privacy filtering results in PrivacyReconciler
- [X] T023 [US1] Add PrivacyReconciler to default pipeline in Router::new() in src/routing/mod.rs

**Checkpoint**: Privacy-aware routing is fully functional - requests are filtered by privacy zone

---

## Phase 4: User Story 2 - Automated Budget Enforcement (Priority: P2)

**Goal**: Track spending against budget limits and adjust routing to favor cost-effective options or block expensive operations

**Independent Test**: 
1. Set monthly budget limit in BudgetReconciler
2. Simulate usage approaching soft limit (75%)
3. Verify BudgetStatus::SoftLimit annotation present
4. Simulate usage at hard limit (100%)
5. Verify expensive backends excluded with BudgetViolation
6. Verify request fails or routes to cheaper option

### Implementation for User Story 2

- [X] T024 [P] [US2] Create BudgetStatus enum in src/control/budget.rs (Normal, SoftLimit, HardLimit)
- [X] T025 [P] [US2] Create BudgetViolation struct in src/control/budget.rs with fields (estimated_cost, current_usage, limit, message)
- [X] T026 [US2] Implement BudgetStatus::allows_cost() method in src/control/budget.rs
- [X] T027 [US2] Implement BudgetStatus::prefer_cheaper() method in src/control/budget.rs
- [X] T028 [US2] Create BudgetReconciler struct in src/control/budget.rs with fields (cost_model HashMap, monthly_limit Option)
- [X] T029 [US2] Implement Reconciler trait for BudgetReconciler in src/control/budget.rs with FailOpen error policy
- [X] T030 [US2] Implement cost estimation logic in BudgetReconciler::estimate_cost() using tokens and cost_model
- [X] T031 [US2] Implement budget status calculation in BudgetReconciler::get_budget_status() based on current usage
- [X] T032 [US2] Add budget filtering logic in BudgetReconciler::reconcile() to exclude backends when hard limit reached
- [X] T033 [US2] Add budget annotations to RoutingIntent (estimated_cost, budget_status, budget_excluded)
- [X] T034 [US2] Add trace logging for budget status and cost estimates in BudgetReconciler
- [X] T035 [US2] Add BudgetReconciler to default pipeline in Router::new() in src/routing/mod.rs (after PrivacyReconciler)

**Checkpoint**: Budget enforcement is functional - requests respect budget limits

---

## Phase 5: User Story 3 - Quality Tier Guarantees (Priority: P2)

**Goal**: Ensure requests are never silently downgraded to lower-tier backends and provide explicit feedback when tier unavailable

**Independent Test**: 
1. Configure backends with different capability tiers
2. Submit request with minimum tier requirement
3. Verify lower-tier backends excluded with CapabilityMismatch
4. Verify only sufficient-tier backends remain as candidates
5. Verify request fails with clear error if no backends meet tier

### Implementation for User Story 3

- [X] T036 [P] [US3] Create CapabilityMismatch struct in src/control/capability.rs with fields (required_tier, backend_tier, missing_capabilities, message)
- [X] T037 [P] [US3] Add capability_tier: Option&lt;u8&gt; field to AgentProfile in src/agent/types.rs
- [X] T038 [US3] Create CapabilityReconciler struct in src/control/capability.rs
- [X] T039 [US3] Implement Reconciler trait for CapabilityReconciler in src/control/capability.rs with FailOpen error policy
- [X] T040 [US3] Implement tier filtering logic in CapabilityReconciler::reconcile() to check backend_tier >= required_tier
- [X] T041 [US3] Add capability exclusion annotations to RoutingIntent (required_tier, capability_excluded HashMap)
- [X] T042 [US3] Add trace logging for capability filtering results in CapabilityReconciler
- [X] T043 [US3] Add CapabilityReconciler to default pipeline in Router::new() in src/routing/mod.rs (after BudgetReconciler)

**Checkpoint**: Capability tier filtering is functional - requests respect minimum tier requirements

---

## Phase 6: User Story 4 - Actionable Error Messages (Priority: P3)

**Goal**: Provide detailed explanations when requests cannot be fulfilled, listing each backend and why it was excluded

**Independent Test**: 
1. Create scenario where all backends excluded for different reasons
2. Submit request that will fail
3. Verify error message contains specific rejection reasons per backend
4. Verify error includes suggested actions (budget reset time, privacy zone settings, etc.)

### Implementation for User Story 4

- [ ] T044 [US4] Create build_detailed_error_message() function in src/control/mod.rs that aggregates all exclusion reasons
- [ ] T045 [US4] Add format_privacy_violation() helper in src/control/privacy.rs for human-readable privacy errors
- [ ] T046 [US4] Add format_budget_violation() helper in src/control/budget.rs for human-readable budget errors with reset time
- [ ] T047 [US4] Add format_capability_mismatch() helper in src/control/capability.rs for human-readable capability errors
- [ ] T048 [US4] Update ReconcileError::NoCandidates in src/control/reconciler.rs to include RoutingIntent annotations
- [ ] T049 [US4] Update From&lt;ReconcileError&gt; for RoutingError in src/routing/error.rs to build detailed messages
- [ ] T050 [US4] Add suggested actions to error messages (e.g., "Budget resets on [date]", "Adjust privacy zone settings")

**Checkpoint**: Error messages are detailed and actionable - users understand why routing failed

---

## Phase 7: User Story 5 - Extensible Policy System (Priority: P3)

**Goal**: Enable adding new routing policies through configuration without modifying core routing logic

**Independent Test**: 
1. Create custom reconciler implementing Reconciler trait
2. Add to pipeline via ReconcilerPipeline::new()
3. Verify custom policy participates in routing decisions
4. Verify other policies unaffected
5. Remove custom reconciler, verify system continues normally

### Implementation for User Story 5

- [ ] T051 [US5] Add comprehensive documentation comments to Reconciler trait in src/control/reconciler.rs explaining contract
- [ ] T052 [US5] Create reconciler builder pattern in src/control/pipeline_builder.rs for flexible pipeline construction
- [ ] T053 [US5] Add Router::with_pipeline() constructor in src/routing/mod.rs to accept custom ReconcilerPipeline
- [ ] T054 [US5] Document reconciler extension in specs/014-control-plane/quickstart.md with example custom reconciler
- [ ] T055 [US5] Add validation in ReconcilerPipeline::new() to ensure SelectionReconciler is always last
- [ ] T056 [US5] Add pipeline introspection methods (list_reconcilers(), get_reconciler_by_name()) in src/control/mod.rs

**Checkpoint**: Pipeline is extensible - custom reconcilers can be added via configuration

---

## Phase 8: Selection & Integration (Complete the Pipeline)

**Purpose**: Implement the final selection reconciler and integrate pipeline into Router

**Note**: This phase completes the pipeline by adding the final selection logic and integrating everything into Router::select_backend()

- [X] T057 [P] Create SelectionReconciler struct in src/control/selection.rs with fields (strategy, weights, round_robin_counter)
- [X] T058 [P] Implement Reconciler trait for SelectionReconciler in src/control/selection.rs with FailClosed error policy
- [X] T059 Implement selection logic in SelectionReconciler::reconcile() using existing routing strategies (Smart, RoundRobin, etc.)
- [X] T060 Add SelectionReconciler as final reconciler in default pipeline in Router::new() in src/routing/mod.rs
- [X] T061 Add pipeline: ReconcilerPipeline field to Router struct in src/routing/mod.rs
- [X] T062 Refactor Router::select_backend() in src/routing/mod.rs to use pipeline instead of imperative logic
- [X] T063 Create Router::select_backend_async() helper method in src/routing/mod.rs for pipeline execution
- [X] T064 Implement RoutingIntent → RoutingResult conversion in src/control/decision.rs
- [X] T065 Preserve existing Router::select_backend() signature for backward compatibility in src/routing/mod.rs
- [X] T066 Add trace collection to Router::select_backend() for debugging in src/routing/mod.rs

**Checkpoint**: Pipeline is fully integrated - Router uses reconcilers for all routing decisions

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories and ensure production readiness

- [ ] T067 [P] Add comprehensive module-level documentation to src/control/mod.rs explaining pipeline architecture
- [ ] T068 [P] Add inline code comments explaining reconciler ordering rationale in Router::new()
- [ ] T069 [P] Update specs/014-control-plane/quickstart.md with real examples from implementation
- [ ] T070 [P] Add pipeline performance benchmarks in benches/routing.rs (target <500μs)
- [ ] T071 [P] Add privacy filtering integration test in tests/control/privacy_tests.rs
- [ ] T072 [P] Add budget tracking integration test in tests/control/budget_tests.rs
- [ ] T073 [P] Add capability filtering integration test in tests/control/capability_tests.rs
- [ ] T074 [P] Add selection logic integration test in tests/control/selection_tests.rs
- [ ] T075 [P] Add end-to-end pipeline test in tests/control/integration_tests.rs
- [ ] T076 Verify all existing routing tests pass in tests/routing/ (backward compatibility check)
- [ ] T077 Add CHANGELOG.md entry documenting reconciler pipeline feature
- [ ] T078 Update README.md with control plane module description
- [ ] T079 Run cargo clippy and fix any warnings in src/control/
- [ ] T080 Run cargo fmt to format all control plane code

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-7)**: All depend on Foundational phase completion
  - User Story 1 (Privacy) - Can start after Foundational
  - User Story 2 (Budget) - Should start after User Story 1 (uses same annotation pattern)
  - User Story 3 (Capability) - Should start after User Story 1 (uses same filtering pattern)
  - User Story 4 (Errors) - Should start after User Stories 1-3 (aggregates their errors)
  - User Story 5 (Extensibility) - Can start after Foundational (independent of concrete reconcilers)
- **Selection & Integration (Phase 8)**: Depends on User Stories 1-3 being complete
- **Polish (Phase 9)**: Depends on Phase 8 completion

### User Story Dependencies

**Note**: While reconcilers can be developed in parallel, they build on each other's patterns. Suggested order for single-developer implementation:

1. **User Story 1 (P1 - Privacy)**: Start here - establishes filtering pattern (no dependencies on other stories)
2. **User Story 2 (P2 - Budget)**: Uses privacy filtering pattern (builds on US1 pattern)
3. **User Story 3 (P2 - Capability)**: Uses privacy filtering pattern (builds on US1 pattern)
4. **User Story 4 (P3 - Errors)**: Aggregates error information from US1-3
5. **User Story 5 (P3 - Extensibility)**: Documents patterns established by US1-4

### Within Each User Story

- Core types before reconciler implementation
- Reconciler implementation before pipeline integration
- Pipeline integration before testing
- Story complete before moving to next priority

### Parallel Opportunities

**Phase 1 (Setup)**: All tasks can run in parallel

**Phase 2 (Foundational)**: Tasks T005-T010 can run in parallel (independent types)

**Phase 3 (US1)**: Tasks T015-T016 can run in parallel (independent types)

**Phase 4 (US2)**: Tasks T024-T025 can run in parallel (independent types)

**Phase 5 (US3)**: Tasks T036-T037 can run in parallel (independent files)

**Phase 8 (Integration)**: Tasks T057-T058 can run in parallel (independent trait impl)

**Phase 9 (Polish)**: Tasks T067-T075 can run in parallel (different files)

---

## Parallel Example: Foundational Phase

```bash
# Launch all core type definitions together:
Task T005: "Create Reconciler trait in src/control/reconciler.rs"
Task T006: "Create ReconcileError enum in src/control/reconciler.rs" 
Task T007: "Create ReconcileErrorPolicy enum in src/control/reconciler.rs"
Task T008: "Create RoutingIntent struct in src/control/intent.rs"
Task T009: "Create RoutingAnnotations struct in src/control/intent.rs"
Task T010: "Create RoutingDecision struct in src/control/decision.rs"

# Then proceed with pipeline implementation:
Task T011: "Create ReconcilerPipeline struct"
Task T012: "Implement ReconcilerPipeline::execute()"
```

---

## Parallel Example: User Story 1 (Privacy)

```bash
# Launch privacy types in parallel:
Task T015: "Create PrivacyConstraint enum in src/control/privacy.rs"
Task T016: "Create PrivacyViolation struct in src/control/privacy.rs"

# Then proceed with reconciler:
Task T017: "Implement PrivacyConstraint::allows_backend()"
Task T018: "Create PrivacyReconciler struct"
# ... etc
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

**Goal**: Privacy-aware routing without budget or capability filtering

1. Complete Phase 1: Setup (~30 minutes)
2. Complete Phase 2: Foundational (~2-3 hours)
3. Complete Phase 3: User Story 1 - Privacy (~2-3 hours)
4. Implement minimal SelectionReconciler (T057-T059) (~1 hour)
5. Integrate into Router (T061-T065) (~1-2 hours)
6. **STOP and VALIDATE**: Test privacy filtering independently
7. Deploy/demo if ready

**Total MVP Time**: ~1 day of focused work

### Incremental Delivery

**Week 1: Foundation + Privacy (MVP)**
- Days 1-2: Setup + Foundational (Phase 1-2)
- Days 3-4: Privacy Reconciler (Phase 3)
- Day 5: Selection + Integration (Phase 8, minimal)
- **Result**: Privacy-aware routing works end-to-end

**Week 2: Budget + Capability**
- Days 1-2: Budget Reconciler (Phase 4)
- Days 3-4: Capability Reconciler (Phase 5)
- Day 5: Testing and refinement
- **Result**: Full policy enforcement (privacy, budget, capability)

**Week 3: Error Messages + Polish**
- Days 1-2: Actionable Errors (Phase 6)
- Days 3-4: Extensibility (Phase 7)
- Day 5: Polish, documentation, benchmarks (Phase 9)
- **Result**: Production-ready reconciler pipeline

### Parallel Team Strategy

With 2-3 developers after Foundational phase completes:

**Developer A**: User Story 1 (Privacy) → User Story 4 (Errors)
**Developer B**: User Story 2 (Budget) → User Story 3 (Capability)
**Developer C**: User Story 5 (Extensibility) → Phase 8 (Integration)

Stories integrate independently via the shared RoutingIntent annotations.

---

## Task Summary

**Total Tasks**: 80
**By Phase**:
- Phase 1 (Setup): 4 tasks
- Phase 2 (Foundational): 10 tasks
- Phase 3 (US1 - Privacy): 9 tasks
- Phase 4 (US2 - Budget): 12 tasks
- Phase 5 (US3 - Capability): 8 tasks
- Phase 6 (US4 - Errors): 7 tasks
- Phase 7 (US5 - Extensibility): 6 tasks
- Phase 8 (Selection & Integration): 10 tasks
- Phase 9 (Polish): 14 tasks

**By User Story**:
- User Story 1 (Privacy): 9 tasks
- User Story 2 (Budget): 12 tasks
- User Story 3 (Capability): 8 tasks
- User Story 4 (Errors): 7 tasks
- User Story 5 (Extensibility): 6 tasks
- Infrastructure: 38 tasks

**Parallelizable Tasks**: 22 tasks marked [P] can run in parallel

**Suggested MVP Scope**: Phase 1 + Phase 2 + Phase 3 (US1 - Privacy) + minimal Phase 8 = ~23 tasks for working privacy-aware routing

---

## Performance Budget

| Component | Target | Maximum | Tasks |
|-----------|--------|---------|-------|
| PrivacyReconciler | <50μs | 100μs | T015-T023 |
| BudgetReconciler | <100μs | 200μs | T024-T035 |
| CapabilityReconciler | <50μs | 100μs | T036-T043 |
| SelectionReconciler | <200μs | 500μs | T057-T060 |
| Pipeline overhead | <100μs | 200μs | T011-T012 |
| **Total Pipeline** | **<500μs** | **<1ms** | Measured in T070 |

**Constitutional Requirement**: <1ms routing decision (50% safety margin with 500μs target)

---

## Notes

- **[P] tasks**: Different files, no dependencies, safe to parallelize
- **[Story] label**: Maps task to specific user story for traceability
- **Router::select_backend() signature**: MUST remain unchanged for backward compatibility (T065)
- **Reconciler ordering**: Privacy → Budget → Capability → Selection (enforced in T055)
- **Error policy**: FailClosed for Privacy and Selection, FailOpen for Budget and Capability
- **Benchmark target**: <500μs total pipeline execution (T070)
- **Existing tests**: All tests in tests/routing/ must pass unchanged (T076)
- Each user story should be independently testable via its acceptance scenarios in spec.md
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Trace logging (RoutingIntent.trace) enables debugging without performance impact
