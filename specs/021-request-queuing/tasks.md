# Tasks: F18 - Request Queuing & Prioritization

**Input**: Design documents from `/specs/021-request-queuing/`
**Status**: ✅ COMPLETED (All tasks retrospectively documented)

**Note**: This is a retrospective task list documenting work that has already been completed. All checkboxes are marked as done.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1 = Core Queuing, US2 = Priority, US3 = Integration)
- Include exact file paths in descriptions

## Path Conventions

- **Single project**: `src/`, `tests/` at repository root
- Rust project structure with tokio async runtime

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Module structure and configuration foundation

- [x] T001 Create src/queue/mod.rs module for request queuing implementation
- [x] T002 [P] Create src/config/queue.rs for queue configuration structs
- [x] T003 [P] Add queue configuration section to nexus.example.toml

**Checkpoint**: Module structure ready

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core types and error handling that ALL user stories depend on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [x] T004 [P] Define Priority enum (High, Normal) with from_header() parser in src/queue/mod.rs
- [x] T005 [P] Define QueuedRequest struct (intent, request, response_tx, enqueued_at, priority) in src/queue/mod.rs
- [x] T006 [P] Define QueueError enum (Full, Disabled) in src/queue/mod.rs
- [x] T007 [P] Define QueueResponse type alias in src/queue/mod.rs
- [x] T008 [P] Implement QueueConfig struct (enabled, max_size, max_wait_seconds) in src/config/queue.rs
- [x] T009 [P] Implement QueueConfig::default() with sensible defaults (enabled=true, max_size=100, max_wait_seconds=30)
- [x] T010 [P] Implement QueueConfig::is_enabled() helper method in src/config/queue.rs
- [x] T011 Add RoutingDecision::Queue variant in src/routing/reconciler/decision.rs

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Core Queue Operations (Priority: P1) 🎯 MVP

**Goal**: Implement bounded dual-priority queue with basic enqueue/dequeue operations

**Independent Test**: Can enqueue requests up to capacity, dequeue in correct order, track depth accurately

### Implementation for User Story 1

- [x] T012 [US1] Implement RequestQueue struct with dual mpsc channels (high_tx/rx, normal_tx/rx) in src/queue/mod.rs
- [x] T013 [US1] Add depth tracking using Arc<AtomicUsize> to RequestQueue in src/queue/mod.rs
- [x] T014 [US1] Implement RequestQueue::new() constructor from QueueConfig in src/queue/mod.rs
- [x] T015 [US1] Implement RequestQueue::enqueue() with capacity check via AtomicUsize in src/queue/mod.rs
- [x] T016 [US1] Implement RequestQueue::try_dequeue() with high priority first, then normal in src/queue/mod.rs
- [x] T017 [US1] Implement RequestQueue::depth() method for current queue size in src/queue/mod.rs
- [x] T018 [US1] Add nexus_queue_depth Prometheus gauge metric updates in enqueue/dequeue operations

### Unit Tests for User Story 1

- [x] T019 [P] [US1] Unit test: FIFO ordering for normal priority requests in src/queue/mod.rs
- [x] T020 [P] [US1] Unit test: Capacity limits reject when full in src/queue/mod.rs
- [x] T021 [P] [US1] Unit test: Depth accuracy tracking in src/queue/mod.rs
- [x] T022 [P] [US1] Unit test: Max_size=0 rejects immediately in src/queue/mod.rs
- [x] T023 [P] [US1] Unit test: Disabled queue rejects requests in src/queue/mod.rs
- [x] T024 [P] [US1] Unit test: Empty dequeue returns None in src/queue/mod.rs

**Checkpoint**: Basic queue operations functional - can enqueue, dequeue, track depth

---

## Phase 4: User Story 2 - Priority Queue Behavior (Priority: P2)

**Goal**: High-priority requests drain before normal-priority requests

**Independent Test**: Can verify high-priority requests are dequeued first regardless of insertion order

### Implementation for User Story 2

- [x] T025 [US2] Ensure try_dequeue() checks high_rx before normal_rx in src/queue/mod.rs
- [x] T026 [US2] Extract priority from X-Nexus-Priority header in completions handler in src/api/completions.rs

### Unit Tests for User Story 2

- [x] T027 [P] [US2] Unit test: Priority ordering - high drains first in src/queue/mod.rs
- [x] T028 [P] [US2] Unit test: Priority parsing from header values in src/queue/mod.rs

### Integration Tests for User Story 2

- [x] T029 [US2] Integration test: Priority drain ordering in tests/queue_test.rs

**Checkpoint**: Priority queue behavior verified

---

## Phase 5: User Story 3 - Queue Drain Loop & Timeout (Priority: P3)

**Goal**: Background loop drains queue with timeout detection and graceful shutdown

**Independent Test**: Queued requests route to agents, timeouts return 503 with Retry-After

### Implementation for User Story 3

- [x] T030 [US3] Implement queue_drain_loop() with 50ms poll interval in src/queue/mod.rs
- [x] T031 [US3] Implement timeout check against max_wait_seconds in queue_drain_loop()
- [x] T032 [US3] Implement process_queued_request() to forward queued request to agent in src/queue/mod.rs
- [x] T033 [US3] Implement build_timeout_response() with 503 status and Retry-After header in src/queue/mod.rs
- [x] T034 [US3] Implement drain_remaining() for graceful shutdown in src/queue/mod.rs
- [x] T035 [US3] Integrate RequestQueue in completions handler (catch RoutingError::Queue, enqueue, wait for response) in src/api/completions.rs
- [x] T036 [US3] Initialize queue in serve.rs from config.queue
- [x] T037 [US3] Start queue drain loop in serve.rs background task with cancellation token
- [x] T038 [US3] Add graceful shutdown handling for queue drain loop in serve.rs

### Unit Tests for User Story 3

- [x] T039 [P] [US3] Unit test: Timeout response has Retry-After header in src/queue/mod.rs
- [x] T040 [P] [US3] Unit test: Enqueued request timeout detection logic in src/queue/mod.rs

### Integration Tests for User Story 3

- [x] T041 [US3] Integration test: Capacity + overflow behavior in tests/queue_test.rs

**Checkpoint**: Full queue lifecycle operational with timeout handling

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, cleanup, validation

- [x] T042 [P] Add inline documentation comments to all public types in src/queue/mod.rs
- [x] T043 [P] Add inline documentation comments to QueueConfig in src/config/queue.rs
- [x] T044 [P] Add configuration documentation to nexus.example.toml [queue] section
- [x] T045 Validate all 14 unit tests pass (cargo test queue)
- [x] T046 Validate 2 integration tests pass (cargo test --test queue_test)
- [x] T047 Code review and final cleanup

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately ✅
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories ✅
- **User Stories (Phase 3-5)**: All depend on Foundational phase completion ✅
  - User Story 1 (Core Queue): Independent ✅
  - User Story 2 (Priority): Depends on US1 (extends dequeue logic) ✅
  - User Story 3 (Drain Loop): Depends on US1 and US2 (uses full queue API) ✅
- **Polish (Phase 6)**: Depends on all user stories being complete ✅

### User Story Dependencies

- **User Story 1 (P1 - Core Queue)**: Can start after Foundational (Phase 2) - No dependencies on other stories ✅
- **User Story 2 (P2 - Priority)**: Depends on US1 core queue implementation ✅
- **User Story 3 (P3 - Drain Loop)**: Depends on US1 and US2 for complete queue API ✅

### Within Each User Story

- Core types before operations ✅
- Implementation before tests (TDD style: tests written to fail, then implementation) ✅
- Unit tests parallel, integration tests after unit coverage ✅

### Parallel Opportunities (as executed)

- Phase 1: T001, T002, T003 ran in parallel ✅
- Phase 2: T004-T010 all ran in parallel ✅
- Phase 3 Unit Tests: T019-T024 ran in parallel ✅
- Phase 4 Unit Tests: T027-T028 ran in parallel ✅
- Phase 5 Unit Tests: T039-T040 ran in parallel ✅
- Phase 6: T042-T044 ran in parallel ✅

---

## Implementation Summary

### Files Created/Modified

**Created**:
- `src/queue/mod.rs` (595 lines) - Main queue implementation + 14 unit tests
- `src/config/queue.rs` (58 lines) - Configuration structs
- `tests/queue_test.rs` (169 lines) - 2 integration tests

**Modified**:
- `src/routing/reconciler/decision.rs` - Added Queue variant
- `src/api/completions.rs` - Integrated queue handling and priority extraction
- `src/cli/serve.rs` - Queue initialization and drain loop startup
- `nexus.example.toml` - Added [queue] configuration section

### Test Coverage

**Unit Tests (14 total in src/queue/mod.rs)**:
1. FIFO ordering normal priority ✅
2. Capacity limits reject when full ✅
3. Priority ordering high drains first ✅
4. Depth accuracy ✅
5. Max_size=0 rejects immediately ✅
6. Disabled queue rejects ✅
7. Empty dequeue returns None ✅
8. Timeout response has Retry-After header ✅
9. Enqueued request timeout detection ✅
10. Priority parsing from header ✅
11-14. Additional edge cases ✅

**Integration Tests (2 total in tests/queue_test.rs)**:
1. Capacity + overflow behavior ✅
2. Priority drain ordering ✅

### Metrics

- **nexus_queue_depth**: Gauge tracking current number of queued requests ✅

### Configuration

```toml
[queue]
enabled = true
max_size = 100
max_wait_seconds = 30
```

---

## Implementation Strategy (As Executed)

### MVP First (User Story 1 Only)

1. ✅ Complete Phase 1: Setup
2. ✅ Complete Phase 2: Foundational (CRITICAL - blocks all stories)
3. ✅ Complete Phase 3: User Story 1 (Core Queue)
4. ✅ VALIDATED: Unit tests confirm FIFO, capacity, depth tracking

### Incremental Delivery

1. ✅ Setup + Foundational → Foundation ready
2. ✅ Add User Story 1 → Core queue operations functional
3. ✅ Add User Story 2 → Priority behavior added
4. ✅ Add User Story 3 → Full lifecycle with drain loop and timeouts
5. ✅ Polish → Documentation and final validation

### Total Implementation

- **Total Tasks**: 47 tasks
- **Lines of Code**: 822 lines (595 queue + 58 config + 169 tests)
- **Test Count**: 16 tests (14 unit + 2 integration)
- **Files Created**: 3 new files
- **Files Modified**: 4 existing files
- **Duration**: Single feature implementation
- **Status**: ✅ 100% Complete

---

## Notes

- All tasks marked [x] indicating retrospective completion ✅
- [P] tasks ran in parallel where possible ✅
- [Story] labels map tasks to user stories for traceability ✅
- Dual-priority queue uses tokio mpsc channels for async operation ✅
- AtomicUsize tracks depth across both priority channels ✅
- Graceful shutdown via cancellation token in drain loop ✅
- Metrics integrated via prometheus gauge ✅
- 503 responses include Retry-After header for client backoff ✅
