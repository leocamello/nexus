# Implementation Plan: Privacy Zones & Capability Tiers

**Branch**: `015-privacy-zones` | **Date**: 2025-02-16 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/015-privacy-zones/spec.md`

## Summary

Implement structural privacy enforcement and capability tier matching in Nexus to guarantee data locality and prevent silent quality downgrades. Privacy zones (Restricted/Open) are backend properties configured by administrators, ensuring sensitive data never leaves local infrastructure. Capability tiers (multi-dimensional scores for reasoning, coding, context, vision, tools) prevent automatic failover to lower-quality models. Optional TrafficPolicies define route-specific requirements via TOML configuration with glob pattern matching.

## Technical Context

**Language/Version**: Rust 1.75 (stable toolchain)  
**Primary Dependencies**: 
  - `tokio` (async runtime)
  - `axum` (HTTP framework for request header extraction)
  - `serde` + `toml` (configuration parsing)
  - `glob` (pattern matching for TrafficPolicy)
  - `tracing` (structured logging)
  
**Storage**: In-memory only (no database, per Principle VIII: Stateless by Design)  
**Testing**: `cargo test` + property-based tests with `proptest` for affinity distribution  
**Target Platform**: Linux/macOS/Windows servers  
**Project Type**: Single binary (Rust monolith)  
**Performance Goals**: 
  - PrivacyReconciler: <50μs per request
  - CapabilityReconciler: <100μs per request
  - Total pipeline overhead: <500μs (meets <1ms routing target)
  - Pattern matching: <10μs (pre-compiled glob patterns)
  
**Constraints**: 
  - Zero allocations in hot path (reconciler filtering)
  - OpenAI API compatibility (headers in response, context in error body)
  - Configuration hot-reload within 5-30 seconds
  - Memory overhead: <5KB per backend for parsed capability tiers
  
**Scale/Scope**: 
  - Typical deployment: 5-20 backends
  - TrafficPolicies: 5-50 patterns
  - Request throughput: 1000+ concurrent requests
  - No persistent state (all per-request computation)

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Simplicity Gate

- [x] Using ≤3 main modules for initial implementation? 
  - **YES**: PrivacyReconciler, CapabilityReconciler, TrafficPolicy (extends existing routing config)
- [x] No speculative "might need" features?
  - **YES**: Only implementing P1/P2 features from spec (privacy zones, tier enforcement, client headers)
- [x] No premature optimization?
  - **YES**: Simple filtering logic first, benchmark-driven optimization only if needed
- [x] Start with simplest approach that could work?
  - **YES**: HashMap metadata lookup, linear filtering, no caching until proven necessary

### Anti-Abstraction Gate

- [x] Using Axum/Tokio/reqwest directly (no wrapper layers)?
  - **YES**: Direct HeaderMap access in completions handler, no custom HTTP abstractions
- [x] Single representation for each data type?
  - **YES**: PrivacyZone enum, CapabilityTier struct, TrafficPolicy struct (no conversions)
- [x] No "framework on top of framework" patterns?
  - **YES**: Extends existing ReconcilerPipeline, no new frameworks
- [x] Abstractions justified by actual (not theoretical) needs?
  - **YES**: RejectionReason enum for structured error responses (actual P3 requirement)

### Integration-First Gate

- [x] API contracts defined before implementation?
  - **YES**: contracts/request-headers.md, contracts/traffic-policy-config.md, contracts/error-responses.md
- [x] Integration tests planned with real/mock backends?
  - **YES**: Test plan in research.md includes unit + integration + contract tests
- [x] End-to-end flow testable?
  - **YES**: quickstart.md provides curl-based E2E test scenarios

### Performance Gate

- [x] Routing decision target: < 1ms?
  - **YES**: PrivacyReconciler <50μs + CapabilityReconciler <100μs = <500μs total
- [x] Total overhead target: < 5ms?
  - **YES**: Reconciler pipeline <500μs well within budget
- [x] Memory baseline target: < 50MB?
  - **YES**: <5KB per backend for parsed tiers, no allocations in hot path

**GATE STATUS**: ✅ PASS - All gates satisfied, no complexity violations to justify

## Project Structure

### Documentation (this feature)

```text
specs/015-privacy-zones/
├── plan.md                              # This file (Phase 0+1+2 planning)
├── research.md                          # Phase 0 output (technical decisions)
├── data-model.md                        # Phase 1 output (entities, validation)
├── quickstart.md                        # Phase 1 output (5-minute setup guide)
├── contracts/                           # Phase 1 output (API contracts)
│   ├── request-headers.md               # X-Nexus-Strict/Flexible headers
│   ├── traffic-policy-config.md         # TOML configuration schema
│   └── error-responses.md               # 503 error format
└── tasks.md                             # Phase 2 output (NOT created by /speckit.plan)
```

### Source Code (repository root)

**Structure**: Single Rust project (extends existing control plane)

```text
src/
├── control/                             # Control plane (existing)
│   ├── mod.rs                           # Pipeline builder
│   ├── privacy.rs                       # ✨ EXTEND: PrivacyReconciler
│   ├── capability.rs                    # ✨ EXTEND: CapabilityReconciler
│   ├── intent.rs                        # ✨ EXTEND: RoutingAnnotations
│   └── decision.rs                      # (existing, no changes)
│
├── config/                              # Configuration (existing)
│   ├── backend.rs                       # ✨ EXTEND: BackendConfig with capability_tier
│   ├── routing.rs                       # ✨ NEW: TrafficPolicy, CapabilityRequirements
│   └── mod.rs                           # (existing loader)
│
├── routing/                             # Routing logic (existing)
│   ├── requirements.rs                  # ✨ EXTEND: RequestRequirements with routing_preference
│   └── mod.rs                           # (existing, no changes)
│
├── api/                                 # API handlers (existing)
│   ├── completions.rs                   # ✨ EXTEND: Extract X-Nexus-* headers
│   └── error.rs                         # ✨ EXTEND: 503 error context
│
└── agent/                               # Agent types (existing)
    └── types.rs                         # ✨ PrivacyZone enum (already exists)

tests/
├── privacy_reconciler_tests.rs          # ✨ NEW: Unit tests for privacy filtering
├── capability_reconciler_tests.rs       # ✨ NEW: Unit tests for tier matching
├── traffic_policy_tests.rs              # ✨ NEW: Pattern matching tests
└── routing_integration.rs               # ✨ EXTEND: E2E privacy+tier scenarios
```

**Structure Decision**: 
- Extends existing control plane architecture (RFC-001)
- No new top-level modules (fits within existing `control/`, `config/`, `routing/`)
- PrivacyReconciler and CapabilityReconciler already stubbed out
- TrafficPolicy is new config entity in `config/routing.rs`
- All integration happens via ReconcilerPipeline (no coupling to HTTP layer)

## Complexity Tracking

**No violations**: All constitution gates passed. No complexity justifications needed.

---

## Phase 0: Research (COMPLETE)

**Output**: `research.md` documenting all technical decisions

**Key Decisions Made**:
1. Multi-dimensional capability scoring (reasoning, coding, context_window, vision, tools)
2. TrafficPolicy TOML configuration with glob pattern matching
3. Request header extraction (X-Nexus-Strict, X-Nexus-Flexible) in completions handler
4. Cross-zone overflow with history blocking (configurable: block-entirely, fresh-only)
5. Backend affinity via consistent hashing (best-effort, no persistent state)
6. Actionable 503 error responses with structured context
7. Performance targets: <50μs privacy, <100μs tier, <500μs total
8. Observability: Prometheus counters with dimensional labels
9. Testing: Unit + Integration + Contract + Property-based
10. Configuration migration: Backwards compatible, deprecate single `tier` field

**All "NEEDS CLARIFICATION" Resolved**: Ready for Phase 1 design.

---

## Phase 1: Design & Contracts (COMPLETE)

**Output**: `data-model.md`, `contracts/*`, `quickstart.md`

**Artifacts Generated**:

### Data Model (`data-model.md`)
- Core enums: `PrivacyZone`, `RoutingPreference`, `OverflowMode`
- Core structs: `CapabilityTier`, `CapabilityRequirements`, `TrafficPolicy`, `RejectionReason`
- Configuration extensions: `BackendConfig`, `RoutingConfig`, `RequestRequirements`
- Reconciler annotations: `RoutingAnnotations`, `OverflowDecision`
- Entity relationships and state machines documented

### API Contracts (`contracts/`)
1. **request-headers.md**: X-Nexus-Strict and X-Nexus-Flexible behavior
2. **traffic-policy-config.md**: TOML schema with examples and validation rules
3. **error-responses.md**: 503 error format with rejection reasons

### Quickstart (`quickstart.md`)
- 5-minute setup guide with configuration examples
- Client integration (TypeScript, Python, cURL)
- Operational guidance (monitoring, alerts, troubleshooting)

**Design Validated**: All contracts reviewed, entities have validation rules.

---

## Phase 2: Task Generation (NEXT STEP)

**Command**: `/speckit.tasks` (separate from this plan command)

**Expected Output**: `tasks.md` with:
- Task breakdown (T01-T20+)
- Dependency ordering (Phase 0 research → Phase 1 contracts → Phase 2 implementation)
- Acceptance criteria per task
- Test-first development tasks (TDD: write tests → RED → GREEN → REFACTOR)

**Not Generated Here**: This plan command stops after Phase 1. Task generation is a separate workflow.

---

## Implementation Notes

### Order of Operations

1. **Config Layer First** (no runtime impact until reconcilers use it)
   - Add `CapabilityTier` struct to `BackendConfig`
   - Add `TrafficPolicy` to `RoutingConfig`
   - Add header extraction to `RequestRequirements`

2. **Reconcilers Second** (core filtering logic)
   - Extend `PrivacyReconciler` with overflow mode logic
   - Extend `CapabilityReconciler` with multi-dimensional tier matching
   - Wire into `ReconcilerPipeline`

3. **Error Handling Third** (observability)
   - Add `RejectionReason` to 503 error responses
   - Add Prometheus metrics for rejections
   - Add structured logging

4. **Integration Fourth** (E2E validation)
   - Test with mock backends (Ollama, vLLM, OpenAI)
   - Test client header extraction
   - Test TrafficPolicy pattern matching

### Test-Driven Development Workflow

**Mandatory TDD Process** (per Constitution Section "Testing Standards"):

1. **Write contract test** (define expected behavior)
2. **Write integration test** (E2E scenario)
3. **Write unit test** (specific function)
4. **Run tests** → Confirm RED (tests fail)
5. **Implement code** → Tests pass GREEN
6. **Refactor** → Keep tests GREEN
7. **Check acceptance criteria** → Mark `[x]` in tasks.md

**No implementation code before tests exist and fail.**

### Risk Mitigation

1. **Backend affinity load imbalance**: Monitor backend utilization, use virtual nodes if needed
2. **Complex TrafficPolicy matching latency**: Pre-compile glob patterns at config load, cache match results
3. **Clients not understanding 503 format**: Include human-readable message, keep structured context optional
4. **Privacy zone defaults surprising users**: Log loudly at startup which backends are restricted vs. open

---

## Branch & Artifacts

**Branch**: `015-privacy-zones` (created by setup-plan.sh)  
**Spec**: `/home/lhnascimento/Projects/nexus/specs/015-privacy-zones/spec.md`  
**Plan**: `/home/lhnascimento/Projects/nexus/specs/015-privacy-zones/plan.md` (this file)

**Phase 0 Artifacts**:
- ✅ `research.md` (17KB, 10 technical decisions)

**Phase 1 Artifacts**:
- ✅ `data-model.md` (20KB, 9 core entities, validation rules)
- ✅ `contracts/request-headers.md` (2.5KB, 2 headers with examples)
- ✅ `contracts/traffic-policy-config.md` (8KB, TOML schema + examples)
- ✅ `contracts/error-responses.md` (6KB, 5 rejection reasons)
- ✅ `quickstart.md` (9.5KB, setup guide + client integration)

**Phase 2 Artifacts** (next command):
- ⏳ `tasks.md` (generated by `/speckit.tasks`, not this command)

---

## Next Steps

1. Run `/speckit.tasks` to generate dependency-ordered task breakdown
2. Review generated tasks for completeness
3. Begin implementation with TDD workflow (tests first)
4. Use acceptance criteria in tasks.md to track progress
5. Run constitution check after each major milestone

**Planning Complete**: Ready for task generation and implementation.
