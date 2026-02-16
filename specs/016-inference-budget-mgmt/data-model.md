# Data Model: F14 Inference Budget Management

**Date**: 2025-01-24  
**Feature**: Inference Budget Management  
**Branch**: `016-inference-budget-mgmt`

## Overview

This document defines the data structures for budget management, tokenizer registry, and metrics. Most entities already exist in the Control Plane PR - this feature enhances them with tokenizer support and metrics integration.

---

## Entity Diagram

```
┌─────────────────┐
│ RoutingIntent   │  (existing - src/routing/reconciler/intent.rs)
├─────────────────┤
│ budget_status   │─────┐
│ cost_estimate   │─┐   │
└─────────────────┘ │   │
                    │   │
                    ▼   ▼
        ┌──────────────────────┐        ┌─────────────────┐
        │ CostEstimate         │        │ BudgetStatus    │
        ├──────────────────────┤        │ (enum)          │
        │ input_tokens: u32    │        ├─────────────────┤
        │ estimated_output: u32│        │ Normal          │
        │ cost_usd: f64        │        │ SoftLimit       │
        │ token_count_tier: u8 │        │ HardLimit       │
        └──────────────────────┘        └─────────────────┘
                    │
                    │ calculated_by
                    ▼
        ┌──────────────────────┐
        │ BudgetReconciler     │  (existing - src/routing/reconciler/budget.rs)
        ├──────────────────────┤
        │ config: BudgetConfig │
        │ pricing: PricingTable│
        │ budget_state: ...    │
        │ tokenizer_registry   │───┐  NEW: Add tokenizer registry
        └──────────────────────┘   │
                                   │
                                   ▼
                    ┌──────────────────────────┐
                    │ TokenizerRegistry        │  NEW: src/agent/tokenizer.rs
                    ├──────────────────────────┤
                    │ matchers: Vec<Matcher>   │
                    │ fallback: HeuristicToken │
                    ├──────────────────────────┤
                    │ get_tokenizer(model)     │
                    │ count_tokens(model,text) │
                    └──────────────────────────┘
                                   │
                    ┌──────────────┼──────────────┐
                    ▼              ▼              ▼
            ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
            │ TiktokenExact│ │Approx Tiktoken│ │ Heuristic  │
            │ (OpenAI)     │ │ (Anthropic)  │ │ (Unknown)   │
            ├─────────────┤ ├─────────────┤ ├─────────────┤
            │ o200k_base   │ │ cl100k_base  │ │ len/4*1.15x │
            │ cl100k_base  │ │              │ │             │
            └─────────────┘ └─────────────┘ └─────────────┘

        ┌──────────────────────┐
        │ BudgetMetrics        │  (existing - src/routing/reconciler/budget.rs)
        ├──────────────────────┤
        │ current_month_spending│
        │ last_reconciliation  │
        │ month_key: String    │
        └──────────────────────┘
                    │
                    │ stored_in
                    ▼
        ┌──────────────────────┐
        │ DashMap<String,      │
        │   BudgetMetrics>     │
        └──────────────────────┘

        ┌──────────────────────┐
        │ BudgetConfig         │  (existing - src/config/routing.rs)
        ├──────────────────────┤
        │ monthly_limit_usd    │
        │ soft_limit_percent   │
        │ hard_limit_action    │
        │ reconciliation_secs  │
        └──────────────────────┘
```

---

## E1: BudgetStatus (Existing)

**Location**: `src/routing/reconciler/intent.rs` lines 22-32

```rust
/// Current budget status affecting routing decisions (FR-019)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BudgetStatus {
    /// Spending below soft limit (75% default) - all agents available
    #[default]
    Normal,

    /// Spending between soft and hard limit - prefer local agents
    SoftLimit,

    /// Spending at or above hard limit - block cloud agents
    HardLimit,
}
```

**Relationships**:
- Set by: `BudgetReconciler::calculate_budget_status()`
- Used by: `SchedulerReconciler` to adjust agent scores
- Exposed via: `X-Nexus-Budget-Status` response header, `/v1/stats` endpoint

**State Transitions**:
```
Normal (0-80%) ──> SoftLimit (80-100%) ──> HardLimit (100%+)
       │                      │                    │
       └──────────────────────┴────────────────────┴─── Month rollover ──> Normal
```

**Validation**:
- Thresholds configured via `BudgetConfig.soft_limit_percent` (default 75.0)
- Transition logged with `tracing::info!` for audit trail

---

## E2: CostEstimate (Existing, Enhance)

**Location**: `src/routing/reconciler/intent.rs` lines 36-48

```rust
/// Cost estimate for request (FR-018)
#[derive(Debug, Clone, Default)]
pub struct CostEstimate {
    /// Input token count (from RequestRequirements or exact tokenizer)
    pub input_tokens: u32,

    /// Estimated output tokens (heuristic: input_tokens / 2)
    pub estimated_output_tokens: u32,

    /// Total estimated cost in USD
    pub cost_usd: f64,

    /// Token count tier: 0=exact, 1=approximation, 2=heuristic (FR-012)
    pub token_count_tier: u8,
}
```

**Enhancement**: Add semantic meaning to `token_count_tier`:
```rust
impl CostEstimate {
    pub const TIER_EXACT: u8 = 0;        // tiktoken exact match
    pub const TIER_APPROXIMATION: u8 = 1; // tiktoken approximation or SentencePiece
    pub const TIER_HEURISTIC: u8 = 2;     // character-based fallback

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

**Relationships**:
- Populated by: `BudgetReconciler::estimate_cost()` using `TokenizerRegistry`
- Used for: Budget tracking, Prometheus metrics, response headers
- Recorded in: `nexus_cost_per_request_usd` histogram, `nexus_token_count_tier_total` counter

**Validation**:
- `cost_usd >= 0.0` (enforced by PricingTable)
- `token_count_tier` in [0, 1, 2]
- `input_tokens > 0` for all non-empty requests

---

## E3: Tokenizer Trait (New)

**Location**: `src/agent/tokenizer.rs` (new file)

```rust
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TokenizerError {
    #[error("Tokenization failed: {0}")]
    Encoding(String),
    
    #[error("Model not supported by tokenizer: {0}")]
    ModelNotSupported(String),
}

/// Trait for token counting implementations (R1)
pub trait Tokenizer: Send + Sync {
    /// Count tokens in text.
    fn count_tokens(&self, text: &str) -> Result<u32, TokenizerError>;
    
    /// Get tier for this tokenizer (0=exact, 1=approximation, 2=heuristic)
    fn tier(&self) -> u8;
    
    /// Human-readable name for logging
    fn name(&self) -> &str;
}
```

**Implementations**:

### I1: TiktokenExact (OpenAI)
```rust
pub struct TiktokenExactTokenizer {
    encoding: tiktoken_rs::CoreBPE,
    tier: u8,
}

impl TiktokenExactTokenizer {
    pub fn o200k_base() -> Result<Self, TokenizerError> {
        Ok(Self {
            encoding: tiktoken_rs::o200k_base()?,
            tier: CostEstimate::TIER_EXACT,
        })
    }
    
    pub fn cl100k_base() -> Result<Self, TokenizerError> {
        Ok(Self {
            encoding: tiktoken_rs::cl100k_base()?,
            tier: CostEstimate::TIER_EXACT,
        })
    }
}

impl Tokenizer for TiktokenExactTokenizer {
    fn count_tokens(&self, text: &str) -> Result<u32, TokenizerError> {
        self.encoding.encode_with_special_tokens(text)
            .len()
            .try_into()
            .map_err(|e| TokenizerError::Encoding(format!("{}", e)))
    }
    
    fn tier(&self) -> u8 { self.tier }
    fn name(&self) -> &str { "tiktoken_exact" }
}
```

### I2: TiktokenApproximation (Anthropic, approx for OpenAI)
```rust
pub struct TiktokenApproximationTokenizer {
    encoding: tiktoken_rs::CoreBPE,
}

impl TiktokenApproximationTokenizer {
    pub fn new() -> Result<Self, TokenizerError> {
        Ok(Self {
            encoding: tiktoken_rs::cl100k_base()?,
        })
    }
}

impl Tokenizer for TiktokenApproximationTokenizer {
    fn count_tokens(&self, text: &str) -> Result<u32, TokenizerError> {
        self.encoding.encode_with_special_tokens(text)
            .len()
            .try_into()
            .map_err(|e| TokenizerError::Encoding(format!("{}", e)))
    }
    
    fn tier(&self) -> u8 { CostEstimate::TIER_APPROXIMATION }
    fn name(&self) -> &str { "tiktoken_approximation" }
}
```

### I3: HeuristicTokenizer (Fallback)
```rust
pub struct HeuristicTokenizer {
    multiplier: f64, // 1.15 for conservative estimate
}

impl HeuristicTokenizer {
    pub fn new() -> Self {
        Self { multiplier: 1.15 }
    }
}

impl Tokenizer for HeuristicTokenizer {
    fn count_tokens(&self, text: &str) -> Result<u32, TokenizerError> {
        // Character-based heuristic: ~4 chars per token (English average)
        let base_estimate = (text.len() / 4).max(1);
        let conservative = (base_estimate as f64 * self.multiplier) as u32;
        Ok(conservative)
    }
    
    fn tier(&self) -> u8 { CostEstimate::TIER_HEURISTIC }
    fn name(&self) -> &str { "heuristic" }
}
```

---

## E4: TokenizerRegistry (New)

**Location**: `src/agent/tokenizer.rs`

```rust
use globset::{Glob, GlobMatcher};
use std::sync::Arc;

pub struct TokenizerRegistry {
    /// Ordered list of (pattern, tokenizer) for matching models
    matchers: Vec<(GlobMatcher, Arc<dyn Tokenizer>)>,
    
    /// Fallback for unknown models
    fallback: Arc<dyn Tokenizer>,
}

impl TokenizerRegistry {
    /// Create registry with default OpenAI/Anthropic/fallback configuration
    pub fn new() -> Result<Self, TokenizerError> {
        let mut matchers = Vec::new();
        
        // OpenAI GPT-4 Turbo, GPT-4o → o200k_base (exact)
        let o200k_models = Glob::new("gpt-4-turbo*")?;
        matchers.push((
            o200k_models.compile_matcher(),
            Arc::new(TiktokenExactTokenizer::o200k_base()?) as Arc<dyn Tokenizer>
        ));
        
        // OpenAI GPT-3.5, GPT-4 base → cl100k_base (exact)
        let cl100k_models = Glob::new("gpt-{3.5,4}")?;
        matchers.push((
            cl100k_models.compile_matcher(),
            Arc::new(TiktokenExactTokenizer::cl100k_base()?)
        ));
        
        // Anthropic Claude → cl100k_base (approximation)
        let claude_models = Glob::new("claude-*")?;
        matchers.push((
            claude_models.compile_matcher(),
            Arc::new(TiktokenApproximationTokenizer::new()?)
        ));
        
        // Fallback for all other models
        let fallback = Arc::new(HeuristicTokenizer::new());
        
        Ok(Self { matchers, fallback })
    }
    
    /// Find tokenizer for a model name
    pub fn get_tokenizer(&self, model: &str) -> Arc<dyn Tokenizer> {
        for (matcher, tokenizer) in &self.matchers {
            if matcher.is_match(model) {
                return Arc::clone(tokenizer);
            }
        }
        Arc::clone(&self.fallback)
    }
    
    /// Count tokens for a model + text
    pub fn count_tokens(&self, model: &str, text: &str) -> Result<u32, TokenizerError> {
        let tokenizer = self.get_tokenizer(model);
        tokenizer.count_tokens(text)
    }
}
```

**Relationships**:
- Used by: `BudgetReconciler` to estimate costs
- Configured at: Startup in `cli/serve.rs` (shared instance)
- Cached in: `BudgetReconciler` struct as `Arc<TokenizerRegistry>`

---

## E5: BudgetMetrics (Existing)

**Location**: `src/routing/reconciler/budget.rs` lines 20-54

```rust
/// Per-month budget tracking metrics (T050)
#[derive(Debug, Clone)]
pub struct BudgetMetrics {
    /// Total spending in current month (USD)
    pub current_month_spending: f64,

    /// Last time spending was reconciled
    pub last_reconciliation_time: chrono::DateTime<chrono::Utc>,

    /// Month key (e.g., "2024-03") for rollover detection
    pub month_key: String,
}

impl BudgetMetrics {
    pub fn new() -> Self {
        let now = chrono::Utc::now();
        Self {
            current_month_spending: 0.0,
            last_reconciliation_time: now,
            month_key: now.format("%Y-%m").to_string(),
        }
    }

    fn current_month_key() -> String {
        chrono::Utc::now().format("%Y-%m").to_string()
    }
}
```

**Storage**: `Arc<DashMap<String, BudgetMetrics>>` with single key `"global"`

**State Updates**:
1. **On request completion**: `BudgetReconciler::record_spending(cost_usd)` increments `current_month_spending`
2. **On reconciliation**: `BudgetReconciliationLoop::reconcile_spending()` checks for month rollover
3. **On month rollover**: Reset `current_month_spending` to 0.0, update `month_key`

**Concurrency**: DashMap provides lock-free concurrent access for high-throughput request handling

---

## E6: BudgetConfig (Existing)

**Location**: `src/config/routing.rs` lines 150-188

```rust
/// Budget management configuration (FR-016 to FR-022)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BudgetConfig {
    /// Monthly spending limit in USD. None means no limit (FR-016).
    pub monthly_limit_usd: Option<f64>,

    /// Percentage of monthly limit that triggers SoftLimit status (FR-019).
    /// Default: 75%. At SoftLimit, local agents are preferred over cloud.
    pub soft_limit_percent: f64,

    /// Action to take when hard limit (100%) is reached (FR-021).
    pub hard_limit_action: HardLimitAction,

    /// Reconciliation interval in seconds (FR-022).
    pub reconciliation_interval_secs: u64,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            monthly_limit_usd: None,
            soft_limit_percent: 75.0,
            hard_limit_action: HardLimitAction::default(),
            reconciliation_interval_secs: 60,
        }
    }
}
```

**TOML Example**:
```toml
[routing.budget]
monthly_limit_usd = 100.0
soft_limit_percent = 80.0
hard_limit_action = "block_cloud"
reconciliation_interval_secs = 60
```

---

## E7: HardLimitAction (Existing)

**Location**: `src/config/routing.rs` lines 137-145

```rust
/// Action to take when hard budget limit is reached (FR-021)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HardLimitAction {
    /// Log a warning but allow all requests
    #[default]
    Warn,
    
    /// Block cloud agents only (keep local agents)
    BlockCloud,
    
    /// Block all agents
    BlockAll,
}
```

**Behavior**:
- **Warn**: Logs warning but allows all agents (soft enforcement)
- **BlockCloud**: Excludes `PrivacyZone::Open` agents at hard limit
- **BlockAll**: Excludes all agents → request fails with 503

---

## E8: PricingTable (Existing)

**Location**: `src/agent/pricing.rs` lines 48-150

Already implemented with hardcoded pricing for OpenAI, Anthropic, Google AI. No changes needed - just used by `BudgetReconciler::estimate_cost()`.

---

## E9: Prometheus Metrics (New)

**Location**: Recorded in `src/routing/reconciler/budget.rs` and `src/metrics/mod.rs`

### Gauges
```rust
// In BudgetReconciler::reconcile()
metrics::gauge!("nexus_budget_spending_usd")
    .set(budget_state.current_month_spending);

metrics::gauge!("nexus_budget_utilization_percent")
    .set(spending_percent);

metrics::gauge!("nexus_budget_status")
    .set(budget_status as u8 as f64); // 0=Normal, 1=SoftLimit, 2=HardLimit
```

### Histograms
```rust
// In BudgetReconciler::estimate_cost()
metrics::histogram!("nexus_cost_per_request_usd")
    .record(cost_estimate.cost_usd);

// In TokenizerRegistry::count_tokens()
metrics::histogram!("nexus_token_count_duration_seconds",
    "tier" => tokenizer.tier_name())
    .record(duration.as_secs_f64());
```

### Counters
```rust
// In TokenizerRegistry::count_tokens()
metrics::counter!("nexus_token_count_tier_total",
    "tier" => tokenizer.tier_name(),
    "model" => model)
    .increment(1);

// In BudgetReconciliationLoop::reconcile_spending()
metrics::counter!("nexus_budget_events_total",
    "event_type" => "month_rollover")
    .increment(1);
```

---

## E10: StatsResponse (Enhance)

**Location**: `src/metrics/types.rs`

**Add budget fields**:
```rust
#[derive(Debug, Clone, Serialize)]
pub struct StatsResponse {
    // ... existing fields ...
    
    // Budget statistics (new)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget: Option<BudgetStats>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BudgetStats {
    pub current_spending_usd: f64,
    pub monthly_limit_usd: Option<f64>,
    pub utilization_percent: f64,
    pub status: String, // "Normal", "SoftLimit", "HardLimit"
    pub billing_month: String,
    pub last_reconciliation: String, // ISO 8601 timestamp
}
```

---

## Validation Rules

### Budget State Invariants
1. `current_month_spending >= 0.0` (enforced by addition-only operations)
2. `month_key` matches regex `^\d{4}-\d{2}$` (e.g., "2024-01")
3. `soft_limit_percent` in range [0.0, 100.0] (enforced by config validation)

### Cost Estimate Invariants
1. `cost_usd >= 0.0` (enforced by PricingTable)
2. `input_tokens + estimated_output_tokens > 0` for non-empty requests
3. `token_count_tier` in [0, 1, 2]

### Tokenizer Registry Invariants
1. Always returns a tokenizer (never None) - fallback guarantees
2. Glob patterns are validated at startup (fail-fast on invalid patterns)
3. Tokenizers are thread-safe (Tokenizer trait requires Send + Sync)

---

## Migration Notes

**Existing Code**:
- BudgetReconciler, BudgetMetrics, BudgetConfig already exist
- No schema migrations needed (all in-memory)

**New Code**:
- `src/agent/tokenizer.rs` (new file, ~300 lines)
- Enhancements to `BudgetReconciler::estimate_cost()` to use TokenizerRegistry
- Metrics recording in existing reconciler hooks

**Backward Compatibility**:
- Zero-config default: budget enforcement disabled if `monthly_limit_usd = None`
- Existing requests see no behavior change when budget is not configured
- New response headers only added when `budget_status != Normal`

---

**Next Step**: Generate API contracts (Prometheus metrics spec + /v1/stats schema)
