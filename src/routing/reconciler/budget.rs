//! BudgetReconciler - enforces spending limits and cost estimation
//!
//! Tracks monthly spending, estimates per-request cost, and enforces
//! budget limits by adjusting routing decisions. Runs AFTER PrivacyReconciler
//! and BEFORE SchedulerReconciler in the pipeline.

use super::intent::{BudgetStatus, CostEstimate, RoutingIntent};
use super::Reconciler;
use crate::agent::pricing::PricingTable;
use crate::agent::tokenizer::TokenizerRegistry;
use crate::agent::PrivacyZone;
use crate::config::{BudgetConfig, HardLimitAction};
use crate::registry::Registry;
use crate::routing::error::RoutingError;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

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

impl Default for BudgetMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl BudgetMetrics {
    /// Create new metrics for the current month
    pub fn new() -> Self {
        let now = chrono::Utc::now();
        Self {
            current_month_spending: 0.0,
            last_reconciliation_time: now,
            month_key: now.format("%Y-%m").to_string(),
        }
    }

    /// Get current month key
    fn current_month_key() -> String {
        chrono::Utc::now().format("%Y-%m").to_string()
    }
}

/// Shared budget state key for the global spending tracker
pub const GLOBAL_BUDGET_KEY: &str = "global";

/// BudgetReconciler estimates costs and enforces spending limits.
///
/// # Pipeline Position
/// RequestAnalyzer → PrivacyReconciler → **BudgetReconciler** → SchedulerReconciler
///
/// # Behavior
/// 1. Estimate cost for the request based on token counts and model pricing
/// 2. Calculate budget status (Normal/SoftLimit/HardLimit) based on spending
/// 3. At SoftLimit: mark status (SchedulerReconciler adjusts scores)
/// 4. At HardLimit: exclude cloud or all agents based on config
///
/// # Zero-Config Default (FR-016)
/// If no monthly_limit_usd is configured, all agents pass through unchanged.
pub struct BudgetReconciler {
    registry: Arc<Registry>,
    config: BudgetConfig,
    pricing: PricingTable,
    tokenizer_registry: Arc<TokenizerRegistry>,
    budget_state: Arc<DashMap<String, BudgetMetrics>>,
}

impl BudgetReconciler {
    /// Create a new BudgetReconciler with shared budget state.
    pub fn new(
        registry: Arc<Registry>,
        config: BudgetConfig,
        tokenizer_registry: Arc<TokenizerRegistry>,
        budget_state: Arc<DashMap<String, BudgetMetrics>>,
    ) -> Self {
        // Initialize global metrics if not present
        budget_state
            .entry(GLOBAL_BUDGET_KEY.to_string())
            .or_default();

        Self {
            registry,
            config,
            pricing: PricingTable::new(),
            tokenizer_registry,
            budget_state,
        }
    }

    /// Estimate cost for a request (FR-017, FR-018).
    ///
    /// Uses TokenizerRegistry when prompt text is available for accurate counting.
    /// Falls back to requirements.estimated_tokens for backward compatibility.
    fn estimate_cost(&self, model: &str, input_tokens: u32) -> CostEstimate {
        // For now, use the pre-computed estimated_tokens from RequestRequirements
        // TODO: Pass actual prompt text for precise tokenization
        // This maintains compatibility with existing code while preparing for upgrade

        let estimated_output_tokens = input_tokens / 2;

        let cost_usd = self
            .pricing
            .estimate_cost(model, input_tokens, estimated_output_tokens)
            .unwrap_or(0.0);

        // Determine tier based on tokenizer available for this model
        let tokenizer = self.tokenizer_registry.get_tokenizer(model);
        let token_count_tier = tokenizer.tier();

        CostEstimate {
            input_tokens,
            estimated_output_tokens,
            cost_usd,
            token_count_tier,
        }
    }

    /// Calculate budget status based on current spending vs limits (FR-019).
    fn calculate_budget_status(&self) -> BudgetStatus {
        let monthly_limit = match self.config.monthly_limit_usd {
            Some(limit) => limit,
            None => return BudgetStatus::Normal, // No limit configured
        };

        if monthly_limit <= 0.0 {
            return BudgetStatus::Normal;
        }

        let current_spending = self
            .budget_state
            .get(GLOBAL_BUDGET_KEY)
            .map(|m| m.current_month_spending)
            .unwrap_or(0.0);

        let spending_percent = (current_spending / monthly_limit) * 100.0;
        let soft_threshold = self.config.soft_limit_percent;

        if spending_percent >= 100.0 {
            BudgetStatus::HardLimit
        } else if spending_percent >= soft_threshold {
            BudgetStatus::SoftLimit
        } else {
            BudgetStatus::Normal
        }
    }

    /// Record estimated cost in the budget state.
    /// Called after routing decision to track spending.
    pub fn record_spending(&self, cost_usd: f64) {
        if cost_usd <= 0.0 {
            return;
        }

        let current_month = BudgetMetrics::current_month_key();

        self.budget_state
            .entry(GLOBAL_BUDGET_KEY.to_string())
            .and_modify(|metrics| {
                // Check for month rollover
                if metrics.month_key != current_month {
                    // T027: Log budget reset on month rollover
                    tracing::info!(
                        old_month = %metrics.month_key,
                        new_month = %current_month,
                        previous_spending = metrics.current_month_spending,
                        "Budget reset: new billing cycle started"
                    );

                    // T028: Record month rollover event
                    metrics::counter!("nexus_budget_events_total", "event_type" => "month_rollover")
                        .increment(1);

                    metrics.current_month_spending = 0.0;
                    metrics.month_key = current_month.clone();
                }
                metrics.current_month_spending += cost_usd;
            })
            .or_insert_with(|| {
                let mut m = BudgetMetrics::new();
                m.current_month_spending = cost_usd;
                m
            });
    }

    /// Determine the effective privacy zone for a backend.
    fn get_backend_privacy_zone(&self, agent_id: &str) -> PrivacyZone {
        if let Some(agent) = self.registry.get_agent(agent_id) {
            return agent.profile().privacy_zone;
        }
        if let Some(backend) = self.registry.get_backend(agent_id) {
            return backend.backend_type.default_privacy_zone();
        }
        // Unknown backend → treat as Open (cloud)
        PrivacyZone::Open
    }
}

impl Reconciler for BudgetReconciler {
    fn name(&self) -> &'static str {
        "BudgetReconciler"
    }

    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        // FR-016: No budget configured → pass through
        if self.config.monthly_limit_usd.is_none() {
            // Still populate cost estimate for informational purposes
            intent.cost_estimate =
                self.estimate_cost(&intent.resolved_model, intent.requirements.estimated_tokens);
            return Ok(());
        }

        // Step 1: Estimate cost for this request (FR-017, FR-018)
        let cost_estimate =
            self.estimate_cost(&intent.resolved_model, intent.requirements.estimated_tokens);
        intent.cost_estimate = cost_estimate.clone();

        // Record cost metric (US2: Precise Tracking)
        metrics::histogram!("nexus_cost_per_request_usd", "model" => intent.resolved_model.clone())
            .record(cost_estimate.cost_usd);

        // Step 2: Calculate budget status (FR-019)
        let budget_status = self.calculate_budget_status();
        intent.budget_status = budget_status;

        tracing::debug!(
            model = %intent.resolved_model,
            cost_usd = cost_estimate.cost_usd,
            token_count_tier = cost_estimate.token_count_tier,
            tier_name = cost_estimate.tier_name(),
            budget_status = ?budget_status,
            candidates = intent.candidate_agents.len(),
            "BudgetReconciler: evaluated budget status"
        );

        match budget_status {
            BudgetStatus::Normal => {
                // All agents available, no action needed
            }
            BudgetStatus::SoftLimit => {
                // FR-020: Don't exclude agents, just mark status
                // SchedulerReconciler will reduce cloud agent scores by 50%
                tracing::info!(
                    "BudgetReconciler: soft limit reached, \
                     SchedulerReconciler will prefer local agents"
                );
            }
            BudgetStatus::HardLimit => {
                // FR-021: Exclude agents based on hard_limit_action
                match self.config.hard_limit_action {
                    HardLimitAction::Warn => {
                        tracing::warn!(
                            "BudgetReconciler: hard limit reached, \
                             but action is Warn — allowing all agents"
                        );
                    }
                    HardLimitAction::BlockCloud => {
                        let candidate_ids: Vec<String> = intent.candidate_agents.clone();
                        for agent_id in &candidate_ids {
                            let zone = self.get_backend_privacy_zone(agent_id);
                            if zone == PrivacyZone::Open {
                                intent.exclude_agent(
                                    agent_id.clone(),
                                    "BudgetReconciler",
                                    format!(
                                        "Monthly budget hard limit reached; \
                                         cloud agent '{}' blocked (zone: {:?})",
                                        agent_id, zone
                                    ),
                                    "Increase monthly_limit_usd or wait for next billing cycle"
                                        .to_string(),
                                );
                            }
                        }
                    }
                    HardLimitAction::BlockAll => {
                        let candidate_ids: Vec<String> = intent.candidate_agents.clone();
                        for agent_id in &candidate_ids {
                            intent.exclude_agent(
                                agent_id.clone(),
                                "BudgetReconciler",
                                "Monthly budget hard limit reached; all agents blocked".to_string(),
                                "Increase monthly_limit_usd or wait for next billing cycle"
                                    .to_string(),
                            );
                        }
                    }
                }
            }
        }

        tracing::debug!(
            remaining = intent.candidate_agents.len(),
            excluded = intent.excluded_agents.len(),
            "BudgetReconciler: enforcement complete"
        );

        Ok(())
    }
}

/// Background task that periodically reconciles budget spending (FR-022).
///
/// Follows the same pattern as HealthChecker: spawns a tokio task
/// with CancellationToken for graceful shutdown.
pub struct BudgetReconciliationLoop {
    budget_state: Arc<DashMap<String, BudgetMetrics>>,
    budget_config: BudgetConfig,
    interval_secs: u64,
}

impl BudgetReconciliationLoop {
    /// Create a new reconciliation loop.
    pub fn new(
        budget_state: Arc<DashMap<String, BudgetMetrics>>,
        budget_config: BudgetConfig,
        interval_secs: u64,
    ) -> Self {
        Self {
            budget_state,
            budget_config,
            interval_secs,
        }
    }

    /// Start the background reconciliation task (FR-022).
    /// Returns a JoinHandle that resolves when the loop stops.
    pub fn start(self, cancel_token: CancellationToken) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(self.interval_secs));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            tracing::info!(
                interval_secs = self.interval_secs,
                "Budget reconciliation loop started"
            );

            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        tracing::info!("Budget reconciliation loop shutting down");
                        break;
                    }
                    _ = interval.tick() => {
                        self.reconcile_spending();
                    }
                }
            }
        })
    }

    /// Reconcile spending data and record metrics.
    ///
    /// Records Prometheus gauges for budget spending, utilization, and status.
    /// Checks for month rollover and updates timestamps.
    fn reconcile_spending(&self) {
        let current_month = BudgetMetrics::current_month_key();
        let now = chrono::Utc::now();

        self.budget_state
            .entry(GLOBAL_BUDGET_KEY.to_string())
            .and_modify(|metrics| {
                // Check for month rollover
                if metrics.month_key != current_month {
                    tracing::info!(
                        old_month = %metrics.month_key,
                        new_month = %current_month,
                        final_spending = metrics.current_month_spending,
                        "Budget month rollover, resetting spending"
                    );
                    metrics.current_month_spending = 0.0;
                    metrics.month_key = current_month.clone();
                }
                metrics.last_reconciliation_time = now;
            })
            .or_default();

        if let Some(metrics) = self.budget_state.get(GLOBAL_BUDGET_KEY) {
            // T030: Record current spending gauge
            metrics::gauge!(
                "nexus_budget_spending_usd",
                "billing_month" => metrics.month_key.clone()
            )
            .set(metrics.current_month_spending);

            // T031, T032: Record utilization and status gauges
            if let Some(monthly_limit) = self.budget_config.monthly_limit_usd {
                if monthly_limit > 0.0 {
                    let utilization = (metrics.current_month_spending / monthly_limit) * 100.0;
                    metrics::gauge!(
                        "nexus_budget_utilization_percent",
                        "billing_month" => metrics.month_key.clone()
                    )
                    .set(utilization);

                    // Calculate status: 0=Normal, 1=SoftLimit, 2=HardLimit
                    let status = if utilization >= 100.0 {
                        2.0 // HardLimit
                    } else if utilization >= self.budget_config.soft_limit_percent {
                        1.0 // SoftLimit
                    } else {
                        0.0 // Normal
                    };
                    metrics::gauge!(
                        "nexus_budget_status",
                        "billing_month" => metrics.month_key.clone()
                    )
                    .set(status);
                }
            }

            tracing::debug!(
                month = %metrics.month_key,
                spending = metrics.current_month_spending,
                "Budget reconciliation completed"
            );
        }

        // T033: Record budget limit gauge (global, no billing_month label)
        if let Some(limit) = self.budget_config.monthly_limit_usd {
            metrics::gauge!("nexus_budget_limit_usd").set(limit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tokenizer::TokenizerRegistry;
    use crate::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model};
    use crate::routing::reconciler::intent::RoutingIntent;
    use crate::routing::RequestRequirements;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, AtomicU64};

    fn create_backend(id: &str, model_id: &str, backend_type: BackendType) -> Backend {
        Backend {
            id: id.to_string(),
            name: id.to_string(),
            url: format!("http://{}", id),
            backend_type,
            status: BackendStatus::Healthy,
            last_health_check: Utc::now(),
            last_error: None,
            models: vec![Model {
                id: model_id.to_string(),
                name: model_id.to_string(),
                context_length: 4096,
                supports_vision: false,
                supports_tools: false,
                supports_json_mode: false,
                max_output_tokens: None,
            }],
            priority: 1,
            pending_requests: AtomicU32::new(0),
            total_requests: AtomicU64::new(0),
            avg_latency_ms: AtomicU32::new(50),
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
        }
    }

    fn create_intent(model: &str, candidates: Vec<String>) -> RoutingIntent {
        RoutingIntent::new(
            "req-1".to_string(),
            model.to_string(),
            model.to_string(),
            RequestRequirements {
                model: model.to_string(),
                estimated_tokens: 1000,
                needs_vision: false,
                needs_tools: false,
                needs_json_mode: false,
            },
            candidates,
        )
    }

    fn budget_config(limit: Option<f64>, action: HardLimitAction) -> BudgetConfig {
        BudgetConfig {
            monthly_limit_usd: limit,
            soft_limit_percent: 75.0,
            hard_limit_action: action,
            reconciliation_interval_secs: 60,
        }
    }

    fn tokenizer_registry() -> Arc<TokenizerRegistry> {
        Arc::new(TokenizerRegistry::new().expect("Failed to create tokenizer registry"))
    }

    // === FR-016: Zero-config default ===

    #[test]
    fn no_budget_limit_passes_all_through() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("local", "llama3:8b", BackendType::Ollama))
            .unwrap();
        registry
            .add_backend(create_backend("cloud", "llama3:8b", BackendType::OpenAI))
            .unwrap();

        let state = Arc::new(DashMap::new());
        let reconciler = BudgetReconciler::new(
            Arc::clone(&registry),
            budget_config(None, HardLimitAction::BlockCloud),
            tokenizer_registry(),
            state,
        );

        let mut intent = create_intent("llama3:8b", vec!["local".into(), "cloud".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents.len(), 2);
        assert!(intent.excluded_agents.is_empty());
        assert_eq!(intent.budget_status, BudgetStatus::Normal);
    }

    // === FR-017: Cost estimation ===

    #[test]
    fn estimates_cost_for_known_model() {
        let registry = Arc::new(Registry::new());
        let state = Arc::new(DashMap::new());
        let reconciler = BudgetReconciler::new(
            Arc::clone(&registry),
            budget_config(Some(100.0), HardLimitAction::Warn),
            tokenizer_registry(),
            state,
        );

        let estimate = reconciler.estimate_cost("gpt-4-turbo", 1000);
        assert!(estimate.cost_usd > 0.0);
        assert_eq!(estimate.input_tokens, 1000);
        assert_eq!(estimate.estimated_output_tokens, 500);
    }

    #[test]
    fn estimates_zero_cost_for_unknown_model() {
        let registry = Arc::new(Registry::new());
        let state = Arc::new(DashMap::new());
        let reconciler = BudgetReconciler::new(
            Arc::clone(&registry),
            budget_config(Some(100.0), HardLimitAction::Warn),
            tokenizer_registry(),
            state,
        );

        let estimate = reconciler.estimate_cost("llama3:8b", 1000);
        assert_eq!(estimate.cost_usd, 0.0); // Local models have no pricing
    }

    // === FR-019: Budget status transitions ===

    #[test]
    fn budget_status_normal_below_soft_limit() {
        let registry = Arc::new(Registry::new());
        let state = Arc::new(DashMap::new());
        let reconciler = BudgetReconciler::new(
            Arc::clone(&registry),
            budget_config(Some(100.0), HardLimitAction::Warn),
            tokenizer_registry(),
            Arc::clone(&state),
        );

        // Spending: $50 of $100 = 50% < 75% soft limit
        reconciler.record_spending(50.0);
        assert_eq!(reconciler.calculate_budget_status(), BudgetStatus::Normal);
    }

    #[test]
    fn budget_status_soft_limit() {
        let registry = Arc::new(Registry::new());
        let state = Arc::new(DashMap::new());
        let reconciler = BudgetReconciler::new(
            Arc::clone(&registry),
            budget_config(Some(100.0), HardLimitAction::Warn),
            tokenizer_registry(),
            Arc::clone(&state),
        );

        // Spending: $80 of $100 = 80% > 75% soft limit
        reconciler.record_spending(80.0);
        assert_eq!(
            reconciler.calculate_budget_status(),
            BudgetStatus::SoftLimit
        );
    }

    #[test]
    fn budget_status_hard_limit() {
        let registry = Arc::new(Registry::new());
        let state = Arc::new(DashMap::new());
        let reconciler = BudgetReconciler::new(
            Arc::clone(&registry),
            budget_config(Some(100.0), HardLimitAction::Warn),
            tokenizer_registry(),
            Arc::clone(&state),
        );

        // Spending: $100 of $100 = 100% → hard limit
        reconciler.record_spending(100.0);
        assert_eq!(
            reconciler.calculate_budget_status(),
            BudgetStatus::HardLimit
        );
    }

    // === FR-020: SoftLimit doesn't exclude agents ===

    #[test]
    fn soft_limit_does_not_exclude_agents() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("local", "gpt-4-turbo", BackendType::Ollama))
            .unwrap();
        registry
            .add_backend(create_backend("cloud", "gpt-4-turbo", BackendType::OpenAI))
            .unwrap();

        let state = Arc::new(DashMap::new());
        let reconciler = BudgetReconciler::new(
            Arc::clone(&registry),
            budget_config(Some(100.0), HardLimitAction::BlockCloud),
            tokenizer_registry(),
            Arc::clone(&state),
        );

        reconciler.record_spending(80.0); // 80% → SoftLimit

        let mut intent = create_intent("gpt-4-turbo", vec!["local".into(), "cloud".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        // Both agents still candidates (SchedulerReconciler handles scoring)
        assert_eq!(intent.candidate_agents.len(), 2);
        assert_eq!(intent.budget_status, BudgetStatus::SoftLimit);
    }

    // === FR-021: HardLimit exclusion ===

    #[test]
    fn hard_limit_block_cloud_excludes_only_cloud() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("local", "gpt-4-turbo", BackendType::Ollama))
            .unwrap();
        registry
            .add_backend(create_backend("cloud", "gpt-4-turbo", BackendType::OpenAI))
            .unwrap();

        let state = Arc::new(DashMap::new());
        let reconciler = BudgetReconciler::new(
            Arc::clone(&registry),
            budget_config(Some(100.0), HardLimitAction::BlockCloud),
            tokenizer_registry(),
            Arc::clone(&state),
        );

        reconciler.record_spending(100.0); // 100% → HardLimit

        let mut intent = create_intent("gpt-4-turbo", vec!["local".into(), "cloud".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents, vec!["local"]);
        assert_eq!(intent.excluded_agents, vec!["cloud"]);
        assert_eq!(intent.budget_status, BudgetStatus::HardLimit);
    }

    #[test]
    fn hard_limit_block_all_excludes_everything() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("local", "gpt-4-turbo", BackendType::Ollama))
            .unwrap();
        registry
            .add_backend(create_backend("cloud", "gpt-4-turbo", BackendType::OpenAI))
            .unwrap();

        let state = Arc::new(DashMap::new());
        let reconciler = BudgetReconciler::new(
            Arc::clone(&registry),
            budget_config(Some(100.0), HardLimitAction::BlockAll),
            tokenizer_registry(),
            Arc::clone(&state),
        );

        reconciler.record_spending(100.0); // 100% → HardLimit

        let mut intent = create_intent("gpt-4-turbo", vec!["local".into(), "cloud".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert!(intent.candidate_agents.is_empty());
        assert_eq!(intent.excluded_agents.len(), 2);
        assert_eq!(intent.budget_status, BudgetStatus::HardLimit);
    }

    #[test]
    fn hard_limit_warn_allows_all() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("local", "gpt-4-turbo", BackendType::Ollama))
            .unwrap();
        registry
            .add_backend(create_backend("cloud", "gpt-4-turbo", BackendType::OpenAI))
            .unwrap();

        let state = Arc::new(DashMap::new());
        let reconciler = BudgetReconciler::new(
            Arc::clone(&registry),
            budget_config(Some(100.0), HardLimitAction::Warn),
            tokenizer_registry(),
            Arc::clone(&state),
        );

        reconciler.record_spending(100.0); // 100% → HardLimit

        let mut intent = create_intent("gpt-4-turbo", vec!["local".into(), "cloud".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents.len(), 2);
        assert!(intent.excluded_agents.is_empty());
        assert_eq!(intent.budget_status, BudgetStatus::HardLimit);
    }

    // === Cost estimate populated on intent ===

    #[test]
    fn cost_estimate_populated_on_intent() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("cloud", "gpt-4-turbo", BackendType::OpenAI))
            .unwrap();

        let state = Arc::new(DashMap::new());
        let reconciler = BudgetReconciler::new(
            Arc::clone(&registry),
            budget_config(Some(100.0), HardLimitAction::Warn),
            tokenizer_registry(),
            state,
        );

        let mut intent = create_intent("gpt-4-turbo", vec!["cloud".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.cost_estimate.input_tokens, 1000);
        assert_eq!(intent.cost_estimate.estimated_output_tokens, 500);
        assert!(intent.cost_estimate.cost_usd > 0.0);
    }

    // === Rejection reasons ===

    #[test]
    fn rejection_reason_includes_required_fields() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("cloud", "gpt-4-turbo", BackendType::OpenAI))
            .unwrap();

        let state = Arc::new(DashMap::new());
        let reconciler = BudgetReconciler::new(
            Arc::clone(&registry),
            budget_config(Some(100.0), HardLimitAction::BlockCloud),
            tokenizer_registry(),
            Arc::clone(&state),
        );

        reconciler.record_spending(100.0);

        let mut intent = create_intent("gpt-4-turbo", vec!["cloud".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.rejection_reasons.len(), 1);
        let reason = &intent.rejection_reasons[0];
        assert_eq!(reason.agent_id, "cloud");
        assert_eq!(reason.reconciler, "BudgetReconciler");
        assert!(reason.reason.contains("hard limit"));
        assert!(!reason.suggested_action.is_empty());
    }

    // === Record spending ===

    #[test]
    fn record_spending_accumulates() {
        let registry = Arc::new(Registry::new());
        let state = Arc::new(DashMap::new());
        let reconciler = BudgetReconciler::new(
            Arc::clone(&registry),
            budget_config(Some(100.0), HardLimitAction::Warn),
            tokenizer_registry(),
            Arc::clone(&state),
        );

        reconciler.record_spending(10.0);
        reconciler.record_spending(20.0);

        let metrics = state.get(GLOBAL_BUDGET_KEY).unwrap();
        assert!((metrics.current_month_spending - 30.0).abs() < f64::EPSILON);
    }

    #[test]
    fn record_spending_ignores_zero_or_negative() {
        let registry = Arc::new(Registry::new());
        let state = Arc::new(DashMap::new());
        let reconciler = BudgetReconciler::new(
            Arc::clone(&registry),
            budget_config(Some(100.0), HardLimitAction::Warn),
            tokenizer_registry(),
            Arc::clone(&state),
        );

        reconciler.record_spending(0.0);
        reconciler.record_spending(-5.0);

        let metrics = state.get(GLOBAL_BUDGET_KEY).unwrap();
        assert!((metrics.current_month_spending).abs() < f64::EPSILON);
    }

    // === Token count tiers ===

    #[test]
    fn token_count_tiers() {
        let registry = Arc::new(Registry::new());
        let state = Arc::new(DashMap::new());
        let reconciler = BudgetReconciler::new(
            Arc::clone(&registry),
            budget_config(None, HardLimitAction::Warn),
            tokenizer_registry(),
            state,
        );

        // gpt-4 has exact tokenizer (tier 0)
        assert_eq!(reconciler.estimate_cost("gpt-4", 500).token_count_tier, 0);
        assert_eq!(reconciler.estimate_cost("gpt-4", 5000).token_count_tier, 0);

        // claude has approximation tokenizer (tier 1)
        assert_eq!(
            reconciler
                .estimate_cost("claude-3-sonnet", 1000)
                .token_count_tier,
            1
        );

        // unknown models use heuristic (tier 2)
        assert_eq!(
            reconciler.estimate_cost("llama3:8b", 1000).token_count_tier,
            2
        );
    }

    // === Background reconciliation loop ===

    #[tokio::test]
    async fn reconciliation_loop_starts_and_stops() {
        let state = Arc::new(DashMap::new());
        state.insert(GLOBAL_BUDGET_KEY.to_string(), BudgetMetrics::new());

        let budget_config = budget_config(Some(100.0), HardLimitAction::Warn);
        let loop_task = BudgetReconciliationLoop::new(Arc::clone(&state), budget_config, 1);
        let cancel = CancellationToken::new();
        let handle = loop_task.start(cancel.clone());

        // Let it run for a bit
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify it updated the timestamp
        let metrics = state.get(GLOBAL_BUDGET_KEY).unwrap();
        assert!(!metrics.month_key.is_empty());
        drop(metrics);

        // Stop it
        cancel.cancel();
        let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
        assert!(result.is_ok());
    }

    // === BudgetConfig serde ===

    #[test]
    fn budget_config_defaults() {
        let config = BudgetConfig::default();
        assert!(config.monthly_limit_usd.is_none());
        assert!((config.soft_limit_percent - 75.0).abs() < f64::EPSILON);
        assert_eq!(config.hard_limit_action, HardLimitAction::Warn);
        assert_eq!(config.reconciliation_interval_secs, 60);
    }

    #[test]
    fn budget_config_serde_roundtrip() {
        let toml_str = r#"
            monthly_limit_usd = 50.0
            soft_limit_percent = 80.0
            hard_limit_action = "block_cloud"
            reconciliation_interval_secs = 30
        "#;
        let config: BudgetConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.monthly_limit_usd, Some(50.0));
        assert!((config.soft_limit_percent - 80.0).abs() < f64::EPSILON);
        assert_eq!(config.hard_limit_action, HardLimitAction::BlockCloud);
        assert_eq!(config.reconciliation_interval_secs, 30);
    }

    #[test]
    fn budget_config_partial_serde() {
        let toml_str = r#"
            monthly_limit_usd = 100.0
        "#;
        let config: BudgetConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.monthly_limit_usd, Some(100.0));
        assert!((config.soft_limit_percent - 75.0).abs() < f64::EPSILON); // default
        assert_eq!(config.hard_limit_action, HardLimitAction::Warn); // default
    }
}
