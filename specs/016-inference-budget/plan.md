# Implementation Plan: Inference Budget Management

**Branch**: `016-inference-budget` | **Date**: 2025-01-22 | **Spec**: [specs/016-inference-budget/spec.md](./spec.md)
**Input**: Feature specification from `/specs/016-inference-budget/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Implements cost-aware routing with graceful degradation to prevent runaway cloud inference costs. The feature adds budget tracking, soft/hard limit enforcement, and cost estimation to the existing routing pipeline. For v0.3 MVP, uses heuristic token counting (chars/4) with provider-agnostic cost estimation. Exact tokenization (tiktoken-rs, tokenizers crate) deferred to v0.4 to avoid new dependencies in this release.

## Technical Context

**Language/Version**: Rust 1.75+ (stable toolchain)  
**Primary Dependencies**: 
- Existing: `tokio`, `axum`, `metrics` (Prometheus), `dashmap`, `serde`, `tracing`
- **NO NEW** tokenization crates for v0.3 (heuristic only)
- v0.4 will add: `tiktoken-rs` (OpenAI), `tokenizers` (HuggingFace)

**Storage**: In-memory only (DashMap for atomic spending counters, no persistence)  
**Testing**: `cargo test` with mock backends, property-based tests for cost calculations  
**Target Platform**: Linux/macOS/Windows servers (single binary)  
**Project Type**: Single project (Rust server application)  

**Performance Goals**: 
- Budget check: < 100μs per request (in-memory atomic read)
- Cost estimation: < 50μs (simple arithmetic on token counts)
- Reconciliation loop: 60-second intervals, < 5ms per cycle
- Routing overhead: Budget adds < 0.1ms to existing < 1ms routing target

**Constraints**: 
- No external service dependencies (all in-process)
- No database or persistent state (in-memory atomic counters)
- Must integrate with existing reconciler pipeline (src/control/reconciler.rs)
- Must use existing InferenceAgent::count_tokens() trait method
- Graceful degradation required (fail-open for budget service errors)

**Scale/Scope**: 
- Support 1000+ requests/second with atomic spending updates
- Monthly budgets: $0.01 to $100,000+ (f64 precision)
- Track 10-100 concurrent backends with per-provider cost models
- Acceptable overage: up to [concurrent_requests] × [avg_cost] due to eventual consistency

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Simplicity Gate
- [x] Using ≤3 main modules for initial implementation? 
  - **YES**: Budget config (1 struct), BudgetReconciler enhancement (existing stub), background loop (1 task)
- [x] No speculative "might need" features?
  - **YES**: Implementing only P1/P2 requirements (basic tracking, soft/hard limits). No preemptive v0.4 tokenizers.
- [x] No premature optimization?
  - **YES**: Using simple atomic counters + DashMap. No complex caching or pre-computation.
- [x] Start with simplest approach that could work?
  - **YES**: Heuristic tokenization (chars/4) for v0.3 MVP, exact tokenizers deferred to v0.4.

### Anti-Abstraction Gate
- [x] Using Axum/Tokio/reqwest directly (no wrapper layers)?
  - **YES**: Integrates with existing `metrics` crate (Prometheus), `tokio::spawn` for reconciliation loop.
- [x] Single representation for each data type?
  - **YES**: `BudgetStatus` (existing), `CostEstimate` (new), `BudgetConfig` (new) - one type per concept.
- [x] No "framework on top of framework" patterns?
  - **YES**: Extends existing `Reconciler` trait, no new abstractions.
- [x] Abstractions justified by actual (not theoretical) needs?
  - **YES**: `BudgetReconciler` is already in pipeline, just enhancing existing design.

### Integration-First Gate
- [x] API contracts defined before implementation?
  - **YES**: Budget config schema (TOML), Prometheus metrics schema, routing intent annotations.
- [x] Integration tests planned with real/mock backends?
  - **YES**: Mock backends with known costs, test soft/hard limit enforcement in routing.
- [x] End-to-end flow testable?
  - **YES**: Request → count tokens → estimate cost → check budget → route/block → update spending.

### Performance Gate
- [x] Routing decision target: < 1ms?
  - **YES**: Budget check is atomic read (< 100μs), cost estimation is arithmetic (< 50μs), total < 0.15ms overhead.
- [x] Total overhead target: < 5ms?
  - **YES**: Budget adds < 0.2ms to existing routing pipeline.
- [x] Memory baseline target: < 50MB?
  - **YES**: Adds ~1KB per backend for cost model + single atomic counter for current spending (~8 bytes).

**Status**: ✅ All gates PASS - No complexity justification needed.

## Project Structure

## Project Structure

### Documentation (this feature)

```text
specs/016-inference-budget/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output - tokenization approaches, cost models, reconciliation patterns
├── data-model.md        # Phase 1 output - BudgetConfig, CostEstimate, BudgetMetrics schemas
├── quickstart.md        # Phase 1 output - Operator guide for configuring budgets
├── contracts/           # Phase 1 output
│   └── budget-config.toml   # Budget configuration schema example
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
src/
├── config/
│   ├── mod.rs               # Add BudgetConfig to NexusConfig
│   └── budget.rs            # NEW: Budget configuration types and validation
├── control/
│   ├── budget.rs            # ENHANCE: BudgetReconciler with cost estimation + limit enforcement
│   ├── reconciler.rs        # EXISTING: Pipeline executor (no changes)
│   └── intent.rs            # ENHANCE: Add budget_status and cost_estimate annotations
├── metrics/
│   └── mod.rs               # ENHANCE: Add budget-related Prometheus metrics
├── agent/
│   └── mod.rs               # EXISTING: InferenceAgent::count_tokens() already defined (no changes)
└── main.rs                  # ENHANCE: Spawn BudgetReconciliationLoop background task

tests/
├── contract/
│   └── budget_config_test.rs      # Validate budget TOML parsing
├── integration/
│   ├── budget_tracking_test.rs    # Test cost calculation and spending updates
│   └── budget_enforcement_test.rs # Test soft/hard limit routing behavior
└── unit/
    └── cost_estimation_test.rs    # Unit tests for cost calculation logic
```

**Structure Decision**: This feature integrates into the existing single-project Rust codebase. No new modules required—only enhancements to `config/`, `control/`, and `metrics/` modules. The BudgetReconciler stub already exists at `src/control/budget.rs` and will be enhanced with cost estimation and limit enforcement logic. Background reconciliation loop spawned in `main.rs` alongside existing health checker.

## Complexity Tracking

> **No violations - Constitution Check passed all gates.**

This feature requires no complexity justification. It integrates cleanly into the existing reconciler pipeline without new abstractions or dependencies (for v0.3 MVP).

---

## Implementation Phases (Summary)

### Phase 0: Configuration & State (COMPLETED - Design)
**Artifacts**: 
- research.md: Tokenization strategy, pricing tables, reconciliation patterns
- data-model.md: BudgetConfig, BudgetState, CostEstimate entities
- contracts/budget-config.toml: Configuration schema with examples

**Key Decisions**:
- Heuristic tokenization (chars/4 * 1.15) for v0.3 MVP
- Hardcoded pricing tables (no external APIs)
- Atomic counter (Arc<AtomicU64>) for lock-free spending updates
- 60-second reconciliation loop for billing cycle resets

### Phase 1: Cost Estimation & Tracking
**Files**:
- `src/config/budget.rs` (NEW): BudgetConfig type and validation
- `src/config/mod.rs` (ENHANCE): Add BudgetConfig to NexusConfig
- `src/control/budget/pricing.rs` (NEW): PricingRegistry with hardcoded tables
- `src/control/budget.rs` (ENHANCE): Add cost estimation to BudgetReconciler

**Tests**:
- Unit: Pricing lookups (exact match, prefix match, unknown fallback)
- Unit: Cost calculation (input + output tokens)
- Integration: Cost estimation from chat completion requests

### Phase 2: Soft Limit Enforcement
**Files**:
- `src/control/budget.rs` (ENHANCE): Implement BudgetStatus logic
- `src/control/intent.rs` (ENHANCE): Add budget_status annotation
- `src/control/selection.rs` (ENHANCE): Prefer local agents when SoftLimit

**Tests**:
- Integration: Verify local-first routing at 80% budget
- Integration: Verify warning logs emitted

### Phase 3: Hard Limit Enforcement
**Files**:
- `src/control/budget.rs` (ENHANCE): Filter backends based on hard_limit_action
- `src/api/chat.rs` (ENHANCE): Return 429 errors when action=reject

**Tests**:
- Integration: Verify cloud blocking at 100% budget (local-only)
- Integration: Verify 429 errors at 100% budget (reject)
- Unit: Verify BudgetViolation messages

### Phase 4: Metrics & Observability
**Files**:
- `src/metrics/mod.rs` (ENHANCE): Add budget metrics functions
- `src/control/budget.rs` (ENHANCE): Record cost estimates in metrics

**Tests**:
- Integration: Verify metrics exposed at /metrics
- Contract: Prometheus metrics schema validation

### Phase 5: Billing Cycle Reset
**Files**:
- `src/main.rs` (ENHANCE): Spawn BudgetReconciliationLoop task
- `src/control/budget.rs` (NEW): BudgetReconciliationLoop background task

**Tests**:
- Unit: Mock time advancement, verify reset logic
- Integration: Verify logs on reset

---

## Next Steps (for /speckit.tasks command)

1. Generate tasks.md with dependency-ordered task list
2. Each task should reference:
   - Files to modify (from Implementation Phases above)
   - Tests to write (TDD: tests first)
   - Acceptance criteria (from spec.md)
   - Constitution gates to verify

3. Task structure:
   - **T1**: Configuration schema (BudgetConfig, validation)
   - **T2**: Pricing registry (hardcoded tables, lookup logic)
   - **T3**: Cost estimation (integrate with count_tokens())
   - **T4**: Budget state management (AtomicU64, BudgetStatus)
   - **T5**: Soft limit routing (prefer local at 80%)
   - **T6**: Hard limit enforcement (filter/reject at 100%)
   - **T7**: Prometheus metrics (gauges, counters, histograms)
   - **T8**: Billing cycle reset (background loop)
   - **T9**: Integration tests (end-to-end budget scenarios)
   - **T10**: Documentation updates (README, FEATURES.md)

---

## Generated Artifacts

All design artifacts have been generated in Phase 0 and Phase 1:

```
specs/016-inference-budget/
├── plan.md                           ← This file
├── research.md                       ← Phase 0 output (14KB, 5 research questions)
├── data-model.md                     ← Phase 1 output (19KB, 6 entities)
├── quickstart.md                     ← Phase 1 output (13KB, operator guide)
└── contracts/
    ├── budget-config.toml            ← Phase 1 output (7.5KB, config schema)
    └── prometheus-metrics.md         ← Phase 1 output (10KB, metrics contract)
```

**Total**: 63KB of documentation across 6 files

---

## Branch & Next Command

**Current Branch**: `016-inference-budget`  
**Plan Location**: `specs/016-inference-budget/plan.md`  
**Next Command**: `copilot /speckit.tasks specs/016-inference-budget/spec.md`  

This will generate `specs/016-inference-budget/tasks.md` with actionable, dependency-ordered tasks for implementation.
