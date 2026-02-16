//! Budget tracking and enforcement reconciler

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Budget status for cost-aware routing
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
    /// Check if backend is affordable under current budget
    pub fn allows_cost(&self, estimated_cost: f64) -> bool {
        match self {
            BudgetStatus::Normal => true,
            BudgetStatus::SoftLimit { .. } => true, // Prefer cheaper but allow
            BudgetStatus::HardLimit { current, limit } => current + estimated_cost <= *limit,
        }
    }

    /// Should prefer lower-cost options
    pub fn prefer_cheaper(&self) -> bool {
        matches!(self, BudgetStatus::SoftLimit { .. })
    }
}

/// Reason a backend was excluded by budget policy
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetViolation {
    /// Estimated cost for this request
    pub estimated_cost: f64,

    /// Current budget usage
    pub current_usage: f64,

    /// Budget limit
    pub limit: f64,

    /// Human-readable explanation
    pub message: String,
}

impl BudgetViolation {
    pub fn new(estimated_cost: f64, current_usage: f64, limit: f64) -> Self {
        let message = format!(
            "Request cost ${:.2} would exceed budget (${:.2}/${:.2})",
            estimated_cost,
            current_usage + estimated_cost,
            limit
        );
        Self {
            estimated_cost,
            current_usage,
            limit,
            message,
        }
    }
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

impl CostEstimate {
    /// Calculate cost from token counts and pricing
    pub fn calculate(
        input_tokens: u32,
        estimated_output_tokens: u32,
        pricing: &crate::control::budget::ModelPricing,
        tier: TokenCountTier,
        provider: String,
        model: String,
    ) -> Self {
        let cost_usd = (pricing.input_cost_per_million * input_tokens as f64 / 1_000_000.0)
            + (pricing.output_cost_per_million * estimated_output_tokens as f64 / 1_000_000.0);

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

/// Runtime budget state (in-memory only)
pub struct BudgetState {
    /// Current spending in cents (atomic for lock-free updates)
    /// Using cents to avoid floating-point atomics
    current_spending_cents: Arc<AtomicU64>,

    /// Monthly limit in USD (from config)
    pub monthly_limit_usd: f64,

    /// Soft limit percentage (from config)
    soft_limit_percent: u8,

    /// Hard limit action (from config)
    pub hard_limit_action: crate::config::HardLimitAction,

    /// Last billing cycle reset timestamp
    last_reset: Arc<Mutex<DateTime<Utc>>>,
}

impl BudgetState {
    /// Create new budget state from config
    pub fn new(config: &crate::config::BudgetConfig) -> Self {
        Self {
            current_spending_cents: Arc::new(AtomicU64::new(0)),
            monthly_limit_usd: config.monthly_limit,
            soft_limit_percent: config.soft_limit_percent,
            hard_limit_action: config.hard_limit_action,
            last_reset: Arc::new(Mutex::new(Utc::now())),
        }
    }

    /// Add cost to current spending (lock-free)
    pub fn add_spending(&self, cost_usd: f64) {
        let cost_cents = (cost_usd * 100.0) as u64;
        self.current_spending_cents
            .fetch_add(cost_cents, Ordering::Relaxed);
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
        let percentage = ((current / limit) * 100.0) as u8;

        if percentage >= 100 {
            BudgetStatus::HardLimit { current, limit }
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
        *self.last_reset.lock().unwrap() = Utc::now();
        tracing::info!(
            limit_usd = self.monthly_limit_usd,
            "Monthly budget reset: ${:.2} available",
            self.monthly_limit_usd
        );
    }
}

/// Reconciler for budget tracking and enforcement
pub struct BudgetReconciler {
    /// Pricing registry for model cost lookup
    pricing_registry: Arc<PricingRegistry>,

    /// Budget state tracker
    budget_state: Arc<BudgetState>,

    /// Whether budget enforcement is enabled
    enabled: bool,
}

impl BudgetReconciler {
    /// Create new budget reconciler
    pub fn new(
        pricing_registry: Arc<PricingRegistry>,
        budget_state: Arc<BudgetState>,
        enabled: bool,
    ) -> Self {
        Self {
            pricing_registry,
            budget_state,
            enabled,
        }
    }

    /// Estimate tokens from request text using heuristic (chars/4 * 1.15 conservative multiplier)
    #[allow(dead_code)] // Will be used when raw_input is available in RequestRequirements
    fn estimate_tokens(&self, text: &str) -> u32 {
        // Heuristic: 1 token ≈ 4 characters
        // Apply 1.15x conservative multiplier for unknown models
        let base_tokens = (text.len() / 4) as u32;
        ((base_tokens as f64) * 1.15) as u32
    }

    /// Estimate output tokens from input tokens (heuristic: input * 0.5)
    fn estimate_output_tokens(&self, input_tokens: u32) -> u32 {
        ((input_tokens as f64) * 0.5) as u32
    }

    /// Determine provider from backend type
    fn get_provider(&self, backend: &crate::registry::Backend) -> String {
        match backend.backend_type {
            crate::registry::BackendType::OpenAI => "openai".to_string(),
            crate::registry::BackendType::Anthropic => "anthropic".to_string(),
            crate::registry::BackendType::Ollama
            | crate::registry::BackendType::LlamaCpp
            | crate::registry::BackendType::VLLM
            | crate::registry::BackendType::Exo
            | crate::registry::BackendType::LMStudio => "local".to_string(),
            crate::registry::BackendType::Generic => "unknown".to_string(),
        }
    }
}

#[async_trait]
impl crate::control::reconciler::Reconciler for BudgetReconciler {
    async fn reconcile(
        &self,
        intent: &mut crate::control::intent::RoutingIntent,
    ) -> Result<(), crate::control::reconciler::ReconcileError> {
        // Skip if budget enforcement disabled
        if !self.enabled {
            intent.annotations.budget_status = Some(BudgetStatus::Normal);
            return Ok(());
        }

        // Get current budget status
        let budget_status = self.budget_state.budget_status();
        intent.annotations.budget_status = Some(budget_status);

        // Estimate cost for first candidate (representative estimate)
        if let Some(backend) = intent.candidate_backends.first() {
            // Count input tokens using heuristic (estimate from model name length as proxy)
            // In production, this would use actual request body text
            // For now, use a conservative default of 500 tokens per request
            let input_tokens = 500u32; // Conservative default
            let estimated_output_tokens = self.estimate_output_tokens(input_tokens);

            // Get model name from request or use backend's first model
            let model = if !intent.request_requirements.model.is_empty() {
                intent.request_requirements.model.clone()
            } else if let Some(first_model) = backend.models.first() {
                first_model.id.clone()
            } else {
                "unknown".to_string()
            };

            // Get provider
            let provider = self.get_provider(backend);

            // Lookup pricing
            let pricing = self.pricing_registry.get_pricing(&model);

            // Calculate cost estimate
            let cost_estimate = CostEstimate::calculate(
                input_tokens,
                estimated_output_tokens,
                &pricing,
                TokenCountTier::Estimated,
                provider,
                model,
            );

            // Store cost estimate
            intent.annotations.estimated_cost = Some(cost_estimate.cost_usd);
            intent.annotations.cost_estimate = Some(cost_estimate.clone());

            // Record spending
            self.budget_state.add_spending(cost_estimate.cost_usd);

            // Record metrics
            crate::metrics::budget::record_cost_estimate(
                cost_estimate.cost_usd,
                &cost_estimate.provider,
                &cost_estimate.model,
                "Estimated",
            );
            crate::metrics::budget::update_budget_metrics(
                self.budget_state.current_spending_usd(),
                self.budget_state.monthly_limit_usd,
                (self.budget_state.current_spending_usd() / self.budget_state.monthly_limit_usd)
                    * 100.0,
            );

            intent.trace(format!(
                "Budget: ${:.4} estimated ({:?}), status {:?}",
                cost_estimate.cost_usd, cost_estimate.token_count_tier, budget_status
            ));
        }

        // Handle soft limit: prefer local agents (done in selection reconciler)
        if matches!(budget_status, BudgetStatus::SoftLimit { .. }) {
            tracing::warn!(
                usage_percent = match budget_status {
                    BudgetStatus::SoftLimit { usage_percent } => usage_percent,
                    _ => 0,
                },
                "Budget soft limit reached: preferring local agents"
            );
            crate::metrics::budget::increment_soft_limit_activation();
        }

        // Handle hard limit: filter out cloud backends based on hard_limit_action
        if matches!(budget_status, BudgetStatus::HardLimit { .. }) {
            tracing::error!(
                current = match budget_status {
                    BudgetStatus::HardLimit { current, .. } => current,
                    _ => 0.0,
                },
                limit = match budget_status {
                    BudgetStatus::HardLimit { limit, .. } => limit,
                    _ => 0.0,
                },
                action = ?self.budget_state.hard_limit_action,
                "Budget hard limit reached: enforcing {:?} action",
                self.budget_state.hard_limit_action
            );
            crate::metrics::budget::increment_hard_limit_activation();

            // Filter cloud backends if action is LocalOnly
            if matches!(
                self.budget_state.hard_limit_action,
                crate::config::HardLimitAction::LocalOnly
            ) {
                let mut excluded = HashMap::new();
                intent.candidate_backends.retain(|backend| {
                    let is_cloud = matches!(
                        backend.backend_type,
                        crate::registry::BackendType::OpenAI
                            | crate::registry::BackendType::Anthropic
                    );

                    if is_cloud {
                        let current = self.budget_state.current_spending_usd();
                        let limit = self.budget_state.monthly_limit_usd;
                        excluded.insert(
                            backend.name.clone(),
                            BudgetViolation::new(0.0, current, limit),
                        );
                        crate::metrics::budget::increment_blocked_request("cloud_filtered");
                        false
                    } else {
                        true
                    }
                });
                intent.annotations.budget_excluded = excluded;
            }
        }

        Ok(())
    }

    fn error_policy(&self) -> crate::control::reconciler::ReconcileErrorPolicy {
        crate::control::reconciler::ReconcileErrorPolicy::FailOpen // Can estimate locally
    }

    fn name(&self) -> &str {
        "BudgetReconciler"
    }
}

// Pricing submodule
pub mod pricing;

pub use pricing::{ModelPricing, PricingRegistry};
