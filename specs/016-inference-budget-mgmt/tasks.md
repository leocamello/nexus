# Tasks: F14 Inference Budget Management

**Feature Branch**: `016-inference-budget-mgmt`  
**Input**: Design documents from `/specs/016-inference-budget-mgmt/`  
**Prerequisites**: plan.md (✓), spec.md (✓), research.md (✓), data-model.md (✓), contracts/ (✓)

**Tests**: This feature spec does NOT explicitly request tests. Tasks focus on implementation only. Tests can be added as a future enhancement if needed.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

---

## Format: `- [ ] [ID] [P?] [Story?] Description`

- **Checkbox**: ALWAYS start with `- [ ]` (markdown checkbox)
- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1, US2, US3, US4)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and tokenizer infrastructure setup

- [X] T001 Verify Rust toolchain 1.87 stable and required dependencies (tiktoken-rs, metrics, dashmap, chrono) in Cargo.toml
- [X] T002 [P] Create src/agent/tokenizer.rs file with module structure and exports
- [X] T003 [P] Update src/agent/mod.rs to export tokenizer module

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core tokenizer infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T004 [P] Implement Tokenizer trait with count_tokens(), tier(), and name() methods in src/agent/tokenizer.rs
- [X] T005 [P] Implement TokenizerError enum with Encoding and ModelNotSupported variants in src/agent/tokenizer.rs
- [X] T006 Implement TiktokenExactTokenizer struct with o200k_base() and cl100k_base() constructors in src/agent/tokenizer.rs
- [X] T007 Implement TiktokenApproximationTokenizer struct with new() constructor in src/agent/tokenizer.rs
- [X] T008 Implement HeuristicTokenizer struct with 1.15x multiplier in src/agent/tokenizer.rs
- [X] T009 Implement TokenizerRegistry struct with get_tokenizer() and count_tokens() methods in src/agent/tokenizer.rs
- [X] T010 Add glob pattern matchers for OpenAI (gpt-4-turbo*, gpt-{3.5,4}) and Anthropic (claude-*) models in TokenizerRegistry in src/agent/tokenizer.rs
- [X] T011 Add CostEstimate::TIER_EXACT, TIER_APPROXIMATION, TIER_HEURISTIC constants and tier_name() method in src/routing/reconciler/intent.rs

**Checkpoint**: Tokenizer foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Cost Control with Soft Limits (Priority: P1) 🎯 MVP

**Goal**: Set monthly inference budget with gradual traffic shift to cost-efficient options as spending approaches limits

**Independent Test**: Configure monthly budget, send requests until 75% threshold (default soft limit), observe routing shift from cloud-preferred to local-preferred behavior

### Implementation for User Story 1

- [X] T012 [US1] Enhance BudgetReconciler::estimate_cost() to accept TokenizerRegistry parameter in src/routing/reconciler/budget.rs
- [X] T013 [US1] Replace heuristic token counting with TokenizerRegistry.count_tokens() call in BudgetReconciler::estimate_cost() in src/routing/reconciler/budget.rs
- [X] T014 [US1] Set CostEstimate.token_count_tier from tokenizer.tier() in BudgetReconciler::estimate_cost() in src/routing/reconciler/budget.rs
- [X] T015 [US1] Verify BudgetReconciler::calculate_budget_status() correctly transitions at soft_limit_percent threshold in src/routing/reconciler/budget.rs
- [X] T016 [US1] Add TokenizerRegistry field to BudgetReconciler struct in src/routing/reconciler/budget.rs
- [X] T017 [US1] Wire BudgetReconciliationLoop with cancellation token in cli/serve.rs startup sequence
- [X] T018 [US1] Verify SchedulerReconciler adjusts agent scores based on BudgetStatus (existing behavior from Control Plane PR)

**Checkpoint**: Soft limit routing shift should be fully functional - can test by sending requests until 75% budget utilized

---

## Phase 4: User Story 2 - Precise Cost Tracking (Priority: P2)

**Goal**: Accurate per-request cost estimates using provider-specific token counting for audit-grade budget enforcement

**Independent Test**: Send identical requests to different providers (OpenAI, Anthropic, local) and verify cost estimates use appropriate tokenizers

### Implementation for User Story 2

- [X] T019 [P] [US2] Add nexus_cost_per_request_usd histogram metric recording in BudgetReconciler::estimate_cost() in src/routing/reconciler/budget.rs
- [X] T020 [P] [US2] Add nexus_token_count_duration_seconds histogram recording in TokenizerRegistry::count_tokens() in src/agent/tokenizer.rs
- [X] T021 [P] [US2] Add nexus_token_count_tier_total counter recording in TokenizerRegistry::count_tokens() in src/agent/tokenizer.rs
- [X] T022 [US2] Add timing instrumentation around tokenizer.count_tokens() calls in TokenizerRegistry in src/agent/tokenizer.rs
- [X] T023 [US2] Add tracing::debug! logs for token count results with tier information in BudgetReconciler::estimate_cost() in src/routing/reconciler/budget.rs

**Checkpoint**: Prometheus metrics should show breakdown of exact vs approximation vs heuristic token counting per model

---

## Phase 5: User Story 3 - Hard Limit Protection (Priority: P3)

**Goal**: Configurable actions when monthly budget exhausted (local-only, queue, reject) based on availability requirements

**Independent Test**: Exhaust monthly budget (100% utilization) and verify configured hard_limit_action is enforced

### Implementation for User Story 3

- [X] T024 [US3] Verify HardLimitAction enum (Warn, BlockCloud, BlockAll) exists in src/config/routing.rs (already implemented in Control Plane PR)
- [X] T025 [US3] Verify BudgetReconciler enforces hard_limit_action when BudgetStatus::HardLimit in src/routing/reconciler/budget.rs (already implemented)
- [X] T026 [US3] Verify month rollover detection in BudgetReconciliationLoop::reconcile_spending() using month_key comparison in src/routing/reconciler/budget.rs
- [X] T027 [US3] Add tracing::info! log for budget reset on month rollover in BudgetReconciliationLoop in src/routing/reconciler/budget.rs
- [X] T028 [US3] Add nexus_budget_events_total counter for month_rollover event in BudgetReconciliationLoop in src/routing/reconciler/budget.rs
- [X] T029 [US3] Verify in-flight requests complete even when budget exhausted mid-execution (FR-013 requirement verification)

**Checkpoint**: Hard limit enforcement should work - budget resets automatically on first of month

---

## Phase 6: User Story 4 - Budget Visibility and Monitoring (Priority: P4)

**Goal**: Real-time budget status visibility through metrics and response headers for proactive monitoring

**Independent Test**: Generate various load patterns and verify metrics, dashboard, and response headers accurately reflect budget state

### Implementation for User Story 4

- [X] T030 [P] [US4] Add nexus_budget_spending_usd gauge recording in BudgetReconciliationLoop::reconcile() in src/routing/reconciler/budget.rs
- [X] T031 [P] [US4] Add nexus_budget_utilization_percent gauge recording in BudgetReconciliationLoop::reconcile() in src/routing/reconciler/budget.rs
- [X] T032 [P] [US4] Add nexus_budget_status gauge recording in BudgetReconciliationLoop::reconcile() in src/routing/reconciler/budget.rs
- [X] T033 [P] [US4] Add nexus_budget_limit_usd gauge recording on config load in src/routing/reconciler/budget.rs
- [X] T034 [US4] Add BudgetStats struct with fields from contracts/stats-api.json schema in src/metrics/types.rs
- [X] T035 [US4] Add optional budget field to StatsResponse struct in src/metrics/types.rs
- [X] T036 [US4] Populate StatsResponse.budget from BudgetMetrics in /v1/stats handler in src/metrics/handler.rs
- [X] T037 [US4] Add X-Nexus-Budget-Status response header when budget_status != Normal in src/api/completions.rs
- [X] T038 [US4] Add X-Nexus-Budget-Utilization response header when budget_status != Normal in src/api/completions.rs
- [X] T039 [US4] Add X-Nexus-Budget-Remaining response header when budget_status != Normal in src/api/completions.rs
- [X] T040 [US4] Add X-Nexus-Cost-Estimated response header for all requests in src/api/completions.rs

**Checkpoint**: All budget metrics should be visible via /v1/stats endpoint and Prometheus /metrics endpoint, response headers present when budget stressed

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

- [x] T041 [P] Update quickstart.md with actual testing results and example metrics output in specs/016-inference-budget-mgmt/quickstart.md
- [x] T042 [P] Add unit tests for TokenizerRegistry pattern matching (optional enhancement) in tests/unit/tokenizer_test.rs
- [x] T043 [P] Add integration test for soft limit routing shift (optional enhancement) in tests/integration/budget_reconciliation.rs
- [x] T044 [P] Add integration test for month rollover reset (optional enhancement) in tests/integration/budget_reconciliation.rs
- [x] T045 [P] Add contract test for Prometheus metric format validation (optional enhancement) in tests/contract/budget_metrics_test.rs
- [x] T046 Verify quickstart.md scenarios work as documented (all 4 scenarios: zero-config, soft limit, rollover, token accuracy)
- [X] T047 Add documentation for TokenizerRegistry usage in README or developer guide
- [X] T048 Review all error handling paths for graceful degradation (tokenizer failures fall back to heuristic)
- [X] T049 Performance validation: Verify token counting overhead <200ms P95 per SC-007
- [X] T050 Security review: Verify no sensitive data in metrics or logs

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-6)**: All depend on Foundational phase completion
  - User stories can then proceed in parallel (if staffed)
  - Or sequentially in priority order (P1 → P2 → P3 → P4)
- **Polish (Phase 7)**: Depends on all desired user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - Core budget enforcement with soft limits
- **User Story 2 (P2)**: Can start after Foundational (Phase 2) - Metrics for cost tracking (integrates with US1 but independently testable)
- **User Story 3 (P3)**: Can start after Foundational (Phase 2) - Hard limit protection (builds on US1 infrastructure)
- **User Story 4 (P4)**: Can start after US1 complete - Visibility features (requires budget enforcement to be working)

### Within Each User Story

- Phase 2 (Foundational) MUST complete before any US tasks
- US1 tasks T012-T018: Sequential (modifying same budget.rs file)
- US2 tasks T019-T021: Parallel metric recording tasks
- US3 tasks T024-T029: Sequential verification and enhancement
- US4 tasks T030-T033: Parallel gauge recordings
- US4 tasks T034-T040: Sequential API enhancements

### Parallel Opportunities

**Setup Phase (Phase 1)**:
- T002 and T003 can run in parallel (different files)

**Foundational Phase (Phase 2)**:
- T004 and T005 can run in parallel (same file, different sections)
- T006, T007, T008 can run in parallel (same file, different structs)

**User Story 2 (Phase 4)**:
- T019, T020, T021 can run in parallel (different metric types in different locations)

**User Story 4 (Phase 6)**:
- T030, T031, T032, T033 can run in parallel (different gauge metrics)
- T034 and T035 can run in parallel (different files: types.rs vs handler.rs)
- T037, T038, T039, T040 can run in parallel if adding to different response paths

**Polish Phase (Phase 7)**:
- T041, T042, T043, T044, T045 can all run in parallel (different files)

**Across User Stories** (once Foundational complete):
- If team capacity allows, US2 and US3 tasks can start after US1 T018 completes
- US4 should wait for US1 to be functional since it adds visibility to existing budget enforcement

---

## Parallel Example: Foundational Phase

```bash
# After T001, T002, T003 complete, launch foundational components in parallel:
Task T004: "Implement Tokenizer trait in src/agent/tokenizer.rs"
Task T005: "Implement TokenizerError enum in src/agent/tokenizer.rs"

# Then launch all tokenizer implementations together:
Task T006: "TiktokenExactTokenizer in src/agent/tokenizer.rs"
Task T007: "TiktokenApproximationTokenizer in src/agent/tokenizer.rs"
Task T008: "HeuristicTokenizer in src/agent/tokenizer.rs"
```

---

## Parallel Example: User Story 2

```bash
# Launch all metric recordings together (different metric types, different files):
Task T019: "nexus_cost_per_request_usd histogram in src/routing/reconciler/budget.rs"
Task T020: "nexus_token_count_duration_seconds histogram in src/agent/tokenizer.rs"
Task T021: "nexus_token_count_tier_total counter in src/agent/tokenizer.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T003)
2. Complete Phase 2: Foundational (T004-T011) - CRITICAL - blocks all stories
3. Complete Phase 3: User Story 1 (T012-T018)
4. **STOP and VALIDATE**: Test soft limit routing shift independently
5. Deploy/demo if ready - this is the MVP!

**Why this is MVP**: User Story 1 delivers the core value proposition - cost control without service disruption. It enables budget enforcement with graceful degradation, which is the foundation of the entire feature.

### Incremental Delivery

1. **Foundation** (Phase 1 + 2): Complete tokenizer infrastructure → Ready for budget enhancement
2. **MVP** (Phase 3): Add User Story 1 → Test soft limit behavior → Deploy/Demo
3. **Audit Grade** (Phase 4): Add User Story 2 → Verify metrics accuracy → Deploy/Demo
4. **Budget Ceiling** (Phase 5): Add User Story 3 → Test hard limit enforcement → Deploy/Demo
5. **Operational Visibility** (Phase 6): Add User Story 4 → Verify dashboard/headers → Deploy/Demo
6. Each story adds value without breaking previous stories

### Parallel Team Strategy

With multiple developers:

1. **Team completes Setup + Foundational together** (Phases 1-2)
2. Once Foundational is done:
   - **Developer A**: User Story 1 (T012-T018) - Core budget enforcement
   - **Developer B**: User Story 2 (T019-T023) - Metrics (can start after T018)
   - **Developer C**: User Story 3 (T024-T029) - Hard limits (can start after T018)
3. After US1, US2, US3 complete:
   - **Any Developer**: User Story 4 (T030-T040) - Visibility
4. **Everyone**: Polish phase together (T041-T050)

---

## Task Count Summary

- **Phase 1 (Setup)**: 3 tasks
- **Phase 2 (Foundational)**: 8 tasks (BLOCKING)
- **Phase 3 (US1 - Soft Limits)**: 7 tasks 🎯 MVP
- **Phase 4 (US2 - Precise Tracking)**: 5 tasks
- **Phase 5 (US3 - Hard Limits)**: 6 tasks
- **Phase 6 (US4 - Visibility)**: 11 tasks
- **Phase 7 (Polish)**: 10 tasks
- **TOTAL**: 50 tasks

### Tasks per User Story

- **US1 (P1)**: 7 tasks - Cost control with soft limits (MVP)
- **US2 (P2)**: 5 tasks - Precise cost tracking
- **US3 (P3)**: 6 tasks - Hard limit protection
- **US4 (P4)**: 11 tasks - Budget visibility and monitoring

### Parallel Opportunities Identified

- **Setup phase**: 2 tasks can run in parallel
- **Foundational phase**: 5 tasks can run in parallel
- **User Story 2**: 3 tasks can run in parallel
- **User Story 4**: 7 tasks can run in parallel
- **Polish phase**: 5 tasks can run in parallel
- **Total parallelizable**: ~22 tasks out of 50

---

## Notes

- **[P] tasks** = different files or different sections, no dependencies
- **[Story] label** maps task to specific user story for traceability
- **Each user story** should be independently completable and testable
- **Tests are optional** - not included in main implementation tasks since spec doesn't explicitly request TDD
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- **Avoid**: vague tasks, same file conflicts, cross-story dependencies that break independence
- **Existing infrastructure reused**: BudgetReconciler (823 lines, 18 tests), BudgetConfig, PricingTable all exist from Control Plane PR
- **Zero new dependencies**: All required crates (tiktoken-rs, metrics, dashmap, chrono) already in Cargo.toml

---

## Success Criteria Mapping

This task list ensures all Success Criteria (SC-001 to SC-010) from spec.md are met:

- **SC-001** (5% variance): T012-T014 (exact tokenizers for OpenAI/Anthropic)
- **SC-002** (40% cloud spending reduction): T015, T018 (soft limit routing shift)
- **SC-003** (60s transition time): T017, T027 (reconciliation loop wiring)
- **SC-004** (<1% sampling error): T019-T021, T030-T033 (Prometheus metrics)
- **SC-005** (zero terminations): T029 (in-flight request verification)
- **SC-006** (auto reset): T026-T028 (month rollover detection)
- **SC-007** (<200ms overhead): T022 (timing instrumentation), T049 (performance validation)
- **SC-008** (conservative estimates): T008 (1.15x multiplier heuristic)
- **SC-009** (persistence): DEFERRED - spec lists as future enhancement, v1 is in-memory only
- **SC-010** (100% header accuracy): T037-T040 (response header injection)

---

**Status**: ✅ Tasks ready for implementation  
**Next Command**: `/speckit.implement` to execute tasks in dependency order  
**MVP Scope**: Complete through Phase 3 (User Story 1) for minimum viable release
