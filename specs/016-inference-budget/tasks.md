# Tasks: Inference Budget Management (F14)

**Input**: Design documents from `/specs/016-inference-budget/`
**Prerequisites**: plan.md, spec.md, data-model.md, research.md, quickstart.md, contracts/

**Tests**: Tests are NOT included as they were not requested in the feature specification. Focus is on implementation tasks.

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3, US4)
- Include exact file paths in descriptions

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Configuration types and basic structure for budget management

- [X] T001 Create budget configuration module at src/config/budget.rs with BudgetConfig, HardLimitAction types
- [X] T002 [P] Create pricing registry module at src/control/budget/pricing.rs with ModelPricing, PricingRegistry types
- [X] T003 Add budget configuration to NexusConfig in src/config/mod.rs (import BudgetConfig from budget module)
- [X] T004 [P] Add BudgetConfig validation in src/config/budget.rs (monthly_limit >= 0, soft_limit_percent 0-100, billing_cycle_start_day 1-31)
- [X] T005 Initialize budget metrics functions in src/metrics/mod.rs (init_budget_metrics, describe gauges/counters/histograms)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core budget state management and pricing infrastructure

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [X] T006 Create BudgetState struct in src/control/budget.rs (AtomicU64 for spending_cents, monthly_limit_usd, soft_limit_percent, hard_limit_action, last_reset)
- [X] T007 [P] Implement PricingRegistry::default_registry() in src/control/budget/pricing.rs with hardcoded pricing tables (OpenAI, Anthropic, local, __unknown__)
- [X] T008 [P] Implement PricingRegistry::get_pricing() in src/control/budget/pricing.rs (exact match, prefix match, fallback to __unknown__)
- [X] T009 Implement BudgetState::new() in src/control/budget.rs (initialize from BudgetConfig)
- [X] T010 [P] Implement BudgetState::add_spending() in src/control/budget.rs (lock-free AtomicU64::fetch_add with cents conversion)
- [X] T011 [P] Implement BudgetState::current_spending_usd() in src/control/budget.rs (lock-free AtomicU64::load with cents to USD conversion)
- [X] T012 [P] Implement BudgetState::budget_status() in src/control/budget.rs (calculate BudgetStatus from current spending percentage)
- [X] T013 Create CostEstimate struct in src/control/budget.rs (input_tokens, estimated_output_tokens, cost_usd, token_count_tier, provider, model, timestamp)
- [X] T014 [P] Implement CostEstimate::calculate() in src/control/budget.rs (compute cost from token counts and ModelPricing)

**Checkpoint**: Foundation ready - user story implementation can now begin in parallel

---

## Phase 3: User Story 1 - Basic Budget Tracking and Cost Visibility (Priority: P1) 🎯 MVP

**Goal**: Track inference costs in real-time with accurate cost calculation and Prometheus metrics for spending visibility

**Independent Test**: Send inference requests to different providers (OpenAI, Anthropic, local) and verify costs are accurately calculated using heuristic tokenization (chars/4 * 1.15), recorded in Prometheus metrics, and cumulative spending is tracked

### Implementation for User Story 1

- [X] T015 [P] [US1] Create TokenCountTier enum in src/control/budget.rs (Exact, Approximation, Estimated variants)
- [X] T016 [US1] Enhance BudgetReconciler::reconcile() in src/control/budget.rs to call agent.count_tokens() for input token count
- [X] T017 [US1] Implement heuristic output token estimation in src/control/budget.rs (input_tokens * 0.5 for unknown models)
- [X] T018 [US1] Integrate PricingRegistry lookup in BudgetReconciler::reconcile() in src/control/budget.rs (get pricing for provider/model)
- [X] T019 [US1] Create CostEstimate from token counts and pricing in BudgetReconciler::reconcile() in src/control/budget.rs
- [X] T020 [US1] Store CostEstimate in RoutingIntent.annotations.cost_estimate in src/control/budget.rs
- [X] T021 [US1] Call BudgetState::add_spending() with estimated cost in src/control/budget.rs after routing decision
- [X] T022 [P] [US1] Implement update_budget_metrics() in src/metrics/mod.rs (gauge updates for current_spending_usd, monthly_limit_usd, budget_percent_used)
- [X] T023 [P] [US1] Implement record_cost_estimate() in src/metrics/mod.rs (histogram with provider, model, tier labels)
- [X] T024 [US1] Call update_budget_metrics() and record_cost_estimate() from BudgetReconciler::reconcile() in src/control/budget.rs
- [X] T025 [US1] Add heuristic tokenization flag (token_count_tier = Estimated) when using chars/4 * 1.15 in src/control/budget.rs

**Checkpoint**: At this point, User Story 1 should be fully functional - cost tracking works, metrics are exposed, spending is visible in Prometheus

---

## Phase 4: User Story 2 - Graceful Degradation at Soft Limit (Priority: P2)

**Goal**: Automatically prefer cost-efficient local agents when approaching budget limits (80%) to continue serving requests while minimizing cloud costs

**Independent Test**: Set a low monthly budget, consume 80% of it, send new requests and verify that local agents are strongly preferred over cloud agents in routing decisions

### Implementation for User Story 2

- [ ] T026 [US2] Enhance BudgetStatus variants in src/control/budget.rs to include usage_percent field for SoftLimit
- [ ] T027 [US2] Update BudgetReconciler::reconcile() in src/control/budget.rs to calculate and attach BudgetStatus to RoutingIntent.annotations.budget_status
- [ ] T028 [US2] Create agent cost scoring logic in src/control/budget.rs (prefer_cheaper() method or similar)
- [ ] T029 [US2] Integrate BudgetStatus check into routing algorithm at src/control/selection.rs (prefer local agents when BudgetStatus::SoftLimit)
- [ ] T030 [US2] Add warning log "Budget soft limit reached: preferring local agents" when status changes to SoftLimit in src/control/budget.rs
- [ ] T031 [P] [US2] Implement increment_soft_limit_activation() in src/metrics/mod.rs (counter increment)
- [ ] T032 [US2] Call increment_soft_limit_activation() when BudgetStatus transitions to SoftLimit in src/control/budget.rs
- [ ] T033 [US2] Add logic to log warning when cloud routing occurs during SoftLimit in src/control/budget.rs

**Checkpoint**: At this point, User Stories 1 AND 2 should both work independently - cost tracking + soft limit routing preference

---

## Phase 5: User Story 3 - Hard Limit Enforcement with Configurable Actions (Priority: P2)

**Goal**: Enforce hard budget limits (100%) with configurable behavior (local-only, queue, reject) to prevent runaway costs while choosing appropriate fallback strategy

**Independent Test**: Exhaust a monthly budget (100% spending), send new requests and verify that the configured hard_limit_action is applied (local-only excludes cloud, queue queues requests, reject returns 429 errors)

### Implementation for User Story 3

- [ ] T034 [US3] Implement cloud backend filtering logic in BudgetReconciler::reconcile() at src/control/budget.rs when HardLimit + hard_limit_action=local-only
- [ ] T035 [US3] Update BudgetReconciler to exclude cloud backends from candidate_backends when hard_limit_action is local-only in src/control/budget.rs
- [ ] T036 [US3] Add BudgetViolation tracking for excluded backends in src/control/budget.rs (already exists, ensure it's used)
- [ ] T037 [US3] Implement 429 error response in API handler at src/api/chat.rs when hard_limit_action=reject and HardLimit reached
- [ ] T038 [US3] Add queue stub for hard_limit_action=queue in src/control/budget.rs (log warning that queuing not implemented, return 429 with Retry-After header)
- [ ] T039 [P] [US3] Implement increment_hard_limit_activation() in src/metrics/mod.rs (counter increment)
- [ ] T040 [P] [US3] Implement increment_blocked_request() in src/metrics/mod.rs (counter with reason label)
- [ ] T041 [US3] Call increment_hard_limit_activation() when BudgetStatus transitions to HardLimit in src/control/budget.rs
- [ ] T042 [US3] Call increment_blocked_request() for each rejected/blocked request in src/control/budget.rs
- [ ] T043 [US3] Add error log "Budget hard limit reached: [action description]" when HardLimit enforced in src/control/budget.rs
- [ ] T044 [US3] Implement BudgetState::reset_spending() in src/control/budget.rs (AtomicU64::store(0), update last_reset, log reset event)

**Checkpoint**: All hard limit enforcement modes work independently - local-only, reject, and queue stub

---

## Phase 6: User Story 4 - Per-Provider Tokenizer Accuracy (Priority: P3)

**Goal**: Ensure audit-grade token counting accuracy for each provider so cost estimates are accurate and budget enforcement decisions are trustworthy

**Independent Test**: Send identical prompts to different providers and verify that token counts are consistent with provider-reported values using heuristic tokenization (v0.3 uses chars/4 * 1.15, exact tokenizers deferred to v0.4)

### Implementation for User Story 4

- [ ] T045 [P] [US4] Document heuristic tokenization approach in src/control/budget.rs comments (chars/4 * 1.15 conservative multiplier)
- [ ] T046 [US4] Set token_count_tier=Estimated for all heuristic counts in src/control/budget.rs
- [ ] T047 [US4] Add model pattern matching logic in PricingRegistry for provider detection in src/control/budget/pricing.rs
- [ ] T048 [US4] Create provider-specific tokenization paths in BudgetReconciler::reconcile() in src/control/budget.rs (OpenAI, Anthropic, Llama, Unknown)
- [ ] T049 [US4] Apply 1.15x conservative multiplier to unknown model token counts in src/control/budget.rs
- [ ] T050 [US4] Include all CostEstimate fields in metrics export (input_tokens, estimated_output_tokens, cost_usd, token_count_tier) in src/metrics/mod.rs
- [ ] T051 [US4] Add documentation for v0.4 tokenizer upgrade path in src/control/budget/pricing.rs comments (tiktoken-rs, tokenizers crate)

**Checkpoint**: Heuristic tokenization is consistently applied with conservative estimates, all cost data is accurate and flagged appropriately

---

## Phase 7: Billing Cycle Reset & Reconciliation Loop

**Purpose**: Background task for automatic billing cycle resets and spending reconciliation

- [ ] T052 Create BudgetReconciliationLoop struct in src/control/budget.rs (interval, BudgetState handle, BudgetConfig)
- [ ] T053 Implement billing cycle detection logic in src/control/budget.rs (check if first day of month based on billing_cycle_start_day)
- [ ] T054 Implement reconciliation loop logic in src/control/budget.rs (tokio::spawn task, 60-second intervals, date checking)
- [ ] T055 Call BudgetState::reset_spending() when billing cycle resets in src/control/budget.rs
- [ ] T056 Add error handling and logging for reconciliation loop in src/control/budget.rs (loop restart on panic)
- [ ] T057 Spawn BudgetReconciliationLoop background task in src/main.rs alongside health checker
- [ ] T058 Add log "Monthly budget reset: $X.XX available" on reset in src/control/budget.rs (already in BudgetState::reset_spending)

---

## Phase 8: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, validation, and final integration

- [ ] T059 [P] Update README.md or docs/FEATURES.md with budget management overview and quickstart reference
- [ ] T060 [P] Validate budget configuration parsing with example nexus.toml snippets in specs/016-inference-budget/contracts/budget-config.toml
- [ ] T061 [P] Verify all Prometheus metrics are correctly exposed at /metrics endpoint
- [ ] T062 Add edge case handling for $0.00 monthly budget in src/control/budget.rs (treat as immediate hard limit)
- [ ] T063 Add edge case handling for billing_cycle_start_day > days in month in src/control/budget.rs (reset on last day of month, log warning)
- [ ] T064 Document acceptable overage (concurrent_requests × avg_cost) in src/control/budget.rs comments
- [ ] T065 [P] Run through quickstart.md validation scenarios (setup, metrics check, soft/hard limit testing)
- [ ] T066 Code cleanup and ensure all tracing::info/warn/error logs are in place across src/control/budget.rs

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion (T001-T005) - BLOCKS all user stories
- **User Story 1 (Phase 3)**: Depends on Foundational (T006-T014)
- **User Story 2 (Phase 4)**: Depends on User Story 1 (T015-T025) - needs cost tracking infrastructure
- **User Story 3 (Phase 5)**: Depends on User Story 2 (T026-T033) - needs BudgetStatus logic
- **User Story 4 (Phase 6)**: Depends on Foundational (T006-T014) - can run in parallel with US1-US3 if desired
- **Billing Cycle (Phase 7)**: Depends on Foundational (T006-T014) and User Story 3 (T034-T044 for reset logic)
- **Polish (Phase 8)**: Depends on all user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2) - Core cost tracking
- **User Story 2 (P2)**: Can start after User Story 1 - Builds on cost tracking to add soft limit routing
- **User Story 3 (P2)**: Can start after User Story 2 - Builds on soft limit to add hard limit enforcement
- **User Story 4 (P3)**: Can start after Foundational (Phase 2) - Tokenization accuracy improvements

### Within Each User Story

- Setup tasks before foundational
- Foundational state management before cost estimation
- Cost estimation before routing decisions
- Metrics recording after state updates
- Logging throughout

### Parallel Opportunities

- **Phase 1**: T002 (pricing registry), T004 (validation), T005 (metrics init) can run in parallel with T001/T003
- **Phase 2**: T007-T008 (pricing), T010-T012 (BudgetState methods), T014 (CostEstimate) can run in parallel after T006, T009, T013 are complete
- **User Story 1**: T015 (TokenCountTier), T022 (update_budget_metrics), T023 (record_cost_estimate) can run in parallel with other tasks
- **User Story 2**: T031 (increment_soft_limit_activation) can run in parallel with T026-T030
- **User Story 3**: T039-T040 (metrics counters) can run in parallel with T034-T043
- **User Story 4**: All tasks T045-T051 are mostly documentation/refinement and can overlap
- **Polish**: T059, T060, T061, T065 can all run in parallel

---

## Parallel Example: Foundational Phase

```bash
# After T006 (BudgetState struct), T009 (BudgetState::new), T013 (CostEstimate struct):
# Launch these in parallel:
Task: "Implement PricingRegistry::default_registry() in src/control/budget/pricing.rs"
Task: "Implement PricingRegistry::get_pricing() in src/control/budget/pricing.rs"
Task: "Implement BudgetState::add_spending() in src/control/budget.rs"
Task: "Implement BudgetState::current_spending_usd() in src/control/budget.rs"
Task: "Implement BudgetState::budget_status() in src/control/budget.rs"
Task: "Implement CostEstimate::calculate() in src/control/budget.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup (T001-T005)
2. Complete Phase 2: Foundational (T006-T014) - CRITICAL foundation
3. Complete Phase 3: User Story 1 (T015-T025)
4. **STOP and VALIDATE**: Test cost tracking independently
   - Send test requests to mock backends
   - Verify Prometheus metrics show costs
   - Verify spending counter increases
5. Deploy/demo if ready

### Incremental Delivery

1. Complete Setup + Foundational (Phases 1-2) → Foundation ready
2. Add User Story 1 (Phase 3) → Test cost tracking → Deploy/Demo (MVP! 🎯)
3. Add User Story 2 (Phase 4) → Test soft limit routing → Deploy/Demo
4. Add User Story 3 (Phase 5) → Test hard limit enforcement → Deploy/Demo
5. Add User Story 4 (Phase 6) → Test tokenization accuracy → Deploy/Demo
6. Add Billing Cycle (Phase 7) → Test automatic resets → Deploy/Demo
7. Polish (Phase 8) → Final validation and documentation

### Parallel Team Strategy

With multiple developers:

1. Team completes Setup + Foundational together (Phases 1-2)
2. Once Foundational is done:
   - **Developer A**: User Story 1 (cost tracking infrastructure)
   - **Developer B**: User Story 4 (tokenization refinement - can start after Phase 2)
   - **Developer C**: Billing Cycle (Phase 7 - can start after Phase 2)
3. After User Story 1 complete:
   - **Developer A**: User Story 2 (soft limit)
   - **Developer D**: Polish tasks
4. After User Story 2 complete:
   - **Developer A**: User Story 3 (hard limit)

---

## Key Integration Points

### Existing Code Dependencies

- `src/control/budget.rs`: BudgetReconciler stub exists - ENHANCE with cost estimation
- `src/control/intent.rs`: Add budget_status and cost_estimate to annotations
- `src/control/selection.rs`: Add BudgetStatus check for local-first preference
- `src/config/mod.rs`: Add BudgetConfig to NexusConfig enum
- `src/metrics/mod.rs`: Add budget metrics functions
- `src/api/chat.rs`: Add 429 error handling for hard limit rejections
- `src/main.rs`: Spawn BudgetReconciliationLoop task

### No Changes Required

- `src/agent/mod.rs`: InferenceAgent::count_tokens() already exists
- `src/control/reconciler.rs`: Pipeline executor unchanged
- `src/registry/`: Backend registry unchanged

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- BudgetReconciler stub already exists - tasks focus on ENHANCEMENT not creation
- Heuristic tokenization (chars/4 * 1.15) for v0.3 MVP - exact tokenizers deferred to v0.4
- No new dependencies required (no tiktoken-rs, no tokenizers crate)
- All state is in-memory (AtomicU64, no persistence)
- Background reconciliation loop for billing cycle resets (60-second intervals)
- Prometheus metrics for observability
- Constitution compliant (no new abstractions, local-first, explicit contracts)
