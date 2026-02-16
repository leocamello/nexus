# Tasks: Privacy Zones & Capability Tiers

**Input**: Design documents from `/specs/015-privacy-zones-capability-tiers/`
**Context**: Integration work building on PR #157 - core reconcilers exist, wiring them into the Router pipeline

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

**Tests**: Integration tests are included to verify end-to-end behavior.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Project initialization and validation of existing components from PR #157

- [ ] T001 [P] Verify PrivacyReconciler exists and is tested in src/routing/reconciler/privacy.rs
- [ ] T002 [P] Verify TierReconciler exists and is tested in src/routing/reconciler/tier.rs
- [ ] T003 [P] Verify BackendConfig has zone/tier fields in src/config/backend.rs
- [ ] T004 [P] Verify AgentProfile has privacy_zone/capability_tier fields in src/agent/types.rs
- [ ] T005 Fix BackendConfig.effective_tier() default from 3 to 1 (FR-022) in src/config/backend.rs

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can be implemented

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T006 Wire PolicyMatcher into Router for traffic policy matching in src/routing/mod.rs
- [X] T007 Create extract_tier_enforcement_mode() helper function in src/api/completions.rs
- [X] T008 Extend ActionableErrorContext struct with privacy_zone_required and required_tier fields in src/api/error.rs

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 3 - Transparent Backend Configuration (Priority: P1) 🎯 MVP

**Goal**: Backend zone/tier configuration is parsed from TOML and flows to AgentProfile

**Independent Test**: Configure backends with zone/tier fields in TOML, start Nexus, verify AgentProfile contains correct values without making any requests

### Implementation for User Story 3

- [X] T009 [P] [US3] Verify BackendConfig deserialization handles zone field with default in src/config/backend.rs
- [X] T010 [P] [US3] Verify BackendConfig deserialization handles tier field with default in src/config/backend.rs
- [X] T011 [US3] Verify AgentProfile population from BackendConfig in Registry::register_backend() in src/registry/mod.rs
- [X] T012 [US3] Add validation for tier range (1-5) in BackendConfig::validate() in src/config/backend.rs
- [X] T013 [US3] Add validation for zone enum values in BackendConfig::validate() in src/config/backend.rs
- [X] T014 [US3] Test configuration parsing with explicit zone="restricted" and tier=3
- [X] T015 [US3] Test configuration parsing with missing zone field (defaults to backend type)
- [X] T016 [US3] Test configuration parsing with missing tier field (defaults to 1)
- [X] T017 [US3] Test startup validation rejects tier=10 with clear error message

**Checkpoint**: Backend configuration fully functional - zone/tier flow from TOML to AgentProfile

---

## Phase 4: User Story 1 - Privacy-Conscious Local Deployment (Priority: P1)

**Goal**: Privacy zone enforcement prevents cross-zone routing even during failover

**Independent Test**: Configure restricted backend, force it offline, verify 503 with privacy context instead of routing to open backend

### Implementation for User Story 1

- [X] T018 [US1] Wire PrivacyReconciler into Router reconciler pipeline in src/routing/mod.rs
- [X] T019 [US1] Verify pipeline order: Privacy → Budget → Tier → Quality → Scheduler in src/routing/mod.rs
- [X] T020 [US1] Verify PrivacyReconciler reads AgentProfile.privacy_zone from Registry
- [X] T021 [US1] Verify PrivacyReconciler excludes agents with RejectionReason when zone violates constraint
- [X] T022 [US1] Populate privacy_zone_required in ActionableErrorContext from RoutingIntent in src/api/completions.rs
- [X] T023 [US1] Verify X-Nexus-Privacy-Zone header injection in NexusTransparentHeaders in src/api/headers.rs
- [X] T024 [US1] Integration test: restricted backend available → routes to it, returns X-Nexus-Privacy-Zone: restricted
- [X] T025 [US1] Integration test: restricted backend offline, open available → 503 with privacy_zone_required="restricted"
- [X] T026 [US1] Integration test: response includes X-Nexus-Privacy-Zone header matching backend's configured zone
- [X] T027 [US1] Integration test: verify cross-zone failover never happens (restricted never routes to open)

**Checkpoint**: Privacy zone enforcement fully functional - cross-zone routing prevented

---

## Phase 5: User Story 2 - Quality-Aware Failover Control (Priority: P2)

**Goal**: Tier enforcement with strict/flexible modes prevents silent quality downgrades

**Independent Test**: Configure backends with different tiers, send requests with/without X-Nexus-Flexible header, verify routing behavior

### Implementation for User Story 2

- [X] T028 [US2] Wire TierReconciler into Router reconciler pipeline after PrivacyReconciler in src/routing/mod.rs
- [X] T029 [US2] Parse X-Nexus-Strict request header in extract_tier_enforcement_mode() in src/api/completions.rs
- [X] T030 [US2] Parse X-Nexus-Flexible request header in extract_tier_enforcement_mode() in src/api/completions.rs
- [X] T031 [US2] Handle conflicting headers (X-Nexus-Strict takes precedence) in extract_tier_enforcement_mode() in src/api/completions.rs
- [X] T032 [US2] Set tier_enforcement_mode on RoutingIntent from parsed headers in src/api/completions.rs
- [ ] T033 [US2] Verify TierReconciler reads AgentProfile.capability_tier from Registry
- [ ] T034 [US2] Verify TierReconciler excludes under-tier agents in Strict mode
- [ ] T035 [US2] Verify TierReconciler allows higher-tier substitution in Flexible mode
- [ ] T036 [US2] Populate required_tier in ActionableErrorContext from RoutingIntent in src/api/completions.rs
- [ ] T037 [US2] Integration test: no headers → strict mode (default) → only exact/higher tier accepted
- [ ] T038 [US2] Integration test: X-Nexus-Strict: true → exact model matching enforced
- [ ] T039 [US2] Integration test: X-Nexus-Flexible: true → higher tier substitution allowed
- [ ] T040 [US2] Integration test: tier 3 backend offline, tier 2 available, flexible mode → 503 (never downgrade)
- [ ] T041 [US2] Integration test: tier 3 backend offline, tier 4 available, flexible mode → routes to tier 4
- [ ] T042 [US2] Integration test: conflicting headers (both strict and flexible) → strict wins

**Checkpoint**: Tier enforcement fully functional with strict/flexible mode control

---

## Phase 6: User Story 4 - Actionable Error Responses (Priority: P3)

**Goal**: 503 responses include clear context explaining privacy/tier constraints

**Independent Test**: Trigger privacy/tier rejections, inspect 503 response body for required_tier and privacy_zone_required fields

### Implementation for User Story 4

- [ ] T043 [US4] Flow RoutingIntent.rejection_reasons to ActionableErrorContext in src/api/completions.rs
- [ ] T044 [US4] Serialize ActionableErrorContext with privacy/tier fields in 503 JSON response in src/api/error.rs
- [ ] T045 [US4] Ensure privacy_zone_required field uses skip_serializing_if for null values in src/api/error.rs
- [ ] T046 [US4] Ensure required_tier field uses skip_serializing_if for null values in src/api/error.rs
- [ ] T047 [US4] Integration test: privacy rejection → 503 includes privacy_zone_required field
- [ ] T048 [US4] Integration test: tier rejection → 503 includes required_tier field
- [ ] T049 [US4] Integration test: combined rejection → 503 includes both privacy_zone_required and required_tier
- [ ] T050 [US4] Integration test: rejection includes RejectionReason with agent_id, reconciler, reason, suggested_action
- [ ] T051 [US4] Integration test: 503 response includes Retry-After header with appropriate retry timing

**Checkpoint**: Error responses provide actionable debugging information

---

## Phase 7: User Story 5 - Zero-Config Backward Compatibility (Priority: P2)

**Goal**: Existing deployments without zone/tier configuration work unchanged

**Independent Test**: Run Nexus with legacy configuration (no zone/tier fields), verify normal routing without enforcement

### Implementation for User Story 5

- [ ] T052 [US5] Verify default behavior: no traffic_policies → no filtering in PrivacyReconciler
- [ ] T053 [US5] Verify default behavior: no traffic_policies → no filtering in TierReconciler
- [ ] T054 [US5] Integration test: configuration without zone fields → backends default to backend type defaults
- [ ] T055 [US5] Integration test: configuration without tier fields → backends default to tier 1
- [ ] T056 [US5] Integration test: no TrafficPolicies defined → all requests route normally without enforcement
- [ ] T057 [US5] Integration test: mixed configuration (some backends with zones, some without) → correct defaults applied

**Checkpoint**: Backward compatibility verified - existing deployments unaffected

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

- [ ] T058 [P] Add debug logging to PrivacyReconciler exclusion decisions in src/routing/reconciler/privacy.rs
- [ ] T059 [P] Add debug logging to TierReconciler exclusion decisions in src/routing/reconciler/tier.rs
- [ ] T060 [P] Update README.md with privacy zones and capability tiers feature description
- [ ] T061 [P] Update CHANGELOG.md with F13 feature entry
- [ ] T062 Verify all existing reconciler pipeline tests pass after wiring privacy/tier reconcilers
- [ ] T063 Run performance benchmarks to verify reconciler pipeline overhead < 1ms (FR-023)
- [ ] T064 Validate quickstart.md examples by running configuration scenarios
- [ ] T065 Run full integration test suite to verify end-to-end flows

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - BLOCKS all user stories
- **User Stories (Phase 3-7)**: All depend on Foundational phase completion
  - User Story 3 (Configuration): Can start after Phase 2 - No dependencies on other stories
  - User Story 1 (Privacy): Depends on US3 (needs configuration) - Can run after US3
  - User Story 2 (Tier): Depends on US3 (needs configuration) - Can run after US3
  - User Story 4 (Error Context): Depends on US1 and US2 (extends their error handling)
  - User Story 5 (Backward Compat): Can start after Phase 2 - Independent validation
- **Polish (Phase 8)**: Depends on all user stories being complete

### User Story Dependencies

```
Phase 2: Foundational
    ↓
Phase 3: US3 (Configuration) ← Foundation for US1 & US2
    ↓
    ├─→ Phase 4: US1 (Privacy Enforcement)
    └─→ Phase 5: US2 (Tier Enforcement)
         ↓
    Phase 6: US4 (Error Responses) ← Depends on US1 & US2
    
Phase 7: US5 (Backward Compat) ← Independent, validates defaults
```

### Within Each User Story

- Configuration tasks before integration tasks
- Wiring tasks before testing tasks
- Implementation before integration tests
- Story complete before moving to next priority

### Parallel Opportunities

- **Phase 1**: All verification tasks (T001-T004) can run in parallel
- **Phase 2**: T006, T007, T008 can run in parallel (different files)
- **Within US3**: T009, T010 can run in parallel (both config deserialization)
- **Within US1**: T024 (integration test) can run after T018-T023 complete
- **Within US2**: T029, T030 (header parsing) can run in parallel
- **Within US4**: T045, T046 (serialization) can run in parallel
- **Phase 8**: All documentation tasks (T058-T061) can run in parallel
- Once Foundational complete: US3, US5 can start in parallel (US5 validates defaults)
- Once US3 complete: US1, US2 can start in parallel (both need configuration)

---

## Parallel Example: User Story 3 (Configuration)

```bash
# Launch configuration verification tasks together:
Task T009: "Verify BackendConfig deserialization handles zone field"
Task T010: "Verify BackendConfig deserialization handles tier field"

# After deserialization verified, launch validation tasks:
Task T012: "Add validation for tier range (1-5)"
Task T013: "Add validation for zone enum values"
```

---

## Parallel Example: User Story 2 (Tier Enforcement)

```bash
# Launch header parsing tasks together:
Task T029: "Parse X-Nexus-Strict request header"
Task T030: "Parse X-Nexus-Flexible request header"

# Launch serialization tasks together:
Task T045: "Ensure privacy_zone_required uses skip_serializing_if"
Task T046: "Ensure required_tier uses skip_serializing_if"
```

---

## Implementation Strategy

### MVP First (User Story 3 + User Story 1)

1. Complete Phase 1: Setup (verify components from PR #157)
2. Complete Phase 2: Foundational (CRITICAL - blocks all stories)
3. Complete Phase 3: User Story 3 (Configuration foundation)
4. Complete Phase 4: User Story 1 (Privacy enforcement)
5. **STOP and VALIDATE**: Test privacy enforcement independently
6. Deploy/demo privacy zones feature

### Incremental Delivery

1. Complete Setup + Foundational → Foundation ready
2. Add User Story 3 (Configuration) → Test independently → Backends configured
3. Add User Story 1 (Privacy) → Test independently → Deploy/Demo (MVP!)
4. Add User Story 2 (Tier) → Test independently → Deploy/Demo
5. Add User Story 4 (Error Context) → Test independently → Deploy/Demo
6. Add User Story 5 (Backward Compat) → Validate → Deploy/Demo
7. Each story adds value without breaking previous stories

### Parallel Team Strategy

With multiple developers:

1. Team completes Setup + Foundational together
2. Once Foundational is done:
   - Developer A: User Story 3 (Configuration - blocks US1 & US2)
3. Once US3 is done:
   - Developer A: User Story 1 (Privacy)
   - Developer B: User Story 2 (Tier)
   - Developer C: User Story 5 (Backward Compat - independent)
4. Once US1 & US2 done:
   - Developer A or B: User Story 4 (Error Context)
5. Stories complete and integrate independently

---

## Integration Work Context

**CRITICAL**: This is integration work, not greenfield development. All core components exist from PR #157:

- ✅ PrivacyReconciler implemented and tested
- ✅ TierReconciler implemented and tested
- ✅ BackendConfig has zone/tier fields
- ✅ AgentProfile has privacy_zone/capability_tier fields
- ✅ RoutingIntent has constraint fields
- ✅ RejectionReason structure exists
- ✅ X-Nexus-Privacy-Zone header already implemented

**Tasks focus on**:
1. Wiring reconcilers into Router pipeline (Phase 2, US1, US2)
2. Header parsing (US2)
3. Error context enrichment (US4)
4. Configuration validation (US3)
5. Integration tests (all user stories)
6. Documentation (Phase 8)

**Not needed**:
- ❌ Implementing reconciler logic (already done)
- ❌ Creating new data structures (already exist)
- ❌ Designing reconciler abstractions (already in place)

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Verify tests fail before implementing (if creating new tests)
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- **Integration focus**: Wire existing components, don't reimplement
- **Performance target**: Privacy + Tier reconcilers combined < 1ms (FR-023)
- **Backward compatible**: Zero-config deployments work unchanged (FR-020)
