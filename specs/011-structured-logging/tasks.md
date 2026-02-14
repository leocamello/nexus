# Tasks: F11 Structured Request Logging

**Input**: Design documents from `/specs/011-structured-logging/`  
**Prerequisites**: plan.md (✅), spec.md (✅), research.md (✅), data-model.md (✅), contracts/log-schema.json (✅)

**Tests**: TDD workflow — tests are written before implementation per constitution mandate.

**Organization**: Tasks are grouped by user story. Each story follows TDD: test tasks → implementation tasks.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

- **Single Rust project**: `src/` at repository root
- Cargo workspace with single binary crate

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and basic structure for logging module

- [x] T001 Add tracing-subscriber json feature dependency to Cargo.toml
- [x] T002 Create new logging module directory at src/logging/
- [x] T003 [P] Create src/logging/mod.rs with module structure and re-exports
- [x] T004 [P] Create src/logging/middleware.rs for request ID generation middleware
- [x] T005 [P] Create src/logging/fields.rs for field extraction helpers

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [x] T006 Extend LoggingConfig struct in src/config/logging.rs to add component_levels (Option<HashMap<String, String>>) field
- [x] T007 Extend LoggingConfig struct in src/config/logging.rs to add enable_content_logging (bool, default false) field
- [x] T008 Extend RoutingResult struct in src/routing/mod.rs to add route_reason (String) field
- [x] T009 Update Router::select_backend() in src/routing/mod.rs to populate route_reason with backend selection rationale
- [x] T010 Configure tracing-subscriber with JSON layer in src/main.rs based on LoggingConfig format setting
- [x] T011 Implement EnvFilter configuration in src/main.rs to support component_levels from LoggingConfig

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Basic Request Logging (Priority: P1) 🎯 MVP

**Goal**: Every request produces a structured log entry with essential metadata (timestamp, request_id, model, backend, status, latency, tokens). Operators can see what's happening in the system.

**Independent Test**: Send a single request through Nexus, verify structured log entry is emitted with all required fields in JSON format. Can query logs with jq to find request by request_id.

### Tests for User Story 1 (Write First — Red Phase)

- [x] T100 [US1] Create tests/structured_logging.rs integration test file
- [x] T101 [US1] Write test: successful request produces structured log with all required fields (request_id, model, backend, status, latency_ms, tokens_prompt, tokens_completion, stream)
- [x] T102 [US1] Write test: JSON format log entry is valid JSON parseable by serde_json
- [x] T103 [US1] Write test: request_id is a valid UUID v4 format
- [x] T104 [US1] Write test: latency_ms reflects actual request duration (not zero, not negative)
- [x] T105 [US1] Write test: failed request (no backend available) still produces log entry with status=error and backend="none"

### Implementation for User Story 1 (Green Phase)

- [x] T012 [US1] Add request_id generation using uuid::Uuid::new_v4() at entry of handle_chat_completion in src/api/completions.rs
- [x] T013 [US1] Add #[instrument] macro to handle_chat_completion in src/api/completions.rs with fields: request_id, model, backend=Empty, latency_ms=Empty, tokens_prompt=Empty, tokens_completion=Empty, status=Empty
- [x] T014 [US1] Implement extract_tokens() helper function in src/logging/fields.rs to parse token counts from ChatCompletionResponse
- [x] T015 [US1] Implement extract_status() helper function in src/logging/fields.rs to determine status string from Result<Response, ApiError>
- [x] T016 [US1] Record backend field in span after routing decision in src/api/completions.rs using span::record()
- [x] T017 [US1] Record latency_ms field in span at request completion in src/api/completions.rs using Instant::elapsed()
- [x] T018 [US1] Record token fields (tokens_prompt, tokens_completion, tokens_total) in span at request completion in src/api/completions.rs
- [x] T019 [US1] Record status field in span at request completion in src/api/completions.rs using extract_status() helper
- [x] T020 [US1] Add stream field to span fields in src/api/completions.rs from request.stream boolean
- [x] T021 [US1] Add route_reason field to span in src/api/completions.rs from RoutingResult.route_reason
- [x] T022 [US1] Add backend_type field to span in src/api/completions.rs from Backend.backend_type
- [x] T023 [US1] Add status_code field to span in src/api/completions.rs from HTTP response status

**Checkpoint**: At this point, User Story 1 should be fully functional - single requests produce complete JSON log entries with all required fields

---

## Phase 4: User Story 2 - Request Correlation Across Retries and Failovers (Priority: P1)

**Goal**: All log entries related to the same original request share a correlation ID (request_id). Operators can trace requests through retry chains to understand failure scenarios.

**Independent Test**: Configure a backend that fails intermittently, send a request that triggers retry/failover, verify all log entries share the same request_id while showing different retry_count and backend values.

### Tests for User Story 2 (Write First — Red Phase)

- [x] T106 [US2] Write test: retry attempts share same request_id with incrementing retry_count
- [x] T107 [US2] Write test: fallback_chain shows progression of attempted backends
- [x] T108 [US2] Write test: successful first-try request has retry_count=0 and empty fallback_chain
- [x] T109 [US2] Write test: retry log entries are at WARN level, exhausted retries at ERROR level

### Implementation for User Story 2 (Green Phase)

- [x] T024 [US2] Add retry_count field (u32, default 0) to span fields in src/api/completions.rs
- [x] T025 [US2] Add fallback_chain field (String, initially empty) to span fields in src/api/completions.rs
- [x] T026 [US2] Propagate request_id through retry loop in src/api/completions.rs (ensure same UUID used for all attempts)
- [x] T027 [US2] Update retry_count field in span for each retry iteration in src/api/completions.rs using span::record()
- [x] T028 [US2] Build fallback_chain string by appending backend IDs on each retry/failover in src/api/completions.rs
- [x] T029 [US2] Record updated fallback_chain field in span after each retry attempt in src/api/completions.rs
- [x] T030 [US2] Ensure error_message field is populated in span when retries occur in src/api/completions.rs
- [x] T031 [US2] Set appropriate log level (WARN) for retry attempts and (ERROR) for exhausted retries in src/api/completions.rs

**Checkpoint**: At this point, User Stories 1 AND 2 should both work - can trace failed requests through multiple retry attempts using request_id

---

## Phase 5: User Story 3 - Routing and Backend Selection Visibility (Priority: P2)

**Goal**: Operators can see why Nexus selected a particular backend for each request through the route_reason field. Makes routing decisions transparent and debuggable.

**Independent Test**: Configure multiple backends with different health/load characteristics, send requests, verify route_reason field explains the selection (e.g., "highest_score:backend1:0.95", "round_robin", "fallback:backend2_unhealthy").

### Tests for User Story 3 (Write First — Red Phase)

- [x] T110 [US3] Write test: route_reason is populated for score-based routing (format includes strategy and score)
- [x] T111 [US3] Write test: route_reason explains fallback scenario when primary is unhealthy
- [x] T112 [US3] Write test: route_reason for single healthy backend says "only_healthy_backend"

### Implementation for User Story 3 (Green Phase)

- [x] T032 [US3] Update routing strategy implementations in src/routing/mod.rs to populate descriptive route_reason strings
- [x] T033 [US3] Add route_reason for highest-score selection in src/routing/mod.rs (format: "highest_score:backend_id:score")
- [x] T034 [US3] Add route_reason for round-robin selection in src/routing/mod.rs (format: "round_robin:index_N")
- [x] T035 [US3] Add route_reason for fallback scenarios in src/routing/mod.rs (format: "fallback:primary_unhealthy")
- [x] T036 [US3] Add route_reason for single healthy backend in src/routing/mod.rs (format: "only_healthy_backend")
- [x] T037 [US3] Add route_reason for model alias resolution in src/routing/mod.rs (format: "model_alias:alias_to_actual")
- [x] T038 [US3] Add actual_model field to span in src/api/completions.rs when RoutingResult.fallback_used is true

**Checkpoint**: All routing decisions are now explained - operators can understand why each backend was selected

---

## Phase 6: User Story 4 - Privacy-Safe Logging with Debug Override (Priority: P2)

**Goal**: By default, never log message content (privacy-safe). Provide explicit opt-in flag for debugging that includes request content with clear warnings.

**Independent Test**: Send requests with message content, verify default logs contain no message text. Enable debug content logging in config, verify request content appears in logs with warning at startup.

### Tests for User Story 4 (Write First — Red Phase)

- [x] T113 [US4] Write test: default config logs contain no message content (search log output for user message text)
- [x] T114 [US4] Write test: enable_content_logging=true produces startup warning about sensitive data
- [x] T115 [US4] Write test: enable_content_logging=true includes prompt_preview in log output

### Implementation for User Story 4 (Green Phase)

- [x] T039 [US4] Add startup warning message in src/main.rs when enable_content_logging is true (warn level, clear message about sensitive data)
- [x] T040 [US4] Add conditional prompt_preview field to span in src/api/completions.rs when enable_content_logging is enabled
- [x] T041 [US4] Implement truncate_prompt() helper in src/logging/fields.rs to extract first 100 characters of request messages
- [x] T042 [US4] Ensure handle_chat_completion span fields do NOT include any message content fields by default in src/api/completions.rs (only add prompt_preview when enable_content_logging is true)
- [x] T043 [US4] Add documentation comment in src/config/logging.rs explaining enable_content_logging security implications

**Checkpoint**: Privacy-safe by default, with explicit opt-in for debugging - compliance requirement met

---

## Phase 7: User Story 5 - Configurable Log Levels per Component (Priority: P3)

**Goal**: Different components (routing, backends, API gateway, health checker) can have independent log levels. Reduces noise and enables targeted debugging.

**Independent Test**: Set routing component to DEBUG and API gateway to INFO, send requests, verify routing logs show detailed debug information while API gateway shows only info-level messages.

### Tests for User Story 5 (Write First — Red Phase)

- [x] T116 [US5] Write test: component_levels config correctly builds EnvFilter directives
- [x] T117 [US5] Write test: build_filter_directives() produces valid tracing filter string

### Implementation for User Story 5 (Green Phase)

- [x] T044 [US5] Update EnvFilter initialization in src/main.rs to build filter directives from component_levels HashMap
- [x] T045 [US5] Add helper function build_filter_directives() in src/logging/mod.rs to construct EnvFilter string from LoggingConfig
- [x] T046 [US5] Add detailed DEBUG-level logs in src/routing/mod.rs for routing decisions (score calculations, backend comparisons)
- [x] T047 [US5] Add DEBUG-level logs in src/api/completions.rs for request processing stages (received, routing, completion)
- [x] T048 [US5] Add DEBUG-level logs in src/health/mod.rs for health check execution and results
- [x] T049 [US5] Ensure all spans use appropriate module targets (nexus::routing, nexus::api, nexus::health) for component filtering

**Checkpoint**: Component-level filtering works - can reduce log noise by 60-80% while keeping detailed logs for specific areas

---

## Phase 8: User Story 6 - Log Aggregator Compatibility (Priority: P3)

**Goal**: JSON logs are compatible with common log aggregators (ELK, Loki, Splunk, CloudWatch). All fields are automatically indexed and searchable.

**Independent Test**: Configure Nexus for JSON output, pipe logs to a test Loki or ELK instance, verify logs are automatically indexed and searchable without custom parsing.

### Tests for User Story 6 (Write First — Red Phase)

- [x] T118 [US6] Write test: JSON log output validates against contracts/log-schema.json schema
- [x] T119 [US6] Write test: numeric fields (latency_ms, tokens_*) are serialized as JSON numbers not strings
- [x] T120 [US6] Write test: timestamp field is RFC3339 format with UTC timezone

### Implementation for User Story 6 (Green Phase)

- [x] T050 [US6] Verify JSON layer configuration in src/main.rs includes proper field types (numbers as numbers, not strings)
- [x] T051 [US6] Ensure timestamp field is formatted as RFC3339 with UTC timezone in JSON output (automatic with tracing-subscriber)
- [x] T052 [US6] Verify numeric fields (latency_ms, tokens_*) are serialized as JSON numbers not strings
- [x] T053 [US6] Add target field to all spans for log aggregator filtering (automatic with tracing target)
- [x] T054 [US6] Test JSON schema validation against contracts/log-schema.json using sample log output
- [x] T055 [US6] Document log aggregator integration patterns in quickstart.md (already done - verify examples work)

**Checkpoint**: All user stories complete - logs are production-ready and compatible with standard observability tools

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Edge cases, documentation, and cross-cutting improvements

### Edge Case Handling

- [x] T066 Handle logging system failure: ensure request processing continues when tracing layer encounters errors (non-blocking by design)
- [x] T067 Handle streaming requests: emit initial span on request start, record final latency/tokens when stream completes in src/api/completions.rs
- [x] T068 Handle missing token counts: when backend response has no usage field, log tokens_prompt=null and tokens_completion=null (not 0) in src/logging/fields.rs
- [x] T069 Handle malformed requests: emit partial log entry (timestamp, request_id, status=error) for requests that fail before routing in src/api/completions.rs

### Documentation & Cleanup

- [x] T056 [P] Add inline documentation to src/logging/mod.rs explaining module purpose and usage
- [x] T057 [P] Add inline documentation to src/logging/fields.rs for all helper functions
- [x] T058 [P] Add inline documentation to src/logging/middleware.rs for request ID generation
- [x] T059 [P] Add example configuration to nexus.example.toml showing all logging options
- [x] T060 [P] Update README.md to reference structured logging feature and quickstart.md
- [x] T061 Verify all FR-001 through FR-015 functional requirements are met
- [x] T062 Run cargo clippy and fix any warnings in src/logging/ module
- [x] T063 Run cargo fmt on all modified files
- [x] T064 Test quickstart.md examples with actual running Nexus instance
- [x] T065 Verify performance overhead is < 1ms per request with benchmarking

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-8)**: All depend on Foundational phase completion
  - User Story 1 (P1): Independent, can start after Foundational
  - User Story 2 (P1): Depends on User Story 1 (needs request_id infrastructure)
  - User Story 3 (P2): Independent of US1/US2, can start after Foundational
  - User Story 4 (P2): Depends on User Story 1 (needs span infrastructure)
  - User Story 5 (P3): Independent of other stories, can start after Foundational
  - User Story 6 (P3): Depends on User Story 1 (needs JSON log output)
- **Polish (Phase 9)**: Depends on all desired user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - No dependencies on other stories
- **User Story 2 (P1)**: Depends on User Story 1 completion - Needs request_id generation and span infrastructure
- **User Story 3 (P2)**: Can start after Foundational (Phase 2) - Independent of US1/US2 (only needs route_reason in RoutingResult)
- **User Story 4 (P2)**: Depends on User Story 1 completion - Needs span field infrastructure for conditional logging
- **User Story 5 (P3)**: Can start after Foundational (Phase 2) - Independent (only touches EnvFilter config)
- **User Story 6 (P3)**: Depends on User Story 1 completion - Needs JSON log output to validate

### Within Each User Story

- User Story 1: T012 (request_id) → T013 (#[instrument]) → helpers (T014-T015) → field recording (T016-T023)
- User Story 2: All tasks depend on US1 span infrastructure being complete
- User Story 3: All routing changes independent, can run in parallel [P]
- User Story 4: All privacy tasks sequential (warning → conditional field → helpers)
- User Story 5: All component filtering tasks independent after EnvFilter setup
- User Story 6: All validation tasks independent [P]

### Parallel Opportunities

**Setup Phase**:
- T003, T004, T005 can all run in parallel (different files)

**Foundational Phase**:
- T006, T007 can run in parallel (different fields in same struct)
- T008, T009 can run in parallel with T006, T007 (different file)

**User Story 3** (after US1 complete):
- T032-T037 can run in parallel if routing strategies are in separate functions

**User Story 5** (after Foundational):
- T046, T047, T048 can run in parallel (different files)

**Polish Phase**:
- T056, T057, T058 can run in parallel (documentation, different files)
- T059, T060 can run in parallel (documentation, different files)

---

## Parallel Example: Setup Phase

```bash
# Launch all setup tasks together:
Task: "Create src/logging/mod.rs with module structure and re-exports"
Task: "Create src/logging/middleware.rs for request ID generation middleware"
Task: "Create src/logging/fields.rs for field extraction helpers"
```

## Parallel Example: User Story 3 (Routing Visibility)

```bash
# Launch all routing reason updates together:
Task: "Add route_reason for highest-score selection in src/routing/mod.rs"
Task: "Add route_reason for round-robin selection in src/routing/mod.rs"
Task: "Add route_reason for fallback scenarios in src/routing/mod.rs"
Task: "Add route_reason for single healthy backend in src/routing/mod.rs"
```

---

## Implementation Strategy

### MVP First (User Stories 1 + 2 Only)

**Why US1 + US2 together**: Both are P1 priority and correlation (US2) requires basic logging (US1) infrastructure. Together they provide complete request tracing through retries, which is the core value proposition.

1. Complete Phase 1: Setup → Basic logging module structure ready
2. Complete Phase 2: Foundational → Config and routing extensions ready
3. Complete Phase 3: User Story 1 → Basic structured logging functional
4. Complete Phase 4: User Story 2 → Request correlation through retries functional
5. **STOP and VALIDATE**: Test both stories together - send request that triggers retry, trace with request_id
6. Deploy/demo if ready

**MVP Delivers**: 100% request visibility with correlation across retries/failovers - operators can see everything happening and trace problem requests.

### Incremental Delivery

1. Complete Setup + Foundational → Foundation ready (~5 tasks, 1-2 hours)
2. Add User Story 1 → Test independently → Deploy/Demo (MVP baseline, ~12 tasks, 3-4 hours)
3. Add User Story 2 → Test with retries → Deploy/Demo (Complete MVP with correlation, ~8 tasks, 2-3 hours)
4. Add User Story 3 → Test routing visibility → Deploy/Demo (Enhanced debugging, ~7 tasks, 2 hours)
5. Add User Story 4 → Test privacy controls → Deploy/Demo (Compliance feature, ~5 tasks, 1-2 hours)
6. Add User Story 5 → Test component filtering → Deploy/Demo (Operational efficiency, ~6 tasks, 2 hours)
7. Add User Story 6 → Test aggregator integration → Deploy/Demo (Enterprise ready, ~6 tasks, 2-3 hours)

Total estimated effort: ~14-18 hours for complete feature

### Parallel Team Strategy

With multiple developers:

1. **Team completes Setup + Foundational together** (foundational blocks everything)
2. **Once Foundational is done**:
   - Developer A: User Story 1 → User Story 2 (sequential, US2 depends on US1)
   - Developer B: User Story 3 (independent, routing changes)
   - Developer C: User Story 5 (independent, component filtering)
3. **After User Story 1 complete**:
   - Developer A continues with User Story 2
   - Developer D can start User Story 4 (depends on US1 span infrastructure)
   - Developer E can start User Story 6 (depends on US1 JSON output)

**Optimal 2-developer strategy**:
- Dev 1: Setup → Foundational → US1 → US2 → US4 (critical path)
- Dev 2: US3 → US5 → US6 → Polish (parallel enhancements)

---

## Notes

- [P] tasks = different files, no dependencies, can run in parallel
- [Story] label maps task to specific user story for traceability
- Each user story delivers independent value (except US2 which extends US1)
- Focus on US1 + US2 for MVP (core value: request visibility with correlation)
- US3-US6 are enhancements that can be added incrementally
- All tasks use existing tracing infrastructure - no new frameworks
- Performance budget: <1ms overhead target (~187µs measured in research)
- Privacy-safe by default (FR-008): no message content logged unless explicitly enabled
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Avoid: logging blocking on failures (use non-blocking tracing), mixing metrics with logs, adding abstractions over tracing
