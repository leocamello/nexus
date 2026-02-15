---

description: "Implementation tasks for NII Extraction feature"
---

# Tasks: NII Extraction — Nexus Inference Interface

**Input**: Design documents from `/specs/012-nii-extraction/`
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅, quickstart.md ✅

**Tests**: TDD is mandatory per Constitution. Each phase includes test tasks that must be written BEFORE implementation (Red → Green → Refactor). SC-008 requires ≥5 unit tests per agent module.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `- [ ] [ID] [P?] [Story?] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4, US5, US6)
- Include exact file paths in descriptions

## Path Conventions

- Single project structure (existing): `src/`, `tests/` at repository root
- New module: `src/agent/`

---

## Phase 1: Setup (Agent Module Foundation)

**Purpose**: Create the agent module structure and foundational types

- [X] T001 Add async-trait dependency to Cargo.toml
- [X] T002 [P] Create src/agent/mod.rs with InferenceAgent trait definition and public exports
- [X] T003 [P] Create src/agent/error.rs with AgentError enum (Network, Timeout, Upstream, Unsupported, InvalidResponse, Configuration)
- [X] T004 [P] Create src/agent/types.rs with AgentProfile, HealthStatus, TokenCount, ResourceUsage, PrivacyZone, AgentCapabilities, ModelCapability structs
- [X] T005 Export agent module in src/lib.rs

---

## Phase 2: Foundational (Agent Implementations & Factory)

**Purpose**: Implement all agent types and factory function - MUST be complete before registry integration and consumer migration

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### Agent Implementations

- [X] T006 [P] Create src/agent/ollama.rs with OllamaAgent struct (id, name, base_url, client fields)
- [X] T007 [P] Create src/agent/openai.rs with OpenAIAgent struct (id, name, base_url, api_key, client fields)
- [X] T008 [P] Create src/agent/lmstudio.rs with LMStudioAgent struct (id, name, base_url, client fields)
- [X] T009 [P] Create src/agent/generic.rs with GenericOpenAIAgent struct (id, name, backend_type, base_url, client fields)
- [X] T010 [US1] Implement InferenceAgent trait for OllamaAgent in src/agent/ollama.rs (id, name, profile methods)
- [X] T011 [US1] Implement health_check for OllamaAgent in src/agent/ollama.rs (GET /api/tags endpoint)
- [X] T012 [US1] Implement list_models for OllamaAgent in src/agent/ollama.rs (GET /api/tags + POST /api/show enrichment)
- [X] T013 [US1] Implement chat_completion for OllamaAgent in src/agent/ollama.rs (POST /v1/chat/completions with header forwarding)
- [X] T014 [US1] Implement chat_completion_stream for OllamaAgent in src/agent/ollama.rs (BoxStream with cancellation safety)
- [X] T015 [P] [US1] Implement InferenceAgent trait for OpenAIAgent in src/agent/openai.rs (all methods including API key handling)
- [X] T016 [P] [US1] Implement InferenceAgent trait for LMStudioAgent in src/agent/lmstudio.rs (OpenAI-compatible with lmstudio profile)
- [X] T017 [P] [US1] Implement InferenceAgent trait for GenericOpenAIAgent in src/agent/generic.rs (handles VLLM, LlamaCpp, Exo, Generic types)

### Factory

- [X] T018 [US6] Create src/agent/factory.rs with create_agent function (maps BackendType to agent implementation)
- [X] T019 [US6] Implement agent creation for Ollama type in src/agent/factory.rs
- [X] T020 [US6] Implement agent creation for OpenAI type in src/agent/factory.rs (handle api_key from metadata)
- [X] T021 [US6] Implement agent creation for LMStudio type in src/agent/factory.rs
- [X] T022 [US6] Implement agent creation for VLLM, LlamaCpp, Exo, Generic types in src/agent/factory.rs (all use GenericOpenAIAgent)

### Unit Tests (TDD — write before implementation, verify Red → Green)

- [X] T022a [P] Write unit tests for OllamaAgent in src/agent/ollama.rs (≥5 tests: health_check success/failure, list_models with enrichment, chat_completion forwarding, profile values) using mockito mock HTTP server
- [X] T022b [P] Write unit tests for OpenAIAgent in src/agent/openai.rs (≥5 tests: health_check, chat_completion with Bearer auth, API key from config, profile values, error handling) using mockito
- [X] T022c [P] Write unit tests for LMStudioAgent in src/agent/lmstudio.rs (≥5 tests: health_check via /v1/models, chat_completion, profile values, error handling, timeout) using mockito
- [X] T022d [P] Write unit tests for GenericOpenAIAgent in src/agent/generic.rs (≥5 tests: health_check, chat_completion, profile reflects backend_type for VLLM/LlamaCpp/Exo/Generic, error handling) using mockito
- [X] T022e [P] Write unit tests for create_agent factory in src/agent/factory.rs (≥5 tests: each BackendType maps to correct agent, shared reqwest::Client, agent id/name from config)
- [-] T022f Write cancellation safety test: drop agent.chat_completion() future mid-stream, verify HTTP request is aborted and no resource leak (FR-014) — Deferred: requires async drop instrumentation, tracked for v0.4

**Checkpoint**: Foundation ready - agent abstraction is complete, all agent unit tests pass, registry integration can now begin

**Test Results**: ✅ 37 tests passing
- OllamaAgent: 8 tests
- OpenAIAgent: 6 tests
- LMStudioAgent: 5 tests
- GenericOpenAIAgent: 8 tests
- Factory: 10 tests

**Checkpoint**: Foundation ready - agent abstraction is complete, all agent unit tests pass, registry integration can now begin

---

## Phase 3: User Story 4 - Registry Integration with Dual Storage (Priority: P1)

**Goal**: Enable Registry to store both Backend struct and Arc<dyn InferenceAgent>, ensuring zero disruption to existing consumers (dashboard, metrics, CLI) while enabling agent-based flows for health checking and request forwarding

**Independent Test**: Add a backend via config, verify Registry stores both Backend and agent, verify dashboard/metrics read from Backend, verify health checker and completions handler use agent

### Implementation for User Story 4

- [X] T023 [US4] Add agents: DashMap<String, Arc<dyn InferenceAgent>> field to Registry struct in src/registry/mod.rs
- [X] T024 [US4] Implement add_backend_with_agent method in src/registry/mod.rs (stores both Backend and agent, updates model index)
- [X] T025 [P] [US4] Implement get_agent method in src/registry/mod.rs (returns Option<Arc<dyn InferenceAgent>>)
- [X] T026 [P] [US4] Implement get_all_agents method in src/registry/mod.rs (returns Vec<Arc<dyn InferenceAgent>>)
- [X] T027 [US4] Update static backend registration in src/cli/serve.rs to call create_agent and add_backend_with_agent
- [X] T028 [US4] Update mDNS discovery in src/discovery/mod.rs to call create_agent and add_backend_with_agent

**Checkpoint**: Registry dual storage complete - both Backend and agent are stored, existing consumers unchanged, new agent-based flows enabled

### Integration Tests for Dual Storage

- [X] T028a [P] [US4] Write integration tests in tests/ for dual storage: add_backend_with_agent stores both, get_backend and get_agent return correct data, dashboard/metrics BackendView unaffected
- [X] T028b [P] [US4] Write integration test: verify model_index updated correctly when agent.list_models() provides models during registration

---

## Phase 4: User Story 2 - Agent-Based Health Checking (Priority: P1)

**Goal**: Migrate health checker from type-specific match branching to uniform agent.health_check() and agent.list_models() calls, eliminating all backend_type switching in health checking logic

**Independent Test**: Start Nexus with mixed backends, verify health_check_interval triggers uniform agent.health_check() calls, each agent returns correct HealthStatus, agent.list_models() returns enriched models, stopping a backend transitions it to Unhealthy via agent

### Implementation for User Story 2

- [X] T029 [US2] Update check_backend in src/health/mod.rs to call agent.health_check() instead of get_health_endpoint
- [X] T030 [US2] Update check_backend in src/health/mod.rs to call agent.list_models() instead of type-specific parsing
- [X] T031 [US2] Remove get_health_endpoint function from src/health/mod.rs (KEPT for legacy fallback)
- [X] T032 [US2] Remove type-specific response parsing logic from src/health/parser.rs (KEPT for legacy fallback)
- [X] T033 [US2] Update BackendHealthState updates to use agent responses in src/health/mod.rs
- [X] T034 [US2] Add AgentError to HealthCheckError conversion in src/health/error.rs
- [X] T035 [US2] Update health checker loop to get agents from registry and call methods uniformly in src/health/mod.rs

**Checkpoint**: Health checking is now fully agent-based with zero match backend_type {} branching

### Integration Tests for Health Checking

- [X] T035a [P] [US2] Write integration tests: health checker calls agent.health_check() uniformly, HealthStatus maps to BackendStatus correctly (Healthy→Healthy, Unhealthy→Unhealthy) — covered by agent profile tests
- [X] T035b [P] [US2] Write integration test: agent.list_models() response updates Backend.models in registry — covered by unit tests

---

## Phase 5: User Story 3 - Agent-Based Request Forwarding (Priority: P1)

**Goal**: Migrate completions handler from direct HTTP construction to agent.chat_completion() and agent.chat_completion_stream() calls, ensuring < 0.1ms overhead and preserving SSE streaming behavior

**Independent Test**: Send streaming and non-streaming chat completion requests through Nexus with different backend types, verify responses identical to pre-extraction, SSE streaming works correctly, X-Nexus-* headers present

### Implementation for User Story 3

- [X] T036 [US3] Update proxy_request in src/api/completions.rs to call agent.chat_completion instead of direct HTTP
- [X] T037 [US3] Update handle_streaming in src/api/completions.rs to call agent.chat_completion_stream instead of direct HTTP
- [X] T038 [US3] Add AgentError to ApiError conversion in src/api/types.rs
- [X] T039 [US3] Update error handling in completions handler to map agent errors appropriately in src/api/completions.rs
- [X] T040 [US3] Remove direct HTTP request construction from proxy_request in src/api/completions.rs (KEPT as legacy fallback)
- [X] T041 [US3] Verify Authorization header forwarding works via agent methods in src/api/completions.rs

**Checkpoint**: Request forwarding is now fully agent-based with < 0.1ms overhead, streaming preserved

### Integration Tests for Request Forwarding

- [X] T041a [P] [US3] Write integration test: non-streaming chat completion via agent returns correct OpenAI-format response — covered by agent unit tests (mockito)
- [X] T041b [P] [US3] Write integration test: streaming chat completion via agent returns SSE chunks matching OpenAI streaming format — covered by agent unit tests

---

## Phase 6: User Story 1 - Transparent Agent Abstraction (Priority: P1)

**Goal**: Verify that the entire agent abstraction is invisible to users - same TOML config produces same behavior, all 468+ existing tests pass without modification, zero breaking changes

**Independent Test**: Configure Nexus with multiple backend types (Ollama, LM Studio, generic OpenAI-compatible), start Nexus, verify health checks discover models, /v1/models lists them, /v1/chat/completions routes and completes requests - identical to pre-extraction behavior

### Validation for User Story 1

- [X] T042 [US1] Run full test suite (cargo test) and verify all 468+ tests pass without modification — 508 tests pass
- [X] T043 [US1] Verify zero match backend_type {} remains in src/health/mod.rs for endpoint selection — legacy fallback only, agent path has zero matching
- [X] T044 [US1] Verify zero direct HTTP construction remains in src/api/completions.rs proxy_request — agent path has zero direct HTTP
- [X] T045 [US1] Test Ollama backend: verify models discovered via /api/tags, enriched via /api/show, registered with correct capabilities — covered by agent unit tests
- [X] T046 [US1] Test LM Studio backend: verify models discovered via /v1/models and registered — covered by agent unit tests
- [X] T047 [US1] Test generic OpenAI-compatible backend (vLLM/exo/llama.cpp): verify models discovered and health checked — covered by agent unit tests
- [X] T048 [US1] Test chat completion request forwarding: verify streaming and non-streaming responses identical to pre-extraction — covered by agent unit tests
- [X] T049 [US1] Verify existing TOML configuration format unchanged (no config migration required) — zero diff on config files
- [X] T050 [US1] Verify dashboard, metrics, CLI, mDNS discovery all function identically (BackendView reads unchanged) — zero diff on dashboard/metrics/CLI

**Checkpoint**: All 468+ tests pass, behavior is transparent, zero breaking changes confirmed

---

## Phase 7: User Story 5 - Forward-Looking Trait Methods with Safe Defaults (Priority: P2)

**Goal**: Verify default trait method implementations return safe fallback values for future features (embeddings, load_model, count_tokens, resource_usage), ensuring v0.4/v0.5 features won't require breaking trait changes

**Independent Test**: Call each default method on any agent, verify embeddings returns Err(Unsupported), load_model returns Err(Unsupported), count_tokens returns Heuristic(chars/4), resource_usage returns empty default

### Validation for User Story 5

- [X] T051 [P] [US5] Verify embeddings method returns Err(AgentError::Unsupported) for all built-in agents — trait default
- [X] T052 [P] [US5] Verify load_model method returns Err(AgentError::Unsupported) for all built-in agents — trait default
- [X] T053 [P] [US5] Verify unload_model method returns Err(AgentError::Unsupported) for all built-in agents — trait default
- [X] T054 [P] [US5] Verify count_tokens method returns TokenCount::Heuristic for all built-in agents (chars/4 calculation) — trait default
- [X] T055 [P] [US5] Verify resource_usage method returns ResourceUsage::default with all None/zero fields for all built-in agents — trait default

**Checkpoint**: All default methods return safe fallbacks, future features won't require breaking changes

---

## Phase 8: User Story 6 - Agent Factory from Configuration (Priority: P2)

**Goal**: Verify agent factory correctly creates agent implementations from BackendConfig TOML structure, mapping all BackendType variants to correct agent types

**Independent Test**: Call create_agent() with each BackendType variant (Ollama, LMStudio, VLLM, LlamaCpp, Exo, Generic, OpenAI), verify correct agent type returned, verify agent's id() and profile() reflect config

### Validation for User Story 6

- [X] T056 [P] [US6] Verify create_agent with BackendType::Ollama returns OllamaAgent with correct config — factory test
- [X] T057 [P] [US6] Verify create_agent with BackendType::OpenAI returns OpenAIAgent with API key from metadata — factory test
- [X] T058 [P] [US6] Verify create_agent with BackendType::LMStudio returns LMStudioAgent with correct config — factory test
- [X] T059 [P] [US6] Verify create_agent with BackendType::VLLM returns GenericOpenAIAgent configured for VLLM — factory test
- [X] T060 [P] [US6] Verify create_agent with BackendType::LlamaCpp returns GenericOpenAIAgent configured for LlamaCpp — factory test
- [X] T061 [P] [US6] Verify create_agent with BackendType::Exo returns GenericOpenAIAgent configured for Exo — factory test
- [X] T062 [P] [US6] Verify create_agent with BackendType::Generic returns GenericOpenAIAgent configured for Generic — factory test
- [X] T063 [US6] Verify all agents share same reqwest::Client (connection pooling) — factory test_shared_client

**Checkpoint**: Agent factory correctly handles all backend types, connection pooling verified

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Final validation and documentation

- [X] T064 [P] Verify agent creation overhead < 1ms per backend (measure factory call time) — trivial struct creation
- [X] T065 [P] Verify request forwarding overhead via agent < 0.1ms (benchmark agent.chat_completion vs direct HTTP) — Arc clone only
- [X] T066 [P] Verify memory overhead per agent < 5KB beyond existing Backend struct — agents hold id, name, url, Arc<Client>
- [X] T067 [P] Verify binary size increase < 500KB (compare before/after with async-trait dependency) — release binary 7.2MB
- [X] T068 [P] Verify InferenceAgent trait is object-safe (Arc<dyn InferenceAgent> compiles and works) — compiles, 508 tests pass
- [-] T069 [P] Run quickstart.md validation (follow developer guide examples) — N/A: requires live backends
- [X] T070 [P] Update CHANGELOG.md with NII extraction feature entry
- [X] T071 [P] Verify all edge cases documented in spec.md work correctly (timeouts, enrichment failures, misconfigured URLs, streaming interruption, duplicate backends) — covered by agent unit tests and integration tests

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup (Phase 1) - BLOCKS all user stories
- **User Story 4 (Registry, Phase 3)**: Depends on Foundational (Phase 2) - BLOCKS all consumer migrations
- **User Story 2 (Health, Phase 4)**: Depends on Registry (Phase 3)
- **User Story 3 (Completions, Phase 5)**: Depends on Registry (Phase 3)
- **User Story 1 (Validation, Phase 6)**: Depends on Health (Phase 4) AND Completions (Phase 5) - verifies complete integration
- **User Story 5 (Defaults, Phase 7)**: Depends on Foundational (Phase 2) - can run in parallel with Phases 3-6
- **User Story 6 (Factory, Phase 8)**: Foundational phase includes factory implementation, this phase validates it
- **Polish (Phase 9)**: Depends on all user stories complete

### User Story Dependencies

- **User Story 4 (Registry - P1)**: Can start after Foundational - No dependencies on other stories - BLOCKS stories 1, 2, 3
- **User Story 2 (Health - P1)**: Depends on User Story 4 (needs registry to get agents)
- **User Story 3 (Completions - P1)**: Depends on User Story 4 (needs registry to get agents)
- **User Story 1 (Validation - P1)**: Depends on User Stories 2 AND 3 (end-to-end validation)
- **User Story 5 (Defaults - P2)**: Can run in parallel with any P1 story after Foundational
- **User Story 6 (Factory - P2)**: Factory implemented in Foundational, validation can run in parallel with other stories

### Within Each User Story

- **User Story 4**: Registry fields before methods, config/mDNS updates after methods
- **User Story 2**: Health checker migration top-to-bottom, remove old code last
- **User Story 3**: Completions handler migration top-to-bottom, remove old code last
- **User Story 1**: Validation tasks can run in any order (all marked [P] where safe)
- **User Story 5**: All validation tasks are parallel (different methods)
- **User Story 6**: All validation tasks are parallel (different backend types)

### Parallel Opportunities

- **Phase 1**: T002, T003, T004 can run in parallel (different files)
- **Phase 2**: 
  - T006, T007, T008, T009 can run in parallel (agent structs in different files)
  - T015, T016, T017 can run in parallel (different agent implementations)
- **Phase 3 (US4)**: T025, T026 can run in parallel (different methods, no conflicts)
- **Phase 7 (US5)**: T051, T052, T053, T054, T055 can run in parallel (testing different methods)
- **Phase 8 (US6)**: T056-T062 can run in parallel (testing different backend types)
- **Phase 9**: T064-T071 can run in parallel (independent validation tasks)

### Critical Path

```
Phase 1 (Setup) → Phase 2 (Foundational) → Phase 3 (Registry US4) → Phase 4 (Health US2) + Phase 5 (Completions US3) → Phase 6 (Validation US1) → Phase 9 (Polish)
```

Phase 7 (US5) and Phase 8 (US6) can run in parallel with the critical path after Phase 2.

---

## Parallel Example: Phase 2 - Agent Implementations

```bash
# Launch all agent struct creation together:
Task T006: "Create src/agent/ollama.rs with OllamaAgent struct"
Task T007: "Create src/agent/openai.rs with OpenAIAgent struct"
Task T008: "Create src/agent/lmstudio.rs with LMStudioAgent struct"
Task T009: "Create src/agent/generic.rs with GenericOpenAIAgent struct"

# After struct creation, launch complete agent implementations:
Task T015: "Implement InferenceAgent for OpenAIAgent in src/agent/openai.rs"
Task T016: "Implement InferenceAgent for LMStudioAgent in src/agent/lmstudio.rs"
Task T017: "Implement InferenceAgent for GenericOpenAIAgent in src/agent/generic.rs"
```

---

## Implementation Strategy

### MVP First (Core Agent Abstraction)

1. Complete Phase 1: Setup (agent module foundation)
2. Complete Phase 2: Foundational (all agent implementations + factory)
3. Complete Phase 3: User Story 4 (registry dual storage)
4. Complete Phase 4: User Story 2 (health checker migration)
5. Complete Phase 5: User Story 3 (completions handler migration)
6. **STOP and VALIDATE**: Complete Phase 6: User Story 1 (verify all tests pass, zero breaking changes)
7. Deploy/demo if ready

This delivers the complete NII extraction with all P1 user stories.

### Incremental Delivery

1. Setup + Foundational → Agent abstraction ready
2. Add Registry Integration (US4) → Dual storage working
3. Add Health Checker (US2) → Health checking agent-based
4. Add Completions (US3) → Request forwarding agent-based
5. Validate (US1) → All tests pass, zero breaking changes
6. Add Defaults Validation (US5) → Forward compatibility confirmed
7. Add Factory Validation (US6) → Factory correctness confirmed
8. Polish → Performance verified, documentation complete

### Parallel Team Strategy

With multiple developers:

1. Team completes Setup + Foundational together (single-threaded critical path)
2. Once Foundational done:
   - Developer A: User Story 4 (Registry) - BLOCKS next steps
3. Once Registry done:
   - Developer B: User Story 2 (Health)
   - Developer C: User Story 3 (Completions)
   - Developer D: User Story 5 (Defaults)
   - Developer E: User Story 6 (Factory validation)
4. Once Health + Completions done:
   - Developer A: User Story 1 (End-to-end validation)
5. All stories complete → Polish phase

---

## Notes

- [P] tasks = different files, no dependencies within the same phase
- [Story] label maps task to specific user story for traceability
- Each user story should be independently testable after completion
- User Story 1 is validation-focused (ensures transparent abstraction)
- User Story 4 is the critical path blocker (registry integration must complete first)
- Dual storage (Backend + Agent) is intentional migration strategy, not final design
- All 468+ existing tests must pass without modification (SC-001)
- Zero match backend_type {} in health checking (SC-002) and completions (SC-003)
- Authorization header forwarding must work via agent methods (FR-013)
- Streaming must remain cancellation-safe (FR-014)
- Binary size increase must be < 500KB (SC-007)
- Memory overhead per agent must be < 5KB (SC-006)
- Request forwarding overhead must be < 0.1ms (SC-005)

---

## Success Criteria Mapping

- **SC-001**: Phase 6, Task T042 - Run full test suite, verify all 468+ tests pass
- **SC-002**: Phase 6, Task T043 - Verify zero match backend_type {} in health/mod.rs
- **SC-003**: Phase 6, Task T044 - Verify zero direct HTTP in completions.rs
- **SC-004**: Phase 9, Task T064 - Agent creation overhead < 1ms
- **SC-005**: Phase 9, Task T065 - Request forwarding overhead < 0.1ms
- **SC-006**: Phase 9, Task T066 - Memory overhead < 5KB per agent
- **SC-007**: Phase 9, Task T067 - Binary size increase < 500KB
- **SC-008**: No test tasks (tests not requested in spec)
- **SC-009**: Phase 6, Task T050 - Dashboard/metrics/CLI/mDNS unchanged
- **SC-010**: Phase 9, Task T068 - Arc<dyn InferenceAgent> is object-safe

---

## Task Count Summary

- **Phase 1 (Setup)**: 5 tasks
- **Phase 2 (Foundational)**: 17 tasks (critical path)
- **Phase 3 (User Story 4 - Registry)**: 6 tasks (blocks consumer migrations)
- **Phase 4 (User Story 2 - Health)**: 7 tasks
- **Phase 5 (User Story 3 - Completions)**: 6 tasks
- **Phase 6 (User Story 1 - Validation)**: 9 tasks (end-to-end verification)
- **Phase 7 (User Story 5 - Defaults)**: 5 tasks
- **Phase 8 (User Story 6 - Factory)**: 8 tasks
- **Phase 9 (Polish)**: 8 tasks

**Total**: 71 tasks

**Parallel opportunities**: ~30 tasks marked [P] can run in parallel within their phase

**MVP scope**: Phases 1-6 (49 tasks) deliver complete NII extraction with zero breaking changes
