# F13: Privacy Zones & Capability Tiers — Code Walkthrough

Welcome! This walkthrough explains F13 as if you're a junior developer joining the project. We'll walk through every file changed, explain the key Rust concepts, and break down the test strategy.

---

## The Big Picture

**Problem**: When you have both local LLM backends (Ollama on your machine) and cloud backends (OpenAI API), some data shouldn't leave your network. Also, when a high-quality backend goes offline, you don't want requests silently routed to a lower-quality alternative.

**Solution**: F13 adds two structural enforcement mechanisms:

1. **Privacy Zones** — Each backend is tagged as `restricted` (local only) or `open` (cloud/any). Traffic policies declare which models need restricted routing. If no restricted backend is available, you get a clear 503 error instead of silent cloud failover.

2. **Capability Tiers** — Each backend has a tier number (1-5). Clients can request strict mode (default: only same/higher tier) or flexible mode (higher-tier substitution OK, but never downgrade).

### Where F13 Fits in the Architecture

```
Client Request
     │
     ▼
┌─────────────────────────────────────────────┐
│  API Layer (src/api/completions.rs)         │
│  • Extracts X-Nexus-Strict / X-Nexus-Flexible│
│  • Passes tier_enforcement_mode to Router    │
└──────────────┬──────────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────────┐
│  Router (src/routing/mod.rs)                │
│  • Builds reconciler pipeline               │
│  • Runs: Privacy → Budget → Tier → Scheduler│
└──────────────┬──────────────────────────────┘
               │
    ┌──────────┼──────────┐
    ▼          ▼          ▼
┌────────┐ ┌────────┐ ┌────────────┐
│Privacy │ │ Tier   │ │ Scheduler  │
│Reconciler│ │Reconciler│ │Reconciler│
│ Reads  │ │ Reads  │ │ Scores &   │
│ zone   │ │ tier   │ │ selects    │
└────────┘ └────────┘ └────────────┘
```

F13 doesn't add new modules — it **wires** the existing PrivacyReconciler and TierReconciler (from the Control Plane PR #157) into the live Router pipeline, and adds the configuration plumbing to make zones and tiers flow from TOML config all the way to routing decisions.

---

## File-by-File Explanation

### 1. `src/config/backend.rs` — Backend Configuration

This is where zone and tier start their journey.

```rust
pub struct BackendConfig {
    pub name: String,
    pub url: String,
    pub backend_type: BackendType,

    // F13: Privacy zone (restricted = local only, open = any)
    #[serde(default)]
    pub zone: Option<PrivacyZone>,

    // F13: Capability tier (1-5, higher = more capable)
    #[serde(default)]
    pub tier: Option<u8>,

    // ... other fields
}
```

**Key Rust Concepts**:
- `#[serde(default)]` — If the field is missing from TOML, use `Default::default()` (which for `Option<T>` is `None`)
- `Option<PrivacyZone>` — The zone might not be set in config; we'll provide a default based on backend type

**Helper methods provide defaults**:
```rust
pub fn effective_privacy_zone(&self) -> PrivacyZone {
    self.zone.unwrap_or_else(|| self.backend_type.default_privacy_zone())
}

pub fn effective_tier(&self) -> u8 {
    self.tier.unwrap_or(1)  // FR-022: default tier is 1
}
```

**Why `unwrap_or_else` instead of `unwrap_or`?** Because `default_privacy_zone()` is a function call — `unwrap_or_else` only calls it when needed (lazy evaluation).

**Validation** ensures tier is in range:
```rust
if let Some(tier) = self.tier {
    if !(1..=5).contains(&tier) {
        return Err(ConfigError::InvalidTier { tier });
    }
}
```

### 2. `src/agent/factory.rs` — Agent Creation

When Nexus starts, it creates an `InferenceAgent` for each backend. F13 adds zone and tier to the factory function:

```rust
pub fn create_agent(
    id: String,
    name: String,
    url: String,
    backend_type: BackendType,
    client: Arc<Client>,
    metadata: HashMap<String, String>,
    privacy_zone: PrivacyZone,      // F13: new parameter
    capability_tier: Option<u8>,     // F13: new parameter
) -> Result<Arc<dyn InferenceAgent>, AgentError>
```

Each agent type stores these values and returns them via `profile()`:
```rust
fn profile(&self) -> AgentProfile {
    AgentProfile {
        privacy_zone: self.privacy_zone,
        capability_tier: self.capability_tier,
        // ... other fields
    }
}
```

**Key Rust Concept**: `Arc<dyn InferenceAgent>` — this is a **trait object** wrapped in an atomic reference counter. It means "a shared pointer to any type that implements InferenceAgent". This lets us store different agent types (Ollama, OpenAI, etc.) in the same collection.

### 3. `src/api/completions.rs` — Request Handling

This file handles incoming `/v1/chat/completions` requests. F13 adds:

#### Tier Enforcement Mode Extraction

```rust
fn extract_tier_enforcement_mode(headers: &HeaderMap) -> TierEnforcementMode {
    // Strict takes precedence if present
    if headers.contains_key(STRICT_HEADER) {
        return TierEnforcementMode::Strict;
    }
    // Check flexible header
    if let Some(val) = headers.get(FLEXIBLE_HEADER) {
        if val.to_str().ok() == Some("true") {
            return TierEnforcementMode::Flexible;
        }
    }
    // Default to strict — never surprise the developer
    TierEnforcementMode::Strict
}
```

**Why strict is default**: The constitution says "explicit contracts" — if a developer expects GPT-4 quality and gets GPT-3.5, that's a surprise. Strict mode prevents this.

#### Actionable Rejection Responses

When the pipeline rejects a request (e.g., all restricted backends are offline), we build a structured 503:

```rust
fn rejection_response(
    rejection_reasons: Vec<RejectionReason>,
    available_backends: Vec<String>,
) -> Response {
    // Extract privacy_zone_required from PrivacyReconciler rejections
    let privacy_zone_required = rejection_reasons
        .iter()
        .find(|r| r.reconciler == "PrivacyReconciler")
        .map(|r| /* extract zone from reason text */);

    // Extract required_tier from TierReconciler rejections
    let required_tier = rejection_reasons
        .iter()
        .find(|r| r.reconciler == "TierReconciler")
        .and_then(|r| /* parse tier from reason text */);

    // Build structured context
    let context = ActionableErrorContext {
        required_tier,
        available_backends,
        eta_seconds: None,
        privacy_zone_required,
    };

    let error = ServiceUnavailableError::new(message, context);
    // ... add headers and return
}
```

**Key Rust Concept**: `.find()` returns `Option<&T>` — the first element matching the predicate. `.and_then()` chains optional operations: if the previous step returned `None`, skip this step too.

### 4. `src/routing/mod.rs` — Router Pipeline

The Router now builds and runs the full reconciler pipeline:

```rust
fn build_pipeline(&self, ...) -> ReconcilerPipeline {
    let privacy = PrivacyReconciler::new(
        Arc::clone(&self.registry),
        self.policy_matcher.clone(),
    );
    let tier = TierReconciler::new(
        Arc::clone(&self.registry),
        self.policy_matcher.clone(),
    );
    // ... budget, quality, scheduler
    
    ReconcilerPipeline::new(vec![
        Box::new(privacy),    // First: exclude by zone
        Box::new(budget),     // Second: exclude by cost
        Box::new(tier),       // Third: exclude by quality
        Box::new(quality),    // Fourth: quality metrics (future)
        Box::new(scheduler),  // Last: score remaining and select
    ])
}
```

**Pipeline order matters**: Privacy runs first because if a request must stay local, there's no point checking budget or tier for cloud backends.

The `select_backend` method now accepts the optional tier enforcement mode:

```rust
pub fn select_backend(
    &self,
    requirements: &RequestRequirements,
    tier_enforcement_mode: Option<TierEnforcementMode>,
) -> Result<RoutingResult, RoutingError>
```

Passing `None` preserves backward compatibility — existing callers don't need to change.

### 5. `src/routing/reconciler/privacy.rs` — Privacy Reconciler

This was built in PR #157 (Control Plane). F13 wires it into the live pipeline. Here's how it works:

```rust
impl Reconciler for PrivacyReconciler {
    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), ReconcilerError> {
        // Check if a traffic policy applies to this model
        let policy = self.policy_matcher.match_model(&intent.resolved_model);
        
        if let Some(policy) = policy {
            if policy.privacy == PrivacyConstraint::Restricted {
                // Mark intent as requiring restricted zone
                intent.privacy_constraint = Some(PrivacyZone::Restricted);
                
                // Exclude all non-restricted agents
                for agent_id in intent.candidate_agents.clone() {
                    let agent = self.registry.get_agent(&agent_id);
                    if let Some(agent) = agent {
                        if agent.profile().privacy_zone != PrivacyZone::Restricted {
                            intent.exclude_agent(
                                agent_id,
                                "PrivacyReconciler",
                                "zone mismatch: requires restricted",
                                "Add a local backend with zone='restricted'",
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
```

**Key Rust Concept**: `intent.candidate_agents.clone()` — We clone the Vec before iterating because `exclude_agent` mutates `candidate_agents` (removes the agent). You can't iterate and mutate the same collection in Rust.

### 6. `src/routing/reconciler/tier.rs` — Tier Reconciler

Similar to Privacy, but for quality tiers:

```rust
impl Reconciler for TierReconciler {
    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), ReconcilerError> {
        let policy = self.policy_matcher.match_model(&intent.resolved_model);
        
        if let Some(policy) = policy {
            if let Some(min_tier) = policy.min_tier {
                intent.min_capability_tier = Some(min_tier);
                
                for agent_id in intent.candidate_agents.clone() {
                    if let Some(agent) = self.registry.get_agent(&agent_id) {
                        let tier = agent.profile().capability_tier.unwrap_or(1);
                        
                        match intent.tier_enforcement_mode {
                            TierEnforcementMode::Strict => {
                                if tier < min_tier {
                                    intent.exclude_agent(/* ... */);
                                }
                            }
                            TierEnforcementMode::Flexible => {
                                // Same logic: never allow downgrade
                                if tier < min_tier {
                                    intent.exclude_agent(/* ... */);
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
```

**Strict vs Flexible**: In F13's implementation, both modes prevent downgrades. The difference becomes meaningful in F18 (Queuing) where flexible mode might wait for a higher-tier backend to become available instead of rejecting immediately.

---

## Key Rust Concepts Used

### 1. The `Clone` Derive vs Manual Clone

`PrivacyZone` derives `Clone, Copy` because it's a simple enum:
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivacyZone {
    Restricted,
    Open,
}
```
`Copy` means it can be copied by value (like an integer). This is fine for small types.

### 2. Pattern Matching with `if let`

```rust
if let Some(policy) = policy_matcher.match_model(&model) {
    // Only runs if there's a matching policy
}
```
This is equivalent to:
```rust
match policy_matcher.match_model(&model) {
    Some(policy) => { /* use policy */ }
    None => { /* do nothing */ }
}
```
But more concise when you only care about one variant.

### 3. Iterator Chains

```rust
let privacy_zone_required = rejection_reasons
    .iter()                                    // iterate over reasons
    .find(|r| r.reconciler == "PrivacyReconciler")  // find first privacy reason
    .map(|r| "restricted".to_string());        // transform to string
```

Each method returns an `Option` or new iterator, making the chain lazy and composable.

### 4. `Arc::clone` vs `.clone()`

```rust
let registry_clone = Arc::clone(&self.registry);
```
This is preferred over `self.registry.clone()` because it makes explicit that we're cloning the Arc (cheap: just incrementing a counter), not the inner Registry (expensive: copying all data).

---

## Test Walkthrough

### Unit Tests (in source files)

**Privacy reconciler** — 9 tests in `src/routing/reconciler/privacy.rs`:
- Tests that restricted policy excludes open agents
- Tests that unrestricted policy allows all agents
- Tests that no matching policy = no filtering
- Tests rejection reason details (agent_id, reconciler name, suggested action)

**Tier reconciler** — 13 tests in `src/routing/reconciler/tier.rs`:
- Tests strict mode excludes under-tier agents
- Tests flexible mode also excludes under-tier agents
- Tests no policy = no filtering
- Tests edge cases: tier 1 (minimum), tier 5 (maximum)
- Tests rejection reason includes tier numbers

**Config validation** — 9 tests in `src/config/backend.rs`:
- Tests tier range validation (1-5 OK, 0 and 6 rejected)
- Tests zone defaults (Ollama → Restricted, OpenAI → Open)
- Tests effective_tier() and effective_privacy_zone() defaults

**Tier enforcement mode** — Tests in `src/api/completions.rs`:
- Tests header parsing: strict, flexible, both, neither
- Tests default is strict

### Integration Tests (in tests/ directory)

#### `tests/privacy_enforcement_test.rs` (4 tests)
Tests full pipeline privacy enforcement:
```
T024: Restricted backend available → routes to it
T025: Restricted backend offline, open available → 503
T026: Response includes X-Nexus-Privacy-Zone header
T027: Cross-zone failover never happens
```

#### `tests/tier_enforcement_test.rs` (6 tests)
Tests full pipeline tier enforcement:
```
T037: No headers → strict mode → only same/higher tier
T038: X-Nexus-Strict → exact model matching
T039: X-Nexus-Flexible → higher tier substitution
T040: Higher tier offline, lower available → 503
T041: Lower tier offline, higher available → routes to higher
T042: Both headers → strict wins
```

#### `tests/privacy_zone_config_test.rs` (7 tests)
Tests configuration parsing:
```
T014: Explicit zone="restricted" and tier=3 parsed
T015: Missing zone → defaults to backend type
T016: Missing tier → defaults to 1
T017: Invalid tier=10 → validation error
```

#### `tests/actionable_rejection_test.rs` (4 tests)
Tests 503 response structure:
```
T043: Privacy rejection produces reasons with PrivacyReconciler info
T045: Tier rejection includes required tier
T047: Combined privacy + tier rejection
T049: Rejection reasons include suggested actions
```

#### `tests/backward_compat_test.rs` (6 tests)
Tests zero-config backward compatibility:
```
T052: No policies → no filtering
T053: Empty policy matcher passes all
T054: Default zone and tier values
T055: Routing works without F13 headers
T056: Mixed configuration (some with zones, some without)
T057: select_backend(None) preserves behavior
```

### Common Test Patterns

**Creating test backends with agents**:
```rust
fn create_test_backend(id: &str, zone: PrivacyZone, tier: u8) -> (Backend, Arc<dyn InferenceAgent>) {
    let backend = Backend { /* fields */ };
    let agent = create_agent(id, name, url, BackendType::Ollama, client, metadata, zone, Some(tier)).unwrap();
    (backend, agent)
}
```

**Building a test pipeline**:
```rust
let registry = Arc::new(Registry::new());
registry.add_backend_with_agent(backend, agent).unwrap();

let privacy = PrivacyReconciler::new(Arc::clone(&registry), policy_matcher);
let scheduler = SchedulerReconciler::new(/* ... */);
let mut pipeline = ReconcilerPipeline::new(vec![Box::new(privacy), Box::new(scheduler)]);

let mut intent = RoutingIntent::new(/* ... */);
let decision = pipeline.execute(&mut intent);
```

**Asserting rejection**:
```rust
match decision.unwrap() {
    RoutingDecision::Reject { rejection_reasons } => {
        assert_eq!(rejection_reasons[0].reconciler, "PrivacyReconciler");
    }
    other => panic!("Expected Reject, got {:?}", other),
}
```

---

## Summary

F13 is a **wiring feature** — it connects existing reconcilers (from PR #157) to the live request flow. The key insight is that privacy and quality enforcement happen at the **routing layer**, not at the API layer. This means:

- Clients don't need to remember headers for privacy (it's structural)
- Quality downgrades are prevented by default (strict mode)
- Zero config still works (no policies = no filtering)
- Errors are actionable (503 tells you exactly what's needed)

Total: **829 tests** across unit, integration, and end-to-end levels.
