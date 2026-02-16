# Research: Privacy Zones & Capability Tiers Integration

**Date**: 2025-01-24  
**Feature**: F13 - Privacy Zones & Capability Tiers  
**Context**: Integration work connecting components from PR #157

---

## Executive Summary

All core components exist and are tested. No greenfield development required. Research confirms:

1. ✅ **Reconcilers**: PrivacyReconciler and TierReconciler fully implemented with unit tests
2. ✅ **Data Structures**: RoutingIntent, RejectionReason, PrivacyZone, TierEnforcementMode all exist
3. ✅ **Configuration**: BackendConfig has zone/tier fields, AgentProfile has privacy_zone/capability_tier
4. ✅ **Headers**: X-Nexus-Privacy-Zone injection already implemented
5. ⚠️ **Gaps Identified**: 
   - Request header parsing (X-Nexus-Strict/Flexible) not implemented
   - Reconcilers not wired into Router pipeline
   - 503 error context missing privacy/tier fields
   - Integration tests needed for end-to-end flow

---

## 1. Existing Reconciler Validation

### PrivacyReconciler Analysis

**Location**: `src/routing/reconciler/privacy.rs`

**Key Findings**:
- ✅ Implements `Reconciler` trait with `reconcile()` method
- ✅ Reads `AgentProfile.privacy_zone` via `registry.get_agent()`
- ✅ Matches request model against `PolicyMatcher` for privacy constraints
- ✅ Sets `intent.privacy_constraint` when policy matches
- ✅ Excludes agents with `intent.exclude_agent()` including rejection reason
- ✅ Zero-config behavior: no policies = all agents pass through (FR-020)

**Pipeline Behavior**:
```rust
// 1. Look up policy for resolved model
if let Some(policy) = self.policy_matcher.match_model(&intent.resolved_model) {
    if let Some(constraint) = policy.privacy_constraint {
        intent.privacy_constraint = Some(constraint);
    }
}

// 2. Filter candidates by privacy zone
for agent_id in intent.candidate_agents.clone() {
    let zone = self.get_backend_privacy_zone(&agent_id);
    if violates_constraint(zone, intent.privacy_constraint) {
        intent.exclude_agent(
            agent_id,
            "PrivacyReconciler",
            "Privacy zone constraint violation",
            "Check TrafficPolicy configuration"
        );
    }
}
```

**Rejection Reasons**: 
- Includes agent_id, reconciler name, human-readable reason, suggested action
- Stored in `intent.rejection_reasons: Vec<RejectionReason>`
- Ready for 503 error context (just needs plumbing)

### TierReconciler Analysis

**Location**: `src/routing/reconciler/tier.rs`

**Key Findings**:
- ✅ Implements `Reconciler` trait
- ✅ Reads `AgentProfile.capability_tier.unwrap_or(1)` via registry
- ✅ Matches request model against `PolicyMatcher` for tier constraints
- ✅ Sets `intent.min_capability_tier` and respects `intent.tier_enforcement_mode`
- ✅ Supports Strict (default) and Flexible modes (FR-027, FR-028)
- ✅ In Flexible mode: allows higher tiers, blocks lower tiers
- ✅ Zero-config behavior: no policies with min_tier = all pass through

**Enforcement Modes**:
```rust
match intent.tier_enforcement_mode {
    TierEnforcementMode::Strict => {
        // Exclude any agent below min_tier
        if agent_tier < min_tier {
            exclude_with_reason();
        }
    }
    TierEnforcementMode::Flexible => {
        // Allow higher tiers, block lower tiers
        if agent_tier < min_tier && higher_tier_agents_exist {
            exclude_with_reason();
        }
    }
}
```

**Unit Test Coverage**: Both reconcilers have extensive unit tests in their respective files.

---

## 2. Configuration Flow Analysis

### TOML → BackendConfig

**Location**: `src/config/backend.rs`

**Current State**:
```rust
pub struct BackendConfig {
    pub name: String,
    pub url: String,
    pub backend_type: BackendType,
    pub priority: i32,
    pub api_key_env: Option<String>,
    
    #[serde(default)]
    pub zone: Option<PrivacyZone>,  // ✅ Already exists
    
    #[serde(default)]
    pub tier: Option<u8>,            // ✅ Already exists
}

impl BackendConfig {
    pub fn effective_privacy_zone(&self) -> PrivacyZone {
        self.zone.unwrap_or_else(|| self.backend_type.default_privacy_zone())
    }
    
    pub fn effective_tier(&self) -> u8 {
        self.tier.unwrap_or(3)  // ⚠️ Defaults to tier 3, spec says tier 1 (FR-022)
    }
    
    pub fn validate(&self) -> Result<(), String> {
        // ✅ Validates tier range 1-5
        if let Some(tier) = self.tier {
            if !(1..=5).contains(&tier) {
                return Err(format!("Invalid tier {}, must be 1-5", tier));
            }
        }
        Ok(())
    }
}
```

**Gap Identified**: Default tier is 3, but FR-022 specifies tier 1. Need to fix `effective_tier()`.

### BackendConfig → AgentProfile

**Location**: `src/agent/types.rs` (definition), `src/registry/mod.rs` (population)

**AgentProfile Structure**:
```rust
pub struct AgentProfile {
    pub backend_type: String,
    pub version: Option<String>,
    pub privacy_zone: PrivacyZone,         // ✅ Exists
    pub capabilities: AgentCapabilities,
    pub capability_tier: Option<u8>,       // ✅ Exists
}
```

**Population Point**: Need to verify in `Registry::register_backend()` that:
- `profile.privacy_zone = config.effective_privacy_zone()`
- `profile.capability_tier = Some(config.effective_tier())`

**Decision**: Research confirms fields exist. Implementation task will verify population logic.

---

## 3. Header Parsing Strategy

### Existing Patterns

**Location**: `src/api/completions.rs` (POST /v1/chat/completions handler)

**Current Header Usage**:
```rust
// Existing pattern for extracting headers
let headers = req.headers();
let auth_header = headers.get("authorization");
```

**Axum HeaderMap API**:
```rust
use axum::http::HeaderMap;

fn extract_tier_mode(headers: &HeaderMap) -> TierEnforcementMode {
    // Check for X-Nexus-Strict first (safer default takes precedence)
    if headers.get("x-nexus-strict").is_some() {
        return TierEnforcementMode::Strict;
    }
    
    // Then check for X-Nexus-Flexible
    if let Some(value) = headers.get("x-nexus-flexible") {
        if value.to_str().ok() == Some("true") {
            return TierEnforcementMode::Flexible;
        }
    }
    
    // Default to Strict (FR-009)
    TierEnforcementMode::Strict
}
```

**Integration Point**: 
- Extract headers BEFORE calling `router.select_backend()`
- Pass tier mode when constructing RoutingIntent
- Modify RoutingIntent::new() signature OR set after construction

**Decision**: Extract in completions handler, set on RoutingIntent before reconciler pipeline.

### Header Validation

**Rules** (from spec assumptions):
- Conflicting headers (both Strict and Flexible): Strict wins (safer default)
- Invalid values (not "true"): Ignore, use default
- Case-insensitive header names: HTTP standard practice
- Client-provided privacy headers: Ignored (privacy is backend property, FR-024)

---

## 4. Error Response Integration

### Current 503 Generation

**Location**: `src/api/error.rs`, `src/api/completions.rs`

**Existing Structure**:
```rust
// From src/api/error.rs
pub struct ActionableErrorContext {
    pub available_nodes: Option<Vec<String>>,
    pub eta_seconds: Option<u64>,
    // ⚠️ Missing: privacy_zone_required, required_tier
}

pub enum ApiError {
    ServiceUnavailable {
        message: String,
        retry_after: u64,
        context: Option<ActionableErrorContext>,
    },
}
```

**Required Extensions**:
```rust
pub struct ActionableErrorContext {
    pub available_nodes: Option<Vec<String>>,
    pub eta_seconds: Option<u64>,
    
    // FR-015: Privacy zone context
    pub privacy_zone_required: Option<String>,  // "restricted" | "open"
    
    // FR-016: Tier context
    pub required_tier: Option<u8>,  // 1-5
    
    // General rejection reasons
    pub rejection_reasons: Option<Vec<RejectionReason>>,
}
```

**Flow**: RoutingIntent.rejection_reasons → ActionableErrorContext → 503 response JSON

**OpenAI Compatibility**: Context goes inside standard error envelope:
```json
{
  "error": {
    "message": "No backends available for model llama3:70b",
    "type": "service_unavailable",
    "code": null,
    "context": {
      "privacy_zone_required": "restricted",
      "required_tier": 3,
      "rejection_reasons": [...]
    }
  }
}
```

**Decision**: Extend ActionableErrorContext struct, populate from RoutingIntent.rejection_reasons.

---

## 5. Response Header Flow

### X-Nexus-Privacy-Zone Implementation

**Location**: `src/api/headers.rs`

**Current Implementation**:
```rust
pub const HEADER_PRIVACY_ZONE: &str = "x-nexus-privacy-zone";

impl NexusTransparentHeaders {
    pub fn inject_into_response<B>(&self, response: &mut Response<B>) {
        // ... other headers ...
        
        // X-Nexus-Privacy-Zone: "restricted" or "open"
        let privacy_zone_str = match self.privacy_zone {
            PrivacyZone::Restricted => "restricted",
            PrivacyZone::Open => "open",
        };
        headers.insert(
            HeaderName::from_static(HEADER_PRIVACY_ZONE),
            HeaderValue::from_static(privacy_zone_str),
        );
    }
}
```

**Flow Validation**:
1. ✅ `NexusTransparentHeaders` struct has `privacy_zone: PrivacyZone` field
2. ✅ `inject_into_response()` method adds header to HTTP response
3. ⚠️ Need to verify: Where is NexusTransparentHeaders constructed?
4. ⚠️ Need to verify: Does RoutingResult include privacy_zone?

**Trace Through Completions**:
```rust
// In src/api/completions.rs
let routing_result = router.select_backend(...)?;
let backend = routing_result.backend;

// Need to construct NexusTransparentHeaders from routing_result
let headers = NexusTransparentHeaders::new(
    backend.name.clone(),
    backend.backend_type,
    route_reason,
    backend.profile.privacy_zone,  // ← This must exist
    routing_result.cost_estimated,
);

// Inject into response
headers.inject_into_response(&mut response);
```

**Decision**: Verify backend.profile exists and contains privacy_zone. If not, read from Backend struct directly.

---

## 6. Integration Points Checklist

| Component | Status | Action Required |
|-----------|--------|----------------|
| PrivacyReconciler | ✅ Complete | Wire into Router pipeline |
| TierReconciler | ✅ Complete | Wire into Router pipeline |
| PrivacyZone enum | ✅ Complete | None |
| TierEnforcementMode | ✅ Complete | None |
| BackendConfig.zone | ✅ Complete | None |
| BackendConfig.tier | ⚠️ Default wrong | Fix default from 3 → 1 |
| AgentProfile fields | ✅ Complete | Verify population |
| RoutingIntent fields | ✅ Complete | None |
| RejectionReason | ✅ Complete | Flow to 503 response |
| Header parsing | ❌ Missing | Add X-Nexus-Strict/Flexible parsing |
| Error context | ⚠️ Incomplete | Add privacy/tier fields |
| Response headers | ✅ Complete | Verify population from Backend |
| Unit tests | ✅ Complete | Reconcilers already tested |
| Integration tests | ❌ Missing | Create end-to-end tests |

---

## 7. Data Flow Summary

### Request Flow (Config → Routing → Response)

```
┌─────────────────┐
│  nexus.toml     │
│  [[backends]]   │
│  zone = "..."   │
│  tier = N       │
└────────┬────────┘
         │
         ▼
┌─────────────────────┐
│  BackendConfig      │
│  .effective_zone()  │
│  .effective_tier()  │
└─────────┬───────────┘
          │
          ▼
┌──────────────────────┐         ┌──────────────────┐
│  Registry            │────────▶│  AgentProfile    │
│  .register_backend() │         │  .privacy_zone   │
│                      │         │  .capability_tier│
└──────────────────────┘         └──────────────────┘
                                          │
                                          │ Read by reconcilers
                                          ▼
                ┌────────────────────────────────────────┐
                │  Reconciler Pipeline                   │
                │  1. Privacy → filters by zone          │
                │  2. Budget → filters by spending       │
                │  3. Tier → filters by capability       │
                │  4. Quality → scores candidates        │
                │  5. Scheduler → selects final backend  │
                └─────────────────┬──────────────────────┘
                                  │
                                  ▼
            ┌──────────────────────────────┐
            │  RoutingResult               │
            │  .backend (with profile)     │
            │  .route_reason               │
            │  .cost_estimated             │
            └────────────┬─────────────────┘
                         │
                         ▼
            ┌─────────────────────────────┐
            │  NexusTransparentHeaders    │
            │  X-Nexus-Privacy-Zone       │
            │  X-Nexus-Backend-Type       │
            │  X-Nexus-Route-Reason       │
            │  X-Nexus-Cost-Estimated     │
            └─────────────┬───────────────┘
                          │
                          ▼
                   HTTP 200 Response
```

### Rejection Flow (Routing → 503 Response)

```
┌──────────────────────────┐
│  Request Headers         │
│  X-Nexus-Strict: true    │
│  X-Nexus-Flexible: true  │
└───────────┬──────────────┘
            │ Parse before routing
            ▼
┌─────────────────────────────┐
│  RoutingIntent              │
│  .tier_enforcement_mode     │
│  .privacy_constraint        │
│  .min_capability_tier       │
└──────────┬──────────────────┘
           │
           ▼
┌──────────────────────────────────┐
│  Reconciler Pipeline             │
│  (Privacy/Tier excludes agents)  │
└──────────┬───────────────────────┘
           │
           ▼
┌──────────────────────────────────┐
│  RoutingIntent (after pipeline)  │
│  .candidate_agents = []          │
│  .rejection_reasons = [...]      │
└──────────┬───────────────────────┘
           │
           ▼
┌──────────────────────────────────┐
│  RoutingError::Reject            │
│  (no healthy backends)           │
└──────────┬───────────────────────┘
           │
           ▼
┌──────────────────────────────────┐
│  ActionableErrorContext          │
│  .privacy_zone_required          │
│  .required_tier                  │
│  .rejection_reasons              │
└──────────┬───────────────────────┘
           │
           ▼
    ┌──────────────────┐
    │  HTTP 503        │
    │  Retry-After: 30 │
    │  JSON error body │
    └──────────────────┘
```

---

## 8. Edge Cases & Decisions

### Edge Case: Conflicting Headers

**Scenario**: Request has both `X-Nexus-Strict: true` and `X-Nexus-Flexible: true`

**Decision**: Strict takes precedence (safer default, no surprises)

**Rationale**: Better to fail explicitly than silently downgrade quality

### Edge Case: Invalid Tier in Config

**Scenario**: Backend configured with `tier = 10` (out of range)

**Decision**: Reject at startup with validation error

**Current Implementation**: ✅ Already done in `BackendConfig::validate()`

### Edge Case: Missing AgentProfile

**Scenario**: Reconciler calls `registry.get_agent()` returns None

**Current Behavior**: 
- PrivacyReconciler: Treats as Open (cloud) per FR-015
- TierReconciler: Treats as tier 1 per FR-025

**Decision**: Keep current behavior (defensive defaults)

### Edge Case: All Backends Restricted, Request Requires Open

**Scenario**: Privacy policy requires Open, but all backends are Restricted

**Expected**: 503 with privacy_zone_required = "open"

**Decision**: PrivacyReconciler correctly excludes all Restricted backends, returns Reject error

### Edge Case: No Policy Defined

**Scenario**: TrafficPolicy section missing from config

**Expected**: Zero-config backward compatibility (FR-020)

**Current Behavior**: ✅ PolicyMatcher::default() returns empty matcher, all agents pass

---

## 9. Test Strategy

### Unit Tests (Already Exist)

**Location**: `src/routing/reconciler/privacy.rs`, `src/routing/reconciler/tier.rs`

**Coverage**:
- ✅ Privacy constraint enforcement
- ✅ Tier constraint enforcement
- ✅ Strict vs Flexible modes
- ✅ Zero-config behavior
- ✅ Rejection reason generation

### Integration Tests (Need to Create)

**Location**: `tests/privacy_tier_integration_test.rs`

**Scenarios**:
1. Full flow: TOML config → Backend registration → Request → Routing → 503
2. Header parsing: X-Nexus-Strict/Flexible → TierEnforcementMode
3. Response headers: Backend privacy_zone → X-Nexus-Privacy-Zone header
4. Error context: RejectionReason → 503 JSON body
5. Cross-zone failover prevention: Restricted backend offline → 503, not cloud
6. Tier downgrade prevention: Tier 3 offline → 503, not tier 2

**Test Structure**:
```rust
#[tokio::test]
async fn test_privacy_zone_prevents_cross_zone_routing() {
    // 1. Create backends: local (restricted), cloud (open)
    // 2. Configure policy: model="llama3" requires restricted
    // 3. Take local backend offline
    // 4. Send request for llama3
    // 5. Assert: 503 response with privacy_zone_required="restricted"
    // 6. Assert: Cloud backend NOT used
}
```

### Contract Tests (Phase 1 Output)

**Location**: `specs/015-privacy-zones-capability-tiers/contracts/`

**Files**:
- `request-headers.md`: X-Nexus-Strict, X-Nexus-Flexible specs
- `response-headers.md`: X-Nexus-Privacy-Zone spec
- `error-responses.md`: 503 JSON schema with context

---

## 10. Implementation Checklist

### Phase 1: Configuration & Types (No Code Changes Needed)
- [x] Verify BackendConfig has zone/tier fields
- [x] Verify AgentProfile has privacy_zone/capability_tier
- [x] Verify RoutingIntent has constraint fields
- [x] Verify RejectionReason structure
- [ ] Fix BackendConfig.effective_tier() default (3 → 1)

### Phase 2: Router Pipeline Wiring
- [ ] Instantiate PrivacyReconciler in Router::new()
- [ ] Instantiate TierReconciler in Router::new()
- [ ] Build reconciler pipeline: Privacy → Budget → Tier → Quality → Scheduler
- [ ] Pass PolicyMatcher to Privacy/Tier reconcilers
- [ ] Verify pipeline order matches spec

### Phase 3: Header Parsing
- [ ] Add extract_tier_mode() helper in completions.rs
- [ ] Parse X-Nexus-Strict header
- [ ] Parse X-Nexus-Flexible header
- [ ] Set tier_enforcement_mode on RoutingIntent
- [ ] Handle conflicting headers (strict wins)

### Phase 4: Error Context
- [ ] Add privacy_zone_required to ActionableErrorContext
- [ ] Add required_tier to ActionableErrorContext
- [ ] Add rejection_reasons to ActionableErrorContext
- [ ] Flow RoutingIntent.rejection_reasons → ActionableErrorContext
- [ ] Update 503 response builder

### Phase 5: Response Headers
- [ ] Verify Backend has profile with privacy_zone
- [ ] Verify NexusTransparentHeaders populated from backend.profile
- [ ] Test X-Nexus-Privacy-Zone header in responses

### Phase 6: Integration Tests
- [ ] Create tests/privacy_tier_integration_test.rs
- [ ] Test cross-zone failover prevention
- [ ] Test tier downgrade prevention
- [ ] Test header parsing
- [ ] Test error context
- [ ] Test response headers

---

## Conclusion

**All Prerequisites Met**: Components from PR #157 are complete and tested.

**Key Gaps Identified** (all solvable):
1. Header parsing for X-Nexus-Strict/Flexible
2. Reconciler wiring into Router pipeline
3. Error context extension for privacy/tier fields
4. Integration tests for end-to-end flow
5. Minor fix: default tier 3 → 1

**No Blockers**: All integration points exist, data structures are compatible, test infrastructure ready.

**Next Phase**: Create data-model.md documenting request/response flows and data transformations.
