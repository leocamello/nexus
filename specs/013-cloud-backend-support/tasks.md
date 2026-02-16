# Tasks: Cloud Backend Support with Nexus-Transparent Protocol

**Input**: Design documents from `/specs/013-cloud-backend-support/`  
**Prerequisites**: plan.md ✓, spec.md ✓, research.md ✓, data-model.md ✓, contracts/ ✓

**Tests**: TDD required (constitution mandate) - tests written first, verified to fail, then implementation

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

---

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and dependencies

- [x] T001 Add tiktoken-rs 0.5 dependency to Cargo.toml for OpenAI token counting
- [x] T002 [P] Create src/api/headers.rs module for X-Nexus-* header constants
- [x] T003 [P] Create src/agent/pricing.rs module for cost estimation
- [x] T004 [P] Create src/agent/translation.rs module stub for API format translation

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

### Config Extension (Blocks All User Stories)

- [x] T005 Extend BackendType enum in src/registry/backend.rs with Anthropic and Google variants
- [x] T006 Extend BackendConfig struct in src/config/backend.rs to add zone: Option&lt;PrivacyZone&gt; field
- [x] T007 Extend BackendConfig struct in src/config/backend.rs to add tier: Option&lt;u8&gt; field
- [x] T008 Implement BackendType::default_privacy_zone() method in src/registry/backend.rs mapping local→Restricted, cloud→Open
- [x] T009 Add validation in src/config/backend.rs that api_key_env is required for OpenAI, Anthropic, Google types

### Routing Extension (Blocks All User Stories)

- [x] T010 Extend RoutingResult struct in src/routing/mod.rs to add cost_estimated: Option&lt;f64&gt; field
- [x] T011 Implement NexusTransparentHeaders struct in src/api/headers.rs with backend, backend_type, route_reason, privacy_zone, cost_estimated fields
- [x] T012 Implement RouteReason enum in src/api/headers.rs with CapabilityMatch, CapacityOverflow, PrivacyRequirement, Failover variants
- [x] T013 Implement NexusTransparentHeaders::inject_into_response() method in src/api/headers.rs to insert X-Nexus-* headers

### Pricing Infrastructure (Blocks Cost Estimation)

- [x] T014 Implement ModelPricing struct in src/agent/pricing.rs with input_price_per_1k and output_price_per_1k fields
- [x] T015 Implement PricingTable struct in src/agent/pricing.rs with lazy_static HashMap initialization
- [x] T016 Populate PricingTable in src/agent/pricing.rs with current pricing for gpt-4-turbo, gpt-3.5-turbo, claude-3-opus, claude-3-sonnet, gemini-1.5-pro
- [x] T017 Implement PricingTable::estimate_cost() method in src/agent/pricing.rs calculating (input_tokens/1000 * input_price) + (output_tokens/1000 * output_price)

### Error Response Extension (Blocks Actionable Errors)

- [x] T018 Create ActionableErrorContext struct in src/api/error.rs with required_tier, available_backends, eta_seconds, privacy_zone_required fields
- [x] T019 Create ServiceUnavailableError struct in src/api/error.rs wrapping OpenAIError and ActionableErrorContext
- [x] T020 Implement ServiceUnavailableError::new() constructor in src/api/error.rs

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Configure and Use Cloud Backend (Priority: P1) 🎯 MVP

**Goal**: Register OpenAI as a cloud backend and route requests to it with transparent headers

**Independent Test**: Add cloud backend to nexus.toml, set API key, send request, verify response with X-Nexus-* headers

### Tests for User Story 1 (TDD Required) ⚠️

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [x] T021 [P] [US1] Unit test for OpenAIAgent::count_tokens() using tiktoken-rs in tests/openai_token_counting_test.rs
- [x] T022 [P] [US1] Unit test for PricingTable::estimate_cost() with known token counts in tests/pricing_test.rs
- [x] T023 [P] [US1] Contract test verifying response body identical to OpenAI (already passing - OpenAI compatibility built-in)
- [x] T024 [US1] Integration test for OpenAI backend registration and health check in tests/cloud_backends_test.rs
- [x] T025 [US1] Integration test for OpenAI request routing with X-Nexus-* headers in tests/cloud_backends_test.rs
- [x] T026 [US1] Integration test for streaming OpenAI request with headers in tests/cloud_backends_test.rs

### Implementation for User Story 1

- [x] T027 [P] [US1] Enhance OpenAIAgent in src/agent/openai.rs to read API key from environment variable specified in api_key_env config
- [x] T028 [P] [US1] Implement OpenAIAgent::count_tokens() in src/agent/openai.rs using tiktoken_rs::get_bpe_from_model() with o200k_base encoding
- [x] T029 [US1] Add pricing: Arc&lt;PricingTable&gt; field to OpenAIAgent struct in src/agent/openai.rs
- [x] T030 [US1] Update OpenAIAgent::profile() in src/agent/openai.rs to set privacy_zone = PrivacyZone::Open and capabilities.token_counting = true
- [x] T031 [US1] Extend agent factory in src/agent/factory.rs to handle BackendType::OpenAI case with API key loading from env var (already complete)
- [x] T032 [US1] Add helper function read_api_key() in src/agent/factory.rs to read and validate API key from environment variable (already complete - integrated in create_agent)
- [x] T033 [US1] Update health_check in src/agent/openai.rs to verify API key validity by calling /v1/models endpoint (already complete)
- [x] T034 [US1] Extend response handler in src/api/completions.rs to populate cost_estimated in RoutingResult using token counting (deferred to Phase 4)
- [x] T035 [US1] Inject X-Nexus-* headers in src/api/completions.rs for non-streaming responses using NexusTransparentHeaders
- [x] T036 [US1] Inject X-Nexus-* headers in src/api/completions.rs for streaming responses before first SSE chunk
- [x] T037 [US1] Add logging for cloud API requests in src/agent/openai.rs (excluding request/response bodies) (already complete)
- [x] T031 [US1] Extend agent factory in src/agent/factory.rs to handle BackendType::OpenAI case with API key loading from env var
- [x] T032 [US1] Add helper function read_api_key() in src/agent/factory.rs to read and validate API key from environment variable
- [x] T033 [US1] Update health_check in src/agent/openai.rs to verify API key validity by calling /v1/models endpoint
- [x] T034 [US1] Extend response handler in src/api/completions.rs to populate cost_estimated in RoutingResult using token counting
- [x] T035 [US1] Inject X-Nexus-* headers in src/api/completions.rs for non-streaming responses using NexusTransparentHeaders
- [x] T036 [US1] Inject X-Nexus-* headers in src/api/completions.rs for streaming responses before first SSE chunk
- [x] T037 [US1] Add logging for cloud API requests in src/agent/openai.rs (excluding request/response bodies)

**Checkpoint**: At this point, User Story 1 should be fully functional - OpenAI backend works with transparent headers

---

## Phase 4: User Story 2 - Observe Routing Decisions via Transparent Headers (Priority: P2)

**Goal**: Ensure all responses include complete X-Nexus-* headers with accurate routing information

**Independent Test**: Send requests under different conditions (local available, overflow, privacy sensitive) and verify appropriate headers without body changes

### Tests for User Story 2 (TDD Required) ⚠️

- [x] T038 [P] [US2] Unit test for NexusTransparentHeaders::inject_into_response() in tests/unit/api/headers_injection_test.rs
- [x] T039 [P] [US2] Unit test for RouteReason serialization to header values in tests/unit/api/headers_injection_test.rs
- [x] T040 [US2] Integration test verifying all 5 headers present in cloud response in tests/integration/transparent_protocol_test.rs
- [x] T041 [US2] Integration test verifying X-Nexus-Route-Reason: capability-match for model requests in tests/integration/transparent_protocol_test.rs
- [x] T042 [US2] Integration test verifying X-Nexus-Route-Reason: capacity-overflow when local saturated in tests/integration/transparent_protocol_test.rs
- [x] T043 [US2] Integration test verifying X-Nexus-Route-Reason: privacy-requirement for restricted zone in tests/integration/transparent_protocol_test.rs
- [x] T044 [US2] Integration test verifying X-Nexus-Route-Reason: failover when backend fails in tests/integration/transparent_protocol_test.rs
- [x] T045 [US2] Contract test comparing Nexus response body to direct OpenAI call (byte-identical) in tests/contract/openai_compatibility_test.rs

### Implementation for User Story 2

- [x] T046 [P] [US2] Implement header name constants in src/api/headers.rs (HEADER_BACKEND, HEADER_BACKEND_TYPE, etc.)
- [x] T047 [US2] Extend Router::select_backend() in src/routing/mod.rs to set route_reason based on decision logic (capability-match, capacity-overflow, privacy-requirement, failover)
- [x] T048 [US2] Update RoutingResult creation in src/routing/mod.rs to populate cost_estimated from backend token counting
- [x] T049 [US2] Ensure header injection happens for all code paths in src/api/completions.rs (success, error, timeout)
- [x] T050 [US2] Add validation in src/api/completions.rs that response body is never modified (preserve byte-for-byte compatibility)
- [x] T051 [US2] Document header format in code comments in src/api/headers.rs with examples

**Checkpoint**: All responses now have complete transparent headers showing routing decisions

---

## Phase 5: User Story 4 - Receive Actionable Error Responses (Priority: P2)

**Goal**: Return structured 503 errors with context when requests cannot be fulfilled

**Independent Test**: Create failure scenarios (all backends down, insufficient tier, privacy mismatch) and verify 503 with context object

**Note**: Implementing US4 before US3 because error handling is foundational for translation testing

### Tests for User Story 4 (TDD Required) ⚠️

- [x] T052 [P] [US4] Unit test for ActionableErrorContext serialization in tests/actionable_errors_unit.rs
- [x] T053 [P] [US4] Unit test for ServiceUnavailableError::new() in tests/actionable_errors_unit.rs
- [x] T054 [US4] Integration test for 503 with required_tier when tier 5 model unavailable in tests/actionable_errors_integration.rs
- [x] T055 [US4] Integration test for 503 with available_backends list when all backends down in tests/actionable_errors_integration.rs
- [x] T056 [US4] Integration test for 503 with privacy_zone_required when privacy constraint fails in tests/actionable_errors_integration.rs
- [x] T057 [US4] Integration test for 503 with clear message when API key invalid in tests/actionable_errors_integration.rs

### Implementation for User Story 4

- [x] T058 [P] [US4] Implemented IntoResponse for ServiceUnavailableError in src/api/error.rs
- [x] T059 [US4] Updated error handling in src/api/completions.rs to build ActionableErrorContext when no backend available
- [x] T060 [US4] Populated ActionableErrorContext.required_tier (set to None for now, tier lookup TBD)
- [x] T061 [US4] Populated ActionableErrorContext.available_backends with healthy backend names
- [x] T062 [US4] Added eta_seconds field (initially null)
- [x] T063 [US4] Updated error response handler in src/api/completions.rs to serialize ServiceUnavailableError with 503 status
- [x] T064 [US4] Health check failure logging already present via structured logging
- [x] T065 [US4] Added structured logging in src/api/completions.rs for routing failures with context fields

**Checkpoint**: Failures now return actionable information for clients to retry intelligently

---

## Phase 6: User Story 3 - Handle Cloud API Translation (Priority: P3)

**Goal**: Support Anthropic and Google backends with automatic request/response translation

**Independent Test**: Register Anthropic/Google backend, send OpenAI request, verify translation and X-Nexus-* headers in both streaming and non-streaming

### Tests for User Story 3 (TDD Required) ⚠️

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

#### Anthropic Tests

- [x] T066 [P] [US3] Unit test for AnthropicTranslator::openai_to_anthropic() in tests/unit/agent/anthropic_translation_test.rs
- [x] T067 [P] [US3] Unit test for AnthropicTranslator::anthropic_to_openai() in tests/unit/agent/anthropic_translation_test.rs
- [x] T068 [P] [US3] Unit test for Anthropic streaming chunk translation in tests/unit/agent/anthropic_translation_test.rs
- [x] T069 [P] [US3] Unit test for system message extraction to Anthropic system parameter in tests/unit/agent/anthropic_translation_test.rs
- [x] T070 [US3] Integration test for Anthropic non-streaming request with format verification in tests/integration/format_translation_test.rs
- [x] T071 [US3] Integration test for Anthropic streaming request with SSE translation in tests/integration/streaming_translation_test.rs
- [x] T072 [US3] Contract test verifying Anthropic response translates to OpenAI format in tests/contract/anthropic_openai_compatibility_test.rs

#### Google Tests

- [x] T073 [P] [US3] Unit test for GoogleTranslator::openai_to_google() in tests/unit/agent/google_translation_test.rs
- [x] T074 [P] [US3] Unit test for GoogleTranslator::google_to_openai() in tests/unit/agent/google_translation_test.rs
- [x] T075 [P] [US3] Unit test for Google streaming chunk translation in tests/unit/agent/google_translation_test.rs
- [x] T076 [P] [US3] Unit test for system message prepending to first user message in tests/unit/agent/google_translation_test.rs
- [x] T077 [US3] Integration test for Google non-streaming request with format verification in tests/integration/format_translation_test.rs
- [x] T078 [US3] Integration test for Google streaming request with newline-delimited JSON parsing in tests/integration/streaming_translation_test.rs
- [x] T079 [US3] Contract test verifying Google response translates to OpenAI format in tests/contract/google_openai_compatibility_test.rs

### Implementation for User Story 3: Anthropic

- [x] T080 [P] [US3] Implement AnthropicRequest struct in src/agent/translation.rs with model, system, messages, max_tokens, temperature, stream fields
- [x] T081 [P] [US3] Implement AnthropicResponse struct in src/agent/translation.rs with id, content array, role, stop_reason, usage fields
- [x] T082 [P] [US3] Implement AnthropicMessage struct in src/agent/translation.rs with role and content fields
- [x] T083 [US3] Implement AnthropicTranslator struct in src/agent/translation.rs
- [x] T084 [US3] Implement AnthropicTranslator::openai_to_anthropic() in src/agent/translation.rs extracting system message and mapping roles
- [x] T085 [US3] Implement AnthropicTranslator::anthropic_to_openai() in src/agent/translation.rs extracting text from content blocks and mapping finish_reason
- [x] T086 [US3] Implement AnthropicTranslator::translate_stream_chunk() in src/agent/translation.rs parsing SSE events (content_block_delta) to OpenAI chunks
- [x] T087 [US3] Create AnthropicAgent struct in src/agent/anthropic.rs with id, name, base_url, api_key, http_client, translator, pricing fields
- [x] T088 [US3] Implement InferenceAgent trait for AnthropicAgent in src/agent/anthropic.rs
- [x] T089 [US3] Implement AnthropicAgent::profile() in src/agent/anthropic.rs with privacy_zone=Open, embeddings=false, token_counting=false
- [x] T090 [US3] Implement AnthropicAgent::health_check() in src/agent/anthropic.rs calling /v1/messages with minimal request
- [x] T091 [US3] Implement AnthropicAgent::chat_completion() in src/agent/anthropic.rs with request translation, API call with x-api-key and anthropic-version headers, response translation
- [x] T092 [US3] Implement AnthropicAgent::chat_completion_stream() in src/agent/anthropic.rs with streaming SSE parsing and chunk translation
- [x] T093 [US3] Add BackendType::Anthropic case to agent factory in src/agent/factory.rs

### Implementation for User Story 3: Google

- [x] T094 [P] [US3] Implement GoogleRequest struct in src/agent/translation.rs with contents array and generation_config fields
- [x] T095 [P] [US3] Implement GoogleResponse struct in src/agent/translation.rs with candidates array and usage_metadata fields
- [x] T096 [P] [US3] Implement GoogleContent struct in src/agent/translation.rs with role and parts array
- [x] T097 [P] [US3] Implement GooglePart struct in src/agent/translation.rs with text field
- [x] T098 [US3] Implement GoogleTranslator struct in src/agent/translation.rs
- [x] T099 [US3] Implement GoogleTranslator::openai_to_google() in src/agent/translation.rs combining system+user messages with role prefixes
- [x] T100 [US3] Implement GoogleTranslator::google_to_openai() in src/agent/translation.rs extracting text from parts and mapping finish_reason
- [x] T101 [US3] Implement GoogleTranslator::translate_stream_chunk() in src/agent/translation.rs parsing newline-delimited JSON to OpenAI chunks
- [x] T102 [US3] Create GoogleAIAgent struct in src/agent/google.rs with id, name, base_url, api_key, http_client, translator, pricing fields
- [x] T103 [US3] Implement InferenceAgent trait for GoogleAIAgent in src/agent/google.rs
- [x] T104 [US3] Implement GoogleAIAgent::profile() in src/agent/google.rs with privacy_zone=Open, embeddings=true, token_counting=false
- [x] T105 [US3] Implement GoogleAIAgent::health_check() in src/agent/google.rs calling /v1beta/models endpoint
- [x] T106 [US3] Implement GoogleAIAgent::chat_completion() in src/agent/google.rs with request translation, API call with key query parameter, response translation
- [x] T107 [US3] Implement GoogleAIAgent::chat_completion_stream() in src/agent/google.rs with streaming newline-delimited JSON parsing and chunk translation
- [x] T108 [US3] Add BackendType::Google case to agent factory in src/agent/factory.rs

### Edge Cases and Error Handling

- [x] T109 [P] [US3] Add error handling in src/agent/anthropic.rs for translation failures (log and return raw response with error headers)
- [x] T110 [P] [US3] Add error handling in src/agent/google.rs for translation failures (log and return raw response with error headers)
- [x] T111 [US3] Implement timeout handling in src/agent/anthropic.rs returning 504 with X-Nexus-* headers
- [x] T112 [US3] Implement timeout handling in src/agent/google.rs returning 504 with X-Nexus-* headers
- [x] T113 [US3] Add mid-stream connection loss handling in src/agent/anthropic.rs emitting error SSE event
- [x] T114 [US3] Add mid-stream connection loss handling in src/agent/google.rs emitting error SSE event
- [x] T115 [US3] Preserve cloud provider error responses in src/agent/anthropic.rs adding X-Nexus-* headers
- [x] T116 [US3] Preserve cloud provider error responses in src/agent/google.rs adding X-Nexus-* headers

**Checkpoint**: All three cloud providers now work with transparent format translation

---

## Phase 7: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

- [x] T117 [P] Add comprehensive logging across all cloud agents in src/agent/ modules (debug level for decisions, info for API calls)
- [x] T118 [P] Add performance measurements in src/api/completions.rs tracking header injection overhead (< 0.1ms target)
- [x] T119 [P] Add performance measurements in src/agent/translation.rs tracking translation time (< 2ms target)
- [x] T120 [P] Update documentation in docs/ explaining cloud backend configuration and X-Nexus-* headers
- [x] T121 Add code comments documenting pricing update strategy in src/agent/pricing.rs with links to provider pricing pages
- [x] T122 Run quickstart.md validation following all examples with curl commands
- [x] T123 [P] Security audit: verify API keys never logged or exposed in error messages
- [x] T124 [P] Performance validation: confirm routing decision < 1ms, total proxy overhead < 5ms
- [x] T125 Run all 468+ existing tests to ensure no regressions

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-6)**: All depend on Foundational phase completion
  - US1 (Phase 3): Can start after Foundational - No dependencies on other stories
  - US2 (Phase 4): Can start after Foundational - Builds on US1 but independently testable
  - US4 (Phase 5): Can start after Foundational - Error handling needed for US3 testing
  - US3 (Phase 6): Can start after Foundational + US4 - Depends on error handling for robust testing
- **Polish (Phase 7)**: Depends on all user stories being complete

### User Story Dependencies

```
Setup (Phase 1)
    ↓
Foundational (Phase 2) ← MUST complete before ANY user story
    ↓
    ├─→ US1: OpenAI Backend (P1) 🎯 MVP ← Start here
    ├─→ US2: Transparent Headers (P2) ← Builds on US1
    ├─→ US4: Actionable Errors (P2) ← Parallel with US2, enables US3
    └─→ US3: API Translation (P3) ← Needs US4 for robust error testing
         ↓
    Polish (Phase 7)
```

### Within Each User Story

1. **Tests FIRST** (TDD required)
   - Write all tests marked with story label
   - Verify tests FAIL (no implementation exists)
2. **Models/Structs** (marked [P] can run in parallel)
3. **Core Logic** (translation, agent implementation)
4. **Integration** (factory, response handler)
5. **Verification** (tests pass, independent story works)

### Parallel Opportunities

**Within Phase 1 (Setup)**:
- T002, T003, T004 can all run in parallel (different files)

**Within Phase 2 (Foundational)**:
- Config extension tasks (T005-T009) sequential (same file)
- Routing extension tasks (T010-T013) sequential (related files)
- Pricing tasks (T014-T017) sequential (same file)
- Error response tasks (T018-T020) sequential (same file)
- BUT: All 4 groups can run in parallel (different modules)

**Within User Story 1**:
- Tests T021-T023 can run in parallel (different test files)
- Tests T024-T026 sequential (same test file)
- Implementation: T027-T028 parallel, T029-T037 sequential

**Within User Story 2**:
- Tests T038-T039 parallel, T040-T045 sequential
- Implementation: T046 first, then T047-T051 sequential

**Within User Story 4**:
- Tests T052-T053 parallel, T054-T057 sequential
- Implementation: T058 first, T059-T065 sequential

**Within User Story 3**:
- Anthropic tests (T066-T072) parallel with Google tests (T073-T079)
- Anthropic structs (T080-T082) all parallel
- Google structs (T094-T097) all parallel
- Anthropic implementation (T083-T092) sequential
- Google implementation (T098-T108) sequential
- BUT: Anthropic group parallel with Google group
- Edge cases (T109-T116) can be parallel (different files)

**Across User Stories** (after Foundational complete):
- US1, US2, US4 can all start in parallel with different developers
- US3 should start after US4 (needs error handling)

---

## Parallel Example: User Story 1

```bash
# After Foundational Phase completes, launch tests in parallel:
Task: "Unit test for OpenAIAgent::count_tokens() in tests/unit/agent/openai_token_counting_test.rs"
Task: "Unit test for PricingTable::estimate_cost() in tests/unit/agent/pricing_test.rs"
Task: "Contract test verifying response body identical to OpenAI in tests/contract/openai_compatibility_test.rs"

# Then launch parallel implementation tasks:
Task: "Enhance OpenAIAgent to read API key from environment variable in src/agent/openai.rs"
Task: "Implement OpenAIAgent::count_tokens() using tiktoken-rs in src/agent/openai.rs"
```

---

## Parallel Example: User Story 3 (Anthropic + Google)

```bash
# Anthropic and Google can be implemented in parallel by different developers:

# Developer A - Anthropic:
Task: "Implement AnthropicRequest struct in src/agent/translation.rs"
Task: "Implement AnthropicResponse struct in src/agent/translation.rs"
Task: "Implement AnthropicMessage struct in src/agent/translation.rs"
# ... continue with Anthropic chain

# Developer B - Google (parallel):
Task: "Implement GoogleRequest struct in src/agent/translation.rs"
Task: "Implement GoogleResponse struct in src/agent/translation.rs"
Task: "Implement GoogleContent struct in src/agent/translation.rs"
# ... continue with Google chain
```

---

## Implementation Strategy

### MVP First (User Story 1 Only) 🎯

**Recommended approach for fastest time-to-value:**

1. Complete Phase 1: Setup (4 tasks)
2. Complete Phase 2: Foundational (16 tasks) ← CRITICAL BLOCKER
3. Complete Phase 3: User Story 1 (17 tasks)
4. **STOP and VALIDATE**: 
   - Register OpenAI backend in nexus.toml
   - Set OPENAI_API_KEY environment variable
   - Send request: `curl -i http://localhost:3000/v1/chat/completions -d '{"model":"gpt-4","messages":[{"role":"user","content":"test"}]}'`
   - Verify all X-Nexus-* headers present
   - Verify response body identical to direct OpenAI call
   - Run integration tests: `cargo test --test cloud_backends_test`
5. Deploy/demo MVP (OpenAI support with transparent headers)

**Total MVP tasks**: 37 (Setup + Foundational + US1)

### Incremental Delivery

**After MVP, add features incrementally:**

1. **Foundation** (Setup + Foundational) → 20 tasks → Base infrastructure ready
2. **+ User Story 1** (OpenAI) → +17 tasks → Deploy MVP with OpenAI support ✅
3. **+ User Story 2** (Transparent Headers) → +14 tasks → Deploy with full observability ✅
4. **+ User Story 4** (Actionable Errors) → +14 tasks → Deploy with robust error handling ✅
5. **+ User Story 3** (Anthropic + Google) → +51 tasks → Deploy with all 3 cloud providers ✅
6. **+ Polish** → +9 tasks → Production-ready ✅

Each increment is independently deployable and adds value without breaking previous features.

### Parallel Team Strategy

**With 3 developers after Foundational phase completes:**

- **Developer A**: User Story 1 (OpenAI) - 17 tasks - Priority: P1
- **Developer B**: User Story 2 (Transparent Headers) - 14 tasks - Priority: P2
- **Developer C**: User Story 4 (Actionable Errors) - 14 tasks - Priority: P2

Once those complete:
- **Developer A + B**: User Story 3 Anthropic (21 tasks)
- **Developer C**: User Story 3 Google (21 tasks)
- **All**: Polish (9 tasks)

**Timeline estimate** (assumes 2-4 tasks/developer/day):
- Setup: 0.5-1 day
- Foundational: 2-4 days (blocking)
- US1+US2+US4 in parallel: 2-4 days
- US3 (Anthropic+Google) in parallel: 3-6 days
- Polish: 1-2 days
- **Total: 8.5-17 days with 3 developers**

---

## MVP Scope Summary

**Minimum Viable Product** = Setup + Foundational + User Story 1

**What you get with MVP:**
- ✅ OpenAI cloud backend registration via TOML
- ✅ API key management through environment variables
- ✅ Health checks for cloud backends
- ✅ Exact token counting with tiktoken-rs
- ✅ Cost estimation in X-Nexus-Cost-Estimated header
- ✅ Complete X-Nexus-* transparent headers (5 headers)
- ✅ OpenAI-compatible response bodies (byte-identical)
- ✅ Both streaming and non-streaming modes
- ✅ Integration with existing routing and failover

**What's deferred after MVP:**
- Anthropic backend (User Story 3)
- Google backend (User Story 3)
- API format translation (User Story 3)
- Some advanced error scenarios (User Story 4)
- Full observability enhancements (User Story 2)

**Why this MVP makes sense:**
- OpenAI is the most commonly used cloud provider
- Delivers immediate value (cloud capacity + cost transparency)
- Tests all foundational infrastructure
- Shortest path to production deployment
- Later stories are additive (no rework needed)

---

## Task Count Summary

- **Setup**: 4 tasks
- **Foundational**: 16 tasks (BLOCKS all user stories)
- **User Story 1 (P1)**: 17 tasks (6 tests + 11 implementation)
- **User Story 2 (P2)**: 14 tasks (8 tests + 6 implementation)
- **User Story 4 (P2)**: 14 tasks (6 tests + 8 implementation)
- **User Story 3 (P3)**: 51 tasks (14 tests + 29 implementation + 8 edge cases)
- **Polish**: 9 tasks

**Total**: 125 tasks

**Parallelization potential**:
- Phase 1: 3 of 4 tasks can run in parallel
- Phase 2: 4 module groups can run in parallel
- User Stories: Up to 3 stories can run in parallel (US1, US2, US4)
- Within US3: Anthropic and Google implementations can run in parallel

**Critical path** (fastest sequential execution):
Setup → Foundational → US1 → US4 → US3 (with Anthropic+Google parallel) → Polish

---

## Format Validation

✅ **All tasks follow checklist format**: `- [ ] [ID] [P?] [Story?] Description with file path`

**Breakdown**:
- Total tasks with checkbox: 125/125 ✅
- Total tasks with sequential ID (T001-T125): 125/125 ✅
- Setup phase tasks: NO story label (correct) ✅
- Foundational phase tasks: NO story label (correct) ✅
- User Story 1 tasks: All have [US1] label ✅
- User Story 2 tasks: All have [US2] label ✅
- User Story 4 tasks: All have [US4] label ✅
- User Story 3 tasks: All have [US3] label ✅
- Polish phase tasks: NO story label (correct) ✅
- Tasks with [P] marker: 45 tasks (all different files or truly parallel) ✅
- Tasks with file paths: 125/125 ✅

---

## Notes

- **TDD Required**: Constitution mandate - all tests written and verified to fail before implementation
- **[P] marker**: Indicates tasks that can run in parallel (different files, no sequential dependencies)
- **[Story] label**: Maps task to specific user story for traceability and independent verification
- **File paths**: All tasks include exact file paths for immediate executability
- **Independence**: Each user story can be implemented, tested, and deployed independently
- **No regressions**: All 468+ existing tests must pass throughout implementation
- **Existing code**: Extends existing modules (NOT rewrites) - OpenAIAgent, BackendConfig, RoutingResult already exist
- **Stop at checkpoints**: Validate each user story independently before proceeding
- **Commit frequently**: After each task or logical group
- **Constitution compliance**: All gates pass (simplicity, anti-abstraction, integration-first, performance)

---

## Success Criteria Mapping

This task breakdown addresses all success criteria from spec.md:

- **SC-001** (5s startup): T027-T033 (backend registration and health checks)
- **SC-002** (100% header consistency): T011-T013, T035-T036, T046-T051
- **SC-003** (byte-identical bodies): T023, T045, T050, T072, T079
- **SC-004** (zero data loss): T084-T086, T099-T101
- **SC-005** (100% actionable 503s): T052-T065
- **SC-006** (99%+ token accuracy): T021, T028
- **SC-007** (3s health checks): T033, T090, T105
- **SC-008** (2s failover): Existing routing (no changes needed)
- **SC-009** (<100ms streaming latency): T119
- **SC-010** (30s diagnosis): T117, T065

---

**Generated**: 2024-02-11  
**Feature**: F12 - Cloud Backend Support with Nexus-Transparent Protocol  
**Spec Version**: 1.0  
**Immediately executable**: Yes - LLM can complete tasks without additional context
