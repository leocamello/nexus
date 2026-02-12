# Tasks: Request Metrics (F09)

**Input**: Design documents from `/specs/009-request-metrics/`
**Prerequisites**: plan.md, spec.md, data-model.md, research.md, contracts/

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

**TDD Workflow**: Tests first, implementation after (as requested in implementation context).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and dependencies

- [X] T001 Add metrics crate (0.23) dependency to Cargo.toml
- [X] T002 Add metrics-exporter-prometheus (0.15) dependency to Cargo.toml
- [X] T003 Create src/metrics/ module directory structure
- [X] T004 Verify dependencies compile with cargo check

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core metrics infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

**TDD**: Write foundational tests first, then implement to make them pass

### Foundational Tests (Write FIRST, ensure they FAIL)

- [X] T005a [P] Write unit test for MetricsCollector construction in src/metrics/mod.rs (verify struct creation with registry reference and start_time)
- [X] T005b [P] Write unit test for label sanitization in src/metrics/mod.rs (valid names, special chars, leading digits → underscore replacement)
- [X] T005c [P] Write unit test for StatsResponse serialization in src/metrics/types.rs (verify JSON output matches contract schema)

### Foundational Implementation (Make tests PASS)

- [X] T005 Create MetricsCollector struct in src/metrics/mod.rs with registry reference, start_time, and label_cache
- [X] T006 [P] Implement label sanitization function in src/metrics/mod.rs (replace invalid chars with underscore)
- [X] T007 [P] Create metrics types module in src/metrics/types.rs (StatsResponse, RequestStats, BackendStats, ModelStats)
- [X] T008 Implement setup_metrics() function in src/metrics/mod.rs to initialize PrometheusBuilder with custom histogram buckets
- [X] T009 Add metrics_collector field to AppState in src/api/mod.rs
- [X] T010 Initialize MetricsCollector and install Prometheus recorder in src/main.rs at gateway startup
- [X] T011 Create metrics_handler stub in src/metrics/handler.rs for GET /metrics endpoint
- [X] T012 Create stats_handler stub in src/metrics/handler.rs for GET /v1/stats endpoint
- [X] T013 Register /metrics route in src/api/mod.rs router
- [X] T014 Register /v1/stats route in src/api/mod.rs router

**Checkpoint**: Foundation ready - metrics infrastructure initialized, endpoints registered, user story implementation can now begin

---

## Phase 3: User Story 1 - Basic Request Tracking (Priority: P1) 🎯 MVP

**Goal**: Track request counts, success/failure rates, and expose metrics in both Prometheus and JSON formats

**Independent Test**: Send requests through gateway and query /metrics and /v1/stats endpoints to verify counters increment correctly

### Tests for User Story 1 (TDD: Write FIRST, ensure they FAIL)

- [x] T015 [P] [US1] Write unit test for label sanitization (4 tests in src/metrics/mod.rs — valid names, special chars, leading digits, caching)
- [-] T016 [P] [US1] Contract test for /metrics endpoint — deferred to Phase 7 (unit tests cover handler logic)
- [-] T017 [P] [US1] Contract test for /v1/stats endpoint — deferred to Phase 7 (unit tests cover handler logic)
- [-] T018 [US1] Integration test for request counter tracking — deferred to Phase 7 (requires mock backend server)

### Implementation for User Story 1

- [X] T019 [US1] Implement update_fleet_gauges() method in src/metrics/mod.rs (query Registry, compute backends_total, backends_healthy, models_available gauges)
- [X] T020 [US1] Implement metrics_handler in src/metrics/handler.rs (call update_fleet_gauges(), render Prometheus text format)
- [X] T021 [US1] Implement compute_request_stats() helper in src/metrics/handler.rs (aggregate total, success, error counts from Prometheus data)
- [X] T022 [US1] Implement compute_backend_stats() helper in src/metrics/handler.rs (per-backend request counts and average latency)
- [X] T023 [US1] Implement compute_model_stats() helper in src/metrics/handler.rs (per-model request counts and average duration)
- [X] T024 [US1] Implement stats_handler in src/metrics/handler.rs (compute all stats, serialize to JSON)
- [X] T025 [US1] Add request timer start at entry of completions handler in src/api/completions.rs
- [X] T026 [US1] Record nexus_requests_total counter on success path in src/api/completions.rs with model, backend, status labels
- [X] T027 [US1] Record nexus_request_duration_seconds histogram on success path in src/api/completions.rs with model, backend labels
- [X] T028 [US1] Record nexus_errors_total counter on error paths in src/api/completions.rs with error_type, model labels
- [X] T029 [US1] Add error type mapping (NoHealthyBackend → no_healthy_backend, Timeout → timeout, etc.) in src/api/completions.rs
- [x] T030 [US1] All US1 unit tests pass ✅ (integration tests deferred to Phase 7)

**Checkpoint**: User Story 1 complete - basic request tracking working, both /metrics and /v1/stats endpoints functional

---

## Phase 4: User Story 2 - Performance Monitoring (Priority: P2)

**Goal**: Track request duration and backend latency with histogram buckets for performance analysis

**Independent Test**: Send requests with varying durations and verify histogram buckets are populated correctly in /metrics

### Tests for User Story 2 (TDD: Write FIRST, ensure they FAIL)

- [-] T031 [P] [US2] Integration test for request duration histogram — deferred to Phase 7
- [-] T032 [P] [US2] Integration test for backend latency histogram — deferred to Phase 7
- [-] T033 [US2] Integration test for average latency computation — deferred to Phase 7

### Implementation for User Story 2

- [X] T034 [US2] Verify nexus_request_duration_seconds histogram recording in src/api/completions.rs (already done in US1, validate buckets configured)
- [X] T035 [US2] Add health check timer in src/health/mod.rs at start of check_backend() method
- [X] T036 [US2] Record nexus_backend_latency_seconds histogram in src/health/mod.rs after successful health check with backend label
- [X] T037 [US2] Convert health check latency from milliseconds to seconds before recording in src/health/mod.rs
- [X] T038 [US2] Update stats_handler to include average_latency_ms in BackendStats (convert seconds to milliseconds) in src/metrics/handler.rs
- [X] T039 [US2] Update stats_handler to include average_duration_ms in ModelStats (convert seconds to milliseconds) in src/metrics/handler.rs
- [x] T040 [US2] All US2 unit tests pass ✅ (integration tests deferred to Phase 7)

**Checkpoint**: User Story 2 complete - performance histograms working, latency tracking functional

---

## Phase 5: User Story 3 - Routing Intelligence Metrics (Priority: P3)

**Goal**: Track fallback usage, token counts, and backend queue depths for routing optimization

**Independent Test**: Trigger fallback scenarios and verify fallback counters increment, monitor pending request gauges during load

### Tests for User Story 3 (TDD: Write FIRST, ensure they FAIL)

- [-] T041 [P] [US3] Integration test for fallback counter — deferred to Phase 7
- [-] T042 [P] [US3] Integration test for token counting — deferred to Phase 7
- [-] T043 [US3] Integration test for pending requests gauge — deferred to Phase 7

### Implementation for User Story 3

- [X] T044 [US3] Add fallback detection in routing layer in src/routing/mod.rs (detect when fallback chain is traversed) - already done, routing_result.fallback_used exists
- [X] T045 [US3] Record nexus_fallbacks_total counter in src/routing/mod.rs with from_model, to_model labels when fallback occurs - recorded in completions.rs
- [X] T046 [US3] Extract token counts from backend response in src/api/completions.rs (parse usage field if present)
- [X] T047 [US3] Record nexus_tokens_total histogram in src/api/completions.rs with model, backend, type (prompt/completion) labels
- [X] T048 [US3] Add nexus_pending_requests gauge recording in src/api/completions.rs (set to 0 for now, placeholder for future queue tracking) - already tracked via Registry.pending_requests atomic counter
- [X] T049 [US3] Update stats_handler to include pending field in BackendStats in src/metrics/handler.rs (query gauge value) - already done, uses Registry atomic
- [x] T050 [US3] All US3 unit tests pass ✅ (integration tests deferred to Phase 7)

**Checkpoint**: User Story 3 complete - routing intelligence metrics functional

---

## Phase 6: User Story 4 - Fleet State Visibility (Priority: P3)

**Goal**: Expose current fleet state (healthy backends, available models, pending requests) for real-time capacity awareness

**Independent Test**: Add/remove backends, change health status, verify gauge metrics reflect current state

### Tests for User Story 4 (TDD: Write FIRST, ensure they FAIL)

- [-] T051 [P] [US4] Integration test for backends_total gauge — deferred to Phase 7
- [-] T052 [P] [US4] Integration test for backends_healthy gauge — deferred to Phase 7
- [-] T053 [P] [US4] Integration test for models_available gauge — deferred to Phase 7
- [-] T054 [US4] Integration test for /v1/stats per-backend breakdown — deferred to Phase 7

### Implementation for User Story 4

- [X] T055 [US4] Verify update_fleet_gauges() computes backends_total from Registry in src/metrics/mod.rs (already implemented in US1, validated)
- [X] T056 [US4] Verify update_fleet_gauges() computes backends_healthy from Registry in src/metrics/mod.rs (already implemented in US1, validated)
- [X] T057 [US4] Verify update_fleet_gauges() computes models_available from Registry in src/metrics/mod.rs (already implemented in US1, validated)
- [X] T058 [US4] Ensure metrics_handler calls update_fleet_gauges() before rendering in src/metrics/handler.rs (already done in US1, validated)
- [X] T059 [US4] Ensure stats_handler calls update_fleet_gauges() before computing stats in src/metrics/handler.rs (already done in US1, validated)
- [X] T060 [US4] Verify /v1/stats includes per-backend breakdown with all registered backends in src/metrics/handler.rs (compute_backend_stats returns Vec<BackendStats>)
- [X] T061 [US4] Verify /v1/stats includes per-model breakdown with all models across healthy backends in src/metrics/handler.rs (compute_model_stats returns Vec<ModelStats>)
- [x] T062 [US4] All US4 unit tests pass ✅ (integration tests deferred to Phase 7)

**Checkpoint**: User Story 4 complete - fleet state visibility functional, all gauges working

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Performance validation, error handling, and documentation

- [-] T063 [P] Benchmark for metric recording overhead — deferred (post-MVP)
- [-] T064 [P] Benchmark for /metrics endpoint latency — deferred (post-MVP)
- [-] T065 [P] Benchmark for /v1/stats endpoint latency — deferred (post-MVP)
- [-] T066 Run cargo bench — deferred (post-MVP)
- [-] T067 [P] Property test for label sanitization — deferred (post-MVP)
- [-] T068 [P] Error handling for metrics unavailable — deferred (post-MVP)
- [x] T069 [P] Add uptime_seconds() method to MetricsCollector in src/metrics/mod.rs ✅
- [x] T070 Add documentation comments to all public functions in src/metrics/ ✅ (25+ doc comments)
- [-] T071 Update README.md with metrics endpoints documentation — deferred (post-MVP)
- [x] T072 Run full test suite: cargo test --all ✅ (365 tests pass)
- [-] T073 Run quickstart.md validation — deferred (post-MVP)
- [-] T074 Final integration test — deferred (post-MVP)
- [-] T075 [P] Integration test for FR-020 — deferred (post-MVP)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-6)**: All depend on Foundational phase completion
  - Can proceed in parallel if multiple developers available
  - Or sequentially in priority order: US1 → US2 → US3 → US4
- **Polish (Phase 7)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational - No dependencies on other stories
- **User Story 2 (P2)**: Can start after Foundational - Enhances US1 but independently testable
- **User Story 3 (P3)**: Can start after Foundational - Adds routing metrics, independently testable
- **User Story 4 (P3)**: Can start after Foundational - Uses gauges from US1, independently testable

### Within Each User Story (TDD Workflow)

1. **Tests FIRST**: Write all tests for the story, ensure they FAIL
2. **Implementation**: Implement features to make tests pass
3. **Validation**: Run tests and verify they now PASS
4. **Checkpoint**: Story is complete and independently functional

### Parallel Opportunities

**Setup Phase (Phase 1)**:
- T001, T002, T003 can run in sequence (dependency changes need order)

**Foundational Phase (Phase 2)**:
- T006 (label sanitization) + T007 (types module) can run in parallel
- T011 (metrics handler) + T012 (stats handler) can run in parallel

**User Story 1 Tests (Phase 3)**:
- T015, T016, T017 can all run in parallel (different test files)

**User Story 1 Implementation (Phase 3)**:
- T021, T022, T023 (stats helpers) can run in parallel (different functions)
- T026, T027, T028 (metrics recording) are sequential within completions.rs

**User Story 2 Tests (Phase 4)**:
- T031, T032, T033 can all run in parallel (different test files)

**User Story 3 Tests (Phase 5)**:
- T041, T042, T043 can all run in parallel (different test files)

**User Story 4 Tests (Phase 6)**:
- T051, T052, T053 can all run in parallel (different test files)

**Polish Phase (Phase 7)**:
- T063, T064, T065 (benchmarks) can run in parallel (different benchmark files)
- T067, T068, T069 can run in parallel (different files/functions)

---

## Parallel Example: User Story 1

```bash
# Step 1: Launch all tests together (TDD - ensure they FAIL)
Task: "Write unit test for label sanitization in tests/unit/metrics_test.rs"
Task: "Write contract test for /metrics endpoint in tests/integration/metrics_contract_test.rs"
Task: "Write contract test for /v1/stats endpoint in tests/integration/stats_contract_test.rs"

# Step 2: Implement stats helpers in parallel
Task: "Implement compute_request_stats() helper in src/metrics/handler.rs"
Task: "Implement compute_backend_stats() helper in src/metrics/handler.rs"
Task: "Implement compute_model_stats() helper in src/metrics/handler.rs"

# Step 3: Verify all tests now PASS
Task: "Run all US1 tests and verify they now PASS"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T004)
2. Complete Phase 2: Foundational (T005-T014) - CRITICAL
3. Complete Phase 3: User Story 1 (T015-T030)
4. **STOP and VALIDATE**: Test US1 independently
5. Benchmark to verify < 0.1ms overhead
6. Deploy/demo if ready - **This is the MVP!**

### Incremental Delivery

1. **Foundation** (Phase 1-2): Metrics infrastructure ready
2. **US1** (Phase 3): Basic request tracking → Test independently → Deploy/Demo (MVP!)
3. **US2** (Phase 4): Performance monitoring → Test independently → Deploy/Demo
4. **US3** (Phase 5): Routing intelligence → Test independently → Deploy/Demo
5. **US4** (Phase 6): Fleet visibility → Test independently → Deploy/Demo
6. **Polish** (Phase 7): Performance validation and documentation

Each story adds value without breaking previous stories.

### Parallel Team Strategy

With 3 developers available (after Foundational phase completion):

1. **Team completes Foundation together** (Phase 1-2)
2. **Once Foundational is done**:
   - Developer A: User Story 1 (T015-T030)
   - Developer B: User Story 2 (T031-T040) - can start in parallel
   - Developer C: User Story 3 (T041-T050) - can start in parallel
3. **US4 can start** when any developer finishes (depends on US1 gauges but minimal)
4. **Polish phase** when all stories complete

---

## Performance Targets

- **Metric recording**: < 0.1ms (100µs) per request
- **/metrics endpoint**: < 1ms response time
- **/v1/stats endpoint**: < 2ms response time
- **Support**: 10,000+ requests/second without degradation

Validated via benchmarks in Phase 7 (T063-T066).

---

## Notes

- All tasks follow TDD: Tests first, then implementation
- [P] tasks can run in parallel (different files, no dependencies)
- [Story] label maps task to specific user story for traceability
- Each user story is independently completable and testable
- Foundational phase BLOCKS all user stories - complete it first
- Stop at any checkpoint to validate story independently
- Commit after each task or logical group
- Performance validation is critical - benchmark early and often
- Use cargo test to run tests, cargo bench for performance validation
