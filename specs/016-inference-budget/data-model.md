# Data Model: Inference Budget Management

**Feature**: F14 - Inference Budget Management  
**Date**: 2025-01-22  
**Phase**: 1 (Design & Contracts)

## Overview

This document defines the data structures, configuration schema, and entity relationships for the budget management feature. All types are designed for in-memory operation with minimal overhead.

---

## Core Entities

### 1. BudgetConfig

Configuration loaded from nexus.toml `[budget]` section.

**Location**: `src/config/budget.rs`

```rust
use serde::{Deserialize, Serialize};

/// Budget enforcement configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BudgetConfig {
    /// Monthly spending limit in USD (None = budget enforcement disabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monthly_limit: Option<f64>,

    /// Percentage threshold for soft limit (0-100, default: 80)
    pub soft_limit_percent: u8,

    /// Action when hard limit (100%) is reached
    pub hard_limit_action: HardLimitAction,

    /// Day of month when billing cycle resets (1-31, default: 1)
    pub billing_cycle_start_day: u8,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            monthly_limit: None,
            soft_limit_percent: 80,
            hard_limit_action: HardLimitAction::LocalOnly,
            billing_cycle_start_day: 1,
        }
    }
}

/// Action to take when hard budget limit is reached
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HardLimitAction {
    /// Route only to local agents (block cloud)
    LocalOnly,
    
    /// Queue requests requiring cloud agents (future: task queue)
    Queue,
    
    /// Return 429 error for requests requiring cloud agents
    Reject,
}
```

**Validation Rules**:
- `monthly_limit`: If Some, must be >= 0.0
- `soft_limit_percent`: Must be 0-100
- `billing_cycle_start_day`: Must be 1-31
- If `monthly_limit` is None, budget enforcement is disabled (all requests Normal status)

**Example TOML**:
```toml
[budget]
monthly_limit = 100.00
soft_limit_percent = 80
hard_limit_action = "local-only"
billing_cycle_start_day = 1
```

---

### 2. BudgetStatus

Current budget consumption state (existing enum, already in `src/control/budget.rs`).

**Enhancement**: Add variant details for richer logging.

```rust
/// Budget status for cost-aware routing (EXISTING)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BudgetStatus {
    /// Normal operation (under soft limit)
    Normal,

    /// Approaching limit (prefer cheaper options)
    SoftLimit {
        /// Percentage of budget used (0-100)
        usage_percent: u8,
    },

    /// Hard limit reached (block expensive operations)
    HardLimit {
        /// Current spend
        current: f64,
        /// Maximum allowed
        limit: f64,
    },
}

impl BudgetStatus {
    /// Check if backend is affordable under current budget (EXISTING)
    pub fn allows_cost(&self, estimated_cost: f64) -> bool {
        match self {
            BudgetStatus::Normal => true,
            BudgetStatus::SoftLimit { .. } => true, // Prefer cheaper but allow
            BudgetStatus::HardLimit { current, limit } => current + estimated_cost <= *limit,
        }
    }

    /// Should prefer lower-cost options (EXISTING)
    pub fn prefer_cheaper(&self) -> bool {
        matches!(self, BudgetStatus::SoftLimit { .. })
    }
}
```

**Usage**: Attached to `RoutingIntent.annotations.budget_status` to inform routing decisions.

---

### 3. CostEstimate

Per-request cost estimation result.

**Location**: `src/control/budget.rs` (new struct)

```rust
/// Estimated cost for an inference request
#[derive(Debug, Clone, PartialEq)]
pub struct CostEstimate {
    /// Input token count (from InferenceAgent::count_tokens())
    pub input_tokens: u32,
    
    /// Estimated output token count (heuristic: input_tokens * 0.5)
    pub estimated_output_tokens: u32,
    
    /// Estimated cost in USD
    pub cost_usd: f64,
    
    /// Token counting accuracy tier
    pub token_count_tier: TokenCountTier,
    
    /// Provider name (e.g., "openai", "anthropic", "local")
    pub provider: String,
    
    /// Model name (e.g., "gpt-4-turbo", "claude-3-opus")
    pub model: String,
    
    /// Timestamp of estimation
    pub timestamp: std::time::Instant,
}

/// Token counting accuracy tier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenCountTier {
    /// Exact count from provider-specific tokenizer (v0.4+)
    Exact,
    
    /// Approximation with known variance (e.g., Anthropic cl100k_base)
    Approximation,
    
    /// Heuristic estimate (chars/4 with 1.15x multiplier)
    Estimated,
}

impl CostEstimate {
    /// Calculate cost from token counts and pricing
    pub fn calculate(
        input_tokens: u32,
        estimated_output_tokens: u32,
        pricing: &ModelPricing,
        tier: TokenCountTier,
        provider: String,
        model: String,
    ) -> Self {
        let cost_usd = pricing.input_price_per_1k * (input_tokens as f64 / 1000.0)
                     + pricing.output_price_per_1k * (estimated_output_tokens as f64 / 1000.0);
        
        Self {
            input_tokens,
            estimated_output_tokens,
            cost_usd,
            token_count_tier: tier,
            provider,
            model,
            timestamp: std::time::Instant::now(),
        }
    }
}
```

**Usage**: Stored in `RoutingIntent.annotations.cost_estimate` for metrics export.

---

### 4. ModelPricing

Pricing information for a model or model pattern.

**Location**: `src/control/budget/pricing.rs` (new module)

```rust
use std::collections::HashMap;

/// Pricing for input/output tokens
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPricing {
    /// Price per 1K input tokens (USD)
    pub input_price_per_1k: f64,
    
    /// Price per 1K output tokens (USD)
    pub output_price_per_1k: f64,
}

/// Global pricing registry (hardcoded, updated with releases)
pub struct PricingRegistry {
    /// Map of model pattern (regex or exact match) to pricing
    pricing_map: HashMap<String, ModelPricing>,
}

impl PricingRegistry {
    /// Create registry with default pricing tables
    pub fn default_registry() -> Self {
        let mut pricing_map = HashMap::new();
        
        // OpenAI models (Jan 2025 pricing)
        pricing_map.insert("gpt-4-turbo".to_string(), ModelPricing {
            input_price_per_1k: 0.01,
            output_price_per_1k: 0.03,
        });
        pricing_map.insert("gpt-4".to_string(), ModelPricing {
            input_price_per_1k: 0.03,
            output_price_per_1k: 0.06,
        });
        pricing_map.insert("gpt-3.5-turbo".to_string(), ModelPricing {
            input_price_per_1k: 0.0005,
            output_price_per_1k: 0.0015,
        });
        
        // Anthropic models
        pricing_map.insert("claude-3-opus".to_string(), ModelPricing {
            input_price_per_1k: 0.015,
            output_price_per_1k: 0.075,
        });
        pricing_map.insert("claude-3-sonnet".to_string(), ModelPricing {
            input_price_per_1k: 0.003,
            output_price_per_1k: 0.015,
        });
        pricing_map.insert("claude-3-haiku".to_string(), ModelPricing {
            input_price_per_1k: 0.00025,
            output_price_per_1k: 0.00125,
        });
        
        // Default for unknown models (conservative: highest OpenAI tier)
        pricing_map.insert("__unknown__".to_string(), ModelPricing {
            input_price_per_1k: 0.03,
            output_price_per_1k: 0.06,
        });
        
        Self { pricing_map }
    }
    
    /// Get pricing for a model (with fallback to unknown)
    pub fn get_pricing(&self, model: &str) -> ModelPricing {
        // Try exact match first
        if let Some(pricing) = self.pricing_map.get(model) {
            return *pricing;
        }
        
        // Try prefix match (e.g., "gpt-4-turbo-2024-04-09" matches "gpt-4-turbo")
        for (pattern, pricing) in &self.pricing_map {
            if model.starts_with(pattern) {
                return *pricing;
            }
        }
        
        // Fallback to unknown (conservative estimate)
        self.pricing_map["__unknown__"]
    }
}

/// Local models have zero cost
impl ModelPricing {
    pub const LOCAL: ModelPricing = ModelPricing {
        input_price_per_1k: 0.0,
        output_price_per_1k: 0.0,
    };
}
```

**Maintenance**: Update pricing_map in `default_registry()` when providers change pricing.

---

### 5. BudgetState

Runtime state for budget tracking (in-memory only).

**Location**: `src/control/budget.rs` (new struct)

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Runtime budget state (in-memory, not persisted)
pub struct BudgetState {
    /// Current spending in USD cents (atomic for lock-free updates)
    /// Using cents to avoid floating-point atomics
    current_spending_cents: Arc<AtomicU64>,
    
    /// Monthly limit in USD (from config)
    monthly_limit_usd: f64,
    
    /// Soft limit percentage (from config)
    soft_limit_percent: u8,
    
    /// Hard limit action (from config)
    hard_limit_action: HardLimitAction,
    
    /// Last billing cycle reset timestamp
    last_reset: Arc<std::sync::Mutex<chrono::DateTime<chrono::Utc>>>,
}

impl BudgetState {
    /// Create new budget state from config
    pub fn new(config: &BudgetConfig) -> Self {
        let monthly_limit = config.monthly_limit.unwrap_or(f64::MAX);
        
        Self {
            current_spending_cents: Arc::new(AtomicU64::new(0)),
            monthly_limit_usd: monthly_limit,
            soft_limit_percent: config.soft_limit_percent,
            hard_limit_action: config.hard_limit_action,
            last_reset: Arc::new(std::sync::Mutex::new(chrono::Utc::now())),
        }
    }
    
    /// Add cost to current spending (lock-free)
    pub fn add_spending(&self, cost_usd: f64) {
        let cost_cents = (cost_usd * 100.0) as u64;
        self.current_spending_cents.fetch_add(cost_cents, Ordering::Relaxed);
    }
    
    /// Get current spending in USD (lock-free read)
    pub fn current_spending_usd(&self) -> f64 {
        let cents = self.current_spending_cents.load(Ordering::Relaxed);
        cents as f64 / 100.0
    }
    
    /// Calculate current budget status
    pub fn budget_status(&self) -> BudgetStatus {
        let current = self.current_spending_usd();
        let limit = self.monthly_limit_usd;
        let percentage = (current / limit * 100.0) as u8;
        
        if percentage >= 100 {
            BudgetStatus::HardLimit {
                current,
                limit,
            }
        } else if percentage >= self.soft_limit_percent {
            BudgetStatus::SoftLimit {
                usage_percent: percentage,
            }
        } else {
            BudgetStatus::Normal
        }
    }
    
    /// Reset spending counter (called by reconciliation loop)
    pub fn reset_spending(&self) {
        self.current_spending_cents.store(0, Ordering::Relaxed);
        *self.last_reset.lock().unwrap() = chrono::Utc::now();
        tracing::info!(
            limit_usd = self.monthly_limit_usd,
            "Monthly budget reset: ${:.2} available",
            self.monthly_limit_usd
        );
    }
}
```

**Thread Safety**: AtomicU64 for spending counter (lock-free), Mutex for last_reset timestamp (low contention).

---

### 6. BudgetMetrics

Prometheus metrics for budget observability.

**Location**: `src/metrics/mod.rs` (enhancement to existing module)

```rust
use metrics::{describe_counter, describe_gauge, describe_histogram, counter, gauge, histogram};

/// Initialize budget-related Prometheus metrics
pub fn init_budget_metrics() {
    // Gauges for current state
    describe_gauge!(
        "nexus_budget_current_spending_usd",
        "Current monthly spending in USD"
    );
    describe_gauge!(
        "nexus_budget_limit_usd",
        "Configured monthly budget limit in USD"
    );
    describe_gauge!(
        "nexus_budget_percent_used",
        "Percentage of budget consumed (0-100+)"
    );
    
    // Counters for events
    describe_counter!(
        "nexus_budget_requests_blocked_total",
        "Total requests blocked by budget enforcement"
    );
    describe_counter!(
        "nexus_budget_soft_limit_activations_total",
        "Total times soft limit (80%) was activated"
    );
    describe_counter!(
        "nexus_budget_hard_limit_activations_total",
        "Total times hard limit (100%) was activated"
    );
    
    // Histogram for cost distribution
    describe_histogram!(
        "nexus_cost_estimate_usd",
        "Per-request cost estimates in USD"
    );
}

/// Update budget metrics from current state
pub fn update_budget_metrics(state: &BudgetState) {
    let current = state.current_spending_usd();
    let limit = state.monthly_limit_usd;
    let percentage = (current / limit * 100.0).min(999.0); // Cap at 999% for metric
    
    gauge!("nexus_budget_current_spending_usd").set(current);
    gauge!("nexus_budget_limit_usd").set(limit);
    gauge!("nexus_budget_percent_used").set(percentage);
}

/// Record cost estimate in metrics
pub fn record_cost_estimate(estimate: &CostEstimate) {
    let labels = [
        ("provider", estimate.provider.clone()),
        ("model", estimate.model.clone()),
        ("tier", format!("{:?}", estimate.token_count_tier)),
    ];
    
    histogram!("nexus_cost_estimate_usd", &labels).record(estimate.cost_usd);
}

/// Increment soft limit activation counter
pub fn increment_soft_limit_activation() {
    counter!("nexus_budget_soft_limit_activations_total").increment(1);
}

/// Increment hard limit activation counter
pub fn increment_hard_limit_activation() {
    counter!("nexus_budget_hard_limit_activations_total").increment(1);
}

/// Increment blocked request counter
pub fn increment_blocked_request(reason: &str) {
    let labels = [("reason", reason.to_string())];
    counter!("nexus_budget_requests_blocked_total", &labels).increment(1);
}
```

**Integration**: Called from BudgetReconciler and background reconciliation loop.

---

## Entity Relationships

```
NexusConfig
  └── BudgetConfig
       ├── monthly_limit: Option<f64>
       ├── soft_limit_percent: u8
       ├── hard_limit_action: HardLimitAction
       └── billing_cycle_start_day: u8

BudgetState (runtime)
  ├── current_spending_cents: AtomicU64
  ├── monthly_limit_usd: f64
  ├── soft_limit_percent: u8
  └── hard_limit_action: HardLimitAction

RoutingIntent.annotations
  ├── budget_status: Option<BudgetStatus>
  └── cost_estimate: Option<CostEstimate>

CostEstimate
  ├── input_tokens: u32
  ├── estimated_output_tokens: u32
  ├── cost_usd: f64
  ├── token_count_tier: TokenCountTier
  ├── provider: String
  └── model: String

PricingRegistry
  └── pricing_map: HashMap<String, ModelPricing>
       └── ModelPricing
            ├── input_price_per_1k: f64
            └── output_price_per_1k: f64
```

---

## State Transitions

### BudgetStatus State Machine

```
Normal (0-79%)
  ├─[spending reaches 80%]→ SoftLimit (80-99%)
  │                           ├─[spending drops below 80%]→ Normal
  │                           └─[spending reaches 100%]→ HardLimit (100%+)
  └─[billing cycle reset]→ Normal

HardLimit (100%+)
  ├─[billing cycle reset]→ Normal
  └─[spending continues]→ HardLimit (stays)
```

**Transitions triggered by**:
- Spending updates: `BudgetState::add_spending()` called after each request
- Billing cycle reset: `BudgetReconciliationLoop` checks date daily
- Status calculation: `BudgetState::budget_status()` computed on each routing decision

---

## Validation Rules

### BudgetConfig Validation

```rust
impl BudgetConfig {
    /// Validate configuration at startup
    pub fn validate(&self) -> Result<(), String> {
        // Monthly limit must be non-negative
        if let Some(limit) = self.monthly_limit {
            if limit < 0.0 {
                return Err("monthly_limit must be >= 0.0".to_string());
            }
        }
        
        // Soft limit percent must be 0-100
        if self.soft_limit_percent > 100 {
            return Err("soft_limit_percent must be 0-100".to_string());
        }
        
        // Billing cycle day must be 1-31
        if !(1..=31).contains(&self.billing_cycle_start_day) {
            return Err("billing_cycle_start_day must be 1-31".to_string());
        }
        
        Ok(())
    }
}
```

---

## Serialization Formats

### TOML (Config File)
```toml
[budget]
monthly_limit = 100.00
soft_limit_percent = 80
hard_limit_action = "local-only"
billing_cycle_start_day = 1
```

### Prometheus (Metrics Export)
```
# TYPE nexus_budget_current_spending_usd gauge
nexus_budget_current_spending_usd 45.23

# TYPE nexus_budget_limit_usd gauge
nexus_budget_limit_usd 100.00

# TYPE nexus_budget_percent_used gauge
nexus_budget_percent_used 45.23

# TYPE nexus_cost_estimate_usd histogram
nexus_cost_estimate_usd_bucket{provider="openai",model="gpt-4-turbo",tier="Estimated",le="0.001"} 0
nexus_cost_estimate_usd_bucket{provider="openai",model="gpt-4-turbo",tier="Estimated",le="0.01"} 5
nexus_cost_estimate_usd_bucket{provider="openai",model="gpt-4-turbo",tier="Estimated",le="0.1"} 42
nexus_cost_estimate_usd_sum{provider="openai",model="gpt-4-turbo",tier="Estimated"} 2.35
nexus_cost_estimate_usd_count{provider="openai",model="gpt-4-turbo",tier="Estimated"} 50
```

---

## Memory Overhead

| Entity | Size | Count | Total |
|--------|------|-------|-------|
| BudgetConfig | 24 bytes | 1 | 24 bytes |
| BudgetState | 56 bytes | 1 | 56 bytes |
| PricingRegistry | ~1KB | 1 | 1 KB |
| CostEstimate (per request) | 80 bytes | 0 (transient) | 0 bytes |
| **Total** | | | **~1.1 KB** |

**Notes**:
- CostEstimate is created per request but not stored (only exported to metrics)
- BudgetState AtomicU64 is 8 bytes, Mutex<DateTime> is ~40 bytes
- PricingRegistry HashMap with ~20 entries = ~1KB (string keys + ModelPricing values)

---

## Future Enhancements (v0.4+)

### Exact Tokenization
- Add `tiktoken-rs` and `tokenizers` dependencies
- Implement per-provider tokenizers in agent trait methods
- Update `TokenCountTier::Exact` usage

### Provider-Reported Costs
- Poll OpenAI/Anthropic usage APIs for actual costs
- Reconcile estimates vs actuals, log discrepancies
- Adjust spending counter if variance exceeds threshold

### Per-Backend Budgets
- Extend `BudgetConfig` with optional per-backend limits
- Track spending per backend in separate AtomicU64 counters
- Support hierarchical budgets (global + per-backend)

### Request Queuing
- Implement in-memory VecDeque for queued requests
- Background task to process queue when budget resets
- Expose queue depth metric

---

## References

- Research: [specs/016-inference-budget/research.md](./research.md)
- Spec: [specs/016-inference-budget/spec.md](./spec.md)
- Existing: `src/control/budget.rs` (BudgetStatus enum)
- Existing: `src/metrics/mod.rs` (Prometheus integration)
