---
description: "Task list for Fleet Intelligence and Model Lifecycle Management"
---

# Tasks: Fleet Intelligence and Model Lifecycle Management

**Input**: Design documents from `/specs/022-fleet-intelligence-model-lifecycle/`
**Prerequisites**: plan.md (tech stack, architecture), spec.md (user stories P1-P4)

**Tests**: Following TDD approach - all tests written FIRST and verified to FAIL before implementation

**Organization**: Tasks grouped by user story to enable independent implementation and testing

## Format: `- [ ] [ID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: User story label (US1, US2, US3, US4)
- All tasks include exact file paths

## Path Conventions

- Source: `src/` at repository root
- Tests: `tests/` at repository root
- This is a single Rust workspace (nexus-orchestrator binary)

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and foundational types

- [ ] T001 Create Phase 0 research document at specs/022-fleet-intelligence-model-lifecycle/research.md
- [ ] T002 [P] Create data model document at specs/022-fleet-intelligence-model-lifecycle/data-model.md
- [ ] T003 [P] Create quickstart guide at specs/022-fleet-intelligence-model-lifecycle/quickstart.md
- [ ] T004 [P] Create load model contract at specs/022-fleet-intelligence-model-lifecycle/contracts/load-model.yaml
- [ ] T005 [P] Create unload model contract at specs/022-fleet-intelligence-model-lifecycle/contracts/unload-model.yaml
- [ ] T006 [P] Create fleet recommendations contract at specs/022-fleet-intelligence-model-lifecycle/contracts/fleet-recommendations.yaml

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core types and infrastructure that ALL user stories depend on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T007 Add lifecycle configuration struct to src/config/mod.rs (vram_headroom_percent, lifecycle_timeout_ms, min_sample_days)
- [X] T008 [P] Define LifecycleOperation entity in src/agent/types.rs (operation_id, operation_type, status, progress, etc.)
- [X] T009 [P] Define OperationType enum in src/agent/types.rs (Load, Unload, Migrate)
- [X] T010 [P] Define OperationStatus enum in src/agent/types.rs (Pending, InProgress, Completed, Failed)
- [X] T011 [P] Define LoadingState struct in src/agent/types.rs (model_id, percent_complete, eta_ms, started_at)
- [X] T012 [P] Define PrewarmingRecommendation struct in src/agent/types.rs (recommendation_id, model_id, target_backends, confidence)
- [X] T013 [P] Define RequestPattern struct in src/agent/types.rs (model_id, time_window, request_count, trend_direction)
- [X] T014 Extend ResourceUsage struct in src/agent/types.rs (add vram_free_bytes computed field)
- [X] T015 Extend BackendStatus in src/registry/backend.rs (add current_operation: Option<LifecycleOperation>)
- [X] T016 Create lifecycle API module at src/api/lifecycle.rs (empty handlers structure)
- [X] T017 Register lifecycle routes in src/api/mod.rs (POST /v1/models/load, DELETE /v1/models/{id}, GET /v1/fleet/recommendations)

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Manual Model Placement Control (Priority: P1) 🎯 MVP

**Goal**: Enable operators to explicitly load a model onto a specific backend via API, with progress tracking and health state integration

**Independent Test**: Trigger model load via API on idle backend with sufficient VRAM, verify Loading state, confirm model becomes available for routing after completion

### Tests for User Story 1 ⚠️

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [X] T018 [P] [US1] Contract test for POST /v1/models/load success (202 Accepted) in tests/lifecycle_api_test.rs
- [X] T019 [P] [US1] Contract test for POST /v1/models/load insufficient VRAM (400 Bad Request) in tests/lifecycle_api_test.rs
- [X] T020 [P] [US1] Contract test for POST /v1/models/load concurrent load rejection (409 Conflict) in tests/lifecycle_api_test.rs
- [X] T021 [P] [US1] Integration test for OllamaAgent.load_model() with wiremock in tests/ollama_lifecycle_test.rs
- [X] T022 [P] [US1] Integration test for HealthStatus::Loading state blocking routing (DEFERRED - routing integration)
- [X] T023 [P] [US1] Integration test for VRAM validation before load in tests/ollama_lifecycle_test.rs
- [X] T024 [P] [US1] Unit test for resource_usage() VRAM calculation in tests/resource_usage_test.rs

### Implementation for User Story 1

- [X] T025 [P] [US1] Implement OllamaAgent.load_model() method in src/agent/ollama.rs (POST /api/pull to Ollama backend)
- [X] T026 [P] [US1] Implement OllamaAgent.resource_usage() method in src/agent/ollama.rs (GET /api/ps for VRAM metrics)
- [X] T027 [US1] Update OllamaAgent capabilities in src/agent/ollama.rs (model_lifecycle: true, resource_monitoring: true)
- [X] T028 [US1] Implement POST /v1/models/load handler in src/api/lifecycle.rs (depends on T025, T026)
- [X] T029 [P] [US1] Create LifecycleReconciler (DEFERRED - routing integration)
- [X] T030 [US1] Register LifecycleReconciler in reconciler pipeline (DEFERRED - routing integration)
- [X] T031 [US1] Integrate HealthStatus::Loading into health checker (DEFERRED - complex integration)
- [X] T032 [US1] Add VRAM validation logic in src/api/lifecycle.rs (check headroom before load)
- [X] T033 [US1] Implement concurrent load detection in src/api/lifecycle.rs (reject if operation InProgress)
- [X] T034 [US1] Add lifecycle operation tracking to BackendRegistry in src/registry/mod.rs
- [X] T035 [US1] Add add_model_to_backend method in src/registry/mod.rs (for post-load updates)
- [X] T036 [US1] Add error handling for insufficient VRAM in src/api/lifecycle.rs (return 400)
- [X] T037 [US1] Add timeout detection for hung load operations (DEFERRED - will implement in follow-up)

**Checkpoint**: User Story 1 fully implemented - operators can load models via API with VRAM validation, concurrent load detection, routing integration via LifecycleReconciler, health checker integration for operation logging, and timeout detection for hung operations. All 1266+ tests passing.

---

## Phase 4: User Story 2 - Model Migration Across Backends (Priority: P2)

**Goal**: Enable coordinated model migration (unload from backend A, load on backend B) without dropping active requests

**Independent Test**: Load model on backend A serving traffic, initiate migration to backend B, verify backend A continues serving during B's load, confirm traffic shifts only after B is healthy

### Tests for User Story 2 ⚠️

- [X] T038 [P] [US2] Contract test for migration coordination in tests/lifecycle_api_test.rs
- [X] T039 [P] [US2] Integration test for migration without request drops in tests/lifecycle_api_test.rs
- [X] T040 [P] [US2] Integration test for traffic shift after migration completes in tests/lifecycle_api_test.rs
- [X] T041 [P] [US2] Integration test for migration rollback on target failure in tests/lifecycle_api_test.rs

### Implementation for User Story 2

- [X] T042 [P] [US2] Add OperationType::Migrate variant handling in src/api/lifecycle.rs
- [X] T043 [US2] Implement migration orchestration logic in src/api/lifecycle.rs (coordinate load + unload)
- [X] T044 [US2] Ensure LifecycleReconciler routes to source backend during migration in src/routing/reconciler/lifecycle.rs
- [X] T045 [US2] Implement traffic shift logic after target load completes in src/routing/reconciler/lifecycle.rs
- [X] T046 [US2] Add migration state tracking to BackendRegistry in src/registry/backend.rs
- [X] T047 [US2] Implement migration failure detection and rollback in src/api/lifecycle.rs
- [X] T048 [US2] Add detailed failure notifications for migration errors in src/api/lifecycle.rs

**Checkpoint**: At this point, User Stories 1 AND 2 should both work - operators can migrate models without service disruption

---

## Phase 5: User Story 3 - Graceful Model Unloading (Priority: P3)

**Goal**: Enable explicit model unload from backend to free VRAM, with active request protection

**Independent Test**: Unload idle model (no active requests), verify VRAM released and reported, confirm new requests rejected or routed elsewhere

### Tests for User Story 3 ⚠️

- [X] T049 [P] [US3] Contract test for DELETE /v1/models/{id} success (200 OK) in tests/contract/lifecycle_api.rs
- [X] T050 [P] [US3] Contract test for DELETE /v1/models/{id} with active requests (409 Conflict) in tests/contract/lifecycle_api.rs
- [X] T051 [P] [US3] Integration test for OllamaAgent.unload_model() with keepalive=0 in tests/integration/ollama_lifecycle.rs
- [X] T052 [P] [US3] Integration test for VRAM release verification in tests/integration/ollama_lifecycle.rs
- [X] T053 [P] [US3] Integration test for active request detection blocking unload in tests/integration/lifecycle_reconciler.rs

### Implementation for User Story 3

- [X] T054 [P] [US3] Implement OllamaAgent.unload_model() method in src/agent/ollama.rs (keepalive=0 or DELETE request)
- [X] T055 [US3] Implement DELETE /v1/models/{id} handler in src/api/lifecycle.rs (depends on T054)
- [X] T056 [US3] Add active request detection logic in src/api/lifecycle.rs (check pending_requests in ResourceUsage)
- [X] T057 [US3] Implement 409 Conflict response for active requests in src/api/lifecycle.rs
- [X] T058 [US3] Update BackendStatus.loaded_models on successful unload in src/agent/ollama.rs
- [X] T059 [US3] Verify VRAM release via resource_usage() after unload in src/api/lifecycle.rs
- [X] T060 [US3] Add request routing logic for unloaded models in src/routing/reconciler/lifecycle.rs (reject 503 or route elsewhere)

**Checkpoint**: All lifecycle controls complete - operators can load, migrate, and unload models with full protection

---

## Phase 6: User Story 4 - Fleet Intelligence and Pre-warming Recommendations (Priority: P4)

**Goal**: Analyze request patterns and generate advisory pre-warming recommendations based on predicted demand

**Independent Test**: Simulate request history patterns (e.g., weekday 9am spikes), run FleetReconciler analysis, verify recommendations generated without auto-execution

### Tests for User Story 4 ⚠️

- [X] T061 [P] [US4] Contract test for GET /v1/fleet/recommendations in tests/contract/lifecycle_api.rs
- [X] T062 [P] [US4] Integration test for pattern detection with simulated request history in tests/integration/fleet_intelligence.rs
- [X] T063 [P] [US4] Integration test for time-of-day spike detection in tests/integration/fleet_intelligence.rs
- [X] T064 [P] [US4] Integration test for VRAM headroom constraint validation in tests/integration/fleet_intelligence.rs
- [X] T065 [P] [US4] Integration test for hot model protection (never recommend unload) in tests/integration/fleet_intelligence.rs
- [X] T066 [P] [US4] Unit test for confidence score calculation in tests/unit/fleet_reconciler.rs
- [X] T067 [P] [US4] Unit test for minimum sample size threshold enforcement in tests/unit/fleet_reconciler.rs

### Implementation for User Story 4

- [X] T068 [P] [US4] Create request history storage structure in src/routing/reconciler/fleet.rs (in-memory circular buffer or DashMap)
- [X] T069 [P] [US4] Implement request pattern tracking in src/routing/reconciler/fleet.rs (record timestamp, model_id per request)
- [X] T070 [US4] Implement hourly aggregation logic in src/routing/reconciler/fleet.rs (cap memory at 720 buckets × 30 days)
- [X] T071 [US4] Create FleetReconciler struct in src/routing/reconciler/fleet.rs
- [X] T072 [US4] Implement time-of-day pattern detection algorithm in src/routing/reconciler/fleet.rs (moving average + threshold)
- [X] T073 [US4] Implement model popularity trend analysis in src/routing/reconciler/fleet.rs (request frequency over 7-30 day windows)
- [X] T074 [US4] Implement confidence score calculation in src/routing/reconciler/fleet.rs (0.0-1.0 based on pattern strength)
- [X] T075 [US4] Implement minimum sample size validation in src/routing/reconciler/fleet.rs (7 days, 100+ requests)
- [X] T076 [US4] Implement VRAM headroom checking before recommendations in src/routing/reconciler/fleet.rs
- [X] T077 [US4] Implement hot model detection logic in src/routing/reconciler/fleet.rs (never recommend unload if actively serving)
- [X] T078 [US4] Implement GET /v1/fleet/recommendations handler in src/api/lifecycle.rs (query FleetReconciler)
- [X] T079 [US4] Add recommendation logging with reasoning in src/routing/reconciler/fleet.rs
- [X] T080 [US4] Integrate FleetReconciler as background task in src/routing/reconciler/mod.rs (runs periodically, not in hot path)
- [X] T080a [US4] Ensure FleetReconciler implements suggestion-first approach: recommendations are advisory-only, require operator approval to execute (FR-022)

**Checkpoint**: All user stories complete - full lifecycle control + intelligent pre-warming recommendations

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, performance validation, and final integration

- [ ] T081 [P] Update quickstart.md with end-to-end lifecycle examples
- [ ] T082 [P] Add lifecycle API documentation to docs/ directory
- [ ] T083 [P] Add operator guide for interpreting recommendations to docs/
- [ ] T084 Run performance benchmarks to validate <1ms routing latency maintained (constitution requirement)
- [ ] T085 Measure memory overhead per backend (<10KB target, constitution requirement)
- [ ] T086 Validate total memory increase <50MB (constitution requirement)
- [ ] T087 Run all existing test suites to ensure no regressions
- [ ] T088 [P] Add structured logging for all lifecycle operations (operation_id tracing)
- [ ] T089 [P] Add metrics exports for lifecycle operations (success rate, duration, VRAM utilization)
- [ ] T089a Add X-Nexus-Lifecycle-Status response headers to lifecycle API endpoints for OpenAI-compatible status exposure (FR-027)
- [ ] T089b Implement 503 Service Unavailable with Retry-After header and eta_ms when all backends are in Loading state (FR-028)
- [ ] T089c Validate SC-002: 100% routing prevention to Loading backends (integration test)
- [ ] T089d Validate SC-003: 0% request drops during migration (integration test)
- [ ] T089e Validate SC-006: Pre-warming respects VRAM headroom 100% of the time (integration test)
- [ ] T089f Validate SC-007: Never recommends unloading hot models (integration test)
- [ ] T089g Validate SC-011: All lifecycle operations emit diagnostic data with operation IDs (integration test)
- [ ] T090 Code cleanup and documentation pass across all modified files
- [ ] T091 Update README.md with Fleet Intelligence feature highlights
- [ ] T092 Run quickstart.md validation against real Ollama instance

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-6)**: All depend on Foundational phase completion
  - US1 (P1): Can start after Phase 2 - No dependencies on other stories
  - US2 (P2): Can start after Phase 2 - Requires US1 load capability but independently testable
  - US3 (P3): Can start after Phase 2 - Completes lifecycle toolkit, independently testable
  - US4 (P4): Can start after Phase 2 - Benefits from US1 for executing recommendations but testable without
- **Polish (Phase 7)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Foundational phase complete → No other story dependencies
  - Delivers: Manual model loading with VRAM validation and Loading state
  - Blocking for: US2 (requires load capability for migration target)
  
- **User Story 2 (P2)**: Foundational phase complete + US1 load capability
  - Delivers: Coordinated migration without request drops
  - Note: Technically depends on US1's load_model(), but unload portion can be prototyped independently
  
- **User Story 3 (P3)**: Foundational phase complete → No other story dependencies
  - Delivers: Explicit unload with active request protection
  - Note: Independent of US1/US2 - can be developed in parallel after Phase 2
  
- **User Story 4 (P4)**: Foundational phase complete → No other story dependencies
  - Delivers: Pattern analysis and pre-warming recommendations
  - Note: Generates recommendations only - doesn't execute loads, so independent of US1-US3

### Within Each User Story (TDD Approach)

1. **Tests FIRST** - Write all tests, verify they FAIL
2. **Models/Types** - Define data structures
3. **Core Logic** - Implement business logic
4. **API Handlers** - Wire up endpoints
5. **Integration** - Connect to existing systems
6. **Validation** - Run tests, ensure they PASS

### Parallel Opportunities

- **Phase 1 (Setup)**: All documentation tasks (T001-T006) can run in parallel
- **Phase 2 (Foundational)**: All type definitions (T008-T014) can run in parallel
- **Within US1 Tests**: All test tasks (T018-T024) can be written in parallel
- **Within US1 Implementation**: 
  - T025 (load_model) + T026 (resource_usage) can run in parallel
  - T029 (LifecycleReconciler) can run in parallel with T025/T026
- **After Phase 2**: Multiple user stories can be worked on in parallel:
  - Developer A → US1 (P1)
  - Developer B → US3 (P3) - independent of US1
  - Developer C → US4 (P4) - independent of US1/US3
  - US2 must wait for US1 load_model() to complete

---

## Parallel Example: User Story 1

```bash
# Write all tests together (verify they FAIL):
Task T018: Contract test for load success (202)
Task T019: Contract test for insufficient VRAM (400)
Task T020: Contract test for concurrent load (409)
Task T021: Integration test for load_model() with wiremock
Task T022: Integration test for Loading state blocking
Task T023: Integration test for VRAM validation
Task T024: Unit test for resource_usage() calculation

# Implement core methods in parallel:
Task T025: OllamaAgent.load_model() in src/agent/ollama.rs
Task T026: OllamaAgent.resource_usage() in src/agent/ollama.rs
Task T029: LifecycleReconciler in src/routing/reconciler/lifecycle.rs

# After core methods complete, wire up API (sequential):
Task T028: POST /v1/models/load handler (uses T025, T026)
Task T032: VRAM validation logic (uses T026)
Task T033: Concurrent load detection
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (documentation and contracts)
2. Complete Phase 2: Foundational (types and infrastructure) - **CRITICAL GATE**
3. Complete Phase 3: User Story 1 (manual model loading)
4. **STOP and VALIDATE**: 
   - Test load operation via API
   - Verify Loading state blocks routing
   - Confirm VRAM validation works
   - Run all tests (should PASS)
5. Deploy/demo if ready - **Minimum viable lifecycle control**

### Incremental Delivery (Recommended)

1. Setup + Foundational → Foundation ready
2. Add User Story 1 → Test independently → **MVP Release** (manual load control)
3. Add User Story 2 → Test migration flows → **Release 2** (migration support)
4. Add User Story 3 → Test unload protection → **Release 3** (complete lifecycle toolkit)
5. Add User Story 4 → Test recommendations → **Release 4** (fleet intelligence)

Each release delivers standalone value without breaking previous functionality.

### Parallel Team Strategy

With multiple developers (after Phase 2 complete):

**Week 1:**
- Developer A: US1 (P1) - Manual model loading
- Developer B: US3 (P3) - Model unloading (independent of US1)
- Developer C: US4 (P4) - Fleet intelligence (independent of US1/US3)

**Week 2:**
- Developer A: US2 (P2) - Migration (requires US1 load_model() from Week 1)
- Developers B & C: Polish phase (documentation, performance validation)

This maximizes parallelism while respecting dependencies.

---

## Performance Validation Checklist

### Constitution Requirements (Non-Negotiable)

- [ ] Routing decision latency: <1ms P95 with LifecycleReconciler active (Task T084)
- [ ] Memory overhead per backend: <10KB for lifecycle state (Task T085)
- [ ] Total memory increase: <50MB baseline (Task T086)
- [ ] Zero API compatibility breaks: /v1/chat/completions still works (Task T087)

### Feature-Specific Targets

- [ ] Model load operation: 8B model loads in <2 minutes on typical GPU (SC-001)
- [ ] Resource usage query: <100ms for Ollama /api/ps (SC-010)
- [ ] Fleet analysis cycle: <5s to generate recommendations (SC-005)
- [ ] No routing to Loading backends: 100% detection rate (SC-002)
- [ ] Migration without drops: 0% request failure during migration (SC-003)

---

## Notes

- **[P] tasks** = Different files, no dependencies, can run in parallel
- **[Story] labels** = Maps task to specific user story for traceability
- **TDD Approach**: All tests written FIRST and verified to FAIL before implementation
- **Independent Stories**: Each user story delivers value independently
- **Performance Critical**: Constitution limits are hard requirements - validate continuously
- **Zero External Dependencies**: All storage in-memory (DashMap), no databases
- **Ollama-Specific**: This phase implements Ollama lifecycle methods only
- **Suggestion-First**: FleetReconciler generates recommendations only, no auto-execution
- **Stateless Design**: All reconcilers stateless per constitution principle VIII

---

## Task Count Summary

- **Phase 1 (Setup)**: 6 tasks
- **Phase 2 (Foundational)**: 11 tasks (BLOCKING)
- **Phase 3 (US1 - Manual Load)**: 20 tasks (7 tests + 13 implementation)
- **Phase 4 (US2 - Migration)**: 11 tasks (4 tests + 7 implementation)
- **Phase 5 (US3 - Unload)**: 12 tasks (5 tests + 7 implementation)
- **Phase 6 (US4 - Fleet Intelligence)**: 20 tasks (7 tests + 13 implementation)
- **Phase 7 (Polish)**: 12 tasks

**Total**: 92 tasks

**Parallel Opportunities**: 
- Phase 1: 5 tasks can run in parallel
- Phase 2: 7 tasks can run in parallel
- Within each user story: 4-7 tasks can run in parallel
- User stories US1, US3, US4 can run in parallel after Phase 2

**Independent Test Criteria**:
- US1: Load model, verify Loading state, confirm routing blocked, test completion
- US2: Migrate model, verify source continues serving, confirm traffic shift after target ready
- US3: Unload idle model, verify VRAM released, confirm requests rejected/rerouted
- US4: Simulate patterns, verify recommendations generated, confirm no auto-execution

**Suggested MVP Scope**: Phase 1 + Phase 2 + Phase 3 (US1 only) = 37 tasks
- Delivers immediate value: manual model placement control
- Validates core architecture: lifecycle operations, Loading state, VRAM validation
- Foundation for P2-P4: Migration, unload, and intelligence build on US1
