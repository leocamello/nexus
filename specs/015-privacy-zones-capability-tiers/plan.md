# Implementation Plan: Privacy Zones & Capability Tiers

**Branch**: `015-privacy-zones-capability-tiers` | **Date**: 2025-01-24 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/015-privacy-zones-capability-tiers/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See `.specify/templates/commands/plan.md` for the execution workflow.

## Summary

**Primary Requirement**: Wire existing PrivacyReconciler and TierReconciler (from PR #157) into the live Router pipeline, add zone/tier configuration to Backend structs, parse request headers for enforcement mode, ensure cross-zone failover returns 503 with actionable context, and flow privacy/tier information through to 503 responses.

**Technical Approach**: This is **integration work**, not greenfield development. Core reconcilers (`PrivacyReconciler`, `TierReconciler`) and supporting types (`PrivacyZone`, `RoutingIntent`, `TierEnforcementMode`, `RejectionReason`) already exist with unit tests from PR #157. The work involves:

1. **Configuration**: Backend structs already have `zone: Option<PrivacyZone>` and `tier: Option<u8>` fields (in `src/config/backend.rs`). AgentProfile already contains `privacy_zone` and `capability_tier` fields. Need to ensure these flow from TOML → Backend → AgentProfile during registration.

2. **Header Parsing**: Add parsing for `X-Nexus-Strict` and `X-Nexus-Flexible` request headers to set `TierEnforcementMode` on RoutingIntent.

3. **Pipeline Wiring**: Router already constructs a reconciler pipeline (see `src/routing/mod.rs`). Wire PrivacyReconciler and TierReconciler into the pipeline in correct order: Privacy → Budget → Tier → Quality → Scheduler.

4. **Error Context**: Extend existing 503 error responses (in `src/api/error.rs` and `src/api/completions.rs`) to include `privacy_zone_required` and `required_tier` fields from `RejectionReason` structures.

5. **Response Headers**: The `X-Nexus-Privacy-Zone` header injection already exists in `src/api/headers.rs` and is used in completions. Verify it flows from backend through routing to response.

6. **Integration Tests**: Create end-to-end tests in `tests/` that exercise the full flow: config → routing → rejection → 503 response with context.

## Technical Context

**Language/Version**: Rust 1.75 (stable toolchain)  
**Primary Dependencies**: 
- Tokio (async runtime)
- Axum (HTTP framework) 
- reqwest (HTTP client)
- DashMap (concurrent registry)
- serde/toml (config parsing)

**Storage**: N/A (all state in-memory via DashMap and Arc)  
**Testing**: cargo test with integration tests in `tests/`  
**Target Platform**: Linux server, macOS, Windows (cross-platform binary)  
**Project Type**: Single project (Rust backend with embedded dashboard)  
**Performance Goals**: 
- Reconciler pipeline: < 1ms total (Privacy + Tier reconcilers: < 0.2ms combined)
- Routing overhead: < 5ms P95
- 1000+ concurrent requests

**Constraints**: 
- Total routing overhead: < 10ms maximum
- Memory baseline: < 50MB
- Binary size: < 20MB
- OpenAI API compatibility: MUST NOT modify JSON response body

**Scale/Scope**: 
- 10-100 backends per deployment
- 100-1000 models across all backends
- Home lab / small team scale (not cloud-scale)

**Known Constraints from PR #157**:
- PrivacyReconciler and TierReconciler already exist with unit tests
- RoutingIntent struct already contains `privacy_constraint`, `min_capability_tier`, `tier_enforcement_mode` fields
- RejectionReason struct already tracks exclusion reasons per agent
- BackendConfig already has `zone: Option<PrivacyZone>` and `tier: Option<u8>` fields
- AgentProfile already has `privacy_zone: PrivacyZone` and `capability_tier: Option<u8>` fields
- Pipeline order already defined: Privacy → Budget → Tier → Quality → Scheduler

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Simplicity Gate
- [x] Using ≤3 main modules for initial implementation?
  - **YES**: Only modifying existing modules: `routing` (pipeline wiring), `config` (already has fields), `api` (header parsing + error context)
- [x] No speculative "might need" features?
  - **YES**: All features directly map to spec requirements. No future-proofing.
- [x] No premature optimization?
  - **YES**: Using existing reconciler abstractions from PR #157, straightforward integration
- [x] Start with simplest approach that could work?
  - **YES**: Wiring existing components, no new abstractions needed

### Anti-Abstraction Gate
- [x] Using Axum/Tokio/reqwest directly (no wrapper layers)?
  - **YES**: Header parsing uses Axum's HeaderMap directly, error handling uses existing OpenAI error envelope
- [x] Single representation for each data type?
  - **YES**: PrivacyZone enum (single definition), TierEnforcementMode enum (single definition), both already exist
- [x] No "framework on top of framework" patterns?
  - **YES**: Reconciler trait from PR #157 is minimal (2 methods), directly used by Router
- [x] Abstractions justified by actual (not theoretical) needs?
  - **YES**: Reconciler abstraction already proven necessary by PR #157, reusing not creating

### Integration-First Gate
- [x] API contracts defined before implementation?
  - **YES**: Request headers (X-Nexus-Strict/Flexible) and response headers (X-Nexus-Privacy-Zone) already defined in constitution and existing code
- [x] Integration tests planned with real/mock backends?
  - **YES**: Phase 1 includes integration test spec covering full config → routing → 503 response flow
- [x] End-to-end flow testable?
  - **YES**: Can test: TOML config → Backend registration → Request with headers → Routing decision → 503 with context

### Performance Gate
- [x] Routing decision target: < 1ms?
  - **YES**: Privacy reconciler: ~ 0.05ms (hash lookup), Tier reconciler: ~ 0.05ms (integer comparison), well under budget
- [x] Total overhead target: < 5ms?
  - **YES**: Adding 0.1ms to existing reconciler pipeline (already measured in PR #157)
- [x] Memory baseline target: < 50MB?
  - **YES**: Zero additional heap allocations (enums are Copy, fields already in structs)

**Result**: ✅ ALL GATES PASSED - No complexity tracking needed. This is pure integration work using existing, tested components.

## Project Structure

### Documentation (this feature)

```text
specs/015-privacy-zones-capability-tiers/
├── spec.md              # Feature specification (already exists)
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output - integration points and validation
├── data-model.md        # Phase 1 output - request/response flow
├── quickstart.md        # Phase 1 output - configuration examples
├── contracts/           # Phase 1 output - API contracts
│   ├── request-headers.md
│   ├── response-headers.md
│   └── error-responses.md
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

**Integration Points** (modifying existing code):

```text
src/
├── config/
│   └── backend.rs                    # ALREADY HAS zone/tier fields (PR #157)
├── routing/
│   ├── mod.rs                        # MODIFY: Wire PrivacyReconciler & TierReconciler
│   └── reconciler/
│       ├── privacy.rs                # ALREADY EXISTS (PR #157)
│       ├── tier.rs                   # ALREADY EXISTS (PR #157)
│       └── intent.rs                 # ALREADY EXISTS with all required fields
├── agent/
│   └── types.rs                      # ALREADY HAS PrivacyZone enum & AgentProfile fields
├── api/
│   ├── completions.rs                # MODIFY: Parse X-Nexus-Strict/Flexible headers
│   ├── error.rs                      # MODIFY: Add privacy/tier fields to 503 context
│   └── headers.rs                    # VERIFY: X-Nexus-Privacy-Zone already implemented
└── registry/
    └── mod.rs                        # VERIFY: AgentProfile populated from BackendConfig

tests/
├── reconciler_pipeline_test.rs       # ALREADY EXISTS with privacy/tier tests
└── privacy_tier_integration_test.rs  # CREATE: Full end-to-end integration tests
```

**Structure Decision**: Single project structure. All integration points already exist in the codebase from PR #157. This feature is **wiring work** connecting existing pieces:

1. **Configuration Layer** (`src/config/backend.rs`): Fields already exist, need validation
2. **Routing Layer** (`src/routing/mod.rs`): Pipeline construction, add 2 reconcilers
3. **API Layer** (`src/api/completions.rs`): Header parsing + error context enrichment
4. **Testing Layer** (`tests/`): End-to-end integration tests for full flow

No new modules required. No new abstractions. Pure integration of components from PR #157.

## Complexity Tracking

> **No violations** - All constitution gates passed. This is integration work using existing, tested components from PR #157.

---

## Phase 0: Outline & Research

**Objective**: Identify integration points, validate existing implementations, and document any gaps.

### Research Tasks

1. **Existing Reconciler Validation** (PrivacyReconciler, TierReconciler)
   - Verify PrivacyReconciler correctly reads `AgentProfile.privacy_zone`
   - Verify TierReconciler correctly reads `AgentProfile.capability_tier`
   - Confirm both reconcilers populate `RejectionReason` with actionable context
   - Validate pipeline order: Privacy → Budget → Tier → Quality → Scheduler

2. **Configuration Flow** (TOML → Backend → AgentProfile)
   - Trace: `BackendConfig.zone/tier` → `Backend` struct → `AgentProfile` creation
   - Verify defaults: zone = Open, tier = 1 (per FR-021, FR-022)
   - Confirm validation: tier in 1-5, zone in {Restricted, Open}

3. **Header Parsing Strategy**
   - Existing header parsing patterns in `src/api/completions.rs`
   - Best practice: extract headers before routing, set on `RoutingIntent`
   - Handle conflicting headers: strict takes precedence (safer default)

4. **Error Response Integration**
   - Existing 503 generation in `src/api/completions.rs` and `src/api/error.rs`
   - Existing `ActionableErrorContext` structure
   - How to flow `RejectionReason` → 503 response body

5. **Response Header Flow**
   - `X-Nexus-Privacy-Zone` already implemented in `src/api/headers.rs`
   - Trace: `Backend.privacy_zone` → `RoutingResult` → `NexusTransparentHeaders`
   - Verify header injection point in completions endpoint

### Expected Research Outputs

- **Integration checklist**: Exact code locations to modify
- **Data flow diagrams**: Config → Backend → Routing → Response
- **Edge case catalog**: Invalid configs, conflicting headers, offline backends
- **Test strategy**: Unit vs integration test boundaries

**Success Criteria**: All "NEEDS CLARIFICATION" resolved, ready for data model design.
