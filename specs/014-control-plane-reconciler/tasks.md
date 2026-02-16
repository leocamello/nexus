---
description: "Task breakdown for Control Plane — Reconciler Pipeline implementation"
---

# Tasks: Control Plane — Reconciler Pipeline

**Input**: Design documents from `/specs/014-control-plane-reconciler/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅, quickstart.md ✅

**Tests**: Tests are NOT explicitly requested in the specification, so they are NOT included in this task breakdown. The focus is on implementation and integration with existing Router tests (which must continue to pass per FR-006, SC-002).

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

This is a single Rust project with structure:
- **Source**: `src/` at repository root
- **Tests**: `tests/` at repository root
- **Config**: Root-level TOML files

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and module structure for reconciler pipeline

- [X] T001 Create reconciler module structure at src/routing/reconciler/ with mod.rs
- [X] T002 Add globset = "0.4" dependency to Cargo.toml for TrafficPolicy pattern matching
- [X] T003 [P] Add tokio_util dependency to Cargo.toml if not present (for CancellationToken in BudgetReconciliationLoop)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core pipeline infrastructure that MUST be complete before ANY user story reconciler can be implemented

**⚠️ CRITICAL**: No user story reconciler work can begin until this phase is complete

- [X] T004 Define Reconciler trait in src/routing/reconciler/mod.rs with name() and reconcile() methods per data-model.md
- [X] T005 [P] Create RoutingIntent struct in src/routing/reconciler/intent.rs with all fields from FR-002
- [X] T006 [P] Create RoutingDecision enum in src/routing/reconciler/decision.rs with Route, Queue, Reject variants per FR-003
- [X] T007 [P] Create RejectionReason struct in src/routing/reconciler/intent.rs per FR-004
- [X] T008 [P] Create BudgetStatus enum in src/routing/reconciler/intent.rs with Normal, SoftLimit, HardLimit variants per FR-019
- [X] T009 [P] Create CostEstimate struct in src/routing/reconciler/intent.rs per FR-018
- [X] T010 Implement RoutingIntent::new() constructor in src/routing/reconciler/intent.rs
- [X] T011 Implement RoutingIntent::exclude_agent() helper method in src/routing/reconciler/intent.rs
- [X] T012 Create ReconcilerPipeline struct in src/routing/reconciler/mod.rs with Vec<Box<dyn Reconciler>>
- [X] T013 Implement ReconcilerPipeline::new() constructor in src/routing/reconciler/mod.rs
- [X] T014 Implement ReconcilerPipeline::execute() method in src/routing/reconciler/mod.rs per FR-005 (fixed order execution)
- [X] T015 [P] Create AgentSchedulingProfile struct in src/routing/reconciler/scheduling.rs per data-model.md
- [X] T016 [P] Create MetricsSnapshot struct in src/metrics/mod.rs with error_rate, avg_ttft, success_rate methods per data-model.md
- [X] T017 Implement AgentSchedulingProfile::from_backend() constructor in src/routing/reconciler/scheduling.rs
- [X] T018 [P] Extend RoutingError in src/routing/error.rs to add Reject variant with rejection_reasons field

**Checkpoint**: Foundation ready - user story reconciler implementation can now begin in parallel

---

## Phase 3: User Story 1 - Basic Pipeline Execution (Priority: P1) 🎯 MVP

**Goal**: Enable routing system to make decisions through a clean pipeline of independent reconcilers (RequestAnalyzer + SchedulerReconciler minimal viable pipeline)

**Independent Test**: Route a single request through the pipeline with minimal reconcilers and verify the routing decision matches existing Router::select_backend() behavior. All existing Router tests must pass per FR-006.

### Implementation for User Story 1

- [X] T019 [P] [US1] Create RequestAnalyzer struct in src/routing/reconciler/request_analyzer.rs
- [X] T020 [P] [US1] Create SchedulerReconciler struct in src/routing/reconciler/scheduler.rs
- [X] T021 [US1] Implement Reconciler trait for RequestAnalyzer in src/routing/reconciler/request_analyzer.rs with alias resolution (max 3 levels per FR-007)
- [X] T022 [US1] Implement requirement extraction in RequestAnalyzer in src/routing/reconciler/request_analyzer.rs per FR-008 (reuse RequestRequirements from RFC-001 Phase 1)
- [X] T023 [US1] Implement candidate agent population in RequestAnalyzer in src/routing/reconciler/request_analyzer.rs per FR-010
- [X] T024 [US1] Implement Reconciler trait for SchedulerReconciler in src/routing/reconciler/scheduler.rs with scoring formula from FR-029
- [X] T025 [US1] Implement quality_score calculation in SchedulerReconciler in src/routing/reconciler/scheduler.rs per FR-030
- [X] T026 [US1] Implement Queue decision logic in SchedulerReconciler for HealthStatus::Loading per FR-031
- [X] T027 [US1] Implement Reject decision logic in SchedulerReconciler when no candidates remain per FR-032
- [X] T028 [US1] Implement Route decision logic in SchedulerReconciler with agent selection per FR-033
- [X] T029 [US1] Integrate pipeline into Router::select_backend() in src/routing/mod.rs maintaining existing signature per FR-006
- [X] T030 [US1] Add pipeline construction with RequestAnalyzer and SchedulerReconciler in Router::select_backend() in src/routing/mod.rs
- [X] T031 [US1] Convert RoutingDecision to RoutingResult in Router::select_backend() in src/routing/mod.rs
- [X] T032 [US1] Validate all existing Router integration tests pass in tests/routing_integration.rs per FR-038

**Checkpoint**: At this point, User Story 1 should be fully functional - basic pipeline routes requests through RequestAnalyzer and SchedulerReconciler, all existing tests pass

---

## Phase 4: User Story 2 - Privacy Zone Enforcement (Priority: P2)

**Goal**: Ensure requests with privacy constraints never route to cloud agents, keeping sensitive data within controlled infrastructure

**Independent Test**: Configure a TrafficPolicy with privacy="restricted" for a model pattern, send requests, verify cloud agents are excluded with appropriate RejectionReason entries

### Implementation for User Story 2

- [X] T033 [P] [US2] Create TrafficPolicy struct in src/config/routing.rs with model_pattern, privacy, max_cost_per_request, min_tier, fallback_allowed fields per FR-035
- [X] T034 [P] [US2] Create PrivacyConstraint enum in src/config/routing.rs with Unrestricted, Restricted variants per data-model.md
- [X] T035 [P] [US2] Create PolicyMatcher struct in src/routing/reconciler/policy_matcher.rs using globset crate per research.md
- [X] T036 [US2] Implement PrivacyConstraint::allows() method in src/config/routing.rs per data-model.md
- [X] T037 [US2] Implement PolicyMatcher::compile() in src/routing/reconciler/policy_matcher.rs with globset pre-compilation per research.md
- [X] T038 [US2] Implement PolicyMatcher::find_policy() in src/routing/reconciler/policy_matcher.rs with TOML order precedence per research.md
- [X] T039 [US2] Add TrafficPolicy loading to config in src/config/routing.rs from [routing.policies.*] TOML sections per FR-011
- [X] T040 [US2] Create PrivacyReconciler struct in src/routing/reconciler/privacy.rs
- [X] T041 [US2] Implement Reconciler trait for PrivacyReconciler in src/routing/reconciler/privacy.rs
- [X] T042 [US2] Implement policy matching in PrivacyReconciler in src/routing/reconciler/privacy.rs per FR-011
- [X] T043 [US2] Implement privacy_zone filtering in PrivacyReconciler in src/routing/reconciler/privacy.rs per FR-012, FR-013
- [X] T044 [US2] Implement agent exclusion with RejectionReason in PrivacyReconciler in src/routing/reconciler/privacy.rs per FR-014
- [X] T045 [US2] Implement unknown privacy_zone handling as "cloud" in PrivacyReconciler in src/routing/reconciler/privacy.rs per FR-015
- [X] T046 [US2] Add PrivacyReconciler to pipeline construction in Router::select_backend() in src/routing/mod.rs between RequestAnalyzer and SchedulerReconciler per FR-005
- [X] T047 [US2] Add privacy_zone field to AgentProfile in src/agent/types.rs if not already present per FR-012

**Checkpoint**: At this point, User Stories 1 AND 2 should both work independently - privacy constraints correctly exclude cloud agents, all rejection reasons tracked

---

## Phase 5: User Story 3 - Budget Management (Priority: P2)

**Goal**: Enforce spending limits and prefer cost-effective agents when approaching limits to prevent exceeding monthly AI model budget

**Independent Test**: Configure budget limit, simulate requests approaching soft/hard limits, verify BudgetStatus changes trigger appropriate agent filtering (prefer local at soft limit, block cloud at hard limit)

### Implementation for User Story 3

- [X] T048 [P] [US3] Create BudgetConfig struct in src/config/routing.rs with monthly_limit_usd, soft_limit_percent, hard_limit_action fields per FR-016
- [X] T049 [P] [US3] Create HardLimitAction enum in src/config/routing.rs with Warn, BlockCloud, BlockAll variants per FR-016
- [X] T050 [P] [US3] Create BudgetMetrics struct in src/routing/reconciler/budget.rs for DashMap storage
- [X] T051 [P] [US3] Add pricing module or extend existing in src/agent/pricing.rs with get_input_cost() and get_output_cost() per research.md
- [X] T052 [US3] Create BudgetReconciler struct in src/routing/reconciler/budget.rs with Arc<DashMap<String, BudgetMetrics>>
- [X] T053 [US3] Implement estimate_cost() in BudgetReconciler in src/routing/reconciler/budget.rs per FR-017, FR-018, research.md
- [X] T054 [US3] Implement calculate_budget_status() in BudgetReconciler in src/routing/reconciler/budget.rs per FR-019
- [X] T055 [US3] Implement Reconciler trait for BudgetReconciler in src/routing/reconciler/budget.rs
- [X] T056 [US3] Implement cost estimate population in BudgetReconciler in src/routing/reconciler/budget.rs per FR-018
- [X] T057 [US3] Implement BudgetStatus setting in BudgetReconciler in src/routing/reconciler/budget.rs per FR-019
- [X] T058 [US3] Implement local agent preference at SoftLimit in BudgetReconciler in src/routing/reconciler/budget.rs per FR-020
- [X] T059 [US3] Implement cloud agent exclusion at HardLimit in BudgetReconciler in src/routing/reconciler/budget.rs per FR-021
- [X] T060 [US3] Create BudgetReconciliationLoop struct in src/routing/reconciler/budget.rs per research.md
- [X] T061 [US3] Implement BudgetReconciliationLoop::new() in src/routing/reconciler/budget.rs
- [X] T062 [US3] Implement BudgetReconciliationLoop::start() with tokio::spawn in src/routing/reconciler/budget.rs per FR-022, research.md
- [X] T063 [US3] Implement BudgetReconciliationLoop::reconcile_spending() with 60s interval in src/routing/reconciler/budget.rs per FR-022
- [ ] T064 [US3] Integrate BudgetReconciliationLoop startup in cli/serve.rs with CancellationToken per research.md
- [ ] T065 [US3] Pass spending Arc<DashMap> to Router construction in cli/serve.rs
- [ ] T066 [US3] Add BudgetReconciler to pipeline construction in Router::select_backend() in src/routing/mod.rs between PrivacyReconciler and SchedulerReconciler per FR-005
- [ ] T067 [US3] Update SchedulerReconciler to adjust scores for BudgetStatus::SoftLimit in src/routing/reconciler/scheduler.rs per FR-020

**Checkpoint**: All user stories 1-3 should now be independently functional - budget limits enforced, spending tracked, agent preference based on budget status

---

## Phase 6: User Story 4 - Capability Tier Enforcement (Priority: P3)

**Goal**: Provide explicit control over quality-cost tradeoffs to prevent silent downgrades to lower-tier models when applications require specific capabilities

**Independent Test**: Set TrafficPolicy with min_tier=3 for a model pattern, send requests with X-Nexus-Strict header, verify agents below tier 3 are excluded with appropriate RejectionReason

### Implementation for User Story 4

- [X] T068 [P] [US4] Add capability_tier field to AgentProfile or separate metadata in src/agent/types.rs per research.md
- [X] T069 [P] [US4] Create TierReconciler struct in src/routing/reconciler/tier.rs
- [X] T070 [US4] Implement AgentSchedulingProfile::capability_tier() accessor in src/routing/reconciler/scheduling.rs per FR-025
- [X] T071 [US4] Implement Reconciler trait for TierReconciler in src/routing/reconciler/tier.rs
- [X] T072 [US4] Implement policy matching for min_tier in TierReconciler in src/routing/reconciler/tier.rs per FR-024
- [X] T073 [US4] Implement capability_tier filtering in TierReconciler in src/routing/reconciler/tier.rs per FR-026
- [X] T074 [US4] Implement X-Nexus-Strict header handling in TierReconciler in src/routing/reconciler/tier.rs per FR-027
- [X] T075 [US4] Implement X-Nexus-Flexible header fallback logic in TierReconciler in src/routing/reconciler/tier.rs per FR-028
- [ ] T076 [US4] Add TierReconciler to pipeline construction in Router::select_backend() in src/routing/mod.rs between BudgetReconciler and SchedulerReconciler per FR-005

**Checkpoint**: User Story 4 complete - tier constraints enforced, explicit quality control available

---

## Phase 7: User Story 5 - Actionable Error Responses (Priority: P3)

**Goal**: Provide detailed rejection reasons when routing fails so API consumers can take corrective action instead of guessing why requests failed

**Independent Test**: Trigger various rejection scenarios (privacy constraint, budget exceeded, no capable agents) and verify 503 response contains structured rejection_reasons with agent_id, reconciler name, reason, suggested_action

### Implementation for User Story 5

- [X] T077 [P] [US5] Create QualityReconciler stub struct in src/routing/reconciler/quality.rs (reserved for future, minimal implementation)
- [X] T078 [US5] Implement minimal Reconciler trait for QualityReconciler in src/routing/reconciler/quality.rs (pass-through, no filtering)
- [ ] T079 [US5] Add QualityReconciler to pipeline construction in Router::select_backend() in src/routing/mod.rs between TierReconciler and SchedulerReconciler per FR-005
- [ ] T080 [US5] Implement rejection_reasons aggregation in ReconcilerPipeline::execute() in src/routing/reconciler/mod.rs for Reject decision per FR-032
- [ ] T081 [US5] Extend RoutingError::Reject variant in src/routing/error.rs to include structured rejection_reasons per FR-004
- [ ] T082 [US5] Update HTTP response handler for 503 errors in src/http/mod.rs or similar to serialize rejection_reasons as JSON
- [ ] T083 [US5] Add X-Nexus-Rejection-Reasons response header with structured rejection data in HTTP handler
- [ ] T084 [US5] Ensure suggested_action field populated in all reconciler RejectionReason calls (review RequestAnalyzer, PrivacyReconciler, BudgetReconciler, TierReconciler)

**Checkpoint**: All user stories should now be independently functional - actionable error responses provide clear guidance on routing failures

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories and final validation

- [ ] T085 [P] Update docs/ARCHITECTURE.md with reconciler pipeline architecture documentation
- [ ] T086 [P] Add performance benchmarks for pipeline in benches/routing.rs to validate <1ms p95 target per FR-036
- [ ] T087 [P] Add performance benchmarks for RequestAnalyzer in benches/routing.rs to validate <0.5ms target per FR-009
- [ ] T088 [P] Add example TrafficPolicy configurations to nexus.example.toml demonstrating privacy, budget, tier constraints per FR-034
- [ ] T089 Validate pipeline overhead meets <1ms target with profiling per FR-036
- [ ] T090 Validate RequestAnalyzer meets <0.5ms target with profiling per FR-009
- [ ] T091 Run full integration test suite to ensure backward compatibility per FR-038, SC-002
- [ ] T092 [P] Add logging for pipeline execution with reconciler timing and decision tracing
- [ ] T093 [P] Add metrics for per-reconciler latency and exclusion rates
- [ ] T094 Run specs/014-control-plane-reconciler/quickstart.md validation workflow
- [ ] T095 Code review focusing on order-independence of reconcilers per FR-001 (only add constraints, never remove)
- [ ] T096 [P] Update CHANGELOG.md with reconciler pipeline feature description

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-7)**: All depend on Foundational phase completion
  - User Story 1 (P1): Can start after Foundational - No dependencies on other stories
  - User Story 2 (P2): Can start after Foundational - No dependencies on other stories (adds PrivacyReconciler to pipeline)
  - User Story 3 (P2): Depends on User Story 1 for SchedulerReconciler score adjustment (T067) - Otherwise independent
  - User Story 4 (P3): Can start after Foundational - No dependencies on other stories (adds TierReconciler to pipeline)
  - User Story 5 (P3): Depends on all reconcilers being implemented (US1-4) for complete rejection_reasons
- **Polish (Phase 8)**: Depends on all user stories being complete

### Within Each User Story

- Models/structs before reconciler implementation
- Reconciler trait implementation before pipeline integration
- Pipeline integration before validation
- Story complete before moving to next priority

### Parallel Opportunities

**Setup Phase (Phase 1)**:
- T003 can run parallel with T001, T002

**Foundational Phase (Phase 2)**:
- T005, T006, T007, T008, T009 (data structures) can all run in parallel
- T015, T016, T018 can run in parallel
- T010-T011 depend on T005-T009 completion
- T017 depends on T015-T016 completion
- T012-T014 depend on T004-T011 completion

**User Story 1**:
- T019, T020 can run in parallel
- T021-T023 (RequestAnalyzer implementation) can run sequentially but parallel to T024-T028 (SchedulerReconciler implementation)
- T029-T031 depend on T019-T028 completion

**User Story 2**:
- T033, T034, T035 can run in parallel
- T040 creation can run parallel with T036-T039
- T041-T045 are sequential PrivacyReconciler implementation
- T046-T047 are integration tasks

**User Story 3**:
- T048, T049, T050, T051 can run in parallel
- T052-T059 are sequential BudgetReconciler implementation
- T060-T063 are BudgetReconciliationLoop implementation (can run parallel with T052-T059)
- T064-T067 are integration tasks

**User Story 4**:
- T068, T069 can run in parallel
- T070-T075 are sequential TierReconciler implementation
- T076 is integration

**User Story 5**:
- T077-T078 can run in parallel with T080-T081
- T082-T084 depend on all prior reconcilers

**Polish Phase**:
- T085, T086, T087, T088, T092, T093, T096 can all run in parallel
- T089-T091 depend on all implementation complete
- T094-T095 are final validation

---

## Parallel Example: User Story 1

```bash
# Launch reconciler struct creation in parallel:
Task T019: "Create RequestAnalyzer struct in src/routing/reconciler/request_analyzer.rs"
Task T020: "Create SchedulerReconciler struct in src/routing/reconciler/scheduler.rs"

# Then implement reconcilers in parallel:
Task T021-T023: "RequestAnalyzer implementation"  (Developer A)
Task T024-T028: "SchedulerReconciler implementation"  (Developer B)

# Finally integrate (sequential):
Task T029-T032: "Pipeline integration and validation"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (3 tasks, ~30 minutes)
2. Complete Phase 2: Foundational (15 tasks, ~4-5 hours) - CRITICAL, blocks all stories
3. Complete Phase 3: User Story 1 (14 tasks, ~4-5 hours)
4. **STOP and VALIDATE**: Run all existing Router tests, verify they pass
5. This delivers a working reconciler pipeline that maintains backward compatibility

**MVP Scope**: RequestAnalyzer + SchedulerReconciler pipeline with all existing functionality working through new architecture

### Incremental Delivery

1. **Foundation** (Phase 1 + 2): ~5 hours → Pipeline infrastructure ready
2. **MVP** (Phase 3): ~4-5 hours → Basic pipeline working, tests pass
3. **Privacy** (Phase 4): ~3-4 hours → Privacy zone enforcement
4. **Budget** (Phase 5): ~4-5 hours → Budget management with background loop
5. **Tier** (Phase 6): ~2-3 hours → Capability tier enforcement
6. **Errors** (Phase 7): ~2-3 hours → Actionable error responses
7. **Polish** (Phase 8): ~2-3 hours → Documentation, benchmarks, final validation

**Total Estimated Time**: 22-28 hours for complete implementation

### Parallel Team Strategy

With multiple developers after Foundational phase complete:

1. **Team completes Setup + Foundational together** (~5 hours)
2. Once Foundational is done, parallel work:
   - **Developer A**: User Story 1 (4-5 hours)
   - **Developer B**: User Story 2 (3-4 hours, can start immediately)
   - **Developer C**: User Story 4 (2-3 hours, can start immediately)
3. Then sequential work (dependencies):
   - **Developer D**: User Story 3 (4-5 hours, needs US1 T028 for score adjustment)
   - **Developer E**: User Story 5 (2-3 hours, needs all reconcilers complete)
4. **Team**: Polish phase together (~2-3 hours)

---

## Notes

- **[P] tasks** = different files, no dependencies, can run in parallel
- **[Story] label** maps task to specific user story for traceability
- Each user story should be independently completable and testable
- **No tests generated** - spec does not request TDD approach; existing Router tests validate correctness per FR-038, SC-002
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- **Performance critical**: Pipeline must execute in <1ms (FR-036), RequestAnalyzer in <0.5ms (FR-009)
- **Order-independence**: Reconcilers only add constraints, never remove (FR-001, SC-008)
- **Backward compatibility**: Router::select_backend() signature unchanged, all existing tests pass (FR-006, FR-038, SC-002)
