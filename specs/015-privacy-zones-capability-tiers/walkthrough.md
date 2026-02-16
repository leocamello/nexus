# F13: Privacy Zones & Capability Tiers — Code Walkthrough

**Feature**: Privacy Zones & Capability Tiers (F13)  
**Audience**: Junior developers joining the project  
**Last Updated**: 2026-02-17

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: config/backend.rs — The Birth Certificate](#file-1-configbackendrs--the-birth-certificate)
4. [File 2: agent/factory.rs — The Agent Builder](#file-2-agentfactoryrs--the-agent-builder)
5. [File 3: agent/*.rs — The Identity Carriers](#file-3-agentrs--the-identity-carriers)
6. [File 4: api/completions.rs — The Front Door](#file-4-apicompletionsrs--the-front-door)
7. [File 5: routing/mod.rs — The Dispatcher](#file-5-routingmodrs--the-dispatcher)
8. [File 6: routing/reconciler/privacy.rs — The Security Guard](#file-6-routingreconcilerprivacyrs--the-security-guard)
9. [File 7: routing/reconciler/tier.rs — The Quality Inspector](#file-7-routingreconcilertierrs--the-quality-inspector)
10. [File 8: cli/serve.rs — The Startup Plumber](#file-8-cliservers--the-startup-plumber)
11. [File 9: discovery/mod.rs — The Auto-Discovery Update](#file-9-discoverymodrs--the-auto-discovery-update)
12. [Understanding the Tests](#understanding-the-tests)
13. [Key Rust Concepts](#key-rust-concepts)
14. [Common Patterns in This Codebase](#common-patterns-in-this-codebase)
15. [Next Steps](#next-steps)

---

## The Big Picture

Imagine you run a **law firm** with two kinds of filing cabinets. Some are in a locked, on-premises vault (restricted) and some are in a cloud storage service (open). Certain client documents **must never leave the vault** — it doesn't matter if the cloud storage is faster or has more space. If the vault is full, you don't secretly move documents to the cloud; you tell the client to wait.

That's what **Privacy Zones** do for LLM backends. Local backends (Ollama, LM Studio) are the vault. Cloud backends (OpenAI, Anthropic) are the cloud storage. Some models/conversations are too sensitive for the cloud — and this is enforced structurally, not by hoping someone remembers to set a header.

Now imagine those filing cabinets also have **quality ratings** (1-5 stars). When a 4-star cabinet goes offline, you don't silently downgrade to a 2-star cabinet. You either find another 4+ star cabinet or tell the client "we can't meet your quality requirement right now." That's **Capability Tiers**.

### What Problem Does This Solve?

Without F13, Nexus would happily route a "keep this local" request to OpenAI's cloud API if the local Ollama backend was down. And it would silently downgrade from GPT-4 to GPT-3.5 during failover, surprising developers who expected specific quality levels.

F13 makes privacy and quality **structural guarantees**, not opt-in behaviors.

### How F13 Fits Into Nexus

```
┌──────────────────────────────────────────────────────────────────────────┐
│                              Nexus                                      │
│                                                                         │
│  Client Request                                                         │
│    │  POST /v1/chat/completions                                         │
│    │  Headers: X-Nexus-Strict: true (optional)                          │
│    ▼                                                                    │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │  api/completions.rs :: handle()                                  │    │
│  │                                                                  │    │
│  │  ① extract_tier_enforcement_mode(headers)                        │    │
│  │  │  • X-Nexus-Strict → Strict    (default, safer)               │    │
│  │  │  • X-Nexus-Flexible → Flexible (opt-in)                      │    │
│  │  │  • Both → Strict wins                                        │    │
│  │  │                                                               │    │
│  │  ② router.select_backend(requirements, Some(tier_mode))          │    │
│  │  │                                                               │    │
│  │  ③ Handle RoutingError::Reject → rejection_response()            │    │
│  │  │  └─ Structured 503 with privacy/tier context                  │    │
│  └──┼──────────────────────────────────────────────────────────────┘    │
│     │                                                                   │
│     ▼                                                                   │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │  routing/mod.rs :: select_backend()                              │    │
│  │                                                                  │    │
│  │  build_pipeline() → run_pipeline_for_model()                     │    │
│  │                                                                  │    │
│  │  ┌───────────────────────────────────────────────────────────┐   │    │
│  │  │  RECONCILER PIPELINE                                      │   │    │
│  │  │                                                           │   │    │
│  │  │  ① RequestAnalyzer  → populate candidates                 │   │    │
│  │  │  ② PrivacyReconciler → exclude by zone          ◄── F13  │   │    │
│  │  │  ③ BudgetReconciler  → exclude by cost                    │   │    │
│  │  │  ④ TierReconciler    → exclude by quality       ◄── F13  │   │    │
│  │  │  ⑤ QualityReconciler → (future: error rates)              │   │    │
│  │  │  ⑥ SchedulerReconciler → score & select                   │   │    │
│  │  │                                                           │   │    │
│  │  │  Result: Route | Queue | Reject                           │   │    │
│  │  └───────────────────────────────────────────────────────────┘   │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                                                         │
│  Data Flow: TOML config → BackendConfig.zone/tier                       │
│             → effective_privacy_zone() / effective_tier()                │
│             → create_agent(…, privacy_zone, capability_tier)            │
│             → agent.profile().privacy_zone / capability_tier            │
│             → PrivacyReconciler / TierReconciler reads profile          │
└──────────────────────────────────────────────────────────────────────────┘
```

### Key Design Decisions

| Decision | Why |
|----------|-----|
| Privacy is a backend property, not a request header | Clients shouldn't need to remember to set privacy — it's structural |
| Strict mode is the default | Constitution Principle IX: never surprise the developer with quality downgrades |
| Tier defaults to 1, zone defaults to backend type | Zero-config: existing setups work unchanged (FR-022, FR-034) |
| Flexible mode never downgrades, only upgrades | If min_tier=3 and only tier 2 available, flexible still rejects (no silent downgrade) |
| Rejection reasons are per-reconciler | Actionable 503: client knows *which* reconciler blocked and *what* to do about it |
| `Option<TierEnforcementMode>` on select_backend | `None` = backward compatible; existing callers don't need to change |

---

## File Structure

```
src/
├── config/
│   └── backend.rs             # MODIFIED: added zone/tier fields + validation (193 lines, 9 tests)
├── agent/
│   ├── factory.rs             # MODIFIED: added privacy_zone/capability_tier params (520 lines, 16 tests)
│   ├── ollama.rs              # MODIFIED: stores zone/tier, returns in profile() (677 lines, 4 tests)
│   ├── openai.rs              # MODIFIED: stores zone/tier, returns in profile() (594 lines, 6 tests)
│   ├── anthropic.rs           # MODIFIED: stores zone/tier, returns in profile() (1,111 lines, 14 tests)
│   ├── google.rs              # MODIFIED: stores zone/tier, returns in profile() (1,128 lines, 12 tests)
│   ├── generic.rs             # MODIFIED: stores zone/tier, returns in profile() (551 lines, 5 tests)
│   └── lmstudio.rs            # MODIFIED: stores zone/tier, returns in profile() (475 lines, 4 tests)
├── api/
│   └── completions.rs         # MODIFIED: tier mode extraction + rejection_response() (1,058 lines, 5 tests)
├── routing/
│   ├── mod.rs                 # MODIFIED: select_backend() signature + pipeline wiring (1,661 lines, 31 tests)
│   └── reconciler/
│       ├── privacy.rs         # EXISTING: PrivacyReconciler (362 lines, 9 tests)
│       └── tier.rs            # EXISTING: TierReconciler (578 lines, 13 tests)
├── cli/
│   └── serve.rs               # MODIFIED: passes zone/tier from config to create_agent (559 lines)
├── discovery/
│   └── mod.rs                 # MODIFIED: passes default zone to create_agent (817 lines, 8 tests)

tests/
├── privacy_enforcement_test.rs       # NEW: 4 integration tests (387 lines)
├── tier_enforcement_test.rs          # NEW: 6 integration tests (542 lines)
├── privacy_zone_config_test.rs       # NEW: 7 integration tests (236 lines)
├── actionable_rejection_test.rs      # NEW: 4 integration tests (360 lines)
└── backward_compat_test.rs           # NEW: 6 integration tests (287 lines)
```

---

## File 1: config/backend.rs — The Birth Certificate

**Purpose**: Where zone and tier start their journey — the TOML config fields that declare what a backend *is*.  
**Lines**: 193  
**Tests**: 9

This file defines `BackendConfig`, the Rust struct that corresponds to a `[[backends]]` entry in your TOML config file. F13 adds two new optional fields:

```rust
// src/config/backend.rs

use crate::agent::types::PrivacyZone;

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

    /// Privacy zone classification (F13)
    /// Defaults to backend type's default zone if not specified
    #[serde(default)]
    pub zone: Option<PrivacyZone>,            // ◄── NEW: "restricted" or "open"

    /// Capability tier for routing (F13)
    /// Range: 1-5, where 5 is highest capability
    #[serde(default)]
    pub tier: Option<u8>,                      // ◄── NEW: 1 to 5
}
```

Both fields are `Option<T>` with `#[serde(default)]` — this means if a user doesn't specify them in their config, they're `None`, and Nexus fills in sensible defaults:

```rust
impl BackendConfig {
    /// Get the effective privacy zone, using backend type default if not specified.
    /// Ollama/LlamaCpp/LMStudio → Restricted (local), OpenAI/Anthropic/Google → Open (cloud)
    pub fn effective_privacy_zone(&self) -> PrivacyZone {
        self.zone
            .unwrap_or_else(|| self.backend_type.default_privacy_zone())
        //  ^^^^^^^^^^^^^^^^ lazy: only calls default_privacy_zone() if zone is None
    }

    /// Get the effective tier, defaulting to 1 if not specified (FR-022)
    pub fn effective_tier(&self) -> u8 {
        self.tier.unwrap_or(1)
    }

    /// Validate configuration fields
    pub fn validate(&self) -> Result<(), String> {
        // Cloud backends require api_key_env
        if matches!(
            self.backend_type,
            BackendType::OpenAI | BackendType::Anthropic | BackendType::Google
        ) && self.api_key_env.is_none()
        {
            return Err(format!(
                "Backend '{}' requires 'api_key_env' for cloud backend type {:?}",
                self.name, self.backend_type
            ));
        }

        // Tier must be 1-5
        if let Some(tier) = self.tier {
            if !(1..=5).contains(&tier) {
                return Err(format!(
                    "Backend '{}' has invalid tier {}: must be 1-5",
                    self.name, tier
                ));
            }
        }

        Ok(())
    }
}
```

**Why `unwrap_or_else` instead of `unwrap_or`?** Because `self.backend_type.default_privacy_zone()` is a function call. `unwrap_or` would evaluate it eagerly (even when zone is `Some`), while `unwrap_or_else` only calls it when needed. For a cheap function like this, it's mostly a convention — but it's the idiomatic Rust pattern.

This is how a user's TOML maps to these fields:

```toml
# Zone and tier specified explicitly
[[backends]]
name = "openai-gpt4"
url = "https://api.openai.com"
type = "openai"
api_key_env = "OPENAI_API_KEY"
zone = "open"     # explicit
tier = 4          # explicit

# Zone and tier omitted — Nexus fills in defaults
[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
# zone → Restricted (default for Ollama)
# tier → 1 (default)
```

### Key Tests

```rust
#[test]
fn test_effective_privacy_zone_default() {
    // Ollama defaults to Restricted (local)
    let cfg = make_config(BackendType::Ollama);
    assert_eq!(cfg.effective_privacy_zone(), PrivacyZone::Restricted);

    // OpenAI defaults to Open (cloud)
    let cfg = make_config(BackendType::OpenAI);
    assert_eq!(cfg.effective_privacy_zone(), PrivacyZone::Open);
}

#[test]
fn test_effective_tier_default() {
    let cfg = make_config(BackendType::Ollama);
    assert_eq!(cfg.effective_tier(), 1);  // FR-022: default tier is 1
}

#[test]
fn test_effective_privacy_zone_override() {
    let mut cfg = make_config(BackendType::Ollama);
    cfg.zone = Some(PrivacyZone::Open);  // Override Ollama's default
    assert_eq!(cfg.effective_privacy_zone(), PrivacyZone::Open);
}
```

---

## File 2: agent/factory.rs — The Agent Builder

**Purpose**: Creates the right `InferenceAgent` implementation based on backend type, now with zone and tier baked in.  
**Lines**: 520  
**Tests**: 16

The factory function signature grew by two parameters:

```rust
// src/agent/factory.rs

/// Create an agent from backend configuration.
#[allow(clippy::too_many_arguments)]
pub fn create_agent(
    id: String,
    name: String,
    url: String,
    backend_type: BackendType,
    client: Arc<Client>,
    metadata: HashMap<String, String>,
    privacy_zone: PrivacyZone,         // ◄── F13: from BackendConfig.effective_privacy_zone()
    capability_tier: Option<u8>,       // ◄── F13: from BackendConfig.effective_tier()
) -> Result<Arc<dyn InferenceAgent>, AgentError> {
    match backend_type {
        BackendType::Ollama => Ok(Arc::new(OllamaAgent::new(
            id,
            name,
            url,
            client,
            privacy_zone,          // ◄── passed through
            capability_tier,       // ◄── passed through
        ))),
        BackendType::OpenAI => {
            let api_key = extract_api_key(&metadata)?;
            Ok(Arc::new(OpenAIAgent::new(
                id, name, url, client, api_key,
                privacy_zone,
                capability_tier,
            )))
        }
        // ... Anthropic, Google, LMStudio, Generic follow the same pattern
    }
}
```

Every agent type (all 6 of them) was updated to accept and store `privacy_zone` and `capability_tier`. The factory is the single point where all agents are created, so this one function ensures every agent in the system carries its identity.

### Key Tests

```rust
#[test]
fn test_create_ollama_agent_with_zone_and_tier() {
    let agent = create_agent(
        "test-1".into(), "Test".into(), "http://localhost:11434".into(),
        BackendType::Ollama, Arc::new(Client::new()), HashMap::new(),
        PrivacyZone::Restricted, Some(3),
    ).unwrap();

    let profile = agent.profile();
    assert_eq!(profile.privacy_zone, PrivacyZone::Restricted);
    assert_eq!(profile.capability_tier, Some(3));
}
```

---

## File 3: agent/*.rs — The Identity Carriers

**Purpose**: Each agent implementation stores zone and tier as private fields and returns them via `profile()`.  
**Files**: `ollama.rs` (677), `openai.rs` (594), `anthropic.rs` (1,111), `google.rs` (1,128), `generic.rs` (551), `lmstudio.rs` (475)  
**Tests**: 4 + 6 + 14 + 12 + 5 + 4 = 45 total

All six agent implementations follow the same pattern. Here's `OllamaAgent` as the representative:

```rust
// src/agent/ollama.rs

pub struct OllamaAgent {
    id: String,
    name: String,
    base_url: String,
    client: Arc<Client>,
    privacy_zone: PrivacyZone,         // ◄── F13: stored as private field
    capability_tier: Option<u8>,       // ◄── F13: stored as private field
}

impl OllamaAgent {
    pub fn new(
        id: String,
        name: String,
        base_url: String,
        client: Arc<Client>,
        privacy_zone: PrivacyZone,     // ◄── F13: received from factory
        capability_tier: Option<u8>,   // ◄── F13: received from factory
    ) -> Self {
        Self { id, name, base_url, client, privacy_zone, capability_tier }
    }
}

#[async_trait]
impl InferenceAgent for OllamaAgent {
    fn profile(&self) -> AgentProfile {
        AgentProfile {
            backend_type: "ollama".to_string(),
            version: None,
            privacy_zone: self.privacy_zone,           // ◄── F13: returned to callers
            capabilities: AgentCapabilities {
                embeddings: false,
                model_lifecycle: false,
                token_counting: false,
                resource_monitoring: false,
            },
            capability_tier: self.capability_tier,     // ◄── F13: returned to callers
        }
    }
    // ... health_check(), list_models(), chat_completion() unchanged
}
```

**Why store in each agent instead of centrally?** Because the reconcilers call `agent.profile()` to check zone and tier. Agents are the source of truth — the registry may have stale data, but the agent always knows its own identity. This follows the pattern established by the NII (Normalized Inference Interface) in Phase 1.

---

## File 4: api/completions.rs — The Front Door

**Purpose**: The HTTP handler for `/v1/chat/completions`. F13 adds tier header extraction and actionable rejection responses.  
**Lines**: 1,058  
**Tests**: 5

### Part 1: Extracting Tier Enforcement Mode

When a request arrives, the handler checks for `X-Nexus-Strict` and `X-Nexus-Flexible` headers before calling the router:

```rust
// src/api/completions.rs

/// Header name constants
const STRICT_HEADER: &str = "x-nexus-strict";
const FLEXIBLE_HEADER: &str = "x-nexus-flexible";
const REJECTION_REASONS_HEADER: &str = "x-nexus-rejection-reasons";

/// Extract tier enforcement mode from request headers.
/// 1. If `X-Nexus-Strict` is present → Strict mode (safer default)
/// 2. If `X-Nexus-Flexible` is present → Flexible mode
/// 3. If neither present → Strict mode (default, FR-009)
fn extract_tier_enforcement_mode(headers: &HeaderMap) -> TierEnforcementMode {
    // Strict takes precedence if present (FR-007)
    if headers.contains_key(STRICT_HEADER) {
        return TierEnforcementMode::Strict;
    }

    // Check flexible header (FR-008)
    if let Some(val) = headers.get(FLEXIBLE_HEADER) {
        if val.to_str().ok() == Some("true") {
            return TierEnforcementMode::Flexible;
        }
    }

    // Default to strict (FR-009) — never surprise the developer
    TierEnforcementMode::Strict
}
```

**Why strict is default**: The constitution says "explicit contracts" (Principle IX). If a developer expects GPT-4 quality and gets GPT-3.5 during failover, that's a surprise. Strict mode prevents this — you get a clear 503 instead.

The handler calls `select_backend` with the extracted mode:

```rust
// In the non-streaming handler (and similarly in the streaming handler):
let tier_mode = extract_tier_enforcement_mode(&headers);

match state.router.select_backend(&requirements, Some(tier_mode)) {
    Ok(result) => { /* route to selected backend */ }
    Err(crate::routing::RoutingError::Reject { rejection_reasons }) => {
        let backends = /* list available backends */;
        return Ok(rejection_response(rejection_reasons, backends));
    }
    // ... other errors
}
```

### Part 2: Building Actionable 503 Responses

When the pipeline rejects a request (all suitable backends excluded), `rejection_response()` builds a structured error:

```rust
// src/api/completions.rs

fn rejection_response(
    rejection_reasons: Vec<RejectionReason>,
    available_backends: Vec<String>,
) -> Response {
    use crate::api::error::{ActionableErrorContext, ServiceUnavailableError};

    let count = rejection_reasons.len();
    let reconcilers: HashSet<&str> = rejection_reasons
        .iter()
        .map(|r| r.reconciler.as_str())
        .collect();

    // Extract privacy zone from PrivacyReconciler rejection reasons
    let privacy_zone_required = rejection_reasons
        .iter()
        .find(|r| r.reconciler == "PrivacyReconciler")
        .map(|r| {
            if r.reason.contains("restricted") {
                "restricted".to_string()
            } else {
                "open".to_string()
            }
        });

    // Extract required tier from TierReconciler rejection reasons
    // Parses tier number from reason text like "agent tier 2 below minimum 3"
    let required_tier = rejection_reasons
        .iter()
        .find(|r| r.reconciler == "TierReconciler")
        .and_then(|r| {
            r.reason
                .split("minimum ")
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.parse::<u8>().ok())
        });

    let context = ActionableErrorContext {
        required_tier,
        available_backends,
        eta_seconds: None,
        privacy_zone_required,
    };

    let error = ServiceUnavailableError::new(
        format!("Request rejected: {} agents excluded", count),
        context,
    );
    let mut response = error.into_response();

    // Add human-readable rejection header
    let header_value = format!("{} agents rejected by {}", count, reconciler_list.join(", "));
    response.headers_mut().insert(
        HeaderName::from_static(REJECTION_REASONS_HEADER),
        HeaderValue::from_str(&header_value).unwrap(),
    );

    // Add machine-readable rejection details as JSON header
    if let Ok(json) = serde_json::to_string(&rejection_reasons) {
        if let Ok(val) = HeaderValue::from_str(&json) {
            response.headers_mut().insert(
                HeaderName::from_static("x-nexus-rejection-details"),
                val,
            );
        }
    }

    response
}
```

The resulting 503 response body looks like:

```json
{
  "error": {
    "message": "Request rejected: 2 agents excluded",
    "type": "service_unavailable",
    "code": 503,
    "context": {
      "required_tier": 3,
      "available_backends": ["local-ollama", "openai-gpt4"],
      "privacy_zone_required": "restricted"
    }
  }
}
```

This tells the developer **exactly** what went wrong and what to fix.

### Key Tests

```rust
#[test]
fn test_extract_tier_enforcement_mode_no_headers() {
    let headers = HeaderMap::new();
    assert_eq!(extract_tier_enforcement_mode(&headers), TierEnforcementMode::Strict);
}

#[test]
fn test_extract_tier_enforcement_mode_strict() {
    let mut headers = HeaderMap::new();
    headers.insert("x-nexus-strict", "true".parse().unwrap());
    assert_eq!(extract_tier_enforcement_mode(&headers), TierEnforcementMode::Strict);
}

#[test]
fn test_extract_tier_enforcement_mode_flexible() {
    let mut headers = HeaderMap::new();
    headers.insert("x-nexus-flexible", "true".parse().unwrap());
    assert_eq!(extract_tier_enforcement_mode(&headers), TierEnforcementMode::Flexible);
}

#[test]
fn test_extract_tier_enforcement_mode_both_strict_wins() {
    let mut headers = HeaderMap::new();
    headers.insert("x-nexus-strict", "true".parse().unwrap());
    headers.insert("x-nexus-flexible", "true".parse().unwrap());
    assert_eq!(extract_tier_enforcement_mode(&headers), TierEnforcementMode::Strict);
}
```

---

## File 5: routing/mod.rs — The Dispatcher

**Purpose**: The Router builds and runs the reconciler pipeline. F13 changes `select_backend()` to accept a tier enforcement mode.  
**Lines**: 1,661  
**Tests**: 31

### Pipeline Construction

The Router's `build_pipeline()` method creates the full reconciler chain. Privacy and Tier reconcilers share the same `PolicyMatcher`:

```rust
// src/routing/mod.rs

fn build_pipeline(&self, model_aliases: HashMap<String, String>) -> ReconcilerPipeline {
    let analyzer = RequestAnalyzer::new(model_aliases, Arc::clone(&self.registry));
    let privacy =
        PrivacyReconciler::new(Arc::clone(&self.registry), self.policy_matcher.clone());
    let budget = BudgetReconciler::new(
        Arc::clone(&self.registry),
        self.budget_config.clone(),
        Arc::clone(&self.budget_state),
    );
    let tier = TierReconciler::new(Arc::clone(&self.registry), self.policy_matcher.clone());
    let quality = QualityReconciler::new();
    let scheduler = SchedulerReconciler::new(
        Arc::clone(&self.registry),
        self.strategy,
        self.weights,
        Arc::clone(&self.round_robin_counter),
    );
    ReconcilerPipeline::new(vec![
        Box::new(analyzer),    // ① Resolve aliases, find candidates
        Box::new(privacy),     // ② Exclude by zone (F13)
        Box::new(budget),      // ③ Exclude by cost (F14)
        Box::new(tier),        // ④ Exclude by quality (F13)
        Box::new(quality),     // ⑤ Quality metrics (future)
        Box::new(scheduler),   // ⑥ Score remaining and select
    ])
}
```

**Pipeline order matters**: Privacy runs before Tier because if a request must stay local, there's no point checking capability tiers for cloud backends that are already excluded.

### The Updated `select_backend()` Signature

```rust
// src/routing/mod.rs

/// Select the best backend for the given requirements
pub fn select_backend(
    &self,
    requirements: &RequestRequirements,
    tier_enforcement_mode: Option<TierEnforcementMode>,  // ◄── F13: new parameter
) -> Result<RoutingResult, RoutingError> {
    // Step 1: Resolve alias
    let model = self.resolve_alias(&requirements.model);

    // Step 2: Check if model exists
    let all_backends = self.registry.get_backends_for_model(&model);
    let model_exists = !all_backends.is_empty();

    // Step 3: Run pipeline for primary model
    let decision = self.run_pipeline_for_model(requirements, &model, tier_enforcement_mode)?;
    // ...
}
```

The inner method `run_pipeline_for_model()` creates a `RoutingIntent`, sets the tier enforcement mode on it, then runs the pipeline:

```rust
fn run_pipeline_for_model(
    &self,
    requirements: &RequestRequirements,
    model: &str,
    tier_enforcement_mode: Option<TierEnforcementMode>,
) -> Result<RoutingDecision, RoutingError> {
    let mut intent = RoutingIntent::new(
        format!("req-{}", /* timestamp */),
        model.to_string(),
        model.to_string(),
        requirements.clone(),
        vec![],
    );

    // Set tier enforcement mode from request headers (T032)
    if let Some(mode) = tier_enforcement_mode {
        intent.tier_enforcement_mode = mode;
    }

    let mut pipeline = self.build_pipeline(HashMap::new());
    pipeline.execute(&mut intent)
}
```

**Why `Option<TierEnforcementMode>`?** Passing `None` preserves backward compatibility. All existing callers (tests, benchmarks, mDNS discovery) that don't care about tier enforcement just pass `None`, and the intent uses its default (`Strict`).

---

## File 6: routing/reconciler/privacy.rs — The Security Guard

**Purpose**: Filters candidate backends by privacy zone. If a traffic policy says "restricted only", all `Open` (cloud) backends are excluded.  
**Lines**: 362  
**Tests**: 9

This file was created in PR #157 (Control Plane), but F13 makes it live by wiring config data through agents and into the pipeline.

```rust
// src/routing/reconciler/privacy.rs

pub struct PrivacyReconciler {
    registry: Arc<Registry>,
    policy_matcher: PolicyMatcher,
}

impl PrivacyReconciler {
    pub fn new(registry: Arc<Registry>, policy_matcher: PolicyMatcher) -> Self {
        Self { registry, policy_matcher }
    }

    /// Determine the effective privacy zone for a backend.
    /// Checks agent profile first, then backend type default, then assumes Open.
    fn get_backend_privacy_zone(&self, agent_id: &str) -> PrivacyZone {
        // Try agent profile first (most authoritative — has explicit zone from config)
        if let Some(agent) = self.registry.get_agent(agent_id) {
            return agent.profile().privacy_zone;
        }

        // Fall back to backend type default
        if let Some(backend) = self.registry.get_backend(agent_id) {
            return backend.backend_type.default_privacy_zone();
        }

        // Unknown backend → treat as Open (cloud) per FR-015
        // This is the safe default: if we don't know, assume it might be cloud
        tracing::warn!(agent_id = %agent_id, "Unknown agent, treating as Open");
        PrivacyZone::Open
    }
}
```

The `reconcile()` method is the core logic:

```rust
impl Reconciler for PrivacyReconciler {
    fn name(&self) -> &'static str {
        "PrivacyReconciler"
    }

    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        // FR-034: No policies configured → pass through (zero-config default)
        if self.policy_matcher.is_empty() {
            return Ok(());
        }

        // Find matching policy for the resolved model
        let policy = match self.policy_matcher.find_policy(&intent.resolved_model) {
            Some(p) => p,
            None => return Ok(()),  // No matching policy → unrestricted
        };

        // Set the privacy constraint on the intent
        let constraint = policy.privacy;
        intent.privacy_constraint = Some(match constraint {
            PrivacyConstraint::Restricted => PrivacyZone::Restricted,
            PrivacyConstraint::Unrestricted => {
                return Ok(());  // Unrestricted → no filtering needed
            }
        });

        // Filter candidates by privacy zone
        let candidate_ids: Vec<String> = intent.candidate_agents.clone();
        //                                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
        // Clone before iterating because exclude_agent() mutates candidate_agents
        for agent_id in &candidate_ids {
            let zone = self.get_backend_privacy_zone(agent_id);

            if !constraint.allows(zone) {
                intent.exclude_agent(
                    agent_id.clone(),
                    "PrivacyReconciler",
                    format!(
                        "Agent privacy zone {:?} violates {:?} policy for model '{}'",
                        zone, constraint, intent.resolved_model
                    ),
                    "Use a local backend or change the traffic policy to unrestricted"
                        .to_string(),
                );
            }
        }

        Ok(())
    }
}
```

**Key behavior**: The reconciler only takes action if a matching `TrafficPolicy` exists AND it says `Restricted`. No policies = everything passes through. This is the zero-config principle.

### Key Tests

```rust
// In the unit tests (src/routing/reconciler/privacy.rs):

#[test]
fn test_restricted_policy_excludes_open_agents() {
    // Setup: registry with one Ollama (Restricted) and one OpenAI (Open) backend
    // Policy: model "gpt-*" requires Restricted
    // Result: OpenAI excluded, Ollama kept
}

#[test]
fn test_no_policies_passes_all() {
    // Setup: empty PolicyMatcher
    // Result: all candidates pass through unchanged (FR-034)
}

#[test]
fn test_unknown_backend_treated_as_open() {
    // Setup: agent_id not in registry
    // Result: treated as Open → excluded by Restricted policy (FR-015)
}

#[test]
fn test_rejection_reason_includes_details() {
    // Verify: agent_id, reconciler name, reason text, suggested action
    // are all populated in the RejectionReason struct
}
```

---

## File 7: routing/reconciler/tier.rs — The Quality Inspector

**Purpose**: Filters candidate backends by capability tier. Supports strict (default) and flexible enforcement modes.  
**Lines**: 578  
**Tests**: 13

```rust
// src/routing/reconciler/tier.rs

pub struct TierReconciler {
    registry: Arc<Registry>,
    policy_matcher: PolicyMatcher,
}

impl TierReconciler {
    /// Get the effective capability tier for an agent.
    /// Defaults to tier 1 (FR-025) if not specified.
    fn get_agent_capability_tier(&self, agent_id: &str) -> u8 {
        if let Some(agent) = self.registry.get_agent(agent_id) {
            return agent.profile().capability_tier.unwrap_or(1);
        }
        1  // Unknown agent defaults to tier 1
    }
}
```

The `reconcile()` method implements both strict and flexible enforcement:

```rust
impl Reconciler for TierReconciler {
    fn name(&self) -> &'static str {
        "TierReconciler"
    }

    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        // FR-034: No policies → pass through
        if self.policy_matcher.is_empty() {
            return Ok(());
        }

        let policy = match self.policy_matcher.find_policy(&intent.resolved_model) {
            Some(p) => p,
            None => return Ok(()),
        };

        let min_tier = match policy.min_tier {
            Some(t) => t,
            None => return Ok(()),  // No tier requirement in this policy
        };

        intent.min_capability_tier = Some(min_tier);

        let candidate_ids: Vec<String> = intent.candidate_agents.clone();

        match intent.tier_enforcement_mode {
            TierEnforcementMode::Strict => {
                // FR-026/FR-027: Exclude ALL agents below min_tier
                for agent_id in &candidate_ids {
                    let tier = self.get_agent_capability_tier(agent_id);
                    if tier < min_tier {
                        intent.exclude_agent(
                            agent_id.clone(),
                            "TierReconciler",
                            format!(
                                "Agent capability tier {} is below minimum tier {} \
                                 required by policy for model '{}'",
                                tier, min_tier, intent.resolved_model
                            ),
                            format!(
                                "Use a backend with capability tier >= {} or set \
                                 X-Nexus-Flexible header to allow lower-tier fallback",
                                min_tier
                            ),
                        );
                    }
                }
            }
            TierEnforcementMode::Flexible => {
                // FR-028: Only exclude if higher-tier agents remain
                let has_capable = candidate_ids
                    .iter()
                    .any(|id| self.get_agent_capability_tier(id) >= min_tier);

                if has_capable {
                    // Higher-tier agents available → filter out lower-tier ones
                    for agent_id in &candidate_ids {
                        let tier = self.get_agent_capability_tier(agent_id);
                        if tier < min_tier {
                            intent.exclude_agent(/* ... */);
                        }
                    }
                } else {
                    // No capable agents → allow all (flexible fallback)
                    // IMPORTANT: This does NOT downgrade — the SchedulerReconciler
                    // will reject if no candidates remain after all reconcilers run
                    tracing::warn!(
                        model = %intent.resolved_model,
                        min_tier = min_tier,
                        "TierReconciler: no agents meet min_tier, allowing flexible fallback"
                    );
                }
            }
        }

        Ok(())
    }
}
```

**Strict vs Flexible**: Both modes prevent downgrades. The difference:
- **Strict**: If min_tier=3 and only tier 2 is available → exclude tier 2 → no candidates → 503
- **Flexible**: If min_tier=3 and tier 4 is available → use tier 4 (upgrade OK). If only tier 2 is available → still exclude tier 2 → 503 (never downgrade)

The subtle part: in flexible mode, when `has_capable` is `false`, the reconciler *allows all through* — but that doesn't mean they'll succeed. The SchedulerReconciler downstream will still reject if no agent meets the requirements. Flexible mode is about preferring upgrades, not allowing downgrades.

### Key Tests

```rust
#[test]
fn test_strict_mode_excludes_below_tier() {
    // min_tier=3, agent has tier=2 → excluded
}

#[test]
fn test_strict_mode_keeps_at_or_above_tier() {
    // min_tier=3, agent has tier=3 → kept
    // min_tier=3, agent has tier=5 → kept
}

#[test]
fn test_flexible_mode_excludes_lower_when_higher_available() {
    // min_tier=3, agents: tier 2 + tier 4
    // → tier 2 excluded, tier 4 kept (upgrade preferred)
}

#[test]
fn test_flexible_mode_allows_all_when_none_meet_tier() {
    // min_tier=3, agents: tier 1 + tier 2
    // → all kept (no capable agents, flexible fallback)
}

#[test]
fn test_no_tier_in_policy_passes_all() {
    // Policy exists but min_tier is None → no filtering
}

#[test]
fn test_rejection_reason_includes_tier_numbers() {
    // Verify: reason text contains both agent tier and required min_tier
}
```

---

## File 8: cli/serve.rs — The Startup Plumber

**Purpose**: The `serve` command loads backends from config and creates agents. F13's zone/tier flow through here.  
**Lines**: 559

The key call site in `load_backends_from_config()`:

```rust
// src/cli/serve.rs

// Build metadata from backend config
let mut metadata = HashMap::new();
if let Some(api_key_env) = &backend_config.api_key_env {
    metadata.insert("api_key_env".to_string(), api_key_env.clone());
}

let backend = Backend::new(
    id.clone(),
    backend_config.name.clone(),
    backend_config.url.clone(),
    backend_config.backend_type,
    vec![],
    DiscoverySource::Static,
    metadata.clone(),
);

// Create agent for this backend — zone and tier flow from config here
let agent = crate::agent::factory::create_agent(
    id.clone(),
    backend_config.name.clone(),
    backend_config.url.clone(),
    backend_config.backend_type,
    Arc::clone(&client),
    metadata,
    backend_config.effective_privacy_zone(),      // ◄── F13: config → agent
    Some(backend_config.effective_tier()),         // ◄── F13: config → agent
)?;

// Register both backend and agent
registry.add_backend_with_agent(backend, agent)?;
```

This is the **complete data flow**: TOML config → `BackendConfig` → `effective_privacy_zone()`/`effective_tier()` → `create_agent()` → agent struct field → `agent.profile()` → reconciler reads it.

---

## File 9: discovery/mod.rs — The Auto-Discovery Update

**Purpose**: mDNS auto-discovery creates agents for discovered backends. F13 ensures they get default zone/tier values.  
**Lines**: 817  
**Tests**: 8

When a backend is discovered via mDNS (zero-config), it doesn't have explicit zone/tier values in a config file. The discovery code uses defaults:

```rust
// src/discovery/mod.rs — in the service event handler

let agent = crate::agent::factory::create_agent(
    id.clone(),
    name.clone(),
    url.clone(),
    backend_type,
    Arc::clone(&client),
    HashMap::new(),
    backend_type.default_privacy_zone(),  // ◄── F13: Ollama → Restricted, etc.
    None,                                 // ◄── F13: no tier → defaults to 1
)?;
```

This means auto-discovered local backends are automatically `Restricted` (safe by default), and auto-discovered backends have tier 1 (lowest). Admins can override by adding explicit config entries.

---

## Understanding the Tests

### Test Distribution

| File | Tests | What They Cover |
|------|-------|----------------|
| `src/config/backend.rs` | 9 | Zone/tier defaults, validation, effective_* methods |
| `src/agent/factory.rs` | 16 | All 6 agent types with zone/tier creation |
| `src/routing/reconciler/privacy.rs` | 9 | Restricted/unrestricted policies, zone lookup, unknown agents |
| `src/routing/reconciler/tier.rs` | 13 | Strict/flexible modes, tier filtering, rejection reasons |
| `src/api/completions.rs` | 5 | Header parsing (strict/flexible/both/neither), defaults |
| `tests/privacy_enforcement_test.rs` | 4 | End-to-end privacy pipeline with routing decisions |
| `tests/tier_enforcement_test.rs` | 6 | End-to-end tier pipeline with strict/flexible headers |
| `tests/privacy_zone_config_test.rs` | 7 | Config parsing, defaults, validation edge cases |
| `tests/actionable_rejection_test.rs` | 4 | 503 response structure, privacy/tier context |
| `tests/backward_compat_test.rs` | 6 | Zero-config, no-policy, None tier mode |
| **Unit + Integration** | **79** | F13-specific tests |
| **Total project** | **829** | All tests passing |

### Test Patterns

**Pattern 1: The Registry + Agent Test Setup**

Most integration tests follow the same setup pattern — create backends with agents, register them, build a pipeline, run it:

```rust
// From tests/privacy_enforcement_test.rs

fn create_test_backend(
    id: &str,
    name: &str,
    backend_type: BackendType,
    zone: PrivacyZone,
    tier: Option<u8>,
    status: BackendStatus,
) -> (Backend, Arc<dyn InferenceAgent>) {
    let backend = Backend {
        id: id.to_string(),
        name: name.to_string(),
        url: format!("http://localhost:{}", 11434),
        backend_type,
        status,
        models: vec![Model { id: "test-model".to_string(), /* ... */ }],
        // ...
    };

    let agent = create_agent(
        id.to_string(), name.to_string(),
        format!("http://localhost:{}", 11434),
        backend_type, Arc::new(Client::new()),
        metadata, zone, tier,
    ).unwrap();

    (backend, agent)
}
```

**Pattern 2: Pipeline Construction for Integration Tests**

```rust
// Build a minimal pipeline for testing specific reconcilers
let registry = Arc::new(Registry::new());
registry.add_backend_with_agent(backend, agent).unwrap();

let policy_matcher = PolicyMatcher::compile(vec![
    TrafficPolicy {
        model_pattern: "test-*".to_string(),
        privacy: PrivacyConstraint::Restricted,
        min_tier: None,
        // ...
    },
]).unwrap();

let mut pipeline = ReconcilerPipeline::new(vec![
    Box::new(RequestAnalyzer::new(HashMap::new(), Arc::clone(&registry))),
    Box::new(PrivacyReconciler::new(Arc::clone(&registry), policy_matcher)),
    Box::new(SchedulerReconciler::new(/* ... */)),
]);

let mut intent = RoutingIntent::new(/* ... */);
let decision = pipeline.execute(&mut intent);
```

**Pattern 3: Asserting Rejection Decisions**

```rust
match decision.unwrap() {
    RoutingDecision::Reject { rejection_reasons } => {
        assert_eq!(rejection_reasons.len(), 1);
        assert_eq!(rejection_reasons[0].reconciler, "PrivacyReconciler");
        assert!(rejection_reasons[0].reason.contains("restricted"));
        assert!(!rejection_reasons[0].suggested_action.is_empty());
    }
    other => panic!("Expected Reject, got {:?}", other),
}
```

---

## Key Rust Concepts

### 1. `Option<T>` as Zero-Config Enabler

F13 uses `Option` extensively to make fields optional. The pattern:

```rust
pub zone: Option<PrivacyZone>,  // None = "user didn't specify"

// Later, when we need a concrete value:
self.zone.unwrap_or_else(|| self.backend_type.default_privacy_zone())
//        ^^^^^^^^^^^^^^ provide a default only when needed (lazy)
```

This is different from using a raw `PrivacyZone` field with a default — `Option` lets us distinguish "user explicitly set Open" from "user didn't say, we defaulted to Open."

### 2. Clone-Before-Iterate

```rust
let candidate_ids: Vec<String> = intent.candidate_agents.clone();
for agent_id in &candidate_ids {
    if should_exclude(agent_id) {
        intent.exclude_agent(agent_id.clone(), /* ... */);
        //     ^^^^^^^^^^^^^^ this mutates intent.candidate_agents
    }
}
```

Rust's borrow checker prevents iterating over a collection while mutating it. The solution: clone the list of IDs, iterate over the clone, and mutate the original through `intent`. The clone is cheap (just cloning `String`s).

### 3. `#[allow(clippy::too_many_arguments)]`

```rust
#[allow(clippy::too_many_arguments)]
pub fn create_agent(
    id: String, name: String, url: String, backend_type: BackendType,
    client: Arc<Client>, metadata: HashMap<String, String>,
    privacy_zone: PrivacyZone, capability_tier: Option<u8>,
) -> Result<Arc<dyn InferenceAgent>, AgentError>
```

Clippy warns when a function has more than 7 arguments. We suppress it here because the factory needs all these parameters and a builder pattern would add complexity without benefit (this function is only called in two places).

### 4. `matches!` Macro for Multi-Variant Checks

```rust
if matches!(
    self.backend_type,
    BackendType::OpenAI | BackendType::Anthropic | BackendType::Google
) && self.api_key_env.is_none() {
    return Err(/* ... */);
}
```

The `matches!` macro is cleaner than a `match` statement when you just need a boolean — "does this value match any of these patterns?" It returns `true` or `false`.

### 5. `and_then` for Chaining Optional Operations

```rust
let required_tier = rejection_reasons
    .iter()
    .find(|r| r.reconciler == "TierReconciler")  // Option<&RejectionReason>
    .and_then(|r| {                               // Only runs if find() returned Some
        r.reason
            .split("minimum ")
            .nth(1)                                // Option<&str>
            .and_then(|s| s.split_whitespace().next())  // Option<&str>
            .and_then(|s| s.parse::<u8>().ok())         // Option<u8>
    });
```

Each `.and_then()` in the chain only runs if the previous step returned `Some`. If any step returns `None`, the whole chain short-circuits to `None`. This is Rust's alternative to nested `if let` statements.

---

## Common Patterns in This Codebase

### 1. The Config → effective_* → Factory → Profile Pattern

Zone and tier follow a four-step pipeline from TOML to routing decisions:

```
TOML: zone = "restricted", tier = 3
  │
  ▼
BackendConfig { zone: Some(Restricted), tier: Some(3) }
  │
  ├─ effective_privacy_zone() → Restricted  (or default if None)
  ├─ effective_tier() → 3                   (or 1 if None)
  │
  ▼
create_agent(…, Restricted, Some(3))
  │
  ▼
OllamaAgent { privacy_zone: Restricted, capability_tier: Some(3) }
  │
  ▼
agent.profile() → AgentProfile { privacy_zone: Restricted, capability_tier: Some(3) }
  │
  ▼
PrivacyReconciler reads profile → decides to keep or exclude
TierReconciler reads profile → decides to keep or exclude
```

### 2. The Zero-Config Pattern (FR-034)

Every reconciler handles "no config" gracefully:

```rust
// No policies configured? Allow everything.
if self.policy_matcher.is_empty() {
    return Ok(());
}

// No matching policy for this model? Allow everything.
let policy = match self.policy_matcher.find_policy(&model) {
    Some(p) => p,
    None => return Ok(()),
};

// Policy exists but no tier requirement? Allow everything.
let min_tier = match policy.min_tier {
    Some(t) => t,
    None => return Ok(()),
};
```

Each check peels away one layer. If at any point there's nothing to enforce, the reconciler bails early and lets all candidates pass through.

### 3. The Exclude-With-Reason Pattern

Every exclusion must explain itself for actionable 503 responses:

```rust
intent.exclude_agent(
    agent_id,                           // WHO got excluded
    "PrivacyReconciler",                // BY WHOM
    "zone Open violates Restricted",    // WHY (machine-parseable)
    "Use a local backend or change policy",  // WHAT TO DO (human-friendly)
);
```

This is deliberately verbose — no silent exclusions. Every rejection becomes evidence in the 503 response.

### 4. The Optional Second Parameter Pattern

```rust
pub fn select_backend(
    &self,
    requirements: &RequestRequirements,
    tier_enforcement_mode: Option<TierEnforcementMode>,  // None = backward compatible
) -> Result<RoutingResult, RoutingError>
```

When extending a public API, wrapping the new parameter in `Option` preserves backward compatibility. Existing callers pass `None`, new callers pass `Some(mode)`. No breaking changes, no migration needed.

---

## Next Steps

If you're a developer working on Nexus, here's what comes after F13:

1. **F14 (Budget Management)**: The `BudgetReconciler` is already in place — F14 adds user-facing config, spending tracking dashboard, and budget alerts via the reconciler pipeline
2. **v0.4 Quality Tracking**: Fill in the `QualityReconciler` with real error rate and TTFT metrics, then use tier enforcement to route to backends with better quality scores
3. **F18 (Request Queuing)**: Use the `RoutingDecision::Queue` variant when flexible mode can't find a capable agent immediately — wait for one to become available instead of rejecting
4. **Cross-Zone History Blocking**: Implement conversation history scrubbing when requests overflow from restricted to open zones (currently blocks entirely)

To add a new privacy-related policy:
1. Add the field to `TrafficPolicy` in `src/config/routing.rs`
2. Read it in the appropriate reconciler's `reconcile()` method
3. Add exclusion logic with `intent.exclude_agent()`
4. Write unit tests in the reconciler file and integration tests in `tests/`
