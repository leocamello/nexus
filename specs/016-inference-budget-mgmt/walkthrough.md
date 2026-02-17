# F14: Inference Budget Management — Code Walkthrough

**Feature**: Inference Budget Management (F14)  
**Audience**: Junior developers joining the project  
**Last Updated**: 2026-02-17

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [File 1: agent/tokenizer.rs — The Token Counter](#file-1-agenttokenizerrs--the-token-counter)
4. [File 2: agent/pricing.rs — The Price List](#file-2-agentpricingrs--the-price-list)
5. [File 3: config/routing.rs — The Budget Rules](#file-3-configroutingrs--the-budget-rules)
6. [File 4: routing/reconciler/intent.rs — The Cost Receipt](#file-4-routingreconcilerintentrs--the-cost-receipt)
7. [File 5: routing/reconciler/budget.rs — The Accountant](#file-5-routingreconcilerbudgetrs--the-accountant)
8. [File 6: routing/mod.rs — The Budget-Aware Dispatcher](#file-6-routingmodrs--the-budget-aware-dispatcher)
9. [File 7: api/completions.rs — The Response Annotator](#file-7-apicompletionsrs--the-response-annotator)
10. [File 8: metrics/types.rs — The Budget Report Card](#file-8-metricstypesrs--the-budget-report-card)
11. [File 9: metrics/handler.rs — The Stats Window](#file-9-metricshandlerrs--the-stats-window)
12. [Understanding the Tests](#understanding-the-tests)
13. [Key Rust Concepts](#key-rust-concepts)
14. [Common Patterns in This Codebase](#common-patterns-in-this-codebase)
15. [Next Steps](#next-steps)

---

## The Big Picture

Imagine you run a **translation office** that uses both in-house translators
(local backends like Ollama) and premium freelancers (cloud APIs like OpenAI).
The in-house translators are free, but the freelancers charge per word. You have
a monthly freelancer budget of $100.

**Without F14**, you'd have no idea how much you've spent until the credit card
bill arrives. Nexus would happily send work to expensive freelancers even when
cheap in-house translators are available.

**With F14**, you get a budget manager who:
- **Counts words accurately** before sending work out (TokenizerRegistry)
- **Knows the price per word** for each freelancer (PricingTable)
- **Tracks cumulative spending** in real-time (BudgetMetrics)
- **Shifts work to in-house** when you hit 75% budget (SoftLimit)
- **Takes configurable action** when you hit 100% (HardLimit)
- **Reports spending** via metrics and response headers

### What Problem Does This Solve?

Cloud LLM APIs charge per token. If you have a $100/month budget and Nexus
routes 10,000 requests to GPT-4 without tracking costs, you could face a
surprise bill of $500+. F14 makes costs visible and controllable.

### How F14 Fits Into Nexus

```
┌──────────────────────────────────────────────────────────────────────────┐
│                              Nexus                                      │
│                                                                         │
│  Client Request                                                         │
│    │  POST /v1/chat/completions                                         │
│    ▼                                                                    │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │  Reconciler Pipeline (runs for every request, <1ms total)          │ │
│  │                                                                    │ │
│  │  ① RequestAnalyzer  → Extract model, count tokens (estimated)      │ │
│  │  ② PrivacyReconciler → Filter by privacy zone (F13)                │ │
│  │  ③ BudgetReconciler → Estimate cost, check budget status     ◄─F14 │ │
│  │     │                                                              │ │
│  │     ├─ Normal (0-75%):    No change, all backends available        │ │
│  │     ├─ SoftLimit (75-100%): Prefer local, cloud still available    │ │
│  │     └─ HardLimit (100%+):  Block cloud / block all / warn only    │ │
│  │  ④ SchedulerReconciler → Score candidates, pick best backend       │ │
│  │                                                                    │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│    │                                                                    │
│    ▼                                                                    │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │  Forward to Selected Backend → Get Response                        │ │
│  │                                                                    │ │
│  │  Add Headers:                                                      │ │
│  │    X-Nexus-Cost-Estimated: 0.0042    (per-request cost)            │ │
│  │    X-Nexus-Budget-Status: SoftLimit  (only when stressed)          │ │
│  │    X-Nexus-Budget-Utilization: 82.50 (only when stressed)          │ │
│  │    X-Nexus-Budget-Remaining: 17.50   (only when stressed)          │ │
│  └────────────────────────────────────────────────────────────────────┘ │
│                                                                         │
│  Monitoring:                                                            │
│    GET /metrics     → Prometheus gauges (spending, utilization, status)  │
│    GET /v1/stats    → JSON budget stats (current_spending, limit, etc.) │
│                                                                         │
└──────────────────────────────────────────────────────────────────────────┘
```

### Key Design Decisions

| Decision | Choice | Why |
|----------|--------|-----|
| Token counting approach | 3-tier: exact → approximation → heuristic | Different providers have different tokenizers; exact for OpenAI, approximate for Anthropic, conservative fallback for unknown |
| Budget state storage | In-memory `DashMap` | Zero-config, no database dependency; persistence deferred to v2 |
| Soft limit default | 75% | Gives 25% buffer before hard limit triggers |
| Hard limit actions | Warn / BlockCloud / BlockAll | Three severity levels: log-only, local-only, full-stop |
| Budget reset | First day of month, UTC | Simple and predictable; configurable day deferred to v2 |
| Cost header behavior | Only show budget headers when stressed | Reduce noise in headers; cost is always shown for cloud |

---

## File Structure

```
src/
├── agent/
│   ├── tokenizer.rs    ← NEW   511 lines  27 tests  The Token Counter
│   └── pricing.rs      ← exists 217 lines  0 tests   The Price List
├── config/
│   └── routing.rs      ← exists 546 lines  (budget config section)
├── routing/
│   ├── mod.rs           ← modified 1750 lines  get_budget_status_and_utilization()
│   └── reconciler/
│       ├── budget.rs    ← enhanced 926 lines 21 tests  The Accountant
│       └── intent.rs    ← enhanced 411 lines 16 tests  The Cost Receipt
├── api/
│   └── completions.rs   ← modified 1127 lines inject_budget_headers()
└── metrics/
    ├── types.rs         ← enhanced 198 lines  5 tests   Budget Report Card
    └── handler.rs       ← enhanced 230 lines  /v1/stats integration

tests/
├── cloud_cost_header_test.rs   ← 3 integration tests
└── reconciler_pipeline_test.rs ← 7 integration tests (2 budget-related)
```

**F14 Contribution**: 1 new file, 8 modified files, ~710 lines added, 35 new
tests (26 tokenizer + 5 intent + 4 types).

---

## File 1: agent/tokenizer.rs — The Token Counter

**Purpose**: Count how many tokens a piece of text will consume, using the most
accurate method available for each model provider.  
**Lines**: 511  |  **Tests**: 27  |  **Status**: NEW

### Why Does This Exist?

LLM APIs charge per **token**, not per word or character. Different providers
use different tokenization algorithms:
- OpenAI uses **tiktoken** (exact byte-pair encoding)
- Anthropic uses something similar (we approximate with tiktoken)
- Local models use various tokenizers (we fall back to a heuristic)

Getting accurate token counts matters for two reasons:
1. **Cost estimation**: Over-counting wastes budget headroom; under-counting
   causes surprise overruns.
2. **Audit accuracy**: The `token_count_tier` field tells downstream code how
   much to trust the count.

### The Tokenizer Trait

```rust
// Every tokenizer implementation must provide these three methods.
// Send + Sync makes them safe to share across async request handlers.
pub trait Tokenizer: Send + Sync {
    fn count_tokens(&self, text: &str) -> Result<u32, TokenizerError>;
    fn tier(&self) -> u8;    // 0=exact, 1=approximation, 2=heuristic
    fn name(&self) -> &str;  // For logging: "tiktoken_o200k_base", etc.
}
```

### Three Implementations

| Struct | Models Matched | Tier | How It Works |
|--------|---------------|------|-------------|
| `TiktokenExactTokenizer` | `gpt-4-turbo*`, `gpt-4o*`, `gpt-3.5*`, `gpt-4` | 0 (exact) | Uses `tiktoken-rs` with the exact encoding OpenAI uses |
| `TiktokenApproximationTokenizer` | `claude-*` | 1 (approximation) | Uses `cl100k_base` as a close-enough proxy for Claude |
| `HeuristicTokenizer` | Everything else | 2 (heuristic) | `(text.len() / 4) * 1.15` — conservative character-based estimate |

The 1.15x multiplier on the heuristic is intentionally conservative. It's better
to slightly overestimate costs than to underestimate and blow the budget.

### The TokenizerRegistry — Pattern Matching

```rust
pub struct TokenizerRegistry {
    matchers: Vec<(GlobMatcher, Arc<dyn Tokenizer>)>,
    fallback: Arc<dyn Tokenizer>,  // HeuristicTokenizer
}
```

When you call `registry.get_tokenizer("gpt-4-turbo-preview")`, it:
1. Walks the `matchers` list in order
2. Tests each glob pattern against the model name
3. Returns the first match, or the fallback if nothing matches

**Why glob patterns?** Model names evolve (`gpt-4-turbo`, `gpt-4-turbo-2024-04-09`,
`gpt-4-turbo-preview`). Glob matching (`gpt-4-turbo*`) catches them all without
maintaining an exact whitelist.

### The `count_tokens()` Convenience Method

```rust
pub fn count_tokens(&self, model: &str, text: &str) -> Result<u32, TokenizerError> {
    let tokenizer = self.get_tokenizer(model);

    // Measure how long tokenization takes (for Prometheus metrics)
    let start = std::time::Instant::now();
    let result = tokenizer.count_tokens(text);
    let duration = start.elapsed();

    // Record metrics: how fast and which tier
    metrics::histogram!("nexus_token_count_duration_seconds", ...)
        .record(duration.as_secs_f64());
    metrics::counter!("nexus_token_count_tier_total", ...)
        .increment(1);

    result
}
```

This wraps the raw `count_tokens` call with timing and tier metrics. Every time
a token count happens, Prometheus knows:
- How long it took (histogram by tier and model)
- Which tier was used (counter for exact vs. approximation vs. heuristic)

### Key Tests

```
agent::tokenizer::tests::
├── Tier validation
│   ├── tier_constants_are_ordered         — 0 < 1 < 2
│   └── tier_name_mappings                 — "exact", "approximation", etc.
├── Heuristic tokenizer
│   ├── heuristic_default_uses_1_15x_multiplier
│   ├── heuristic_counts_tokens_conservatively
│   ├── heuristic_minimum_one_token        — short text → at least 1
│   ├── heuristic_empty_string             — "" → 1 (safe minimum)
│   └── heuristic_longer_text              — 43 chars → 10-15 tokens
├── Exact tokenizer (tiktoken)
│   ├── tiktoken_o200k_creates_successfully
│   ├── tiktoken_cl100k_creates_successfully
│   ├── tiktoken_exact_counts_hello_world  — "Hello world" → 2-4 tokens
│   └── tiktoken_exact_empty_string        — "" → 0 tokens
├── Approximation tokenizer
│   ├── tiktoken_approximation_creates_successfully
│   └── tiktoken_approximation_counts_tokens
└── Registry pattern matching
    ├── registry_creates_successfully       — 4+ matchers loaded
    ├── registry_gpt4_turbo_uses_exact     — glob: gpt-4-turbo*
    ├── registry_gpt4o_uses_exact          — glob: gpt-4o*
    ├── registry_gpt4_base_uses_exact      — exact: gpt-4
    ├── registry_gpt35_uses_exact          — glob: gpt-3.5*
    ├── registry_claude_uses_approximation — glob: claude-*
    ├── registry_claude_sonnet_uses_approximation
    ├── registry_unknown_model_uses_heuristic — "llama-3-70b"
    ├── registry_local_model_uses_heuristic   — "mistral:latest"
    ├── registry_count_tokens_convenience_works
    ├── registry_count_tokens_fallback_works
    └── exact_is_more_precise_than_heuristic  — cross-tier comparison
```

---

## File 2: agent/pricing.rs — The Price List

**Purpose**: Hardcoded per-token pricing for known cloud model families.  
**Lines**: 217  |  **Tests**: 0  |  **Status**: Pre-existing (from F12)

This file maps model name prefixes to prices:

```rust
// Simplified view of the pricing table
pub struct PricingTable { entries: Vec<PricingEntry> }

pub fn estimate_cost(&self, model: &str, input_tokens: u32, output_tokens: u32)
    -> Option<f64>
```

For example, "gpt-4" costs $0.03 per 1K input tokens, $0.06 per 1K output
tokens. "claude-3-opus" costs $0.015 per 1K input. Local models return `None`
(no pricing — they're free).

**F14 doesn't modify this file** — it was built in F12. But the
`BudgetReconciler` calls `pricing.estimate_cost()` to calculate the dollar
amount from token counts.

---

## File 3: config/routing.rs — The Budget Rules

**Purpose**: TOML configuration for budget limits and behavior.  
**Lines**: 546 (budget section: lines 134-188)  |  **Status**: Pre-existing

### Budget Configuration

```toml
# nexus.example.toml
[budget]
monthly_limit = 100.00         # USD per month (omit for no limit)
soft_limit_percent = 75        # When to start preferring local backends
hard_limit_action = "warn"     # "warn" | "block_cloud" | "block_all"
reconciliation_interval_secs = 60  # How often to reconcile spending
```

### The HardLimitAction Enum

```rust
pub enum HardLimitAction {
    Warn,       // Log warning, route normally (default)
    BlockCloud, // Exclude cloud backends, keep local ones
    BlockAll,   // Block all backends, return 503
}
```

This is configured once in TOML and used by the BudgetReconciler to decide what
happens at 100% budget. The default is `Warn` — even at hard limit, requests
still go through. This follows the constitution principle: "never hard-cut
production."

### Zero-Config Default

```rust
impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            monthly_limit_usd: None,  // No limit → budget enforcement disabled
            soft_limit_percent: 75.0,
            hard_limit_action: HardLimitAction::Warn,
            reconciliation_interval_secs: 60,
        }
    }
}
```

If you don't add a `[budget]` section, budget enforcement is completely disabled.
This is the zero-config principle in action.

---

## File 4: routing/reconciler/intent.rs — The Cost Receipt

**Purpose**: Data structures that carry budget information through the pipeline.  
**Lines**: 411  |  **Tests**: 16 (5 new)  |  **Status**: Enhanced

### CostEstimate — The Per-Request Price Tag

```rust
pub struct CostEstimate {
    pub input_tokens: u32,           // How many input tokens
    pub estimated_output_tokens: u32, // Estimated output tokens (input/2)
    pub cost_usd: f64,              // Total estimated cost in USD
    pub token_count_tier: u8,       // 0=exact, 1=approx, 2=heuristic
}
```

Every request gets a `CostEstimate` attached by the BudgetReconciler. The
`token_count_tier` field tells you how much to trust the cost number:
- Tier 0 (exact): "This cost is based on the same tokenizer OpenAI uses"
- Tier 1 (approximation): "This is close but might be off by ~5%"
- Tier 2 (heuristic): "This is a rough estimate, could be off by ~15%"

### Tier Constants and tier_name()

```rust
impl CostEstimate {
    pub const TIER_EXACT: u8 = 0;
    pub const TIER_APPROXIMATION: u8 = 1;
    pub const TIER_HEURISTIC: u8 = 2;

    pub fn tier_name(&self) -> &'static str {
        match self.token_count_tier {
            Self::TIER_EXACT => "exact",
            Self::TIER_APPROXIMATION => "approximation",
            Self::TIER_HEURISTIC => "heuristic",
            _ => "unknown",
        }
    }
}
```

These constants are duplicated from `tokenizer.rs` for convenience — they match
exactly. The `tier_name()` method converts the numeric tier to a human-readable
string for logging and metrics.

### BudgetStatus — Traffic Light for Spending

```rust
pub enum BudgetStatus {
    Normal,    // 0-75%:    Green light, all backends available
    SoftLimit, // 75-100%:  Yellow, prefer local over cloud
    HardLimit, // 100%+:    Red, take configured action
}
```

This enum is set on the `RoutingIntent` by the BudgetReconciler and read by the
SchedulerReconciler to adjust backend scores.

---

## File 5: routing/reconciler/budget.rs — The Accountant

**Purpose**: The core budget enforcement logic. Estimates costs, tracks
spending, and enforces limits.  
**Lines**: 926  |  **Tests**: 21  |  **Status**: Enhanced (was 823 lines)

This is the most important file in F14. It's a **Reconciler** in the pipeline
pattern — it runs for every request and annotates the `RoutingIntent` with
budget information.

### Pipeline Position

```
RequestAnalyzer → PrivacyReconciler → BudgetReconciler → SchedulerReconciler
                                       ▲ You are here
```

### BudgetReconciler Fields

```rust
pub struct BudgetReconciler {
    registry: Arc<Registry>,              // Access to backend info
    config: BudgetConfig,                 // Monthly limit, soft %, hard action
    pricing: PricingTable,                // Per-token prices
    tokenizer_registry: Arc<TokenizerRegistry>, // NEW in F14: accurate counting
    budget_state: Arc<DashMap<String, BudgetMetrics>>, // Shared spending tracker
}
```

### The reconcile() Method

This is the entry point called by the pipeline for every request:

1. **Estimate cost**: How much will this request cost?
   ```rust
   let cost_estimate = self.estimate_cost(&model, requirements.estimated_tokens);
   intent.cost_estimate = cost_estimate;
   ```

2. **Calculate status**: Are we over budget?
   ```rust
   let status = self.calculate_budget_status();
   intent.budget_status = status;
   ```

3. **Enforce limits**: At HardLimit, take configured action
   ```rust
   match (status, &self.config.hard_limit_action) {
       (HardLimit, BlockCloud) => { /* exclude cloud agents */ }
       (HardLimit, BlockAll)   => { /* exclude all agents */ }
       (HardLimit, Warn)       => { /* log warning, continue */ }
       _ => { /* Normal or SoftLimit: no exclusions */ }
   }
   ```

### estimate_cost() — How Much Will This Cost?

```rust
fn estimate_cost(&self, model: &str, input_tokens: u32) -> CostEstimate {
    let estimated_output_tokens = input_tokens / 2;  // Heuristic: outputs ≈ half inputs

    let cost_usd = self.pricing
        .estimate_cost(model, input_tokens, estimated_output_tokens)
        .unwrap_or(0.0);  // Local models → $0.00

    // F14: Use tokenizer tier for accuracy tracking
    let tokenizer = self.tokenizer_registry.get_tokenizer(model);
    let token_count_tier = tokenizer.tier();

    CostEstimate { input_tokens, estimated_output_tokens, cost_usd, token_count_tier }
}
```

### record_spending() — Track the Running Total

After a request is routed, spending is recorded:

```rust
pub fn record_spending(&self, cost_usd: f64) {
    // Check for month rollover (e.g., Jan → Feb)
    if metrics.month_key != current_month {
        tracing::info!("Budget reset: new billing cycle started");
        metrics::counter!("nexus_budget_events_total", ...)
            .increment(1);
        metrics.current_month_spending = 0.0;
    }
    metrics.current_month_spending += cost_usd;
}
```

The month rollover check is simple: compare `"2026-02"` strings. When they
differ, reset spending to zero and log it. This happens on the first request
of a new month.

### BudgetReconciliationLoop — The Background Watcher

```rust
pub struct BudgetReconciliationLoop {
    budget_state: Arc<DashMap<String, BudgetMetrics>>,
    budget_config: BudgetConfig,
    interval_secs: u64,
}
```

This runs as a background task (started in `cli/serve.rs`) that periodically
pushes budget metrics to Prometheus:

```rust
pub fn start(self, cancel: CancellationToken) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(interval) => {
                    self.reconcile();  // Push metrics to Prometheus
                }
                _ = cancel.cancelled() => break,
            }
        }
    })
}
```

### Key Tests

```
reconciler::budget::tests::
├── Cost estimation
│   ├── estimate_cost_cloud_model       — gpt-4 → positive cost
│   ├── estimate_cost_local_model       — llama3 → $0.00
│   └── token_count_tiers               — tier matches tokenizer
├── Budget status transitions
│   ├── budget_status_normal            — below soft limit
│   ├── budget_status_soft_limit        — between 75% and 100%
│   ├── budget_status_hard_limit        — at or above 100%
│   └── no_limit_configured             — None → always Normal
├── Hard limit enforcement
│   ├── block_cloud_excludes_cloud_agents
│   ├── block_all_excludes_all_agents
│   ├── warn_does_not_exclude
│   └── soft_limit_does_not_exclude_agents
├── Spending tracking
│   ├── record_spending_accumulates
│   └── record_spending_ignores_zero
├── Month rollover
│   └── month_rollover_resets_spending
└── Full reconcile()
    ├── reconcile_sets_budget_status
    └── rejection_reason_includes_required_fields
```

---

## File 6: routing/mod.rs — The Budget-Aware Dispatcher

**Purpose**: The Router that selects backends now also tracks budget state.  
**Lines**: 1750  |  **Status**: Modified (+97 lines)

### RoutingResult — Now With Budget Info

```rust
pub struct RoutingResult {
    pub backend: Arc<Backend>,
    pub actual_model: String,
    pub fallback_used: bool,
    pub route_reason: String,
    pub cost_estimated: Option<f64>,       // Pre-existing (F12)
    pub budget_status: BudgetStatus,       // NEW (F14)
    pub budget_utilization: Option<f64>,   // NEW (F14): percentage
    pub budget_remaining: Option<f64>,     // NEW (F14): USD
}
```

Every routing decision now carries budget context. The `completions.rs` handler
reads these to inject response headers.

### get_budget_status_and_utilization()

```rust
fn get_budget_status_and_utilization(&self)
    -> (BudgetStatus, Option<f64>, Option<f64>)
{
    let monthly_limit = match self.budget_config.monthly_limit_usd {
        Some(limit) if limit > 0.0 => limit,
        _ => return (BudgetStatus::Normal, None, None),
    };

    let current_spending = self.budget_state
        .get(GLOBAL_BUDGET_KEY)
        .map(|m| m.current_month_spending)
        .unwrap_or(0.0);

    let utilization_percent = (current_spending / monthly_limit) * 100.0;
    let remaining = (monthly_limit - current_spending).max(0.0);

    // Same threshold logic as BudgetReconciler
    let status = if utilization_percent >= 100.0 {
        BudgetStatus::HardLimit
    } else if utilization_percent >= soft_threshold {
        BudgetStatus::SoftLimit
    } else {
        BudgetStatus::Normal
    };

    (status, Some(utilization_percent), Some(remaining))
}
```

This method is called after the pipeline finishes, just before building the
`RoutingResult`. It reads the shared `budget_state` DashMap to get the current
spending snapshot.

---

## File 7: api/completions.rs — The Response Annotator

**Purpose**: Injects `X-Nexus-Budget-*` response headers on proxied responses.  
**Lines**: 1127  |  **Status**: Modified (+73 lines)

### inject_budget_headers()

```rust
fn inject_budget_headers<B>(
    response: &mut Response<B>,
    routing_result: &RoutingResult,
) {
    let headers = response.headers_mut();

    // Cost header: only from budget if not already set by F12 headers
    // and cost is meaningful (> 0)
    if !headers.contains_key("x-nexus-cost-estimated") {
        if let Some(cost) = routing_result.cost_estimated {
            if cost > 0.0 {
                headers.insert("x-nexus-cost-estimated",
                    HeaderValue::from_str(&format!("{:.4}", cost)));
            }
        }
    }

    // Budget headers: only when not Normal status
    if routing_result.budget_status != BudgetStatus::Normal {
        headers.insert("x-nexus-budget-status", ...);
        headers.insert("x-nexus-budget-utilization", ...);
        headers.insert("x-nexus-budget-remaining", ...);
    }
}
```

### Why the `contains_key` Check?

The F12 implementation already adds `X-Nexus-Cost-Estimated` via
`NexusTransparentHeaders` for cloud backends (using post-request usage data).
F14 adds a pre-request estimate from the routing pipeline. The `contains_key`
check prevents F14's estimate from overwriting F12's more accurate post-request
value.

### Why Only Show Budget Headers When Stressed?

When budget status is Normal, adding 3 extra headers to every response is noise.
By only showing them at SoftLimit and HardLimit, the headers become a meaningful
signal: "something is happening with your budget."

---

## File 8: metrics/types.rs — The Budget Report Card

**Purpose**: BudgetStats struct for the `/v1/stats` JSON endpoint.  
**Lines**: 198  |  **Tests**: 5 (4 new)  |  **Status**: Enhanced

### BudgetStats Struct

```rust
pub struct BudgetStats {
    pub current_spending_usd: f64,           // $42.50
    pub monthly_limit_usd: Option<f64>,      // Some(100.0) or None
    pub utilization_percent: f64,            // 42.5
    pub status: String,                      // "Normal" | "SoftLimit" | "HardLimit"
    pub billing_month: String,              // "2026-02"
    pub last_reconciliation: String,        // ISO 8601 timestamp
    pub soft_limit_threshold: f64,          // 75.0
    pub hard_limit_action: String,          // "warn" | "block_cloud" | "block_all"
    pub next_reset_date: Option<String>,    // "2026-03-01"
}
```

This is added as an optional field on `StatsResponse`:

```rust
pub struct StatsResponse {
    pub uptime_seconds: u64,
    pub requests: RequestStats,
    pub backends: Vec<BackendStats>,
    pub models: Vec<ModelStats>,
    pub budget: Option<BudgetStats>,  // NEW: Only present when budget configured
}
```

When `budget` is `None`, it's omitted from JSON entirely
(`#[serde(skip_serializing_if = "Option::is_none")]`). This is zero-config
friendly — clients that don't use budgets see no difference.

---

## File 9: metrics/handler.rs — The Stats Window

**Purpose**: The `/v1/stats` HTTP handler now includes budget data.  
**Lines**: 230  |  **Status**: Modified

The handler reads from the shared `budget_state` DashMap and constructs
a `BudgetStats` if a monthly limit is configured:

```rust
// In the stats handler:
let budget = if config.routing.budget.monthly_limit_usd.is_some() {
    let spending = budget_state.get(GLOBAL_BUDGET_KEY)
        .map(|m| m.current_month_spending)
        .unwrap_or(0.0);

    Some(BudgetStats {
        current_spending_usd: spending,
        monthly_limit_usd: config.routing.budget.monthly_limit_usd,
        utilization_percent: ...,
        status: ...,
        // ...
    })
} else {
    None
};
```

---

## Understanding the Tests

### Test Distribution

| Module | Unit Tests | Integration Tests | Total |
|--------|-----------|------------------|-------|
| agent/tokenizer.rs | 27 | — | 27 |
| routing/reconciler/budget.rs | 21 | 2 (pipeline) | 23 |
| routing/reconciler/intent.rs | 16 (5 new) | — | 16 |
| metrics/types.rs | 5 (4 new) | — | 5 |
| api/completions.rs | — | 3 (cost headers) | 3 |
| **Total** | **69** | **5** | **74** |

### Test Patterns to Learn

**Pattern 1: Testing Glob Pattern Matching**

```rust
#[test]
fn registry_gpt4_turbo_uses_exact() {
    let r = TokenizerRegistry::new().unwrap();
    let t = r.get_tokenizer("gpt-4-turbo-preview");
    assert_eq!(t.tier(), TIER_EXACT);
}
```

This tests that the glob pattern `gpt-4-turbo*` correctly matches
`gpt-4-turbo-preview` and dispatches to the exact tokenizer.

**Pattern 2: Testing Budget State Transitions**

```rust
#[test]
fn budget_status_soft_limit() {
    // Setup: $100 limit, $80 spent → 80% > 75% threshold
    let reconciler = create_reconciler_with_spending(100.0, 80.0);
    let status = reconciler.calculate_budget_status();
    assert_eq!(status, BudgetStatus::SoftLimit);
}
```

Budget tests create a reconciler with known state and verify the transition
logic produces the expected status.

**Pattern 3: Testing Response Headers (Integration)**

```rust
#[tokio::test]
async fn test_no_cost_header_on_local_backend() {
    // Setup mock server with Ollama backend type
    let app = create_app_with_type(&mock_server, "local", BackendType::Ollama).await;

    // Send request
    let response = app.call(request).await.unwrap();

    // Local backends should NOT have cost estimation
    assert!(response.headers().get("x-nexus-cost-estimated").is_none());
}
```

Integration tests use `wiremock` to create mock backends and test the full
request flow including header injection.

---

## Key Rust Concepts

If you're new to Rust, here are concepts used heavily in F14:

### 1. Trait Objects (`Arc<dyn Tokenizer>`)

The `TokenizerRegistry` stores different tokenizer implementations behind
`Arc<dyn Tokenizer>`. This is Rust's way of doing polymorphism:

```rust
// All three implement the Tokenizer trait
let exact: Arc<dyn Tokenizer> = Arc::new(TiktokenExactTokenizer::o200k_base()?);
let approx: Arc<dyn Tokenizer> = Arc::new(TiktokenApproximationTokenizer::new()?);
let heuristic: Arc<dyn Tokenizer> = Arc::new(HeuristicTokenizer::new());
```

`dyn Tokenizer` means "I don't know the concrete type at compile time, but I
know it implements `Tokenizer`." `Arc` adds thread-safe reference counting.

### 2. DashMap (Concurrent HashMap)

`DashMap<String, BudgetMetrics>` is a lock-free concurrent hashmap. Multiple
request handlers can read and write budget state without blocking each other:

```rust
// Read: doesn't block other readers
let spending = budget_state.get("global").map(|m| m.current_month_spending);

// Write: briefly locks only the bucket for "global"
budget_state.entry("global").and_modify(|m| {
    m.current_month_spending += cost_usd;
});
```

### 3. Glob Pattern Matching

`globset` provides Unix-style glob matching: `*` matches anything, `?` matches
one character. We use it to match model names against patterns like `gpt-4-turbo*`:

```rust
let glob = Glob::new("gpt-4-turbo*")?;
let matcher = glob.compile_matcher();  // Pre-compiled for speed
matcher.is_match("gpt-4-turbo-2024-04-09"); // true
```

### 4. The `metrics` Crate Pattern

Nexus uses the `metrics` crate for Prometheus integration. The pattern is:

```rust
// Record a value in a histogram (timing, cost)
metrics::histogram!("nexus_cost_per_request_usd", "model" => model)
    .record(cost_usd);

// Increment a counter
metrics::counter!("nexus_token_count_tier_total", "tier" => "exact")
    .increment(1);

// Set a gauge (current value)
metrics::gauge!("nexus_budget_spending_usd")
    .set(current_spending);
```

### 5. Option Chaining with `and_then` and `map`

F14 uses Rust's Option methods extensively for null-safe operations:

```rust
// Instead of: if let Some(u) = response.usage { ... }
let cost = response.usage.as_ref().and_then(|u| {
    pricing.estimate_cost(&model, u.prompt_tokens, u.completion_tokens)
});
```

---

## Common Patterns in This Codebase

### Pattern 1: Reconciler Pipeline

Every reconciler follows the same contract:

```rust
impl Reconciler for BudgetReconciler {
    fn name(&self) -> &str { "Budget" }

    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        // 1. Read from intent
        // 2. Do your work
        // 3. Write results back to intent
        // 4. Optionally exclude agents
    }
}
```

Reconcilers don't know about each other. They all write to the shared
`RoutingIntent` struct, which flows through the pipeline.

### Pattern 2: Zero-Config Defaults

Budget enforcement checks for `None` limit before doing anything:

```rust
let monthly_limit = match self.config.monthly_limit_usd {
    Some(limit) => limit,
    None => return BudgetStatus::Normal,  // Early return: no limit = no enforcement
};
```

### Pattern 3: Graceful Degradation

The hard limit action `Warn` is the default. Even at 100%+ budget, requests
still go through:

```rust
match self.config.hard_limit_action {
    HardLimitAction::Warn => {
        tracing::warn!("Budget hard limit reached, but warn mode allows traffic");
    }
    HardLimitAction::BlockCloud => { /* Only block cloud */ }
    HardLimitAction::BlockAll => { /* Block everything */ }
}
```

### Pattern 4: Headers as Side Channel

F14 follows the Nexus-Transparent Protocol (F12): metadata goes in
`X-Nexus-*` headers, never in the JSON body. This means clients that don't
know about Nexus see a standard OpenAI response; clients that do can read
the headers for extra context.

---

## Next Steps

After understanding F14, here's what to explore next:

1. **F13: Privacy Zones** — The privacy reconciler that runs before budget
   (see `specs/015-privacy-zones-capability-tiers/walkthrough.md`)
2. **F12: Cloud Backend Support** — The NII agent trait and pricing table
   (see `specs/013-cloud-backend-support/walkthrough.md`)
3. **Control Plane** — The reconciler pipeline architecture
   (see `specs/014-control-plane-reconciler/walkthrough.md`)
4. **Try it yourself**: Add a `[budget]` section to `nexus.example.toml`
   and watch the `/v1/stats` endpoint show budget data

### Questions to Investigate

- What happens if two requests finish at the exact same millisecond and both
  try to record spending? (Hint: DashMap handles this safely)
- Why is the output token estimate `input_tokens / 2`? Is this a good heuristic?
- How would you add a new tokenizer for Google's Gemini models?
  (Hint: add a glob pattern and implementation in `TokenizerRegistry::new()`)
