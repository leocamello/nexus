# Control Plane: Reconciler Pipeline — Code Walkthrough

**Feature**: Control Plane — Reconciler Pipeline (RFC-001 Phase 2)  
**Audience**: Junior developers joining the project  
**Last Updated**: 2026-02-16

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: reconciler/mod.rs — The Pipeline Engine](#file-1-reconcilermodrs--the-pipeline-engine)
4. [File 2: reconciler/intent.rs — The Shared Clipboard](#file-2-reconcilerintentrs--the-shared-clipboard)
5. [File 3: reconciler/decision.rs — Three Possible Outcomes](#file-3-reconcilerdecisionrs--three-possible-outcomes)
6. [File 4: reconciler/request_analyzer.rs — The Intake Desk](#file-4-reconcilerrequest_analyzerrs--the-intake-desk)
7. [File 5: reconciler/privacy.rs — The Security Guard](#file-5-reconcilerprivacyrs--the-security-guard)
8. [File 6: reconciler/budget.rs — The Accountant](#file-6-reconcilerbudgetrs--the-accountant)
9. [File 7: reconciler/tier.rs — The Quality Inspector](#file-7-reconcilertierrs--the-quality-inspector)
10. [File 8: reconciler/quality.rs — The Placeholder](#file-8-reconcilerqualityrs--the-placeholder)
11. [File 9: reconciler/scheduler.rs — The Final Judge](#file-9-reconcilerschedulerrs--the-final-judge)
12. [File 10: reconciler/scheduling.rs — The Agent Dossier](#file-10-reconcilerschedulingrs--the-agent-dossier)
13. [File 11: config/routing.rs — Traffic Rules](#file-11-configroutingrs--traffic-rules)
14. [File 12: routing/mod.rs — Wiring It All Together](#file-12-routingmodrs--wiring-it-all-together)
15. [Understanding the Tests](#understanding-the-tests)
16. [Key Rust Concepts](#key-rust-concepts)
17. [Common Patterns in This Codebase](#common-patterns-in-this-codebase)
18. [Next Steps](#next-steps)

---

## The Big Picture

Imagine a **factory assembly line** for routing decisions. Before this feature, routing was a single person (the `Router::select_backend()` function) who had to check everything by themselves — model names, capabilities, health status, scoring — all in one giant function. That's hard to understand, hard to test, and impossible to extend.

The **Reconciler Pipeline** replaces that one person with a **team of specialists on an assembly line**. Each specialist has one job: privacy compliance, budget enforcement, quality tiers, or scheduling. They all annotate the same clipboard (the `RoutingIntent`), and they only **add constraints** — they never erase what someone before them wrote.

### What Problem Does This Solve?

Without this pipeline, adding a new routing concern (like "don't route sensitive data to cloud") would mean editing the monolithic `select_backend()` function, risking breaking existing logic. With the pipeline, you just add a new reconciler to the line — the others don't even need to know it exists. That's **O(1) feature interaction** instead of **O(n²)**.

### How the Pipeline Fits Into Nexus

```
┌─────────────────────────────────────────────────────────────────────────┐
│                               Nexus                                     │
│                                                                         │
│  Client Request                                                         │
│    │                                                                    │
│    ▼                                                                    │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │  Router::select_backend()                                        │   │
│  │                                                                  │   │
│  │  ┌─────────────┐   ┌─────────────────────────────────────────┐   │   │
│  │  │ RoutingIntent│──▶│        RECONCILER PIPELINE              │   │   │
│  │  │ (clipboard)  │   │                                         │   │   │
│  │  │              │   │  ① RequestAnalyzer                      │   │   │
│  │  │ • model      │   │     Resolve aliases, find candidates    │   │   │
│  │  │ • candidates │   │                                         │   │   │
│  │  │ • excluded   │   │  ② PrivacyReconciler                   │   │   │
│  │  │ • reasons    │   │     Remove cloud agents if restricted   │   │   │
│  │  │ • budget     │   │                                         │   │   │
│  │  │ • cost       │   │  ③ BudgetReconciler                    │   │   │
│  │  │              │   │     Estimate cost, enforce limits       │   │   │
│  │  │              │   │                                         │   │   │
│  │  │              │   │  ④ TierReconciler                      │   │   │
│  │  │              │   │     Enforce quality minimums            │   │   │
│  │  │              │   │                                         │   │   │
│  │  │              │   │  ⑤ QualityReconciler (stub)            │   │   │
│  │  │              │   │     Reserved for future quality metrics │   │   │
│  │  │              │   │                                         │   │   │
│  │  │              │   │  ⑥ SchedulerReconciler                 │   │   │
│  │  │              │   │     Score, select best, Route/Reject    │   │   │
│  │  └─────────────┘   └─────────────────────────────────────────┘   │   │
│  │                                │                                  │   │
│  │                                ▼                                  │   │
│  │                     ┌────────────────────┐                        │   │
│  │                     │  RoutingDecision    │                        │   │
│  │                     │                    │                        │   │
│  │                     │  Route { agent }   │──▶ Forward to backend  │   │
│  │                     │  Queue { wait }    │──▶ (future: F18)       │   │
│  │                     │  Reject { reasons }│──▶ 503 with context   │   │
│  │                     └────────────────────┘                        │   │
│  └──────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

### The Golden Rule

**Reconcilers only add constraints, never remove them.** If the PrivacyReconciler says "agent X is excluded", the BudgetReconciler cannot bring it back. This makes the pipeline order-independent for correctness — you could shuffle the reconcilers and still get valid (though possibly different) results.

### Key Design Decisions

| Decision | Why |
|----------|-----|
| Pipeline in `src/routing/reconciler/` (not `src/control/`) | Reconcilers are part of routing logic; keeps imports clean |
| `RoutingIntent` is mutable, passed by `&mut` | Each reconciler modifies the same object; avoids cloning |
| `Reconciler` is a trait, not an enum | Easy to add new reconcilers without touching existing code |
| `QualityReconciler` is a pass-through stub | Placeholder for v0.4 quality tracking; pipeline slot reserved now |
| `unwrap()` in SchedulerReconciler is safe | Only called after `if candidates.is_empty()` guard |

---

## File Structure

```
src/routing/reconciler/
├── mod.rs               # Reconciler trait + ReconcilerPipeline (141 lines)
├── intent.rs            # RoutingIntent, BudgetStatus, CostEstimate, RejectionReason (160 lines)
├── decision.rs          # RoutingDecision: Route | Queue | Reject (43 lines)
├── request_analyzer.rs  # Alias resolution, candidate population (255 lines, 6 tests)
├── privacy.rs           # Privacy zone enforcement (363 lines, 9 tests)
├── budget.rs            # Cost estimation, budget limits, background loop (824 lines, 19 tests)
├── tier.rs              # Capability tier enforcement (579 lines, 13 tests)
├── quality.rs           # Future quality metrics placeholder (115 lines, 4 tests)
├── scheduler.rs         # Scoring, selection, final decision (497 lines, 5 tests)
└── scheduling.rs        # AgentSchedulingProfile (76 lines)

Related files:
├── src/config/routing.rs    # TrafficPolicy, BudgetConfig, PrivacyConstraint
├── src/routing/mod.rs       # build_pipeline(), select_backend() integration
└── src/cli/serve.rs         # BudgetReconciliationLoop startup
```

---

## File 1: reconciler/mod.rs — The Pipeline Engine

**Purpose**: Define the `Reconciler` trait and `ReconcilerPipeline` executor.  
**Lines**: 141  
**Tests**: 0 (tested through integration in other files)

This is the heart of the architecture. The `Reconciler` trait has just two methods:

```rust
pub trait Reconciler: Send + Sync {
    fn name(&self) -> &'static str;
    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError>;
}
```

`Send + Sync` means reconcilers can be shared across threads safely — important since Nexus handles requests concurrently with tokio.

The `ReconcilerPipeline` is a simple `Vec<Box<dyn Reconciler>>` that executes each reconciler in order:

```rust
pub fn execute(&mut self, intent: &mut RoutingIntent) -> Result<RoutingDecision, RoutingError> {
    for reconciler in &self.reconcilers {
        reconciler.reconcile(intent)?;     // ← the ? means: stop pipeline on error
    }
    // After all reconcilers run, check if any candidates remain
    if intent.candidate_agents.is_empty() {
        RoutingDecision::Reject { rejection_reasons: intent.rejection_reasons.clone() }
    } else {
        RoutingDecision::Route { agent_id: intent.candidate_agents[0].clone(), ... }
    }
}
```

**Observability**: The pipeline records per-reconciler latency histograms (`nexus_reconciler_duration_seconds`) and exclusion counters (`nexus_reconciler_exclusions_total`) using the `metrics` crate.

---

## File 2: reconciler/intent.rs — The Shared Clipboard

**Purpose**: The `RoutingIntent` struct that flows through the entire pipeline.  
**Lines**: 160  
**Tests**: 0 (data structure — tested via consumers)

Think of `RoutingIntent` as a clipboard that gets passed down the assembly line. Each worker writes their notes on it, but nobody erases what the previous worker wrote.

### Key Types

**`RoutingIntent`** — The main struct with ~14 fields:

```rust
pub struct RoutingIntent {
    pub request_id: String,              // For tracing
    pub requested_model: String,         // What the client asked for ("gpt-4")
    pub resolved_model: String,          // After alias resolution ("gpt-4-turbo")
    pub requirements: RequestRequirements, // Vision, tools, token estimate
    pub privacy_constraint: Option<PrivacyZone>,  // From TrafficPolicy
    pub budget_status: BudgetStatus,     // Normal | SoftLimit | HardLimit
    pub cost_estimate: CostEstimate,     // Input/output tokens, USD cost
    pub candidate_agents: Vec<String>,   // Agents still eligible
    pub excluded_agents: Vec<String>,    // Agents removed by reconcilers
    pub rejection_reasons: Vec<RejectionReason>, // Why each was removed
    pub route_reason: Option<String>,    // Set by SchedulerReconciler
    // ...
}
```

**`exclude_agent()`** — The most important helper method. Every reconciler calls this instead of manually manipulating the vectors:

```rust
pub fn exclude_agent(&mut self, agent_id: String, reconciler: &'static str,
                     reason: String, suggested_action: String) {
    self.candidate_agents.retain(|id| id != &agent_id);  // Remove from candidates
    self.excluded_agents.push(agent_id.clone());          // Add to excluded
    self.rejection_reasons.push(RejectionReason { ... }); // Record why
}
```

**`BudgetStatus`** — A simple enum with three states:
- `Normal` — Business as usual, spend freely
- `SoftLimit` — Getting expensive, prefer local agents
- `HardLimit` — Over budget, block cloud agents

**`RejectionReason`** — Structured explanation for API consumers:
```rust
pub struct RejectionReason {
    pub agent_id: String,          // Which agent was excluded
    pub reconciler: String,        // Who excluded it ("PrivacyReconciler")
    pub reason: String,            // Why ("cloud agent excluded by privacy constraint")
    pub suggested_action: String,  // What to do ("deploy on-premise agent")
}
```

---

## File 3: reconciler/decision.rs — Three Possible Outcomes

**Purpose**: The `RoutingDecision` enum — the pipeline's output.  
**Lines**: 43  
**Tests**: 0 (enum definition)

```rust
pub enum RoutingDecision {
    Route {
        agent_id: String,
        model: String,
        reason: String,
        cost_estimate: CostEstimate,
    },
    Queue {
        reason: String,
        estimated_wait_ms: u64,
        fallback_agent: Option<String>,
    },
    Reject {
        rejection_reasons: Vec<RejectionReason>,
    },
}
```

- **Route**: "Here's your backend, go!" — the happy path
- **Queue**: "Backend is loading, wait X ms" — used when `HealthStatus::Loading`
- **Reject**: "Nobody can handle this" — returns a 503 with actionable reasons

---

## File 4: reconciler/request_analyzer.rs — The Intake Desk

**Purpose**: First reconciler in the pipeline. Resolves model aliases and populates candidates.  
**Lines**: 255  
**Tests**: 6

The RequestAnalyzer is the "intake desk" — it processes the raw request before any policy enforcement:

1. **Resolve aliases** (max 3 levels): `"my-gpt"` → `"gpt-4"` → `"gpt-4-turbo"` → stop
2. **Populate candidates**: Find all backends that have the resolved model loaded
3. **Set requirements**: Vision, tools, token estimates carried from the request

```rust
fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
    // Step 1: Resolve aliases (max 3 levels to prevent infinite loops)
    let resolved = self.resolve_alias(&intent.requested_model);
    intent.resolved_model = resolved;

    // Step 2: Find all backends with this model
    let candidates = self.registry.get_backends_for_model(&intent.resolved_model);
    intent.candidate_agents = candidates.iter().map(|b| b.id.clone()).collect();

    Ok(())
}
```

### Key Tests

- `test_alias_resolution_max_depth` — Verifies the 3-level chain limit (A→B→C allowed, deeper rejected)
- `test_no_candidates_found` — Verifies empty candidate list when model doesn't exist

---

## File 5: reconciler/privacy.rs — The Security Guard

**Purpose**: Enforces privacy zone constraints. Removes cloud agents when policy says "restricted".  
**Lines**: 363  
**Tests**: 9

This is the PrivacyReconciler — the compliance officer of the pipeline. It reads `TrafficPolicy` rules from TOML config and enforces them:

```rust
fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
    // Step 1: Find matching TrafficPolicy for this model
    let policy = self.policy_matcher.find_policy(&intent.resolved_model);

    // Step 2: If policy says "restricted", exclude cloud agents
    if let Some(policy) = policy {
        if policy.privacy == PrivacyConstraint::Restricted {
            intent.privacy_constraint = Some(PrivacyZone::Restricted);

            for agent_id in intent.candidate_agents.clone() {
                let zone = self.get_backend_privacy_zone(&agent_id);
                if zone == PrivacyZone::Open {  // "Open" = cloud
                    intent.exclude_agent(
                        agent_id,
                        "PrivacyReconciler",
                        "cloud agent excluded by privacy constraint".to_string(),
                        "Deploy an on-premise agent or relax privacy constraint".to_string(),
                    );
                }
            }
        }
    }
    Ok(())
}
```

**Safety first**: If a backend's privacy zone is unknown, it's treated as "cloud" (the safe default). You must explicitly mark a backend as `"local"` or `"restricted"` for it to pass privacy checks.

### Key Tests

- `test_restricted_policy_excludes_cloud` — Cloud agents removed when policy is restricted
- `test_unrestricted_allows_all` — No exclusions with unrestricted policy
- `test_unknown_backend_treated_as_cloud` — Unknown backends are treated as cloud (FR-015)
- `test_no_matching_policy_allows_all` — No TrafficPolicy = no restrictions (zero-config)

---

## File 6: reconciler/budget.rs — The Accountant

**Purpose**: Estimates request cost, tracks spending, and enforces budget limits.  
**Lines**: 824 (the largest reconciler — budget is complex!)  
**Tests**: 19

The BudgetReconciler has three jobs:

### Job 1: Estimate Cost

```rust
fn estimate_cost(&self, model: &str, input_tokens: u32) -> CostEstimate {
    let output_tokens = input_tokens / 2;  // Heuristic: output ≈ half of input
    let cost = pricing::estimate_cost(model, input_tokens, output_tokens);
    CostEstimate { input_tokens, estimated_output_tokens: output_tokens, cost_usd: cost, .. }
}
```

### Job 2: Determine Budget Status

```rust
fn calculate_budget_status(&self) -> BudgetStatus {
    let spent = self.total_spending();
    let limit = self.budget_config.monthly_limit_usd;
    let soft = limit * self.budget_config.soft_limit_percent / 100.0;

    if spent >= limit { BudgetStatus::HardLimit }
    else if spent >= soft { BudgetStatus::SoftLimit }
    else { BudgetStatus::Normal }
}
```

### Job 3: Enforce

- **Normal**: Do nothing — all agents remain
- **SoftLimit**: Don't exclude anyone, but the SchedulerReconciler will prefer local agents
- **HardLimit**: Exclude cloud agents (same as PrivacyReconciler, but for cost reasons)

### Background Reconciliation Loop

The `BudgetReconciliationLoop` runs every 60 seconds in a background tokio task, aggregating actual spending from completed requests. This follows the same pattern as the `HealthChecker`:

```rust
pub async fn start(self, cancel_token: CancellationToken) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(self.interval_secs)) => {
                    self.reconcile_spending();
                }
            }
        }
    })
}
```

### Key Tests

- `test_budget_normal_no_exclusions` — Normal budget doesn't touch candidates
- `test_hard_limit_excludes_cloud` — Hard limit removes cloud agents
- `test_soft_limit_prefers_local` — Soft limit sets BudgetStatus for SchedulerReconciler
- `test_reconciliation_loop_starts_and_stops` — Background loop respects CancellationToken

---

## File 7: reconciler/tier.rs — The Quality Inspector

**Purpose**: Enforces minimum capability tiers. Prevents "silent downgrades" to weaker models.  
**Lines**: 579  
**Tests**: 13

Tiers represent model quality levels (1=basic, 4=flagship). The TierReconciler ensures you don't accidentally get routed to a budget model when you need GPT-4 quality.

Two enforcement modes controlled by request headers:

- **X-Nexus-Strict** (default): Reject agents below `min_tier`. If nobody qualifies, return 503.
- **X-Nexus-Flexible**: Try to find tier-qualifying agents first. If none exist, fall back to lower tiers with a warning.

```rust
fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
    let min_tier = self.get_min_tier(&intent.resolved_model);

    if let Some(min_tier) = min_tier {
        intent.min_capability_tier = Some(min_tier);

        for agent_id in intent.candidate_agents.clone() {
            let tier = self.get_agent_capability_tier(&agent_id);
            if tier < min_tier {
                match intent.tier_enforcement_mode {
                    TierEnforcementMode::Strict => {
                        intent.exclude_agent(agent_id, "TierReconciler",
                            format!("tier {} below minimum {}", tier, min_tier),
                            "Increase budget or use X-Nexus-Flexible header".into());
                    }
                    TierEnforcementMode::Flexible => {
                        // Don't exclude — but SchedulerReconciler will deprioritize
                    }
                }
            }
        }
    }
    Ok(())
}
```

### Key Tests

- `test_strict_mode_excludes_low_tier` — Agents below min_tier are excluded in strict mode
- `test_flexible_mode_allows_fallback` — In flexible mode, lower-tier agents remain as candidates
- `test_no_policy_no_restrictions` — Zero-config: no TrafficPolicy = no tier enforcement

---

## File 8: reconciler/quality.rs — The Placeholder

**Purpose**: Reserved slot for future quality tracking (v0.4).  
**Lines**: 115  
**Tests**: 4

This is a **pass-through reconciler** — it does nothing. Its job is to hold a spot in the pipeline for when quality metrics (error rates, TTFT, success rates) are implemented:

```rust
impl Reconciler for QualityReconciler {
    fn name(&self) -> &'static str { "QualityReconciler" }

    fn reconcile(&self, _intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        Ok(())  // Pass-through: no filtering in v0.3
    }
}
```

### Why Include It?

Pipeline stability. When v0.4 adds quality tracking, it just needs to fill in this reconciler's `reconcile()` method — no pipeline restructuring needed.

---

## File 9: reconciler/scheduler.rs — The Final Judge

**Purpose**: Scores remaining candidates, handles Load/Reject cases, selects the best agent.  
**Lines**: 497  
**Tests**: 5

The SchedulerReconciler is always the **last reconciler** in the pipeline. By the time it runs, all policy reconcilers have had their say. Its job is to pick the best agent from whoever's left.

### Scoring Formula (FR-029)

```
score = priority × (1 - load_factor) × (1 / latency_ema_ms) × quality_score
```

Where:
- `priority` — Backend config priority (lower = better)
- `load_factor` — `pending_requests / (pending_requests + 1)` (busy = lower score)
- `latency_ema_ms` — Exponential moving average of response latency
- `quality_score` — From `AgentSchedulingProfile` (default 1.0 until v0.4)

### Budget Adjustment (FR-020)

When `BudgetStatus::SoftLimit`, cloud agents get their scores cut in half:

```rust
fn apply_budget_adjustment(&self, score: f64, backend: &Backend, intent: &RoutingIntent) -> f64 {
    if intent.budget_status == BudgetStatus::SoftLimit {
        if backend.backend_type.is_cloud() {
            return score * 0.5;  // Halve cloud agent scores at soft limit
        }
    }
    score
}
```

### Three Outcomes

```rust
// 1. No candidates remain → Reject
if candidates.is_empty() {
    return Ok(RoutingDecision::Reject { rejection_reasons: intent.rejection_reasons.clone() });
}

// 2. Best candidate is loading → Queue (reserved for F18)
if best_is_loading {
    return Ok(RoutingDecision::Queue { reason: "Agent loading", estimated_wait_ms: 5000, .. });
}

// 3. Pick the highest-scoring candidate → Route
Ok(RoutingDecision::Route { agent_id: best.id.clone(), model: intent.resolved_model.clone(), .. })
```

---

## File 10: reconciler/scheduling.rs — The Agent Dossier

**Purpose**: `AgentSchedulingProfile` — a snapshot of an agent's metadata for scoring.  
**Lines**: 76  
**Tests**: 0 (simple data mapping)

```rust
pub struct AgentSchedulingProfile {
    pub agent_id: String,
    pub privacy_zone: PrivacyZone,
    pub capability_tier: u8,
    pub current_load: u32,
    pub latency_ema_ms: u32,
    pub available_models: Vec<String>,
}
```

Built from a `Backend` via `AgentSchedulingProfile::from_backend()`, this extracts the scheduling-relevant data from the registry's thread-safe atomics into a simple, copyable snapshot.

---

## File 11: config/routing.rs — Traffic Rules

**Purpose**: TOML configuration for TrafficPolicies and BudgetConfig.  
**Lines**: ~200 (additions to existing file)

### TrafficPolicy

```toml
[[routing.policies]]
model_pattern = "gpt-4*"
privacy = "restricted"
min_tier = 3
```

Loaded at startup, compiled into `globset` matchers for O(1) pattern matching. The `PolicyMatcher` finds the first matching policy for a model name.

### BudgetConfig

```toml
[routing.budget]
monthly_limit_usd = 100.0
soft_limit_percent = 75.0
hard_limit_action = "block_cloud"
```

All optional — if absent, budget tracking is disabled entirely (zero-config principle).

---

## File 12: routing/mod.rs — Wiring It All Together

**Purpose**: Integration point where the pipeline replaces the old routing logic.  
**Lines**: ~100 changed

### build_pipeline()

This function constructs the reconciler chain in the correct order:

```rust
fn build_pipeline(&self) -> ReconcilerPipeline {
    let analyzer = RequestAnalyzer::new(model_aliases, Arc::clone(&self.registry));
    let privacy = PrivacyReconciler::new(Arc::clone(&self.registry), self.policy_matcher.clone());
    let budget = BudgetReconciler::new(self.budget_config.clone(), self.budget_state.clone(), ..);
    let tier = TierReconciler::new(Arc::clone(&self.registry), self.policy_matcher.clone());
    let quality = QualityReconciler::new();
    let scheduler = SchedulerReconciler::new(Arc::clone(&self.registry), self.strategy, ..);

    ReconcilerPipeline::new(vec![
        Box::new(analyzer),   // ① Resolve aliases, find candidates
        Box::new(privacy),    // ② Exclude cloud if restricted
        Box::new(budget),     // ③ Estimate cost, enforce limits
        Box::new(tier),       // ④ Enforce quality minimums
        Box::new(quality),    // ⑤ (future: quality metrics)
        Box::new(scheduler),  // ⑥ Score and select
    ])
}
```

### select_backend()

The external signature is **unchanged** — existing code continues to work:

```rust
pub fn select_backend(&self, requirements: &RequestRequirements) -> Result<RoutingResult, RoutingError> {
    // Old: monolithic function with nested ifs
    // New: build pipeline → execute → convert decision to result
    let mut pipeline = self.build_pipeline();
    let mut intent = RoutingIntent::new(request_id, model, resolved, requirements, candidates);
    let decision = pipeline.execute(&mut intent)?;
    // Convert RoutingDecision → RoutingResult (backward compatible)
}
```

---

## Understanding the Tests

### Test Distribution

| File | Tests | What They Cover |
|------|-------|----------------|
| `request_analyzer.rs` | 6 | Alias resolution, candidate population, max depth |
| `privacy.rs` | 9 | Restricted/unrestricted policies, unknown zones, no-policy |
| `budget.rs` | 19 | Cost estimation, budget status transitions, loop lifecycle |
| `tier.rs` | 13 | Strict/flexible modes, tier filtering, header handling |
| `quality.rs` | 4 | Pass-through behavior, pipeline compatibility |
| `scheduler.rs` | 5 | Scoring formula, budget adjustment, Route/Reject |
| **Total** | **56** | |

### Test Patterns

**Mock Setup**: Most tests create a `Registry` with pre-populated backends:

```rust
#[test]
fn test_restricted_policy_excludes_cloud() {
    // 1. Create registry with a cloud backend and a local backend
    let registry = Arc::new(Registry::new());
    add_backend(&registry, "cloud-1", BackendType::OpenAI);
    add_backend(&registry, "local-1", BackendType::Ollama);

    // 2. Create a restricted policy
    let policy = TrafficPolicy { privacy: PrivacyConstraint::Restricted, .. };
    let matcher = PolicyMatcher::compile(vec![policy]).unwrap();

    // 3. Create reconciler and intent
    let reconciler = PrivacyReconciler::new(registry, matcher);
    let mut intent = RoutingIntent::new(..);

    // 4. Run and assert
    reconciler.reconcile(&mut intent).unwrap();
    assert!(!intent.candidate_agents.contains(&"cloud-1".to_string()));
    assert!(intent.candidate_agents.contains(&"local-1".to_string()));
}
```

**No Integration Tests**: The reconciler tests are all unit tests. Integration testing happens through the existing Router tests — since `select_backend()` delegates to the pipeline, all 513 existing tests validate the pipeline indirectly.

---

## Key Rust Concepts

### 1. Trait Objects (`Box<dyn Reconciler>`)

The pipeline stores reconcilers as `Vec<Box<dyn Reconciler>>`. This is **dynamic dispatch** — the compiler doesn't know at compile time which reconciler it's calling. The trade-off: a tiny virtual function call overhead (~1ns) for the ability to add/remove reconcilers at runtime.

```rust
// Box<dyn Reconciler> means "a heap-allocated object that implements Reconciler"
// The `dyn` keyword means Rust will use a vtable for method dispatch
let reconcilers: Vec<Box<dyn Reconciler>> = vec![
    Box::new(privacy),     // PrivacyReconciler
    Box::new(budget),      // BudgetReconciler — different type, same trait!
];
```

### 2. Interior Mutability (`Arc<DashMap>`)

The `BudgetReconciler` needs to track spending across concurrent requests. It uses `Arc<DashMap<String, BudgetMetrics>>` — `Arc` for shared ownership across threads, `DashMap` for lock-free concurrent reads:

```rust
// Multiple requests can read/write spending simultaneously
self.spending.entry(agent_id).or_default().total_cost += cost;
```

### 3. The `?` Operator (Error Propagation)

In the pipeline's `execute()` method:
```rust
reconciler.reconcile(intent)?;
```
The `?` means: "if `reconcile()` returns `Err`, stop the entire pipeline and return that error." This is how a catastrophic config error short-circuits the pipeline.

### 4. `Send + Sync` Bounds

```rust
pub trait Reconciler: Send + Sync { ... }
```
- `Send`: Can be transferred between threads (required by tokio)
- `Sync`: Can be referenced from multiple threads simultaneously

---

## Common Patterns in This Codebase

### 1. The Exclude-With-Reason Pattern

Every reconciler follows the same pattern for removing agents:

```rust
intent.exclude_agent(
    agent_id,
    "ReconcilerName",         // Who excluded it
    "human-readable reason",  // Why
    "suggested action",       // What the user should do
);
```

This is intentionally verbose — every exclusion must explain itself for the actionable 503 responses.

### 2. The Zero-Config Pattern

All reconcilers handle the "no config" case gracefully:

```rust
// No TrafficPolicy? Allow everything.
let policy = self.policy_matcher.find_policy(&model);
if policy.is_none() { return Ok(()); }

// No budget configured? Skip enforcement.
if self.budget_config.monthly_limit_usd.is_none() { return Ok(()); }
```

### 3. The Background Loop Pattern

`BudgetReconciliationLoop` follows the same pattern as `HealthChecker`:

```rust
// Start with CancellationToken for graceful shutdown
let handle = budget_loop.start(cancel_token.clone());

// On shutdown:
cancel_token.cancel();
handle.await?;
```

### 4. The PolicyMatcher Pattern

TrafficPolicies use `globset` for efficient pattern matching:

```rust
// Compiled once at startup (expensive)
let matcher = PolicyMatcher::compile(vec![
    TrafficPolicy { model_pattern: "gpt-4*", privacy: Restricted, .. },
    TrafficPolicy { model_pattern: "claude-*", min_tier: Some(3), .. },
]);

// Matched per-request (fast: ~1.5µs)
let policy = matcher.find_policy("gpt-4-turbo");  // Matches "gpt-4*"
```

---

## Next Steps

If you're a developer working on Nexus, here's what comes next:

1. **F13 (Privacy Zones)**: The `PrivacyReconciler` is already in place — F13 adds user-facing config and CLI commands for managing zones
2. **F14 (Budget Management)**: The `BudgetReconciler` is ready — F14 adds dashboard visualization and budget alerts
3. **v0.4 Quality Tracking**: Fill in the `QualityReconciler` with real error rate and TTFT metrics
4. **F18 (Request Queuing)**: Use the `RoutingDecision::Queue` variant when agents are `HealthStatus::Loading`

To add a new reconciler:
1. Create `src/routing/reconciler/my_reconciler.rs`
2. Implement `Reconciler` for your struct
3. Add it to `build_pipeline()` in `src/routing/mod.rs`
4. That's it — no other code needs to change
