# Data Model: Privacy Zones & Capability Tiers

**Date**: 2025-01-24  
**Feature**: F13 - Privacy Zones & Capability Tiers  
**Phase**: Phase 1 - Design & Contracts

---

## Overview

This document defines the data structures and transformations for privacy zone and capability tier enforcement. All types already exist from PR #157; this documents their relationships and data flow.

---

## Core Entities

### 1. PrivacyZone (Enumeration)

**Location**: `src/agent/types.rs`

**Definition**:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivacyZone {
    /// Restricted: Must not receive cloud overflow. Local-only backends.
    Restricted,
    
    /// Open: Can receive cloud overflow from restricted zones (if policy allows).
    Open,
}
```

**Properties**:
- **Size**: 1 byte (enum discriminant)
- **Copy**: Yes (performance - no heap allocation)
- **Serialization**: "Restricted" | "Open" in JSON/TOML

**Usage**:
- Backend configuration (`BackendConfig.zone`)
- Agent runtime metadata (`AgentProfile.privacy_zone`)
- Policy constraints (`TrafficPolicy.privacy_constraint`)
- Response headers (`X-Nexus-Privacy-Zone`)

**Defaults**:
- Local backends (Ollama, vLLM, LlamaCpp): `Restricted`
- Cloud backends (OpenAI, Anthropic, Google): `Open`
- Explicit config overrides backend type default

---

### 2. TierEnforcementMode (Enumeration)

**Location**: `src/routing/reconciler/intent.rs`

**Definition**:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TierEnforcementMode {
    /// Default: strict enforcement — reject agents below min_tier (FR-027)
    #[default]
    Strict,
    
    /// Flexible: allow fallback to lower tiers when no capable agents remain (FR-028)
    Flexible,
}
```

**Properties**:
- **Size**: 1 byte
- **Copy**: Yes
- **Default**: `Strict` (safer default - no surprises)

**Mapping from Request Headers**:
```
No headers              → Strict (FR-009)
X-Nexus-Strict: true    → Strict (FR-007)
X-Nexus-Flexible: true  → Flexible (FR-008)
Both headers            → Strict (safer default)
```

**Enforcement Rules**:

| Mode | Agent Tier | Min Tier | Result |
|------|-----------|----------|---------|
| Strict | 2 | 3 | ❌ Reject (below min) |
| Strict | 3 | 3 | ✅ Accept (exact match) |
| Strict | 4 | 3 | ✅ Accept (above min) |
| Flexible | 2 | 3 | ❌ Reject (lower tier) |
| Flexible | 3 | 3 | ✅ Accept (exact match) |
| Flexible | 4 | 3 | ✅ Accept (higher tier, equivalent quality) |

---

### 3. BackendConfig (Configuration)

**Location**: `src/config/backend.rs`

**Definition**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    pub name: String,
    pub url: String,
    #[serde(rename = "type")]
    pub backend_type: BackendType,
    pub priority: i32,
    pub api_key_env: Option<String>,
    
    #[serde(default)]
    pub zone: Option<PrivacyZone>,
    
    #[serde(default)]
    pub tier: Option<u8>,
}
```

**Field Specifications**:

| Field | Type | Required | Default | Validation |
|-------|------|----------|---------|------------|
| name | String | Yes | - | Non-empty |
| url | String | Yes | - | Valid URL |
| backend_type | BackendType | Yes | - | Enum variant |
| priority | i32 | No | 50 | Any i32 |
| api_key_env | Option<String> | Conditional | None | Required for cloud backends |
| zone | Option<PrivacyZone> | No | Backend type default | Restricted \| Open |
| tier | Option<u8> | No | 1 | 1-5 inclusive |

**Methods**:
```rust
impl BackendConfig {
    /// Get effective privacy zone (explicit or backend type default)
    pub fn effective_privacy_zone(&self) -> PrivacyZone {
        self.zone.unwrap_or_else(|| self.backend_type.default_privacy_zone())
    }
    
    /// Get effective tier (explicit or default to 1)
    pub fn effective_tier(&self) -> u8 {
        self.tier.unwrap_or(1)  // FR-022: Default tier 1
    }
    
    /// Validate configuration at startup
    pub fn validate(&self) -> Result<(), String> {
        // Cloud backends require API key
        if is_cloud(self.backend_type) && self.api_key_env.is_none() {
            return Err("Cloud backend requires api_key_env");
        }
        
        // Tier must be 1-5
        if let Some(tier) = self.tier {
            if !(1..=5).contains(&tier) {
                return Err(format!("Invalid tier {}, must be 1-5", tier));
            }
        }
        
        Ok(())
    }
}
```

**TOML Example**:
```toml
[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
zone = "restricted"  # Explicit override
tier = 2             # Capability tier

[[backends]]
name = "cloud-gpt4"
url = "https://api.openai.com/v1"
type = "openai"
api_key_env = "OPENAI_API_KEY"
zone = "open"        # Optional, already default for cloud
tier = 5             # Highest capability
```

---

### 4. AgentProfile (Runtime Metadata)

**Location**: `src/agent/types.rs`

**Definition**:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProfile {
    pub backend_type: String,
    pub version: Option<String>,
    pub privacy_zone: PrivacyZone,
    pub capabilities: AgentCapabilities,
    pub capability_tier: Option<u8>,
}
```

**Population from BackendConfig**:
```rust
// In Registry::register_backend()
let profile = AgentProfile {
    backend_type: config.backend_type.to_string(),
    version: None,  // Populated from backend /version endpoint
    privacy_zone: config.effective_privacy_zone(),
    capabilities: AgentCapabilities::default(),  // Discovered via health check
    capability_tier: Some(config.effective_tier()),
};
```

**Access Pattern**:
```rust
// In reconcilers
if let Some(agent) = registry.get_agent(agent_id) {
    let zone = agent.profile().privacy_zone;
    let tier = agent.profile().capability_tier.unwrap_or(1);
}
```

---

### 5. RoutingIntent (Request-Scoped State)

**Location**: `src/routing/reconciler/intent.rs`

**Definition** (relevant fields):
```rust
#[derive(Debug, Clone)]
pub struct RoutingIntent {
    // Identity
    pub request_id: String,
    pub requested_model: String,
    pub resolved_model: String,
    
    // Constraints from Policies
    pub privacy_constraint: Option<PrivacyZone>,
    pub min_capability_tier: Option<u8>,
    pub tier_enforcement_mode: TierEnforcementMode,
    
    // Agent Selection
    pub candidate_agents: Vec<String>,
    pub excluded_agents: Vec<String>,
    pub rejection_reasons: Vec<RejectionReason>,
    
    // ... other fields ...
}
```

**Lifecycle**:
```rust
// 1. Construction (before reconciler pipeline)
let mut intent = RoutingIntent::new(
    request_id,
    requested_model,
    resolved_model,
    requirements,
    all_agent_ids,
);

// 2. Set enforcement mode from request headers
intent.tier_enforcement_mode = extract_tier_mode(&headers);

// 3. Reconciler pipeline modifies intent
intent.privacy_constraint = Some(PrivacyZone::Restricted);  // From policy
intent.min_capability_tier = Some(3);                       // From policy

// 4. Reconcilers exclude agents
intent.exclude_agent(
    "cloud-gpt4".to_string(),
    "PrivacyReconciler",
    "Privacy zone Restricted does not allow Open backends",
    "Configure a restricted backend or change TrafficPolicy",
);

// 5. Convert to RoutingDecision
let decision = RoutingDecision::from_intent(intent)?;
```

**State Transitions**:
```
Initial: candidate_agents = [all backends], rejection_reasons = []
         ↓
Privacy: candidate_agents = [privacy-compliant], rejection_reasons += [excluded]
         ↓
Budget:  candidate_agents = [within-budget], rejection_reasons += [over-budget]
         ↓
Tier:    candidate_agents = [tier-compliant], rejection_reasons += [under-tier]
         ↓
Quality: candidate_agents = [same, ranked by score]
         ↓
Final:   candidate_agents = [selected] OR empty → 503 error
```

---

### 6. RejectionReason (Exclusion Context)

**Location**: `src/routing/reconciler/intent.rs`

**Definition**:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RejectionReason {
    /// Agent that was excluded
    pub agent_id: String,
    
    /// Reconciler that excluded the agent
    pub reconciler: String,
    
    /// Human-readable reason
    pub reason: String,
    
    /// Suggested corrective action for user
    pub suggested_action: String,
}
```

**Examples**:

```rust
// Privacy rejection
RejectionReason {
    agent_id: "cloud-gpt4".to_string(),
    reconciler: "PrivacyReconciler".to_string(),
    reason: "Backend privacy zone 'open' violates constraint 'restricted'".to_string(),
    suggested_action: "Configure a restricted backend or modify TrafficPolicy".to_string(),
}

// Tier rejection (strict mode)
RejectionReason {
    agent_id: "ollama-llama2".to_string(),
    reconciler: "TierReconciler".to_string(),
    reason: "Backend tier 2 below required minimum tier 4".to_string(),
    suggested_action: "Use X-Nexus-Flexible header to allow tier fallback or configure higher-tier backend".to_string(),
}
```

**Serialization to 503 Response**:
```json
{
  "error": {
    "message": "No backends available for model gpt-4",
    "type": "service_unavailable",
    "code": null,
    "context": {
      "required_tier": 4,
      "privacy_zone_required": "restricted",
      "rejection_reasons": [
        {
          "agent_id": "cloud-gpt4",
          "reconciler": "PrivacyReconciler",
          "reason": "Backend privacy zone 'open' violates constraint 'restricted'",
          "suggested_action": "Configure a restricted backend or modify TrafficPolicy"
        }
      ]
    }
  }
}
```

---

### 7. ActionableErrorContext (Error Response)

**Location**: `src/api/error.rs`

**Current Definition** (needs extension):
```rust
pub struct ActionableErrorContext {
    pub available_nodes: Option<Vec<String>>,
    pub eta_seconds: Option<u64>,
}
```

**Extended Definition** (FR-015, FR-016):
```rust
#[derive(Debug, Clone, Serialize)]
pub struct ActionableErrorContext {
    /// Backends available but not selected (informational)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_nodes: Option<Vec<String>>,
    
    /// Estimated time until backend available (seconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_seconds: Option<u64>,
    
    /// Privacy zone required by policy (if privacy rejection)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy_zone_required: Option<String>,  // "restricted" | "open"
    
    /// Minimum tier required by policy (if tier rejection)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_tier: Option<u8>,  // 1-5
    
    /// Detailed rejection reasons per agent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejection_reasons: Option<Vec<RejectionReason>>,
}
```

**Construction from RoutingIntent**:
```rust
impl ActionableErrorContext {
    pub fn from_routing_intent(intent: &RoutingIntent) -> Self {
        Self {
            available_nodes: Some(intent.candidate_agents.clone()),
            eta_seconds: None,  // Could estimate from health checker
            
            // Extract from intent constraints
            privacy_zone_required: intent.privacy_constraint.map(|z| match z {
                PrivacyZone::Restricted => "restricted".to_string(),
                PrivacyZone::Open => "open".to_string(),
            }),
            
            required_tier: intent.min_capability_tier,
            
            // Include all rejection reasons
            rejection_reasons: if intent.rejection_reasons.is_empty() {
                None
            } else {
                Some(intent.rejection_reasons.clone())
            },
        }
    }
}
```

---

### 8. TrafficPolicy (Optional Configuration)

**Location**: `src/config/routing.rs`

**Definition** (from F14):
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct TrafficPolicy {
    /// Glob pattern for model names (e.g., "gpt-4*", "llama*")
    pub model_pattern: String,
    
    /// Privacy constraint for matched models
    #[serde(default)]
    pub privacy_constraint: Option<PrivacyConstraint>,
    
    /// Minimum capability tier for matched models
    #[serde(default)]
    pub min_tier: Option<u8>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub enum PrivacyConstraint {
    /// Must route to Restricted backends only
    Restricted,
    
    /// Must route to Open backends only (rare)
    Open,
}
```

**TOML Example**:
```toml
[[traffic_policies]]
model_pattern = "llama*"
privacy_constraint = "restricted"
min_tier = 2

[[traffic_policies]]
model_pattern = "gpt-4*"
min_tier = 4
# No privacy_constraint = can route to any zone
```

**Zero-Config Behavior**: If `traffic_policies` section missing, PolicyMatcher returns empty, all reconcilers pass agents through unchanged (FR-020).

---

## Data Transformations

### 1. Configuration → Runtime (Startup)

```
BackendConfig (TOML)
  ├─ .zone (Option<PrivacyZone>)
  ├─ .tier (Option<u8>)
  └─ .backend_type (BackendType)
       ↓
       ├─ effective_privacy_zone()
       ├─ effective_tier()
       └─ validate()
             ↓
        AgentProfile
          ├─ .privacy_zone (PrivacyZone)
          └─ .capability_tier (Option<u8>)
                ↓
           Registry.register_backend()
                ↓
           Agent (Arc<Backend>)
```

### 2. Request → Routing (Request Time)

```
HTTP Request
  ├─ Headers
  │   ├─ X-Nexus-Strict: true
  │   └─ X-Nexus-Flexible: true
  │        ↓
  │   extract_tier_mode()
  │        ↓
  │   TierEnforcementMode
  │
  └─ JSON Body
      └─ .model
           ↓
      RoutingIntent
        ├─ .resolved_model
        ├─ .tier_enforcement_mode
        ├─ .candidate_agents = [all]
        └─ .rejection_reasons = []
             ↓
        Reconciler Pipeline
          ├─ Privacy: sets .privacy_constraint, excludes agents
          ├─ Budget: excludes over-budget agents
          ├─ Tier: sets .min_capability_tier, excludes under-tier agents
          ├─ Quality: scores remaining agents
          └─ Scheduler: selects final agent OR returns Reject
                ↓
           RoutingResult OR RoutingError::Reject
```

### 3. Routing → Response (Success)

```
RoutingResult
  ├─ .backend (Arc<Backend>)
  │   └─ .profile
  │       └─ .privacy_zone
  │
  ├─ .route_reason (String)
  └─ .cost_estimated (Option<f64>)
       ↓
  NexusTransparentHeaders
    ├─ X-Nexus-Backend: backend.name
    ├─ X-Nexus-Backend-Type: "local" | "cloud"
    ├─ X-Nexus-Privacy-Zone: "restricted" | "open"
    ├─ X-Nexus-Route-Reason: route_reason
    └─ X-Nexus-Cost-Estimated: cost_usd
         ↓
    HTTP 200 Response
      ├─ Headers (X-Nexus-*)
      └─ Body (OpenAI-compatible JSON)
```

### 4. Routing → Response (Rejection)

```
RoutingIntent (after pipeline)
  ├─ .candidate_agents = []
  ├─ .rejection_reasons = [...]
  ├─ .privacy_constraint = Some(...)
  └─ .min_capability_tier = Some(...)
       ↓
  RoutingError::Reject
       ↓
  ActionableErrorContext
    ├─ .privacy_zone_required
    ├─ .required_tier
    └─ .rejection_reasons
         ↓
  ApiError::ServiceUnavailable
    ├─ .message
    ├─ .retry_after = 30
    └─ .context
         ↓
  HTTP 503 Response
    ├─ Retry-After: 30
    └─ Body (OpenAI error envelope)
```

---

## Field-Level Specifications

### Privacy Zone Values

| String | Enum | Meaning | Used By |
|--------|------|---------|---------|
| "restricted" | PrivacyZone::Restricted | Local-only, no cloud overflow | Local backends, sensitive models |
| "open" | PrivacyZone::Open | Can receive overflow | Cloud backends, non-sensitive models |

### Capability Tier Values

| Tier | Meaning | Example Models |
|------|---------|---------------|
| 1 | Basic capability | Small local models (llama2:7b) |
| 2 | Moderate capability | Medium local models (llama3:13b) |
| 3 | Standard capability | Large local models (llama3:70b), GPT-3.5 |
| 4 | Advanced capability | GPT-4, Claude 3 Sonnet |
| 5 | Premium capability | GPT-4 Turbo, Claude 3 Opus |

**Tier Comparison Rules**:
- Strict mode: `agent_tier >= min_tier` (exact or higher)
- Flexible mode: `agent_tier >= min_tier` OR (all agents below min_tier)

### Request Header Values

| Header | Valid Values | Default |
|--------|--------------|---------|
| X-Nexus-Strict | "true", "false", absent | true (if absent) |
| X-Nexus-Flexible | "true", "false", absent | false (if absent) |

**Conflict Resolution**: X-Nexus-Strict takes precedence over X-Nexus-Flexible

### Response Header Values

| Header | Format | Example |
|--------|--------|---------|
| X-Nexus-Privacy-Zone | "restricted" \| "open" | "restricted" |
| X-Nexus-Backend | String | "ollama-local" |
| X-Nexus-Backend-Type | "local" \| "cloud" | "local" |
| X-Nexus-Route-Reason | kebab-case | "capability-match" |
| X-Nexus-Cost-Estimated | Decimal USD (4 places) | "0.0042" |

---

## Validation Rules

### At Startup (Config Validation)

```rust
// BackendConfig::validate()
pub fn validate(&self) -> Result<(), String> {
    // 1. Cloud backends must have API key
    if is_cloud(self.backend_type) && self.api_key_env.is_none() {
        return Err("Cloud backend requires api_key_env");
    }
    
    // 2. Tier must be 1-5 if specified (FR-018)
    if let Some(tier) = self.tier {
        if !(1..=5).contains(&tier) {
            return Err(format!("Invalid tier {}, must be 1-5", tier));
        }
    }
    
    // 3. Zone must be valid enum (handled by serde deserialization)
    
    Ok(())
}
```

### At Request Time (Header Validation)

```rust
// In completions handler
fn extract_tier_mode(headers: &HeaderMap) -> TierEnforcementMode {
    // Strict takes precedence (safer default)
    if headers.get("x-nexus-strict").is_some() {
        return TierEnforcementMode::Strict;
    }
    
    // Check flexible header
    if let Some(val) = headers.get("x-nexus-flexible") {
        if val.to_str().ok() == Some("true") {
            return TierEnforcementMode::Flexible;
        }
    }
    
    // Default to strict (FR-009)
    TierEnforcementMode::Strict
}
```

### During Routing (Reconciler Validation)

```rust
// PrivacyReconciler: Check zone compatibility
fn violates_constraint(zone: PrivacyZone, constraint: Option<PrivacyZone>) -> bool {
    match constraint {
        None => false,  // No constraint = always pass
        Some(required) => zone != required,
    }
}

// TierReconciler: Check tier threshold
fn below_min_tier(agent_tier: u8, min_tier: Option<u8>) -> bool {
    match min_tier {
        None => false,  // No requirement = always pass
        Some(min) => agent_tier < min,
    }
}
```

---

## Performance Characteristics

### Memory Footprint

| Type | Size | Count | Total |
|------|------|-------|-------|
| PrivacyZone | 1 byte | Per backend | ~100 bytes |
| TierEnforcementMode | 1 byte | Per request | ~1 byte |
| AgentProfile | ~200 bytes | Per backend | ~20 KB (100 backends) |
| RoutingIntent | ~1 KB | Per request | ~1 KB (ephemeral) |
| RejectionReason | ~256 bytes | Per exclusion | ~25 KB (100 exclusions) |

**Total Additional Memory**: < 50 KB (well within 50 MB baseline)

### Latency Budget

| Operation | Target | Maximum |
|-----------|--------|---------|
| PolicyMatcher lookup | 0.01 ms | 0.05 ms |
| PrivacyReconciler | 0.05 ms | 0.1 ms |
| TierReconciler | 0.05 ms | 0.1 ms |
| **Combined Privacy + Tier** | **0.1 ms** | **0.2 ms** |

**Pipeline Total** (all reconcilers): < 1 ms (within budget)

---

## Backward Compatibility

### Zero-Config Behavior (FR-020)

| Config State | Behavior |
|--------------|----------|
| No `zone` field | Backend type default (local=Restricted, cloud=Open) |
| No `tier` field | Default tier = 1 |
| No `traffic_policies` | PolicyMatcher empty, all reconcilers pass-through |
| No request headers | TierEnforcementMode = Strict |

**Result**: Existing deployments work unchanged. Privacy/tier enforcement is opt-in.

---

## Relationship Diagram

```
┌───────────────────────────────────────────────────────────────┐
│                        Configuration                          │
│  ┌─────────────────┐         ┌──────────────────┐           │
│  │ BackendConfig   │────────▶│ TrafficPolicy    │           │
│  │ .zone           │         │ .model_pattern   │           │
│  │ .tier           │         │ .privacy_const   │           │
│  └────────┬────────┘         │ .min_tier        │           │
│           │                  └──────────────────┘           │
└───────────┼───────────────────────────────────────────────────┘
            │
            ▼
┌───────────────────────────────────────────────────────────────┐
│                      Runtime Registry                         │
│  ┌─────────────────────────────────────────────┐             │
│  │ AgentProfile                                │             │
│  │ ├─ privacy_zone: PrivacyZone                │             │
│  │ └─ capability_tier: Option<u8>              │             │
│  └─────────────────────────────────────────────┘             │
└───────────┬───────────────────────────────────────────────────┘
            │
            ▼
┌───────────────────────────────────────────────────────────────┐
│                    Request Processing                         │
│  ┌──────────────────────────────────────────────────┐        │
│  │ RoutingIntent                                    │        │
│  │ ├─ privacy_constraint: Option<PrivacyZone>       │        │
│  │ ├─ min_capability_tier: Option<u8>               │        │
│  │ ├─ tier_enforcement_mode: TierEnforcementMode    │        │
│  │ ├─ candidate_agents: Vec<String>                 │        │
│  │ └─ rejection_reasons: Vec<RejectionReason>       │        │
│  └──────────────────────────────────────────────────┘        │
└───────────┬───────────────────────────────────────────────────┘
            │
            ▼
┌───────────────────────────────────────────────────────────────┐
│                    Response Generation                        │
│  ┌────────────────────────┐  ┌──────────────────────────┐   │
│  │ NexusTransparentHeaders│  │ ActionableErrorContext   │   │
│  │ X-Nexus-Privacy-Zone   │  │ .privacy_zone_required   │   │
│  └────────────────────────┘  │ .required_tier           │   │
│                              │ .rejection_reasons       │   │
│                              └──────────────────────────┘   │
└───────────────────────────────────────────────────────────────┘
```

---

## Conclusion

All data structures already exist from PR #157. This feature is pure **integration work**:

1. **No new types** - reusing existing enums, structs, fields
2. **No schema changes** - extending existing ActionableErrorContext only
3. **No performance impact** - all operations are O(1) lookups or O(n) filters where n is small (~10-100 backends)
4. **Backward compatible** - defaults ensure existing deployments work unchanged

Next phase: Define API contracts (request headers, response headers, error schemas).
