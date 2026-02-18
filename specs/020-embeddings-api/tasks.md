---
description: "Task list for F17: Embeddings API (Retrospective)"
---

# Tasks: Embeddings API (F17)

**Input**: Design documents from `/specs/020-embeddings-api/`
**Prerequisites**: plan.md, spec.md, data-model.md, contracts/embeddings.json
**Status**: ✅ COMPLETED (Retrospective Documentation)

> **Note**: This is a retrospective tasks document. All tasks have been completed and are marked with [x].

**Organization**: Tasks are grouped by user story to show how each story was implemented and tested independently.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

- Single Rust project: `src/`, `tests/` at repository root
- Paths reference actual files in the Nexus codebase

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and API structure

- [x] T001 Review existing API structure in src/api/mod.rs for endpoint registration patterns
- [x] T002 Review existing Router implementation in src/routing/mod.rs for capability-based selection
- [x] T003 [P] Review InferenceAgent trait in src/agent/mod.rs for extension patterns
- [x] T004 [P] Review AgentCapabilities in src/agent/types.rs for capability flags

**Checkpoint**: ✅ Existing infrastructure ready for extension

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core types and trait extensions that ALL user stories depend on

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [x] T005 Create embeddings module src/api/embeddings.rs with stub types
- [x] T006 [P] Define EmbeddingInput enum (Single/Batch) with #[serde(untagged)] in src/api/embeddings.rs
- [x] T007 [P] Define EmbeddingRequest struct (model, input, encoding_format) in src/api/embeddings.rs
- [x] T008 [P] Define EmbeddingObject struct (object, embedding, index) in src/api/embeddings.rs
- [x] T009 [P] Define EmbeddingUsage struct (prompt_tokens, total_tokens) in src/api/embeddings.rs
- [x] T010 [P] Define EmbeddingResponse struct (object, data, model, usage) in src/api/embeddings.rs
- [x] T011 [P] Implement EmbeddingInput::into_vec() method for normalization in src/api/embeddings.rs
- [x] T012 Add embeddings() method to InferenceAgent trait with default Unsupported impl in src/agent/mod.rs
- [x] T013 Add embeddings: bool field to AgentCapabilities struct in src/agent/types.rs
- [x] T014 Register POST /v1/embeddings route in src/api/mod.rs

**Checkpoint**: ✅ Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 3 - Multi-Backend Routing (Priority: P1) 🎯 MVP

**Goal**: Nexus automatically routes embedding requests to a capable backend based on model name and backend capabilities.

**Independent Test**: Can be tested by configuring different backends and verifying requests are routed to backends that declare embeddings capability.

### Implementation for User Story 3

- [x] T015 [US3] Implement embeddings_handler() function in src/api/embeddings.rs with validation
- [x] T016 [US3] Add input validation (empty check) returning 400 Bad Request in src/api/embeddings.rs
- [x] T017 [US3] Add token estimation using chars/4 heuristic in src/api/embeddings.rs
- [x] T018 [US3] Build RequestRequirements with estimated tokens in src/api/embeddings.rs
- [x] T019 [US3] Call state.router.select_backend() with requirements in src/api/embeddings.rs
- [x] T020 [US3] Add agent lookup from registry by backend ID in src/api/embeddings.rs
- [x] T021 [US3] Add capability check (agent.profile().capabilities.embeddings) in src/api/embeddings.rs
- [x] T022 [US3] Add error handling for routing errors (404, 502, 503) in src/api/embeddings.rs
- [x] T023 [US3] Call agent.embeddings() and handle errors in src/api/embeddings.rs
- [x] T024 [US3] Build EmbeddingResponse from vectors with indexed objects in src/api/embeddings.rs
- [x] T025 [US3] Calculate token usage and add to response in src/api/embeddings.rs

**Checkpoint**: ✅ At this point, routing infrastructure is functional and testable

---

## Phase 4: User Story 1 - Single Text Embedding (Priority: P1) 🎯 MVP

**Goal**: API users can generate embeddings for a single text input using a simple POST request to `/v1/embeddings`.

**Independent Test**: Can be fully tested by sending a single text string and receiving a vector representation.

### Implementation for User Story 1

- [x] T026 [P] [US1] Implement OpenAI agent embeddings() in src/agent/openai.rs (lines 357-422)
- [x] T027 [US1] Add POST /v1/embeddings request building with bearer auth in src/agent/openai.rs
- [x] T028 [US1] Add OpenAI response parsing extracting embedding vectors in src/agent/openai.rs
- [x] T029 [US1] Add timeout (60s) and error handling for OpenAI backend in src/agent/openai.rs
- [x] T030 [P] [US1] Implement Ollama agent embeddings() in src/agent/ollama.rs (lines 291-353)
- [x] T031 [US1] Add POST /api/embed request building for single input in src/agent/ollama.rs
- [x] T032 [US1] Add Ollama response parsing and transformation to OpenAI format in src/agent/ollama.rs
- [x] T033 [US1] Add timeout (60s) and error handling for Ollama backend in src/agent/ollama.rs
- [x] T034 [P] [US1] Set AgentCapabilities.embeddings = true in OpenAI agent profile
- [x] T035 [P] [US1] Set AgentCapabilities.embeddings = true in Ollama agent profile
- [x] T036 [P] [US1] Verify LMStudio agent uses default Unsupported implementation
- [x] T037 [P] [US1] Verify Generic agent uses default Unsupported implementation

**Checkpoint**: ✅ Single text embedding works end-to-end with OpenAI and Ollama backends

---

## Phase 5: User Story 2 - Batch Text Embedding (Priority: P2)

**Goal**: API users can generate embeddings for multiple text inputs in a single request to improve efficiency.

**Independent Test**: Can be tested by sending an array of strings and receiving corresponding embedding vectors indexed by position.

### Implementation for User Story 2

- [x] T038 [US2] Add native batch support to OpenAI agent (single request for Vec) in src/agent/openai.rs
- [x] T039 [US2] Implement batch iteration for Ollama agent (per-input loop) in src/agent/ollama.rs
- [x] T040 [US2] Add vector collection and indexing for Ollama batch results in src/agent/ollama.rs
- [x] T041 [US2] Verify batch input handling in embeddings_handler() in src/api/embeddings.rs

**Checkpoint**: ✅ Batch embedding works with both native (OpenAI) and iterative (Ollama) approaches

---

## Phase 6: Testing & Validation

**Purpose**: Comprehensive testing coverage for all user stories

### Unit Tests (8 tests in src/api/embeddings.rs)

- [x] T042 [P] Add test_embedding_request_deserialize_single_input in src/api/embeddings.rs
- [x] T043 [P] Add test_embedding_request_deserialize_batch_input in src/api/embeddings.rs
- [x] T044 [P] Add test_embedding_request_with_encoding_format in src/api/embeddings.rs
- [x] T045 [P] Add test_embedding_input_into_vec_single in src/api/embeddings.rs
- [x] T046 [P] Add test_embedding_input_into_vec_batch in src/api/embeddings.rs
- [x] T047 [P] Add test_embedding_response_serialization_matches_openai in src/api/embeddings.rs
- [x] T048 [P] Add test_embedding_response_roundtrip in src/api/embeddings.rs
- [x] T049 [P] Add test_embedding_object_serialization in src/api/embeddings.rs

### Integration Tests (5 tests in tests/embeddings_test.rs)

- [x] T050 [P] Add test_embeddings_route_exists in tests/embeddings_test.rs
- [x] T051 [P] Add test_embeddings_returns_valid_response with mock backend in tests/embeddings_test.rs
- [x] T052 [P] Add test_embeddings_model_not_found_returns_error in tests/embeddings_test.rs
- [x] T053 [P] Add test_embeddings_batch_input_accepted in tests/embeddings_test.rs
- [x] T054 [P] Add test_embeddings_invalid_json_returns_422 in tests/embeddings_test.rs

**Checkpoint**: ✅ All 13 tests (8 unit + 5 integration) passing

---

## Phase 7: Documentation & Polish

**Purpose**: Documentation, verification, and final polish

- [x] T055 [P] Create spec.md retrospective documentation in specs/020-embeddings-api/
- [x] T056 [P] Create plan.md retrospective documentation in specs/020-embeddings-api/
- [x] T057 [P] Create data-model.md with type definitions in specs/020-embeddings-api/
- [x] T058 [P] Create quickstart.md with usage examples in specs/020-embeddings-api/
- [x] T059 [P] Create contracts/embeddings.json with OpenAI spec reference in specs/020-embeddings-api/
- [x] T060 [P] Create research.md with design decisions in specs/020-embeddings-api/
- [x] T061 Manual testing with real OpenAI backend
- [x] T062 Manual testing with real Ollama backend
- [x] T063 Manual testing with LMStudio (verify Unsupported error)
- [x] T064 Verify multi-backend routing behavior
- [x] T065 Run cargo test to verify all tests pass
- [x] T066 Run cargo clippy for code quality checks
- [x] T067 Run cargo fmt for code formatting

**Checkpoint**: ✅ Feature complete, documented, and tested

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - completed first
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Story 3 (Phase 3)**: Depends on Foundational phase - Routing infrastructure
- **User Story 1 (Phase 4)**: Depends on Foundational phase - Backend implementations
- **User Story 2 (Phase 5)**: Depends on User Story 1 - Extends single to batch
- **Testing (Phase 6)**: Depends on all user stories being implemented
- **Documentation (Phase 7)**: Retrospective documentation created after completion

### User Story Dependencies

- **User Story 3 (P1)**: Can start after Foundational (Phase 2) - Routing infrastructure
- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - Backend implementations
- **User Story 2 (P2)**: Extends User Story 1 - Adds batch processing

### Within Each User Story

- Foundational types defined before handler implementation
- Handler implementation before backend implementations
- Backend implementations can be done in parallel (OpenAI, Ollama independent)
- Tests written throughout implementation
- All core implementation before documentation

### Parallel Opportunities

- Phase 1 tasks (T001-T004): All parallelizable - different files
- Phase 2 type definitions (T006-T011): All parallelizable - same file, no conflicts
- User Story 1 agent implementations (T026-T029 OpenAI, T030-T033 Ollama): Parallelizable
- User Story 1 capability settings (T034-T037): All parallelizable
- All unit tests (T042-T049): Parallelizable - same file, independent tests
- All integration tests (T050-T054): Parallelizable - same file, independent tests
- All documentation tasks (T055-T060): Parallelizable - different files

---

## Parallel Example: User Story 1

```bash
# Launch OpenAI and Ollama implementations together:
Task T026-T029: "Implement OpenAI agent embeddings() in src/agent/openai.rs"
Task T030-T033: "Implement Ollama agent embeddings() in src/agent/ollama.rs"

# Launch all capability settings together:
Task T034: "Set AgentCapabilities.embeddings = true in OpenAI agent profile"
Task T035: "Set AgentCapabilities.embeddings = true in Ollama agent profile"
Task T036: "Verify LMStudio agent uses default Unsupported implementation"
Task T037: "Verify Generic agent uses default Unsupported implementation"
```

---

## Implementation Strategy

### MVP First (User Stories 3 + 1)

1. ✅ Complete Phase 1: Setup
2. ✅ Complete Phase 2: Foundational (CRITICAL - blocks all stories)
3. ✅ Complete Phase 3: User Story 3 (Routing infrastructure)
4. ✅ Complete Phase 4: User Story 1 (Single text embedding)
5. ✅ **VALIDATED**: Single text embedding works end-to-end with OpenAI and Ollama

### Incremental Delivery

1. ✅ Complete Setup + Foundational → Foundation ready
2. ✅ Add User Story 3 (Routing) → Test independently → Routing works
3. ✅ Add User Story 1 (Single embedding) → Test independently → MVP complete
4. ✅ Add User Story 2 (Batch) → Test independently → Full feature complete
5. ✅ Add Testing (Phase 6) → 13 tests passing
6. ✅ Add Documentation (Phase 7) → Retrospective docs created

### Implementation Timeline

**Actual Implementation Flow**:
1. Foundation types and trait extensions (T005-T014)
2. Routing handler implementation (T015-T025)
3. OpenAI agent implementation (T026-T029)
4. Ollama agent implementation (T030-T033)
5. Batch support added (T038-T041)
6. Unit tests throughout (T042-T049)
7. Integration tests after implementation (T050-T054)
8. Retrospective documentation (T055-T060)
9. Manual testing and validation (T061-T067)

---

## Task Summary

### Total Task Count: 67 tasks (all completed)

**By Phase**:
- Phase 1 (Setup): 4 tasks
- Phase 2 (Foundational): 10 tasks
- Phase 3 (User Story 3 - Routing): 11 tasks
- Phase 4 (User Story 1 - Single): 12 tasks
- Phase 5 (User Story 2 - Batch): 4 tasks
- Phase 6 (Testing): 13 tasks
- Phase 7 (Documentation & Polish): 13 tasks

**By User Story**:
- User Story 3 (Multi-Backend Routing): 11 tasks
- User Story 1 (Single Text Embedding): 12 tasks
- User Story 2 (Batch Text Embedding): 4 tasks
- Infrastructure/Shared: 14 tasks
- Testing: 13 tasks
- Documentation: 13 tasks

**By Type**:
- Implementation tasks: 41 tasks
- Unit test tasks: 8 tasks
- Integration test tasks: 5 tasks
- Documentation tasks: 6 tasks
- Manual validation tasks: 7 tasks

**Parallel Opportunities Identified**: 32 tasks marked [P]

---

## Implementation Notes

### Files Modified

**New Files** (~450 lines total):
- `src/api/embeddings.rs` (301 lines) - API types, handler, 8 unit tests
- `tests/embeddings_test.rs` (146 lines) - 5 integration tests

**Modified Files** (~130 lines total):
- `src/api/mod.rs` - Route registration (1 line)
- `src/agent/mod.rs` - Trait method + default impl (4 lines)
- `src/agent/types.rs` - Capability field (1 line)
- `src/agent/openai.rs` - embeddings() implementation (66 lines)
- `src/agent/ollama.rs` - embeddings() implementation (63 lines)

**Documentation Files** (retrospective):
- `specs/020-embeddings-api/spec.md`
- `specs/020-embeddings-api/plan.md`
- `specs/020-embeddings-api/data-model.md`
- `specs/020-embeddings-api/quickstart.md`
- `specs/020-embeddings-api/research.md`
- `specs/020-embeddings-api/contracts/embeddings.json`
- `specs/020-embeddings-api/tasks.md` (this file)

### Key Design Decisions

1. **OpenAI Compatibility**: OpenAI format as standard for ecosystem compatibility
2. **Capability-Based Routing**: Extended AgentCapabilities with embeddings flag
3. **Unified Router**: Reused existing Router for consistency
4. **Opt-In Design**: Default trait implementation returns Unsupported
5. **Token Estimation**: Simple chars/4 heuristic for routing
6. **Batch Strategy**: Native for OpenAI, iterative for Ollama

### Testing Strategy

- **Unit Tests**: Type serialization, input conversion, error handling
- **Integration Tests**: End-to-end flow with mock backends
- **Manual Tests**: Real OpenAI and Ollama backends, error scenarios

### Architecture Compliance

- ✅ Follows RFC-001 NII architecture
- ✅ Uses unified Router for backend selection
- ✅ Agent trait provides extension point
- ✅ OpenAI-compatible format for ecosystem compatibility
- ✅ Stateless design with no persistent storage

---

## Format Validation

✅ **CONFIRMED**: All 67 tasks follow the strict checklist format:
- ✅ All tasks start with `- [x]` (completed checkbox)
- ✅ All tasks have sequential IDs (T001-T067)
- ✅ 32 tasks marked [P] for parallelizability (different files, no dependencies)
- ✅ 27 tasks have [Story] labels (US1, US2, US3)
- ✅ All implementation tasks include specific file paths
- ✅ Clear action descriptions for each task

---

## Notes

- [x] markers indicate ALL tasks were completed (retrospective documentation)
- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story was independently completable and testable
- Foundation phase blocked all user stories (correct dependency)
- OpenAI and Ollama implementations were parallelizable
- All tests written and passing
- Documentation created retrospectively
- Feature complete and production-ready

---

**Document Version**: 1.0  
**Created**: 2025-02-17  
**Type**: Retrospective Task List (Post-Implementation)  
**Total Tasks**: 67 (all completed ✅)  
**Total Lines Added**: ~580 lines (code + tests)  
**Test Coverage**: 13 tests (8 unit + 5 integration)
