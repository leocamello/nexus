# Tasks: Cloud Backend Support with Nexus-Transparent Protocol

**Input**: Design documents from `/specs/013-cloud-backend/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/

**Tests**: This feature does NOT explicitly request TDD, so tests are OPTIONAL and included only for critical functionality validation.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

- **Single project**: Rust binary at repository root
- Paths: `src/`, `tests/` at `/home/lhnascimento/Projects/nexus/`

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and dependency setup for cloud backend support

- [ ] T001 Add tiktoken-rs = "0.5" dependency to Cargo.toml for OpenAI token counting
- [X] T002 [P] Add BackendType enum variants (Anthropic, Google) to src/registry/mod.rs or src/agent/types.rs
- [X] T003 [P] Extend BackendConfig struct with zone: PrivacyZone and tier: Option<u8> fields in src/config/backend.rs

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [ ] T004 Create PricingConfig struct and pricing module in src/pricing/mod.rs with provider/model pricing structure
- [ ] T005 [P] Add pricing configuration loading in src/config/mod.rs to parse [pricing.{provider}] sections from TOML
- [X] T006 [P] Create RouteReason enum (capability-match, capacity-overflow, privacy-requirement, backend-failover) in src/api/headers.rs
- [X] T007 Create NexusHeaders struct in src/api/headers.rs with backend, backend_type, route_reason, privacy_zone, cost_estimated fields
- [X] T008 Implement NexusHeaders::inject_into() method in src/api/headers.rs to serialize headers into axum HeaderMap
- [X] T009 Create ActionableErrorContext struct in src/api/error.rs with required_tier, available_backends, eta_seconds fields
- [X] T010 Implement create_503_response() function in src/api/error.rs returning OpenAI-compatible error with context
- [ ] T011 Create estimate_cost() function in src/pricing/mod.rs accepting provider, model, prompt_tokens, completion_tokens and returning Option<f32>

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Configure Cloud Backend for Overflow Capacity (Priority: P1) 🎯 MVP

**Goal**: Enable OpenAI GPT-4 as a cloud backend to handle overflow traffic when local inference servers reach capacity. Routing decisions visible in response headers.

**Independent Test**: Configure one cloud backend, send requests that exceed local capacity, verify requests routed to cloud backend with appropriate X-Nexus-* headers in responses.

### Implementation for User Story 1

- [ ] T012 [P] [US1] Enhance OpenAIAgent struct in src/agent/openai.rs to add encoding_o200k: Arc<CoreBPE> and encoding_cl100k: Arc<CoreBPE> fields
- [ ] T013 [US1] Update OpenAIAgent::new() in src/agent/openai.rs to load tiktoken encodings (o200k_base and cl100k_base) and cache in Arc
- [ ] T014 [P] [US1] Implement count_tokens() method in src/agent/openai.rs selecting encoding by model family and returning TokenCount::Exact
- [ ] T015 [P] [US1] Update OpenAIAgent::profile() in src/agent/openai.rs to set token_counting capability to true
- [ ] T016 [US1] Update agent factory in src/agent/mod.rs or src/registry/mod.rs to recognize BackendType::OpenAI and create OpenAIAgent from config with api_key loaded from api_key_env
- [ ] T017 [US1] Enhance health_check validation in OpenAIAgent (src/agent/openai.rs) to detect authentication failures and return HealthStatus::Unhealthy on 401/403
- [ ] T018 [US1] Modify routing logic in src/routing/mod.rs to check zone and tier constraints when selecting backends (filter by PrivacyZone if needed)
- [ ] T019 [US1] Inject X-Nexus-* headers in chat_completion handler in src/api/completions.rs after agent.chat_completion() by calling NexusHeaders::inject_into()
- [ ] T020 [US1] Add cost estimation call in src/api/completions.rs using estimate_cost() from pricing module and include in NexusHeaders if Some
- [ ] T021 [US1] Update error handling in src/api/completions.rs routing flow to call create_503_response() when no backend available with ActionableErrorContext

**Checkpoint**: At this point, User Story 1 should be fully functional - OpenAI backends configured, routed to on overflow, headers present

---

## Phase 4: User Story 2 - Transparent Routing Visibility (Priority: P2)

**Goal**: Enable debugging of routing decisions through X-Nexus-* response headers showing backend name, type, route reason, privacy zone, and estimated cost.

**Independent Test**: Send requests through Nexus and validate that all X-Nexus-* headers are present and contain accurate information about routing decisions, backend types, and costs.

### Implementation for User Story 2

- [ ] T022 [P] [US2] Add determine_route_reason() function in src/routing/mod.rs or src/api/completions.rs to determine RouteReason from routing decision context
- [ ] T023 [P] [US2] Enhance NexusHeaders creation in src/api/completions.rs to populate route_reason field by analyzing routing decision (capacity check, capability match, failover, etc)
- [ ] T024 [US2] Add privacy_zone field population in NexusHeaders from backend.zone in src/api/completions.rs
- [ ] T025 [US2] Implement streaming response header injection in src/api/completions.rs for chat_completion_stream handler to inject headers before first SSE chunk
- [ ] T026 [US2] Add validation in src/api/completions.rs to ensure X-Nexus-Cost-Estimated header is only included for cloud backends (check backend_type)
- [ ] T027 [US2] Add logging for routing decisions in src/api/completions.rs including backend selected, route reason, and cost (if applicable)

**Checkpoint**: At this point, User Stories 1 AND 2 should both work independently - all headers present and accurate

---

## Phase 5: User Story 3 - Multi-Provider Cloud Support (Priority: P3)

**Goal**: Enable Anthropic Claude and Google Gemini as cloud backends with automatic API translation between provider formats and OpenAI-compatible format.

**Independent Test**: Configure Anthropic and Google backends, send identical requests through Nexus, verify that responses are correctly translated to OpenAI format regardless of which cloud backend served the request.

### Implementation for User Story 3

#### Anthropic Agent

- [X] T028 [P] [US3] Create AnthropicAgent struct in src/agent/anthropic.rs with id, name, base_url, api_key, client fields
- [X] T029 [P] [US3] Define Anthropic API types in src/agent/anthropic.rs: AnthropicRequest, AnthropicMessage, AnthropicResponse, AnthropicContent, AnthropicUsage
- [X] T030 [US3] Implement translate_request() function in src/agent/anthropic.rs to extract system message and convert OpenAI messages to Anthropic format
- [X] T031 [P] [US3] Implement translate_response() function in src/agent/anthropic.rs to flatten content[0].text and map stop_reason to finish_reason
- [X] T032 [US3] Implement InferenceAgent::health_check() for AnthropicAgent in src/agent/anthropic.rs using /v1/messages endpoint with minimal request
- [X] T033 [P] [US3] Implement InferenceAgent::list_models() for AnthropicAgent in src/agent/anthropic.rs returning static model list (Claude variants)
- [X] T034 [US3] Implement InferenceAgent::chat_completion() for AnthropicAgent in src/agent/anthropic.rs with request translation, x-api-key and anthropic-version headers, and response translation
- [X] T035 [US3] Update agent factory in src/agent/mod.rs or src/registry/mod.rs to recognize BackendType::Anthropic and create AnthropicAgent from config

#### Google Agent

- [ ] T036 [P] [US3] Create GoogleAgent struct in src/agent/google.rs with id, name, base_url, api_key, client, model_endpoint_base fields
- [ ] T037 [P] [US3] Define Google AI API types in src/agent/google.rs: GoogleRequest, GoogleContent, GoogleResponse, GoogleCandidate, GoogleUsageMetadata
- [ ] T038 [US3] Implement translate_request() function in src/agent/google.rs to convert messages to contents with role mapping (assistant→model) and parts structure
- [ ] T039 [P] [US3] Implement translate_response() function in src/agent/google.rs to extract candidates[0].content and map finishReason to finish_reason
- [ ] T040 [US3] Implement InferenceAgent::health_check() for GoogleAgent in src/agent/google.rs using /v1beta/models endpoint
- [ ] T041 [P] [US3] Implement InferenceAgent::list_models() for GoogleAgent in src/agent/google.rs returning static model list (Gemini variants)
- [ ] T042 [US3] Implement InferenceAgent::chat_completion() for GoogleAgent in src/agent/google.rs with request translation, x-goog-api-key header, and response translation
- [ ] T043 [US3] Update agent factory in src/agent/mod.rs or src/registry/mod.rs to recognize BackendType::Google and create GoogleAgent from config

#### Integration and Failover

- [ ] T044 [US3] Implement failover logic in src/api/completions.rs or src/routing/mod.rs to retry with alternative cloud provider on upstream failure
- [ ] T045 [US3] Update NexusHeaders creation in src/api/completions.rs to set route_reason to backend-failover when failover occurs
- [ ] T046 [US3] Add pricing entries in example config for Anthropic and Google models (use contracts/cloud-config.toml as reference)

**Checkpoint**: All three cloud providers (OpenAI, Anthropic, Google) should work with format translation and failover

---

## Phase 6: User Story 4 - Actionable Error Responses (Priority: P3)

**Goal**: Provide structured 503 error responses with context object containing required_tier, available_backends list, and eta_seconds for capacity availability.

**Independent Test**: Saturate all backends and send a request that cannot be routed, then validate the 503 response contains the context object with actionable information.

### Implementation for User Story 4

- [ ] T047 [P] [US4] Implement BackendStatus struct in src/api/error.rs with name, status (healthy/unhealthy/at_capacity), zone, tier fields
- [ ] T048 [US4] Enhance create_503_response() in src/api/error.rs to accept ActionableErrorContext and serialize as context field in error object
- [ ] T049 [P] [US4] Implement estimate_eta() function in src/routing/mod.rs or src/api/error.rs to calculate eta_seconds based on queue depth or average request time
- [ ] T050 [US4] Update routing failure handling in src/api/completions.rs to populate ActionableErrorContext with required_tier from request, available_backends from registry, and eta_seconds from estimate
- [ ] T051 [US4] Add error code classification in src/api/error.rs (capacity_exceeded, no_capable_backend, privacy_constraint_violation) based on failure reason
- [ ] T052 [US4] Update registry query in src/api/completions.rs to collect all backends matching capability for available_backends list in error context
- [ ] T053 [US4] Add status determination logic in src/api/error.rs to classify backends as healthy/unhealthy/at_capacity based on health check and capacity metrics

**Checkpoint**: All user stories should now be independently functional with actionable error responses

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

- [ ] T054 [P] Add integration tests in tests/integration/test_cloud_backends.rs validating OpenAI agent with mock HTTP responses
- [ ] T055 [P] Add integration tests in tests/integration/test_api_translation.rs validating Anthropic and Google translation functions
- [ ] T056 [P] Add contract tests in tests/contract/test_nexus_headers.rs validating all 5 X-Nexus-* headers are present and formatted correctly
- [ ] T057 [P] Add contract tests in tests/contract/test_actionable_errors.rs validating 503 response structure matches actionable-error.json schema
- [ ] T058 [P] Add unit tests in tests/unit/test_token_counting.rs validating tiktoken-rs integration for both encodings
- [ ] T059 [P] Add unit tests in tests/unit/test_cost_estimation.rs validating cost calculation formula with sample pricing
- [ ] T060 [P] Add unit tests in tests/unit/test_streaming_headers.rs validating headers present in streaming responses before first chunk
- [ ] T061 Update documentation in docs/ or README.md with cloud backend configuration examples from contracts/cloud-config.toml
- [ ] T062 [P] Add error handling for missing environment variables (api_key_env) during agent creation with clear error messages
- [ ] T063 [P] Add logging for cloud API authentication failures with security-safe error messages (no key exposure)
- [ ] T064 Run quickstart.md validation following all 5 implementation steps and testing checklist
- [ ] T065 Performance validation: Verify header injection adds <0.1ms overhead per request
- [ ] T066 Performance validation: Verify token counting completes in <1ms for typical prompts (2KB)
- [ ] T067 [P] Security review: Ensure API keys never logged or exposed in error messages

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-6)**: All depend on Foundational phase completion
  - User stories can proceed in parallel (if staffed)
  - Or sequentially in priority order (P1 → P2 → P3 → P3)
- **Polish (Phase 7)**: Depends on all desired user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - No dependencies on other stories
  - Deliverable: OpenAI cloud backend working with headers and cost estimation
- **User Story 2 (P2)**: Can start after Foundational (Phase 2) - Extends US1 routing decisions
  - Dependency: Uses routing infrastructure from US1
  - Deliverable: Complete header transparency with routing visibility
- **User Story 3 (P3)**: Can start after Foundational (Phase 2) - Independent of US1/US2
  - Deliverable: Anthropic and Google cloud backends with API translation
- **User Story 4 (P3)**: Can start after Foundational (Phase 2) - Uses routing infrastructure
  - Light dependency: Uses routing/registry from US1 for backend status
  - Deliverable: Actionable 503 errors with context

### Within Each User Story

- Models/types before translation functions
- Translation functions before agent implementation
- Agent implementation before factory registration
- Factory registration before routing integration
- Routing integration before header injection

### Parallel Opportunities

- All Setup tasks (T001-T003) can run in parallel
- Most Foundational tasks (T005-T011) marked [P] can run in parallel after T004
- Within US1: T012+T014+T015 can run in parallel (different OpenAI agent methods)
- Within US2: T022-T024 can run in parallel (different header population logic)
- Within US3: All Anthropic tasks (T028-T034) can run in parallel with all Google tasks (T036-T043)
- All Polish phase tasks (T054-T067) marked [P] can run in parallel

---

## Parallel Example: User Story 3 (Multi-Provider)

```bash
# Launch Anthropic agent development:
Task: "Create AnthropicAgent struct in src/agent/anthropic.rs"
Task: "Define Anthropic API types in src/agent/anthropic.rs"
Task: "Implement translate_request() function in src/agent/anthropic.rs"
Task: "Implement translate_response() function in src/agent/anthropic.rs"

# Launch Google agent development (PARALLEL with Anthropic):
Task: "Create GoogleAgent struct in src/agent/google.rs"
Task: "Define Google AI API types in src/agent/google.rs"
Task: "Implement translate_request() function in src/agent/google.rs"
Task: "Implement translate_response() function in src/agent/google.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T003)
2. Complete Phase 2: Foundational (T004-T011) - CRITICAL - blocks all stories
3. Complete Phase 3: User Story 1 (T012-T021)
4. **STOP and VALIDATE**: Test OpenAI cloud backend independently
   - Configure OpenAI backend in TOML
   - Set OPENAI_API_KEY environment variable
   - Send request, verify response has all X-Nexus-* headers
   - Verify cost estimation in header
5. Deploy/demo if ready

### Incremental Delivery

1. Complete Setup + Foundational → Foundation ready
2. Add User Story 1 → Test independently → Deploy/Demo (OpenAI cloud backend - MVP!)
3. Add User Story 2 → Test independently → Deploy/Demo (routing visibility enhanced)
4. Add User Story 3 → Test independently → Deploy/Demo (Anthropic + Google support)
5. Add User Story 4 → Test independently → Deploy/Demo (actionable errors)
6. Each story adds value without breaking previous stories

### Parallel Team Strategy

With multiple developers:

1. Team completes Setup + Foundational together (T001-T011)
2. Once Foundational is done:
   - Developer A: User Story 1 (OpenAI enhancement + headers)
   - Developer B: User Story 3 Anthropic (T028-T035)
   - Developer C: User Story 3 Google (T036-T043)
3. After all complete:
   - Any developer: User Story 2 (header visibility refinement)
   - Any developer: User Story 4 (actionable errors)

---

## Summary

**Total Tasks**: 67 tasks organized into 7 phases

**Task Breakdown by Phase**:
- Phase 1 (Setup): 3 tasks
- Phase 2 (Foundational): 8 tasks
- Phase 3 (US1 - Cloud Backend Config): 10 tasks
- Phase 4 (US2 - Routing Visibility): 6 tasks
- Phase 5 (US3 - Multi-Provider): 19 tasks
- Phase 6 (US4 - Actionable Errors): 7 tasks
- Phase 7 (Polish): 14 tasks

**Task Breakdown by Story**:
- US1 (P1): 10 implementation tasks (T012-T021)
- US2 (P2): 6 implementation tasks (T022-T027)
- US3 (P3): 19 implementation tasks (T028-T046)
- US4 (P3): 7 implementation tasks (T047-T053)

**Parallel Opportunities**: 28 tasks marked [P] can run in parallel within their phase

**Independent Test Criteria**:
- US1: Configure OpenAI, exceed local capacity, verify routing and headers
- US2: Send requests, validate all X-Nexus-* headers present and accurate
- US3: Configure multiple providers, verify translation and format compatibility
- US4: Saturate all backends, verify 503 with structured context

**Suggested MVP Scope**: Phase 1 + Phase 2 + Phase 3 (US1 only)
- Delivers core value: cloud backend overflow capacity with OpenAI
- ~21 tasks to complete for MVP
- Estimated time: 1-2 days for experienced Rust developer

**Format Validation**: ✅ All tasks follow the checklist format:
- Checkbox: `- [ ]` present
- Task ID: Sequential (T001-T067)
- [P] marker: Present for parallelizable tasks (28 tasks)
- [Story] label: Present for user story phases (US1, US2, US3, US4)
- Description: Clear action with exact file path

---

## Notes

- Tests are OPTIONAL in this feature (not explicitly requested) but included for critical validation
- [P] tasks = different files or independent logic, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Follow Constitution principles: simplicity, no premature optimization, OpenAI compatibility
- Keep response body unchanged (headers only per Constitution Principle III)
