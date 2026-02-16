# Data Model: Privacy Zones & Capability Tiers

**Feature Branch**: `015-privacy-zones`  
**Date**: 2025-02-16

## Overview

This document defines the core entities, enums, and relationships for implementing privacy zone enforcement and capability tier matching in Nexus. All entities follow the existing patterns established in RFC-001 Control Plane Architecture.

---

## 1. Core Enums

### 1.1 PrivacyZone

**Location**: `src/agent/types.rs` (already exists)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivacyZone {
    /// Local-only backend, no cloud overflow allowed
    Restricted,
    
    /// Cloud backend, can receive overflow from any zone
    Open,
}

impl Default for PrivacyZone {
    fn default() -> Self {
        Self::Restricted  // Fail-safe default
    }
}
```

**Validation Rules**:
- MUST be one of: "Restricted" or "Open" (case-insensitive in TOML)
- Defaults to `Restricted` if not specified (fail-safe)
- Cannot be changed during runtime (requires backend re-registration)

**State Transitions**: None (immutable property of backend)

---

### 1.2 RoutingPreference

**Location**: `src/routing/requirements.rs` (new)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingPreference {
    /// Only route to the exact requested model (default)
    Strict,
    
    /// Allow tier-equivalent model alternatives
    Flexible,
}

impl Default for RoutingPreference {
    fn default() -> Self {
        Self::Strict  // Never surprise developers
    }
}
```

**Validation Rules**:
- Extracted from `X-Nexus-Strict` or `X-Nexus-Flexible` headers
- If both headers present, `Strict` takes precedence
- Only applies to capability tier matching, NEVER privacy zones

**State Transitions**: None (per-request preference)

---

### 1.3 OverflowMode

**Location**: `src/config/routing.rs` (new)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverflowMode {
    /// Block all cross-zone overflow (strict compliance)
    BlockEntirely,
    
    /// Allow overflow for fresh conversations only (no history)
    FreshOnly,
}

impl Default for OverflowMode {
    fn default() -> Self {
        Self::BlockEntirely  // Fail-safe default
    }
}
```

**Validation Rules**:
- Configured per TrafficPolicy
- `FreshOnly` rejects requests with > 1 message or any "assistant" role messages
- Cannot be overridden by request headers

**State Transitions**: None (configuration-driven policy)

---

## 2. Core Structs

### 2.1 CapabilityTier

**Location**: `src/config/backend.rs` (extend existing)

```rust
/// Multi-dimensional capability scores for a backend
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityTier {
    /// Reasoning capability (0-10, higher is better)
    #[serde(default)]
    pub reasoning: Option<u8>,
    
    /// Coding capability (0-10, higher is better)
    #[serde(default)]
    pub coding: Option<u8>,
    
    /// Context window size in tokens
    #[serde(default)]
    pub context_window: Option<u32>,
    
    /// Supports vision/image inputs
    #[serde(default)]
    pub vision: bool,
    
    /// Supports tool/function calling
    #[serde(default)]
    pub tools: bool,
}

impl CapabilityTier {
    /// Check if this tier meets minimum requirements
    pub fn meets_requirements(&self, required: &CapabilityRequirements) -> bool {
        // All specified requirements must be met
        if let Some(min_reasoning) = required.min_reasoning {
            if self.reasoning.unwrap_or(0) < min_reasoning {
                return false;
            }
        }
        
        if let Some(min_coding) = required.min_coding {
            if self.coding.unwrap_or(0) < min_coding {
                return false;
            }
        }
        
        if let Some(min_context) = required.min_context_window {
            if self.context_window.unwrap_or(0) < min_context {
                return false;
            }
        }
        
        if required.vision_required && !self.vision {
            return false;
        }
        
        if required.tools_required && !self.tools {
            return false;
        }
        
        true
    }
}
```

**Validation Rules**:
- `reasoning` and `coding` scores MUST be 0-10 (validated at config load)
- `context_window` MUST be > 0 if specified
- Optional fields default to `None` (capability not declared)
- Backends self-report capabilities (no automatic benchmarking)

**State Transitions**: None (immutable configuration property)

---

### 2.2 CapabilityRequirements

**Location**: `src/config/routing.rs` (new)

```rust
/// Minimum capability requirements from TrafficPolicy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CapabilityRequirements {
    #[serde(default)]
    pub min_reasoning: Option<u8>,
    
    #[serde(default)]
    pub min_coding: Option<u8>,
    
    #[serde(default)]
    pub min_context_window: Option<u32>,
    
    #[serde(default)]
    pub vision_required: bool,
    
    #[serde(default)]
    pub tools_required: bool,
}
```

**Validation Rules**:
- Scores MUST be 0-10 if specified
- All requirements are optional (if not specified, no constraint applied)
- Combined with RequestRequirements to form complete filtering criteria

**State Transitions**: None (policy configuration)

---

### 2.3 TrafficPolicy

**Location**: `src/config/routing.rs` (new)

```rust
/// Route-specific traffic policy for privacy and capability enforcement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficPolicy {
    /// Route pattern (glob syntax: "code-*", "vision-*", etc.)
    pub pattern: String,
    
    /// Required privacy zone (optional)
    #[serde(default)]
    pub privacy: Option<PrivacyZone>,
    
    /// Cross-zone overflow mode
    #[serde(default)]
    pub overflow_mode: OverflowMode,
    
    /// Capability requirements
    #[serde(flatten)]
    pub capabilities: CapabilityRequirements,
}

impl TrafficPolicy {
    /// Check if this policy matches a model name
    pub fn matches(&self, model_name: &str) -> bool {
        // Use glob crate for pattern matching
        let pattern = glob::Pattern::new(&self.pattern)
            .expect("Invalid policy pattern");
        pattern.matches(model_name)
    }
    
    /// Get priority for matching (more specific = higher priority)
    pub fn priority(&self) -> u32 {
        // Exact match: 100
        // Contains wildcard: 50
        // Starts with wildcard: 10
        if !self.pattern.contains('*') {
            100
        } else if self.pattern.starts_with('*') {
            10
        } else {
            50
        }
    }
}
```

**Validation Rules**:
- Pattern MUST be valid glob syntax (validated at config load)
- If `privacy` is None, privacy zone enforcement uses backend defaults
- `overflow_mode` defaults to `BlockEntirely` (fail-safe)
- Capability requirements are all optional

**State Transitions**: None (configuration-driven)

**Relationships**:
- Multiple policies can exist, sorted by priority
- First matching policy wins (specificity ordering)
- If no policy matches, backend defaults apply

---

### 2.4 PrivacyConstraint (Extend Existing)

**Location**: `src/control/privacy.rs` (already exists, minor extension)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivacyConstraint {
    /// No privacy restrictions (can use any backend)
    Unrestricted,
    
    /// Must use local backends only (no cloud)
    Restricted,
    
    /// Custom zone (future: organization-specific zones)
    Zone(PrivacyZone),
}

impl PrivacyConstraint {
    /// Create from TrafficPolicy privacy zone
    pub fn from_policy(policy_zone: Option<PrivacyZone>) -> Self {
        match policy_zone {
            Some(PrivacyZone::Restricted) => Self::Restricted,
            Some(PrivacyZone::Open) => Self::Unrestricted,
            None => Self::Unrestricted,  // No policy = no constraint
        }
    }
}
```

**Validation Rules**:
- `Unrestricted` allows any backend
- `Restricted` blocks Open zone backends
- `Zone(z)` only allows exact zone match (future extensibility)

**State Transitions**: Determined per-request from TrafficPolicy

---

### 2.5 RejectionReason (New)

**Location**: `src/control/mod.rs` (new)

```rust
/// Structured reason for backend rejection
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RejectionReason {
    PrivacyZoneMismatch {
        required: String,
        actual: String,
    },
    TierInsufficientReasoning {
        required: u8,
        actual: u8,
    },
    TierInsufficientCoding {
        required: u8,
        actual: u8,
    },
    ContextWindowTooSmall {
        required: u32,
        actual: u32,
    },
    MissingVisionCapability,
    MissingToolsCapability,
    OverflowBlockedWithHistory,
}

impl RejectionReason {
    /// Human-readable message
    pub fn message(&self) -> String {
        match self {
            Self::PrivacyZoneMismatch { required, actual } => {
                format!("Backend zone '{}' does not match required zone '{}'", actual, required)
            }
            Self::TierInsufficientReasoning { required, actual } => {
                format!("Backend reasoning tier {} below required {}", actual, required)
            }
            Self::TierInsufficientCoding { required, actual } => {
                format!("Backend coding tier {} below required {}", actual, required)
            }
            Self::ContextWindowTooSmall { required, actual } => {
                format!("Backend context window {} below required {}", actual, required)
            }
            Self::MissingVisionCapability => {
                "Backend does not support vision inputs".to_string()
            }
            Self::MissingToolsCapability => {
                "Backend does not support tool/function calling".to_string()
            }
            Self::OverflowBlockedWithHistory => {
                "Cross-zone overflow blocked: request contains conversation history".to_string()
            }
        }
    }
}
```

**Validation Rules**:
- Used in 503 error responses for debugging
- Serialized to JSON for client consumption
- Logged for observability

**State Transitions**: Created during reconciler pipeline, attached to error response

---

## 3. Configuration Extensions

### 3.1 BackendConfig Extension

**Location**: `src/config/backend.rs` (extend existing)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    pub name: String,
    pub url: String,
    #[serde(rename = "type")]
    pub backend_type: BackendType,
    
    #[serde(default = "default_priority")]
    pub priority: i32,
    
    #[serde(default)]
    pub api_key_env: Option<String>,
    
    /// Privacy zone classification
    #[serde(default = "default_zone")]
    pub zone: PrivacyZone,
    
    /// Deprecated: single-tier scoring (use capability_tier instead)
    #[serde(default)]
    #[deprecated(note = "Use capability_tier for multi-dimensional scoring")]
    pub tier: Option<u8>,
    
    /// Multi-dimensional capability scoring (NEW)
    #[serde(default)]
    pub capability_tier: Option<CapabilityTier>,
}

fn default_zone() -> PrivacyZone {
    PrivacyZone::Restricted  // Fail-safe default
}
```

**Validation Rules**:
- `zone` defaults to `Restricted` (explicit opt-in for Open zone)
- `tier` logs deprecation warning if used
- `capability_tier` takes precedence if both `tier` and `capability_tier` specified
- Scores validated at config load time (0-10 range)

---

### 3.2 RoutingConfig Extension

**Location**: `src/config/routing.rs` (extend existing)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RoutingConfig {
    pub strategy: RoutingStrategy,
    pub max_retries: u32,
    pub weights: RoutingWeights,
    
    #[serde(default)]
    pub aliases: HashMap<String, String>,
    
    #[serde(default)]
    pub fallbacks: HashMap<String, Vec<String>>,
    
    /// Traffic policies for privacy and capability enforcement (NEW)
    #[serde(default)]
    pub policies: HashMap<String, TrafficPolicy>,
}
```

**Example TOML**:
```toml
[routing.policies."code-*"]
privacy = "restricted"
min_reasoning = 7
min_coding = 8
overflow_mode = "block-entirely"

[routing.policies."chat-*"]
min_reasoning = 5
overflow_mode = "fresh-only"
```

**Validation Rules**:
- Policy keys are glob patterns
- Policies are optional (empty HashMap = no policy enforcement)
- Invalid patterns cause config load error
- Policies are sorted by priority at load time

---

### 3.3 RequestRequirements Extension

**Location**: `src/routing/requirements.rs` (extend existing)

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct RequestRequirements {
    pub model: String,
    pub estimated_tokens: u32,
    pub needs_vision: bool,
    pub needs_tools: bool,
    pub needs_json_mode: bool,
    
    // Control plane extensions (existing)
    pub privacy_zone: Option<PrivacyZone>,
    pub budget_limit: Option<f64>,
    pub min_capability_tier: Option<u8>,
    
    // NEW: Routing preference from headers
    pub routing_preference: RoutingPreference,
}

impl RequestRequirements {
    /// Extract requirements from request with headers
    pub fn from_request_with_headers(
        request: &ChatCompletionRequest,
        headers: &HeaderMap,
    ) -> Self {
        let mut reqs = Self::from_request(request);
        
        // Extract routing preference from headers
        reqs.routing_preference = if headers.contains_key("x-nexus-flexible") {
            RoutingPreference::Flexible
        } else {
            RoutingPreference::Strict  // Default
        };
        
        reqs
    }
}
```

**Validation Rules**:
- `routing_preference` defaults to `Strict` if no headers present
- `X-Nexus-Strict` header (if present) always overrides `X-Nexus-Flexible`
- Preference only affects tier matching, never privacy zones

---

## 4. Reconciler Annotations Extension

### 4.1 RoutingAnnotations Extension

**Location**: `src/control/intent.rs` (extend existing)

```rust
#[derive(Debug, Clone, Default)]
pub struct RoutingAnnotations {
    // Privacy Policy (existing)
    pub privacy_constraints: Option<PrivacyConstraint>,
    pub privacy_excluded: HashMap<String, PrivacyViolation>,
    
    // Budget Policy (existing)
    pub estimated_cost: Option<f64>,
    pub budget_status: Option<BudgetStatus>,
    pub budget_excluded: HashMap<String, BudgetViolation>,
    
    // Capability Policy (existing, extend)
    pub required_tier: Option<u8>,
    pub capability_excluded: HashMap<String, CapabilityMismatch>,
    
    // NEW: Applied traffic policy
    pub applied_policy: Option<String>,  // Policy pattern that matched
    
    // NEW: Cross-zone overflow decision
    pub overflow_decision: Option<OverflowDecision>,
    
    // NEW: Backend affinity key
    pub affinity_key: Option<u64>,
    
    // Observability (existing)
    pub trace_info: Vec<String>,
    pub fallback_used: bool,
}
```

**Validation Rules**:
- `applied_policy` tracks which TrafficPolicy matched (for audit logs)
- `overflow_decision` documents why overflow was allowed/blocked
- `affinity_key` used for sticky routing within privacy zones

---

### 4.2 OverflowDecision (New)

**Location**: `src/control/privacy.rs` (new)

```rust
#[derive(Debug, Clone)]
pub enum OverflowDecision {
    /// Overflow allowed (fresh conversation, no history)
    AllowedFresh,
    
    /// Overflow blocked (conversation history detected)
    BlockedWithHistory,
    
    /// Overflow blocked (policy requires block-entirely)
    BlockedByPolicy,
    
    /// No overflow needed (backend available in same zone)
    NotNeeded,
}
```

**Validation Rules**:
- Set by PrivacyReconciler during overflow evaluation
- Logged for audit trails
- Included in 503 error response context

**State Transitions**:
```
Initial State: NotNeeded
  → If restricted backend unavailable → Check overflow_mode
    → If BlockEntirely → BlockedByPolicy
    → If FreshOnly + no history → AllowedFresh
    → If FreshOnly + has history → BlockedWithHistory
```

---

## 5. Entity Relationships

```
TrafficPolicy (1) --matches--> (N) RequestRequirements
TrafficPolicy (1) --requires--> (1) PrivacyConstraint
TrafficPolicy (1) --specifies--> (1) CapabilityRequirements

BackendConfig (1) --declares--> (1) PrivacyZone
BackendConfig (1) --declares--> (1) CapabilityTier

RoutingIntent (1) --annotated-by--> (N) PrivacyViolation
RoutingIntent (1) --annotated-by--> (N) CapabilityMismatch
RoutingIntent (1) --annotated-by--> (1) OverflowDecision

RejectionReason (1) --included-in--> (1) 503 Error Response
```

---

## 6. State Machine: Privacy Zone Routing

```
┌─────────────────┐
│  Request Arrives │
└────────┬─────────┘
         │
         ▼
┌─────────────────────────┐
│ Match TrafficPolicy     │
│ (by model name pattern) │
└────────┬────────────────┘
         │
         ▼
┌─────────────────────────┐
│ Extract Privacy Zone    │
│ Constraint from Policy  │
└────────┬────────────────┘
         │
         ▼
    ┌────────┐
    │ Filter │
    │Backends│
    └───┬────┘
        │
        ├─────────────────────────┐
        │                         │
        ▼                         ▼
┌───────────────┐      ┌──────────────────┐
│ Zone Matches? │      │ Zone Mismatch?   │
│   → ALLOW     │      │ → EXCLUDE        │
└───────┬───────┘      │   + Log Reason   │
        │              └──────────────────┘
        ▼
┌────────────────┐
│ Check Overflow │
│ Mode (if needed)│
└───────┬────────┘
        │
        ├────────────────────────┐
        │                        │
        ▼                        ▼
┌──────────────┐      ┌─────────────────┐
│Has History?  │      │  Fresh Request? │
│  → BLOCK     │      │  → Check Policy │
└──────────────┘      └─────────┬───────┘
                                 │
                      ┌──────────┴──────────┐
                      │                     │
                      ▼                     ▼
              ┌──────────────┐    ┌────────────────┐
              │FreshOnly Mode│    │Block Entirely  │
              │  → ALLOW     │    │  → REJECT      │
              └──────────────┘    └────────────────┘
```

---

## 7. Database Schema

**N/A**: Nexus is stateless by design (Principle VIII). All entities are in-memory only.

- Backend configurations loaded from TOML at startup
- TrafficPolicies loaded from TOML at startup
- RoutingIntent created per-request, discarded after response
- No persistent session tracking or conversation history storage

---

## 8. Validation Summary

| Entity | Key Validations |
|--------|-----------------|
| PrivacyZone | Must be Restricted or Open |
| CapabilityTier | Scores 0-10, context_window > 0 |
| TrafficPolicy | Valid glob pattern, optional fields |
| RoutingPreference | Extracted from headers, defaults to Strict |
| OverflowMode | BlockEntirely or FreshOnly |
| BackendConfig | Zone defaults to Restricted, tier scores 0-10 |
| RequestRequirements | Standard OpenAI request format |
| RejectionReason | Serializable to JSON, human-readable message |

---

## 9. Migration Notes

### Backwards Compatibility:

**Existing configs work unchanged**:
- Backends without `zone` default to `Restricted` (safe)
- Backends without `capability_tier` have no tier enforcement
- Requests without headers use strict routing (safe)
- No TrafficPolicies → no policy enforcement (safe)

**New features opt-in**:
- Add `zone = "Open"` to enable cloud backends
- Add `capability_tier` to enable tier enforcement
- Add `[routing.policies]` to enable route-specific policies
- Clients add headers to enable flexible routing

---

**Data Model Complete**: All entities defined with validation rules and relationships documented.
