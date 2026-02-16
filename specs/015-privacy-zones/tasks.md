# Tasks: Privacy Zones & Capability Tiers

**Feature Branch**: `015-privacy-zones`  
**Input**: Design documents from `/home/lhnascimento/Projects/nexus/specs/015-privacy-zones/`  
**Prerequisites**: plan.md ✅, spec.md ✅, research.md ✅, data-model.md ✅, contracts/ ✅, quickstart.md ✅

**Tests**: Not explicitly requested in spec - focus on implementation with validation testing only

**Organization**: Tasks are grouped by user story (P1, P2, P3) to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4, US5)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Configuration schema extensions for privacy zones and capability tiers

- [ ] T001 [P] Extend PrivacyZone enum in src/agent/types.rs with Default implementation (fail-safe: Restricted)
- [ ] T002 [P] Create CapabilityTier struct in src/config/backend.rs with reasoning, coding, context_window, vision, tools fields
- [ ] T003 [P] Create CapabilityRequirements struct in src/config/routing.rs with optional min_* fields
- [ ] T004 [P] Create RoutingPreference enum in src/routing/requirements.rs (Strict, Flexible with Strict default)
- [ ] T005 [P] Create OverflowMode enum in src/config/routing.rs (BlockEntirely, FreshOnly with BlockEntirely default)
- [ ] T006 Extend BackendConfig in src/config/backend.rs to add capability_tier field (optional CapabilityTier)
- [ ] T007 Validate capability_tier scores (0-10 range for reasoning/coding, context_window > 0) at config load in src/config/backend.rs

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [ ] T008 Create TrafficPolicy struct in src/config/routing.rs with pattern, privacy, overflow_mode, capabilities fields
- [ ] T009 Add TrafficPolicy::matches() method using glob crate for pattern matching in src/config/routing.rs
- [ ] T010 Add TrafficPolicy::priority() method for specificity ordering (exact=100, glob=50, wildcard=10) in src/config/routing.rs
- [ ] T011 Extend RoutingConfig in src/config/routing.rs to add policies HashMap field with serde default
- [ ] T012 [P] Create RejectionReason enum in src/control/mod.rs with variants for privacy/tier/overflow violations
- [ ] T013 [P] Implement RejectionReason::message() method for human-readable error messages in src/control/mod.rs
- [ ] T014 Extend RequestRequirements in src/routing/requirements.rs to add routing_preference field (RoutingPreference)
- [ ] T015 Add RequestRequirements::from_request_with_headers() method to extract X-Nexus-Strict/Flexible headers in src/routing/requirements.rs
- [ ] T016 Extend RoutingAnnotations in src/control/intent.rs to add applied_policy, overflow_decision, affinity_key fields

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Enforce Privacy Boundaries at Backend Level (Priority: P1) 🎯 MVP

**Goal**: Ensure sensitive data never leaves restricted backends, blocking cloud overflow during capacity constraints

**Independent Test**: Configure backend as zone="restricted", send requests that would overflow to cloud, verify request succeeds on local OR returns 503 (never silently routes to cloud)

### Implementation for User Story 1

- [ ] T017 [US1] Update PrivacyReconciler::get_constraint() in src/control/privacy.rs to read privacy zone from TrafficPolicy (pattern-match request model)
- [ ] T018 [US1] Update PrivacyReconciler::check_backend() in src/control/privacy.rs to read zone from BackendConfig.zone field (not metadata)
- [ ] T019 [US1] Add backend affinity logic in src/control/privacy.rs: compute_affinity_key() hashing first user message content
- [ ] T020 [US1] Add select_with_affinity() method in src/control/privacy.rs using consistent hashing (key % backends.len())
- [ ] T021 [US1] Update PrivacyReconciler to store affinity_key in intent.annotations for sticky routing in src/control/privacy.rs
- [ ] T022 [US1] Add logging for privacy zone rejections with backend name, zone mismatch in src/control/privacy.rs
- [ ] T023 [US1] Wire PrivacyReconciler into ReconcilerPipeline in src/control/mod.rs (first position, before other reconcilers)
- [ ] T024 [US1] Update error handler in src/api/error.rs to include RejectionReason context in 503 responses for privacy violations

**Checkpoint**: At this point, privacy zone enforcement is fully functional - restricted backends never overflow to open zones

**Acceptance Validation**:
- Configure local backend as zone="Restricted", cloud as zone="Open"
- Send request → routes to restricted backend ✅
- Overload restricted backend → returns 503 with Retry-After ✅
- Never routes to cloud backend ✅
- Multiple restricted backends use affinity for sticky routing ✅

---

## Phase 4: User Story 2 - Prevent Quality Downgrades During Failover (Priority: P1)

**Goal**: Ensure requests never route to lower-tier models without explicit consent, maintaining predictable quality

**Independent Test**: Configure backends with different capability scores, request high-tier model, make it unavailable, verify system returns 503 (never silently downgrades)

### Implementation for User Story 2

- [ ] T025 [P] [US2] Implement CapabilityTier::meets_requirements() method in src/config/backend.rs (check all declared capabilities)
- [ ] T026 [US2] Update CapabilityReconciler in src/control/capability.rs to read capability_tier from BackendConfig (not metadata string)
- [ ] T027 [US2] Update CapabilityReconciler to match CapabilityRequirements from TrafficPolicy (pattern-match request model) in src/control/capability.rs
- [ ] T028 [US2] Implement multi-dimensional tier filtering: reasoning, coding, context_window, vision, tools in src/control/capability.rs
- [ ] T029 [US2] Add tier rejection logging with dimension details (required vs actual scores) in src/control/capability.rs
- [ ] T030 [US2] Wire CapabilityReconciler into ReconcilerPipeline in src/control/mod.rs (after PrivacyReconciler, before other reconcilers)
- [ ] T031 [US2] Update error handler in src/api/error.rs to include tier requirement context in 503 responses

**Checkpoint**: At this point, capability tier enforcement is fully functional - only same-or-higher tier backends are used

**Acceptance Validation**:
- Configure backend A with reasoning=9/coding=9, backend B with reasoning=6/coding=7
- Request with min_reasoning=9 → routes to backend A ✅
- Backend A unavailable → returns 503, never routes to backend B ✅
- TrafficPolicy with min_coding=8 → filters backends correctly ✅
- 503 response includes tier_insufficient_reasoning context ✅

---

## Phase 5: User Story 3 - Client Control Over Routing Flexibility (Priority: P2)

**Goal**: Let clients choose strict (exact model) or flexible (tier-equivalent alternatives) routing via headers

**Independent Test**: Send request with X-Nexus-Strict header when model unavailable → returns 503; send with X-Nexus-Flexible header → allows tier-equivalent alternative

### Implementation for User Story 3

- [ ] T032 [US3] Extract X-Nexus-Strict and X-Nexus-Flexible headers in src/api/completions.rs handler
- [ ] T033 [US3] Call RequestRequirements::from_request_with_headers() instead of from_request() in src/api/completions.rs
- [ ] T034 [US3] Implement flexible routing logic in CapabilityReconciler: if Flexible mode, allow tier-equivalent backends in src/control/capability.rs
- [ ] T035 [US3] Add tier-equivalence check: all capability dimensions must be same-or-higher (no partial matches) in src/control/capability.rs
- [ ] T036 [US3] Ensure Strict mode is default when no headers present (already in RoutingPreference::default()) in src/routing/requirements.rs
- [ ] T037 [US3] Log routing preference (Strict/Flexible) in CapabilityReconciler trace annotations in src/control/capability.rs
- [ ] T038 [US3] Return actual model used in response.model field (OpenAI API compatibility) in src/api/completions.rs

**Checkpoint**: At this point, clients can control routing flexibility via headers - Strict blocks alternatives, Flexible allows tier-equivalent

**Acceptance Validation**:
- Send request with X-Nexus-Strict: true, model unavailable → returns 503 ✅
- Send request with X-Nexus-Flexible: true, model unavailable → routes to tier-equivalent alternative ✅
- No headers → defaults to Strict mode ✅
- Flexible mode with only lower-tier alternatives → returns 503 (no downgrade) ✅
- Response includes actual model name used ✅

---

## Phase 6: User Story 4 - Cross-Zone Overflow with Context Protection (Priority: P2)

**Goal**: Allow overflow from restricted to open zones for fresh conversations only, blocking history forwarding to prevent data leakage

**Independent Test**: Force overflow from restricted to open zone with new conversation (succeeds); attempt with conversation history (blocked with clear error)

### Implementation for User Story 4

- [ ] T039 [US4] Create OverflowDecision enum in src/control/privacy.rs (AllowedFresh, BlockedWithHistory, BlockedByPolicy, NotNeeded)
- [ ] T040 [US4] Implement has_conversation_history() helper in src/control/privacy.rs (checks messages.len() > 1 or any "assistant" role)
- [ ] T041 [US4] Implement allows_cross_zone_overflow() in src/control/privacy.rs using TrafficPolicy.overflow_mode
- [ ] T042 [US4] Update PrivacyReconciler to evaluate overflow when restricted backends unavailable in src/control/privacy.rs
- [ ] T043 [US4] Allow overflow to Open zone if overflow_mode=FreshOnly AND no history detected in src/control/privacy.rs
- [ ] T044 [US4] Block overflow if overflow_mode=BlockEntirely OR history detected in src/control/privacy.rs
- [ ] T045 [US4] Store OverflowDecision in intent.annotations.overflow_decision for audit logging in src/control/privacy.rs
- [ ] T046 [US4] Add RejectionReason::OverflowBlockedWithHistory variant to error responses in src/control/mod.rs

**Checkpoint**: At this point, cross-zone overflow is controlled - only fresh conversations overflow, history is never forwarded

**Acceptance Validation**:
- Restricted backend unavailable, fresh conversation (1 message) → overflows to Open zone ✅
- Restricted backend unavailable, conversation history (multiple messages) → returns 503 ✅
- overflow_mode=BlockEntirely → never allows overflow regardless of history ✅
- Overflow events logged with from_zone, to_zone, has_history ✅

---

## Phase 7: User Story 5 - Actionable Error Responses for Debugging (Priority: P3)

**Goal**: Provide developers with structured 503 error responses including rejection reasons and retry hints for debugging

**Independent Test**: Trigger various rejection scenarios (privacy mismatch, tier unavailable), verify 503 response includes appropriate RejectionReason context and Retry-After headers

### Implementation for User Story 5

- [ ] T047 [P] [US5] Extend API error type in src/api/error.rs to include context field (optional HashMap for RejectionReason)
- [ ] T048 [P] [US5] Implement RejectionReason serialization to JSON in src/control/mod.rs (serde with tag="type")
- [ ] T049 [US5] Add Retry-After header calculation (conservative 30-60s default) in src/api/error.rs
- [ ] T050 [US5] Map PrivacyViolation to RejectionReason::PrivacyZoneMismatch in error handler in src/api/error.rs
- [ ] T051 [US5] Map CapabilityMismatch to RejectionReason::TierInsufficient* variants in error handler in src/api/error.rs
- [ ] T052 [US5] Map OverflowDecision::BlockedWithHistory to RejectionReason::OverflowBlockedWithHistory in error handler in src/api/error.rs
- [ ] T053 [US5] Include available_backends list in error context (backend names that were excluded) in src/api/error.rs
- [ ] T054 [US5] Add structured logging for all rejection reasons in reconcilers (tracing::info with rejection details) in src/control/privacy.rs and src/control/capability.rs

**Checkpoint**: All user stories complete - 503 errors now include actionable debugging context

**Acceptance Validation**:
- Privacy zone mismatch → 503 with rejection_reason="privacy_zone_mismatch", required/actual zones ✅
- Tier unavailable → 503 with rejection_reason="tier_insufficient_reasoning", required/actual scores ✅
- All 503 responses include Retry-After header ✅
- Error logs include actionable context for administrators ✅

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Observability, documentation, configuration validation, and migration support

- [ ] T055 [P] Add Prometheus metrics counter for privacy_zone_rejections_total with labels (zone, backend) in src/control/privacy.rs
- [ ] T056 [P] Add Prometheus metrics counter for tier_rejections_total with labels (dimension, required, actual, backend) in src/control/capability.rs
- [ ] T057 [P] Add Prometheus metrics counter for cross_zone_overflow_total with labels (from_zone, to_zone, has_history) in src/control/privacy.rs
- [ ] T058 [P] Add Prometheus metrics counter for affinity_break_total with labels (backend, reason) in src/control/privacy.rs
- [ ] T059 [P] Add TOML configuration validation for invalid glob patterns in TrafficPolicy at load time in src/config/routing.rs
- [ ] T060 [P] Add deprecation warning logging if BackendConfig.tier field is used instead of capability_tier in src/config/backend.rs
- [ ] T061 [P] Update quickstart.md examples with correct TOML syntax (from docs/015-privacy-zones/quickstart.md to main docs)
- [ ] T062 [P] Add configuration hot-reload support for TrafficPolicies (already supported by existing config loader, validate)
- [ ] T063 [P] Write integration test for complete P1 flow (privacy + tier enforcement) in tests/routing_integration.rs
- [ ] T064 [P] Write property-based test for affinity key distribution (no hash collisions, even load) in tests/privacy_reconciler_tests.rs
- [ ] T065 Run cargo test to validate all unit tests pass
- [ ] T066 Run cargo clippy to ensure no linter warnings
- [ ] T067 Run cargo fmt to ensure consistent code formatting
- [ ] T068 Run quickstart.md validation with local Ollama backend (manual test)
- [ ] T069 Performance benchmark: verify PrivacyReconciler <50μs per request using criterion in benches/privacy_reconciler.rs
- [ ] T070 Performance benchmark: verify CapabilityReconciler <100μs per request using criterion in benches/capability_reconciler.rs

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-7)**: All depend on Foundational phase completion
  - User Story 1 (P1): Can start after Foundational - No dependencies on other stories
  - User Story 2 (P1): Can start after Foundational - Independent of US1
  - User Story 3 (P2): Depends on US2 completion (extends CapabilityReconciler)
  - User Story 4 (P2): Depends on US1 completion (extends PrivacyReconciler)
  - User Story 5 (P3): Depends on US1, US2, US4 completion (consolidates all error responses)
- **Polish (Phase 8)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1) - Privacy Enforcement**: Foundation → US1 (independent, MVP candidate)
- **User Story 2 (P1) - Tier Enforcement**: Foundation → US2 (independent, MVP candidate)
- **User Story 3 (P2) - Client Headers**: Foundation → US2 → US3 (extends tier reconciler)
- **User Story 4 (P2) - Overflow Control**: Foundation → US1 → US4 (extends privacy reconciler)
- **User Story 5 (P3) - Error Context**: Foundation → US1 + US2 + US4 → US5 (consolidates errors)

### Within Each User Story

- Configuration extensions before reconciler logic
- Reconciler logic before pipeline wiring
- Pipeline wiring before error handling
- Error handling before validation tests

### Parallel Opportunities

#### Phase 1: Setup (All can run in parallel)
```bash
# All T001-T007 are marked [P] - different files, no dependencies
Task T001: src/agent/types.rs
Task T002: src/config/backend.rs (CapabilityTier)
Task T003: src/config/routing.rs (CapabilityRequirements)
Task T004: src/routing/requirements.rs
Task T005: src/config/routing.rs (OverflowMode)
# T006, T007 sequential in src/config/backend.rs
```

#### Phase 2: Foundational (Partial parallelism)
```bash
# T008-T011 sequential in src/config/routing.rs
# T012-T013 can run in parallel
Task T012: src/control/mod.rs (RejectionReason enum)
Task T013: src/control/mod.rs (message method)
# T014-T016 sequential in different files
```

#### User Story 1: Privacy
```bash
# T017-T024 mostly sequential (same file: src/control/privacy.rs)
# T024 is separate file, can prepare in parallel with T023
```

#### User Story 2: Tier
```bash
# T025 can run early (src/config/backend.rs)
Task T025: Parallel start (different file from T026-T030)
# T026-T030 sequential (src/control/capability.rs)
# T031 separate file (src/api/error.rs)
```

#### User Story 5: Errors
```bash
# T047-T048 can run in parallel (different concerns)
Task T047: src/api/error.rs
Task T048: src/control/mod.rs
# T049-T054 have dependencies, run sequentially
```

#### Phase 8: Polish
```bash
# Most polish tasks are marked [P] - can run in parallel
Task T055: src/control/privacy.rs (metrics)
Task T056: src/control/capability.rs (metrics)
Task T057: src/control/privacy.rs (overflow metrics)
Task T058: src/control/privacy.rs (affinity metrics)
Task T059: src/config/routing.rs (validation)
Task T060: src/config/backend.rs (deprecation)
Task T061: Documentation
Task T062: Config validation
Task T063-T064: Tests (different files)
# T065-T070 sequential verification steps
```

---

## Implementation Strategy

### MVP First (User Stories 1 + 2 Only) - Recommended

**Why US1 + US2**: Both are P1 priorities and independent. Together they deliver complete privacy + tier enforcement.

1. Complete Phase 1: Setup (T001-T007)
2. Complete Phase 2: Foundational (T008-T016)
3. Complete Phase 3: User Story 1 - Privacy Enforcement (T017-T024)
4. Complete Phase 4: User Story 2 - Tier Enforcement (T025-T031)
5. **STOP and VALIDATE**: Test privacy and tier enforcement independently
   - Configure restricted/open backends
   - Verify privacy zone blocking
   - Verify tier filtering
   - Verify 503 responses with basic context
6. Deploy/demo if ready (core value delivered)

**MVP Delivers**:
- ✅ Sensitive data never leaves restricted backends (SC-001: 100%)
- ✅ No silent quality downgrades (SC-002: 100%)
- ✅ Fast 503 responses with basic context (SC-003: <100ms)
- ✅ Backend affinity for multi-turn conversations (SC-005: 95%)

### Incremental Delivery (After MVP)

1. **Iteration 1: MVP (US1 + US2)** → Foundation + Privacy + Tier
2. **Iteration 2: Add US3** → Client header control (flexible routing)
3. **Iteration 3: Add US4** → Cross-zone overflow with history blocking
4. **Iteration 4: Add US5** → Rich error context and debugging
5. **Iteration 5: Polish** → Metrics, benchmarks, documentation

Each iteration adds value without breaking previous functionality.

### Parallel Team Strategy

With 2-3 developers after Foundational phase:

1. **Developer A**: User Story 1 (Privacy) - T017-T024
2. **Developer B**: User Story 2 (Tier) - T025-T031
3. **Both merge**: Test integration, verify no conflicts
4. **Developer A**: User Story 4 (Overflow) - T039-T046
5. **Developer B**: User Story 3 (Headers) - T032-T038
6. **Developer C**: User Story 5 (Errors) - T047-T054
7. **All**: Polish phase in parallel - T055-T064

---

## Success Criteria Tracking

From spec.md Success Criteria section:

- **SC-001** (100% privacy enforcement): Validated by User Story 1 checkpoint ✅
- **SC-002** (100% tier enforcement): Validated by User Story 2 checkpoint ✅
- **SC-003** (503 response <100ms): Benchmarked in T069-T070 ✅
- **SC-004** (Actionable 503 context): Delivered by User Story 5 ✅
- **SC-005** (95% affinity success): Property-tested in T064 ✅
- **SC-006** (99% overflow success): Validated by User Story 4 checkpoint ✅
- **SC-007** (Configuration hot-reload): Validated in T062 ✅
- **SC-008** (10ms reconciliation overhead): Benchmarked in T069-T070 ✅

---

## Notes

- **[P] tasks**: Different files, no dependencies, can run in parallel
- **[Story] label**: Maps task to specific user story for traceability (US1, US2, US3, US4, US5)
- **Tests**: Validation testing only (not TDD) per spec - no explicit test requirement
- **MVP Scope**: US1 + US2 deliver 80% of value with 50% of work
- **Commit Strategy**: Commit after each user story checkpoint
- **Performance**: Benchmark after implementation (T069-T070) to validate <500μs total overhead
- **Configuration**: All config changes are backwards compatible (zone defaults to Restricted)
- **Error Handling**: Privacy reconciler uses FailClosed policy (never compromise security)

---

## File Path Summary

**Core Implementation Files** (where work happens):
- `src/agent/types.rs` - PrivacyZone enum (already exists, extend)
- `src/config/backend.rs` - BackendConfig, CapabilityTier (extend)
- `src/config/routing.rs` - TrafficPolicy, CapabilityRequirements, OverflowMode (new)
- `src/routing/requirements.rs` - RequestRequirements, RoutingPreference (extend)
- `src/control/privacy.rs` - PrivacyReconciler (already exists, extend)
- `src/control/capability.rs` - CapabilityReconciler (already exists, extend)
- `src/control/mod.rs` - RejectionReason, pipeline wiring (extend)
- `src/control/intent.rs` - RoutingAnnotations (extend)
- `src/api/completions.rs` - Header extraction (extend)
- `src/api/error.rs` - Error response formatting (extend)

**Test Files** (validation only):
- `tests/routing_integration.rs` - E2E privacy+tier tests
- `tests/privacy_reconciler_tests.rs` - Property-based affinity tests
- `benches/privacy_reconciler.rs` - Performance benchmarks
- `benches/capability_reconciler.rs` - Performance benchmarks

**Total Task Count**: 70 tasks
- Phase 1 (Setup): 7 tasks
- Phase 2 (Foundational): 9 tasks
- Phase 3 (US1 - Privacy): 8 tasks
- Phase 4 (US2 - Tier): 7 tasks
- Phase 5 (US3 - Headers): 7 tasks
- Phase 6 (US4 - Overflow): 8 tasks
- Phase 7 (US5 - Errors): 8 tasks
- Phase 8 (Polish): 16 tasks

**Parallel Opportunities**: 28 tasks marked [P] (40% can run concurrently with proper staffing)

**MVP Task Count**: 31 tasks (Setup + Foundational + US1 + US2) - delivers core privacy + tier enforcement
