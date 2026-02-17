---
description: "Implementation tasks for Quality Tracking & Backend Profiling feature"
status: "Complete"
---

# Tasks: Quality Tracking & Backend Profiling

**Feature**: F16 - Quality Tracking & Backend Profiling  
**Status**: ✅ **COMPLETE** - All tasks implemented  
**Branch**: `019-quality-tracking`  
**Input**: Design documents from `/specs/019-quality-tracking/`

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and quality tracking foundation

- [x] T001 Add quality tracking configuration types in src/config/quality.rs
- [x] T002 Define AgentQualityMetrics struct in src/agent/mod.rs
- [x] T003 [P] Add quality metrics to BackendStats in src/metrics/types.rs

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core quality tracking infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [x] T004 Implement QualityMetricsStore with rolling window storage in src/agent/quality.rs
- [x] T005 Implement RequestOutcome struct for request history in src/agent/quality.rs
- [x] T006 Add record_outcome method to QualityMetricsStore in src/agent/quality.rs
- [x] T007 Add get_metrics method to QualityMetricsStore in src/agent/quality.rs
- [x] T008 Add get_all_metrics method to QualityMetricsStore in src/agent/quality.rs
- [x] T009 Implement recompute_all with rolling window pruning in src/agent/quality.rs
- [x] T010 Add quality_store and quality_config to Router struct in src/routing/mod.rs
- [x] T011 Initialize QualityMetricsStore in Router::new in src/routing/mod.rs
- [x] T012 Add quality_store() accessor method to Router in src/routing/mod.rs

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Automatic Quality-Based Routing (Priority: P1) 🎯 MVP

**Goal**: Automatically route requests away from degraded backends based on error rate metrics

**Independent Test**: Can be fully tested by simulating backend failures and verifying requests are automatically routed to healthy backends. Delivers immediate value by improving request success rates.

### Implementation for User Story 1

- [x] T013 [US1] Create QualityReconciler struct in src/routing/reconciler/quality.rs
- [x] T014 [US1] Implement Reconciler trait for QualityReconciler in src/routing/reconciler/quality.rs
- [x] T015 [US1] Implement error rate filtering logic in QualityReconciler::reconcile in src/routing/reconciler/quality.rs
- [x] T016 [US1] Handle agents with no request history (default metrics) in src/routing/reconciler/quality.rs
- [x] T017 [US1] Add rejection reasons for excluded agents in src/routing/reconciler/quality.rs
- [x] T018 [US1] Integrate QualityReconciler into reconciler pipeline in src/routing/mod.rs
- [x] T019 [US1] Position QualityReconciler after TierReconciler in pipeline in src/routing/mod.rs
- [x] T020 [US1] Unit test: excludes_high_error_agents_above_threshold in src/routing/reconciler/quality.rs
- [x] T021 [US1] Unit test: preserves_healthy_agents_below_threshold in src/routing/reconciler/quality.rs
- [x] T022 [US1] Unit test: all_excluded_produces_rejection_reasons in src/routing/reconciler/quality.rs
- [x] T023 [US1] Unit test: fresh_start_no_history_all_pass in src/routing/reconciler/quality.rs

**Checkpoint**: At this point, User Story 1 should be fully functional and testable independently

---

## Phase 4: User Story 2 - Performance-Aware Request Distribution (Priority: P2)

**Goal**: Route requests to backends with lower time-to-first-token (TTFT) for better user experience

**Independent Test**: Can be tested by simulating backends with different TTFT profiles and verifying faster backends receive priority. Delivers measurable performance improvements.

### Implementation for User Story 2

- [x] T024 [US2] Add quality_store and quality_config to SchedulerReconciler in src/routing/reconciler/scheduler.rs
- [x] T025 [US2] Implement apply_ttft_penalty method in SchedulerReconciler in src/routing/reconciler/scheduler.rs
- [x] T026 [US2] Apply TTFT penalty to raw scores in greedy selection in src/routing/reconciler/scheduler.rs
- [x] T027 [US2] Apply TTFT penalty to raw scores in exhaustive selection in src/routing/reconciler/scheduler.rs
- [x] T028 [US2] Handle zero threshold (no penalty) case in src/routing/reconciler/scheduler.rs
- [x] T029 [US2] Calculate proportional penalty for TTFT exceeding threshold in src/routing/reconciler/scheduler.rs
- [x] T030 [US2] Pass quality_store and quality_config to SchedulerReconciler in src/routing/mod.rs
- [x] T031 [US2] Unit test: ttft_penalty_proportional_to_threshold_excess in src/routing/reconciler/scheduler.rs
- [x] T032 [US2] Unit test: no_penalty_when_below_threshold in src/routing/reconciler/scheduler.rs
- [x] T033 [US2] Unit test: larger_excess_gets_larger_penalty in src/routing/reconciler/scheduler.rs

**Checkpoint**: At this point, User Stories 1 AND 2 should both work independently

---

## Phase 5: User Story 3 - Quality Metrics Observability (Priority: P3)

**Goal**: Monitor quality metrics through Prometheus and API endpoints for operational visibility

**Independent Test**: Can be tested by verifying metrics are exposed correctly in Prometheus and /v1/stats endpoints. Delivers operational insights without affecting routing behavior.

### Implementation for User Story 3

- [x] T034 [P] [US3] Implement quality_reconciliation_loop function in src/agent/quality.rs
- [x] T035 [P] [US3] Add periodic recompute_all calls in reconciliation loop in src/agent/quality.rs
- [x] T036 [P] [US3] Update Prometheus gauges in reconciliation loop in src/agent/quality.rs
- [x] T037 [P] [US3] Export nexus_agent_error_rate gauge in src/agent/quality.rs
- [x] T038 [P] [US3] Export nexus_agent_success_rate_24h gauge in src/agent/quality.rs
- [x] T039 [P] [US3] Export nexus_agent_ttft_seconds histogram in src/agent/quality.rs
- [x] T040 [P] [US3] Add cancellation token support to reconciliation loop in src/agent/quality.rs
- [x] T041 [P] [US3] Add quality metrics to BackendStats struct in src/metrics/types.rs
- [x] T042 [P] [US3] Update compute_backend_stats to include quality metrics in src/metrics/handler.rs
- [x] T043 [P] [US3] Retrieve metrics from QualityMetricsStore in stats handler in src/metrics/handler.rs
- [x] T044 [P] [US3] Handle agents with no history (return None for metrics) in src/metrics/handler.rs
- [x] T045 [P] [US3] Pass quality_store to compute_backend_stats in src/metrics/handler.rs
- [x] T046 [US3] Spawn quality_reconciliation_loop in serve command in src/cli/serve.rs
- [x] T047 [US3] Pass cancellation token to quality loop in src/cli/serve.rs
- [x] T048 [US3] Register quality task handle for graceful shutdown in src/cli/serve.rs

**Checkpoint**: All user stories should now be independently functional with full observability

---

## Phase 6: User Story 4 - Configurable Quality Thresholds (Priority: P3)

**Goal**: Configure quality thresholds via TOML configuration for deployment flexibility

**Independent Test**: Can be tested by modifying TOML configuration values and verifying the system applies new thresholds. Delivers deployment flexibility.

### Implementation for User Story 4

- [x] T049 [P] [US4] Define QualityConfig struct with error_rate_threshold in src/config/quality.rs
- [x] T050 [P] [US4] Add ttft_penalty_threshold_ms to QualityConfig in src/config/quality.rs
- [x] T051 [P] [US4] Add metrics_interval_seconds to QualityConfig in src/config/quality.rs
- [x] T052 [P] [US4] Implement Default trait for QualityConfig in src/config/quality.rs
- [x] T053 [P] [US4] Add serde attributes for TOML deserialization in src/config/quality.rs
- [x] T054 [P] [US4] Add documentation with TOML example in src/config/quality.rs
- [x] T055 [US4] Use config().metrics_interval_seconds in reconciliation loop in src/agent/quality.rs
- [x] T056 [US4] Use error_rate_threshold from config in QualityReconciler in src/routing/reconciler/quality.rs
- [x] T057 [US4] Use ttft_penalty_threshold_ms from config in SchedulerReconciler in src/routing/reconciler/scheduler.rs

**Checkpoint**: All user stories with full configuration support

---

## Phase 7: Integration & Quality Recording

**Purpose**: Connect quality tracking to actual request processing

- [x] T058 [P] Record successful request outcomes in completions handler in src/api/completions.rs
- [x] T059 [P] Record failed request outcomes in completions handler in src/api/completions.rs
- [x] T060 [P] Calculate TTFT from request timing in src/api/completions.rs
- [x] T061 [P] Record outcomes for both streaming and non-streaming requests in src/api/completions.rs
- [x] T062 [P] Record outcomes before fallback attempts in src/api/completions.rs

**Checkpoint**: Quality tracking fully integrated with request lifecycle

---

## Phase 8: Testing & Validation

**Purpose**: Comprehensive test coverage for quality tracking system

### Unit Tests - QualityMetricsStore

- [x] T063 [P] Unit test: store_returns_default_for_unknown_agent in src/agent/quality.rs
- [x] T064 [P] Unit test: record_outcome_stores_data in src/agent/quality.rs
- [x] T065 [P] Unit test: recompute_handles_empty_store in src/agent/quality.rs
- [x] T066 [P] Unit test: success_rate_24h_computed in src/agent/quality.rs
- [x] T067 [P] Unit test: all_successes_give_zero_error_rate in src/agent/quality.rs
- [x] T068 [P] Unit test: last_failure_ts_tracked in src/agent/quality.rs
- [x] T069 [P] Unit test: get_all_metrics_returns_all in src/agent/quality.rs

### Unit Tests - QualityReconciler

- [x] T070 [P] Unit test: pass_through_preserves_all_candidates in src/routing/reconciler/quality.rs
- [x] T071 [P] Unit test: pass_through_with_empty_candidates in src/routing/reconciler/quality.rs
- [x] T072 [P] Unit test: name_returns_quality_reconciler in src/routing/reconciler/quality.rs
- [x] T073 [P] Unit test: default_creates_pass_through in src/routing/reconciler/quality.rs

**Checkpoint**: All components thoroughly tested

---

## Phase 9: Polish & Documentation

**Purpose**: Improvements that affect multiple user stories

- [x] T074 [P] Add inline documentation for QualityMetricsStore in src/agent/quality.rs
- [x] T075 [P] Add inline documentation for QualityReconciler in src/routing/reconciler/quality.rs
- [x] T076 [P] Add inline documentation for quality_reconciliation_loop in src/agent/quality.rs
- [x] T077 [P] Add inline documentation for QualityConfig in src/config/quality.rs
- [x] T078 [P] Add TOML configuration example in src/config/quality.rs
- [x] T079 [P] Add module-level documentation in src/agent/quality.rs
- [x] T080 [P] Document pipeline position in QualityReconciler in src/routing/reconciler/quality.rs
- [x] T081 [P] Document TTFT penalty behavior in SchedulerReconciler in src/routing/reconciler/scheduler.rs

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: ✅ Complete - No dependencies
- **Foundational (Phase 2)**: ✅ Complete - Depends on Setup (Phase 1)
- **User Story 1 (Phase 3)**: ✅ Complete - Depends on Foundational (Phase 2)
- **User Story 2 (Phase 4)**: ✅ Complete - Depends on Foundational (Phase 2)
- **User Story 3 (Phase 5)**: ✅ Complete - Depends on Foundational (Phase 2)
- **User Story 4 (Phase 6)**: ✅ Complete - Depends on Foundational (Phase 2)
- **Integration (Phase 7)**: ✅ Complete - Depends on all user stories
- **Testing (Phase 8)**: ✅ Complete - Depends on implementation phases
- **Polish (Phase 9)**: ✅ Complete - Depends on all phases

### User Story Dependencies

- **User Story 1 (P1)**: ✅ Independent - Core error rate filtering
- **User Story 2 (P2)**: ✅ Independent - TTFT-based routing (uses same store as US1)
- **User Story 3 (P3)**: ✅ Independent - Observability (reads from same store)
- **User Story 4 (P3)**: ✅ Independent - Configuration (enhances US1-US3)

### Within Each User Story

- Tests (if included) MUST be written and FAIL before implementation ✅
- Models before services ✅
- Services before endpoints ✅
- Core implementation before integration ✅
- Story complete before moving to next priority ✅

### Parallel Opportunities (Utilized)

- All Setup tasks marked [P] ran in parallel ✅
- All Foundational tasks ran sequentially (blocking dependencies)
- User stories 2-4 could have been started in parallel after Foundational phase ✅
- All US3 tasks marked [P] could run in parallel ✅
- All US4 tasks marked [P] could run in parallel ✅
- All test tasks marked [P] could run in parallel ✅
- All documentation tasks marked [P] could run in parallel ✅

---

## Implementation Strategy (Followed)

### MVP First (User Story 1 Only)

1. ✅ Complete Phase 1: Setup
2. ✅ Complete Phase 2: Foundational (CRITICAL - blocks all stories)
3. ✅ Complete Phase 3: User Story 1
4. ✅ **VALIDATED**: User Story 1 tested independently
5. ✅ Ready for deployment

### Incremental Delivery (Executed)

1. ✅ Complete Setup + Foundational → Foundation ready
2. ✅ Add User Story 1 → Test independently → MVP complete
3. ✅ Add User Story 2 → Test independently → Performance optimization
4. ✅ Add User Story 3 → Test independently → Observability added
5. ✅ Add User Story 4 → Test independently → Configuration flexibility
6. ✅ Each story added value without breaking previous stories

---

## Summary

**Total Tasks**: 81 tasks  
**Status**: ✅ All Complete

### Task Count per User Story

- **Setup (Phase 1)**: 3 tasks ✅
- **Foundational (Phase 2)**: 9 tasks ✅
- **User Story 1 - Automatic Quality-Based Routing (P1)**: 11 tasks ✅
- **User Story 2 - Performance-Aware Distribution (P2)**: 10 tasks ✅
- **User Story 3 - Quality Metrics Observability (P3)**: 15 tasks ✅
- **User Story 4 - Configurable Thresholds (P3)**: 9 tasks ✅
- **Integration (Phase 7)**: 5 tasks ✅
- **Testing (Phase 8)**: 11 tasks ✅
- **Polish (Phase 9)**: 8 tasks ✅

### Parallel Opportunities Identified

- Phase 1: 2 parallel tasks (T002, T003)
- Phase 3 US1: Tests could run in parallel with models
- Phase 4 US2: Tests could run in parallel
- Phase 5 US3: 13 parallel tasks (T034-T045)
- Phase 6 US4: 5 parallel tasks (T049-T053)
- Phase 7: 5 parallel tasks (T058-T062)
- Phase 8: 11 parallel test tasks (T063-T073)
- Phase 9: 8 parallel documentation tasks (T074-T081)

### Independent Test Criteria

✅ **User Story 1**: Simulate backend failures, verify automatic routing to healthy backends  
✅ **User Story 2**: Simulate different TTFT profiles, verify faster backends get priority  
✅ **User Story 3**: Verify metrics exposed in Prometheus and /v1/stats endpoints  
✅ **User Story 4**: Modify TOML config, verify new thresholds applied  

### MVP Scope (Delivered)

✅ User Story 1 (P1) - Automatic Quality-Based Routing

This MVP delivers immediate value by automatically routing requests away from degraded backends based on error rate metrics, improving request success rates without any manual intervention.

---

## Key Files Modified

### Core Implementation
- `src/agent/quality.rs` - QualityMetricsStore, rolling window tracking, reconciliation loop
- `src/agent/mod.rs` - AgentQualityMetrics struct definition
- `src/config/quality.rs` - QualityConfig with thresholds
- `src/routing/reconciler/quality.rs` - QualityReconciler for error rate filtering
- `src/routing/reconciler/scheduler.rs` - TTFT penalty integration
- `src/routing/mod.rs` - Quality store integration, reconciler pipeline

### Observability
- `src/metrics/types.rs` - BackendStats with quality metrics
- `src/metrics/handler.rs` - /v1/stats endpoint with quality data
- `src/metrics/mod.rs` - Prometheus gauge registration

### Integration
- `src/api/completions.rs` - Outcome recording for requests
- `src/cli/serve.rs` - Quality reconciliation loop startup

---

## Format Validation

✅ **ALL tasks follow the checklist format**:
- Checkbox: `[x]` for completed tasks
- Task ID: Sequential (T001-T081)
- [P] marker: Present on parallelizable tasks
- [Story] label: Present on all user story tasks (US1, US2, US3, US4)
- Description: Clear action with exact file path

---

## Notes

- Feature fully implemented and tested ✅
- All unit tests passing ✅
- Integrated with existing routing pipeline ✅
- Prometheus metrics exposed ✅
- /v1/stats endpoint includes quality data ✅
- Background reconciliation loop running ✅
- Configuration via TOML supported ✅
- Documentation complete ✅
- Ready for production use ✅
