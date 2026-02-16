# Implementation Plan: Control Plane — Reconciler Pipeline

**Branch**: `014-control-plane-reconciler` | **Date**: 2025-01-09 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/014-control-plane-reconciler/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

Replace the imperative Router::select_backend() god-function with a pipeline of independent Reconcilers that annotate shared routing state. This architectural transformation enables Privacy Zones (F13) and Budget Management (F14) to be implemented as composable reconcilers without O(n²) feature interaction. The pipeline executes 6 reconcilers in fixed order: RequestAnalyzer → PrivacyReconciler → BudgetReconciler → TierReconciler → QualityReconciler → SchedulerReconciler. Each reconciler only adds constraints to RoutingIntent; they never remove constraints, ensuring order-independence and composability. The design maintains backward compatibility: Router::select_backend() method signature remains unchanged, and all existing tests pass without modification.

## Technical Context

**Language/Version**: Rust 1.87 (stable toolchain)  
**Primary Dependencies**: Tokio (async runtime), Axum (HTTP framework), DashMap (concurrent state), thiserror (error handling)  
**Storage**: In-memory only (no persistence required)  
**Testing**: cargo test with proptest for property-based scoring tests  
**Target Platform**: Linux server (also macOS/Windows compatible)  
**Project Type**: Single project (library + binary)  
**Performance Goals**: Pipeline execution <1ms p95, RequestAnalyzer <0.5ms, total overhead <2ms per request  
**Constraints**: Zero-copy where possible, no heap allocations in hot path, maintain backward compatibility with existing Router tests  
**Scale/Scope**: 1000+ concurrent requests, pipeline processing ~500K requests/day typical workload

**Existing Infrastructure**:
- RequestRequirements struct already implemented in `src/routing/requirements.rs` (RFC-001 Phase 1 complete)
- AgentProfile with PrivacyZone enum exists in `src/agent/types.rs`
- Router::select_backend() in `src/routing/mod.rs` is the god-function to be replaced
- Model alias resolution and fallback chains already working
- TOML config loading with serde already in place (`src/config/routing.rs`)

**Open Questions Requiring Research**:
- How to implement BudgetReconciliationLoop background task with Tokio (spawned task vs separate service)?
- Best pattern for AgentSchedulingProfile: extend existing Backend struct or new parallel struct?
- Where to store budget state: in Router, separate BudgetTracker service, or Registry extension?
- How to implement TrafficPolicy glob pattern matching (use glob crate vs manual matching)?
- Cost estimation integration: extend existing agent.count_tokens() or new trait method?

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Simplicity Gate
- [x] Using ≤3 main modules for initial implementation? YES - only `src/routing/reconciler/` module added with pipeline logic
- [x] No speculative "might need" features? YES - only implementing reconcilers for defined user stories (Privacy, Budget, Tier)
- [x] No premature optimization? YES - start with straightforward trait + Vec<Box<dyn Reconciler>> pipeline, optimize if profiling shows need
- [x] Start with simplest approach that could work? YES - trait-based pipeline with mutable RoutingIntent struct

### Anti-Abstraction Gate
- [x] Using Axum/Tokio/reqwest directly (no wrapper layers)? YES - no new HTTP abstractions, reusing existing stack
- [x] Single representation for each data type? YES - RoutingIntent is single source of truth, no parallel state structures
- [x] No "framework on top of framework" patterns? YES - Reconciler trait is minimal interface, not a framework
- [x] Abstractions justified by actual (not theoretical) needs? YES - Pipeline pattern solves real O(n²) feature interaction problem documented in spec

### Integration-First Gate
- [x] API contracts defined before implementation? YES - RoutingIntent struct, RoutingDecision enum, RejectionReason struct all specified in FR-002/003/004
- [x] Integration tests planned with real/mock backends? YES - existing Router tests remain, new reconciler tests with mock AgentSchedulingProfiles
- [x] End-to-end flow testable? YES - Router::select_backend() maintains same signature, existing integration tests verify compatibility

### Performance Gate
- [x] Routing decision target: < 1ms? YES - FR-036 specifies <1ms total pipeline overhead
- [x] Total overhead target: < 5ms? YES - <2ms target aligns with constitution latency budget
- [x] Memory baseline target: < 50MB? YES - RoutingIntent is stack-allocated struct, reconcilers are stateless

**GATE RESULT: ✅ ALL GATES PASSED**

No violations to justify. Design follows constitution principles:
- Zero Configuration: TrafficPolicies optional (FR-034)
- Explicit Contracts: Detailed RejectionReason instead of silent failures (FR-004)
- OpenAI-Compatible: No API changes, routing metadata in X-Nexus-* headers
- Intelligent Routing: Pipeline enables composable routing logic (Constitution Principle V)

## Project Structure

### Documentation (this feature)

```text
specs/014-control-plane-reconciler/
├── spec.md              # Feature specification (existing)
├── plan.md              # This file (in progress)
├── research.md          # Phase 0 output (to be generated)
├── data-model.md        # Phase 1 output (to be generated)
├── quickstart.md        # Phase 1 output (to be generated)
├── contracts/           # Phase 1 output (to be generated)
│   ├── routing-intent.md       # RoutingIntent struct contract
│   ├── routing-decision.md     # RoutingDecision enum contract
│   ├── reconciler-trait.md     # Reconciler trait contract
│   └── traffic-policy.toml     # TrafficPolicy TOML schema
└── tasks.md             # Phase 2 output (/speckit.tasks - NOT created by this command)
```

### Source Code (repository root)

```text
src/routing/
├── mod.rs                        # Router struct (MODIFY: integrate pipeline)
├── requirements.rs               # RequestRequirements (EXISTING, reuse)
├── scoring.rs                    # Scoring logic (EXISTING, reuse in SchedulerReconciler)
├── strategies.rs                 # Routing strategies (EXISTING, reuse)
├── error.rs                      # RoutingError (MODIFY: add Reject variant)
└── reconciler/                   # NEW MODULE for pipeline
    ├── mod.rs                    # Pipeline executor + Reconciler trait
    ├── intent.rs                 # RoutingIntent struct
    ├── decision.rs               # RoutingDecision enum
    ├── request_analyzer.rs       # RequestAnalyzer reconciler
    ├── privacy.rs                # PrivacyReconciler
    ├── budget.rs                 # BudgetReconciler + BudgetReconciliationLoop
    ├── tier.rs                   # TierReconciler
    ├── quality.rs                # QualityReconciler (stub for now)
    ├── scheduler.rs              # SchedulerReconciler
    └── tests.rs                  # Pipeline integration tests

src/config/
└── routing.rs                    # MODIFY: add TrafficPolicy config structs

src/agent/
└── types.rs                      # MODIFY: add AgentSchedulingProfile struct

tests/
└── routing_integration.rs        # EXISTING: must pass without modification
```

**Structure Decision**: Single project structure maintained. New reconciler module at `src/routing/reconciler/` contains all pipeline logic. This keeps routing concerns co-located while separating the god-function (Router::select_backend) from the pipeline architecture. AgentSchedulingProfile added to agent/types.rs to extend existing agent abstraction. TrafficPolicy config naturally extends existing routing config in src/config/routing.rs.

## Complexity Tracking

> **No violations identified - Constitution Check passed all gates.**

The reconciler pipeline architecture is justified by actual needs documented in the feature specification:
- Solves real O(n²) feature interaction problem (FR-001 rationale)
- Enables independent testing of routing concerns (SC-008)
- Required for Privacy Zones (F13) and Budget Management (F14) features

The Reconciler trait abstraction is minimal (2 methods: name() and reconcile()) and follows existing Nexus patterns (similar to InferenceAgent trait in src/agent/mod.rs).

---

## Phase 0: Research ✅ COMPLETE

Research has been completed and documented in `research.md`. Key decisions:

1. **BudgetReconciliationLoop**: Service struct + tokio::spawn with CancellationToken (follows health checker pattern)
2. **AgentSchedulingProfile**: New struct composing AgentProfile + Backend metrics + quality metrics
3. **TrafficPolicy matching**: Use `globset` crate (1.5µs overhead, pre-compiled patterns)
4. **Budget state storage**: Arc<DashMap> shared between reconciliation loop and reconciler
5. **Cost estimation**: Reuse existing token counting + pricing (no new agent methods)

All decisions documented with rationales, performance implications, and alternatives rejected.

---

## Phase 1: Design & Contracts ✅ COMPLETE

Design artifacts generated:

### Data Model (`data-model.md`)
- Reconciler trait with behavioral contract
- RoutingIntent struct with state transitions
- RoutingDecision enum (Route | Queue | Reject)
- BudgetStatus, CostEstimate, RejectionReason
- AgentSchedulingProfile composition
- TrafficPolicy, BudgetConfig, MetricsSnapshot
- All structures validated against functional requirements

### API Contracts (`contracts/`)
- **reconciler-trait.md**: Behavioral contract with invariants, performance requirements, testing contract
- **routing-intent.md**: State transitions, helper methods, invariants
- **routing-decision.md**: Variant semantics, HTTP response formats, decision logic
- **traffic-policy.toml**: TOML schema, validation rules, pattern precedence

### Quickstart Guide (`quickstart.md`)
- 7-phase implementation roadmap
- Code examples for each reconciler
- Integration into existing Router
- Configuration examples
- Testing strategy and performance checklist

---

## Phase 2: Planning Complete — Next: Task Generation

**Implementation plan is complete**. Next steps:

1. Run `/speckit.tasks` to generate actionable task breakdown
2. Follow TDD: Write tests first (RED phase), then implement
3. Implement in order: Core infrastructure → RequestAnalyzer → Privacy → Budget → Tier → Scheduler → Integration
4. Validate performance with benches/routing.rs

**Estimated implementation time**: 8-12 hours for core pipeline + 6 reconcilers

---

## Summary: Deliverables

This plan provides:

✅ **Technical Context**: Rust/Tokio/Axum stack, performance goals, existing infrastructure  
✅ **Constitution Validation**: All gates passed, zero violations  
✅ **Research Findings**: 5 technical decisions with rationales (research.md)  
✅ **Data Model**: Complete struct definitions with validation rules (data-model.md)  
✅ **API Contracts**: 4 contract documents with behavioral specifications (contracts/)  
✅ **Quickstart Guide**: 7-phase roadmap with code examples (quickstart.md)  
✅ **Agent Context**: Updated copilot context with new technologies

**Status**: Ready for task generation and implementation
