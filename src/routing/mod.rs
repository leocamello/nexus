//! Intelligent routing system for selecting optimal backends
//!
//! This module implements the routing logic that selects the best backend
//! for each request based on model requirements, backend capabilities, and
//! current system state.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::Arc;

pub mod error;
pub mod reconciler;
pub mod requirements;
pub mod scoring;
pub mod strategies; // Reconciler pipeline module

pub use error::RoutingError;
pub use requirements::RequestRequirements;
pub use scoring::{score_backend, ScoringWeights};
pub use strategies::RoutingStrategy;

use crate::agent::quality::QualityMetricsStore;
use crate::agent::tokenizer::TokenizerRegistry;
use crate::config::{BudgetConfig, PolicyMatcher, QualityConfig};
use crate::registry::{Backend, BackendStatus, Registry};
use crate::routing::reconciler::budget::BudgetMetrics;
use dashmap::DashMap;
use reconciler::budget::BudgetReconciler;
use reconciler::decision::RoutingDecision;
use reconciler::intent::RoutingIntent;
use reconciler::lifecycle::LifecycleReconciler;
use reconciler::privacy::PrivacyReconciler;
use reconciler::quality::QualityReconciler;
use reconciler::request_analyzer::RequestAnalyzer;
use reconciler::scheduler::SchedulerReconciler;
use reconciler::tier::TierReconciler;
use reconciler::ReconcilerPipeline;

/// Result of a successful routing decision
#[derive(Debug)]
pub struct RoutingResult {
    /// The selected backend
    pub backend: Arc<Backend>,
    /// The actual model name used (may differ if fallback)
    pub actual_model: String,
    /// True if a fallback model was used
    pub fallback_used: bool,
    /// Explanation of backend selection decision
    /// Examples: "highest_score:0.95", "round_robin:index_3", "only_healthy_backend"
    pub route_reason: String,
    /// Estimated cost in USD (F12: Cloud Backend Support)
    /// Populated for cloud backends with token counting capability
    pub cost_estimated: Option<f64>,
    /// Budget status at routing time (F14: Budget Management)
    pub budget_status: reconciler::intent::BudgetStatus,
    /// Budget utilization percentage at routing time (F14: Budget Management)
    pub budget_utilization: Option<f64>,
    /// Budget remaining in USD at routing time (F14: Budget Management)
    pub budget_remaining: Option<f64>,
}

/// Router selects the best backend for each request
#[allow(dead_code)] // Fields will be used in subsequent tasks
pub struct Router {
    /// Reference to backend registry
    registry: Arc<Registry>,

    /// Routing strategy to use
    strategy: RoutingStrategy,

    /// Scoring weights for smart strategy
    weights: ScoringWeights,

    /// Model aliases (alias → target)
    aliases: HashMap<String, String>,

    /// Fallback chains (model → [fallback1, fallback2, ...])
    fallbacks: HashMap<String, Vec<String>>,

    /// Round-robin counter for round-robin strategy (shared with pipeline)
    round_robin_counter: Arc<AtomicU64>,

    /// Pre-compiled traffic policy matcher for privacy enforcement
    policy_matcher: PolicyMatcher,

    /// Budget configuration for cost enforcement
    budget_config: BudgetConfig,

    /// Shared budget state for spending tracking
    budget_state: Arc<DashMap<String, BudgetMetrics>>,

    /// Tokenizer registry for accurate cost estimation (F14)
    tokenizer_registry: Arc<TokenizerRegistry>,

    /// Shared quality metrics store for quality-aware routing
    quality_store: Arc<QualityMetricsStore>,

    /// Quality configuration for thresholds
    quality_config: QualityConfig,

    /// Whether request queuing is enabled (T026)
    queue_enabled: bool,
}

impl Router {
    /// Create a new router with the given configuration
    pub fn new(
        registry: Arc<Registry>,
        strategy: RoutingStrategy,
        weights: ScoringWeights,
    ) -> Self {
        let tokenizer_registry =
            Arc::new(TokenizerRegistry::new().expect("Failed to initialize tokenizer registry"));
        let quality_config = QualityConfig::default();
        let quality_store = Arc::new(QualityMetricsStore::new(quality_config.clone()));
        Self {
            registry,
            strategy,
            weights,
            aliases: HashMap::new(),
            fallbacks: HashMap::new(),
            round_robin_counter: Arc::new(AtomicU64::new(0)),
            policy_matcher: PolicyMatcher::default(),
            budget_config: BudgetConfig::default(),
            budget_state: Arc::new(DashMap::new()),
            tokenizer_registry,
            quality_store,
            quality_config,
            queue_enabled: false,
        }
    }

    /// Create a new router with aliases and fallbacks
    pub fn with_aliases_and_fallbacks(
        registry: Arc<Registry>,
        strategy: RoutingStrategy,
        weights: ScoringWeights,
        aliases: HashMap<String, String>,
        fallbacks: HashMap<String, Vec<String>>,
    ) -> Self {
        let tokenizer_registry =
            Arc::new(TokenizerRegistry::new().expect("Failed to initialize tokenizer registry"));
        let quality_config = QualityConfig::default();
        let quality_store = Arc::new(QualityMetricsStore::new(quality_config.clone()));
        Self {
            registry,
            strategy,
            weights,
            aliases,
            fallbacks,
            round_robin_counter: Arc::new(AtomicU64::new(0)),
            policy_matcher: PolicyMatcher::default(),
            budget_config: BudgetConfig::default(),
            budget_state: Arc::new(DashMap::new()),
            tokenizer_registry,
            quality_store,
            quality_config,
            queue_enabled: false,
        }
    }

    /// Create a new router with aliases, fallbacks, and traffic policies
    pub fn with_aliases_fallbacks_and_policies(
        registry: Arc<Registry>,
        strategy: RoutingStrategy,
        weights: ScoringWeights,
        aliases: HashMap<String, String>,
        fallbacks: HashMap<String, Vec<String>>,
        policy_matcher: PolicyMatcher,
        quality_config: QualityConfig,
    ) -> Self {
        let tokenizer_registry =
            Arc::new(TokenizerRegistry::new().expect("Failed to initialize tokenizer registry"));
        let quality_store = Arc::new(QualityMetricsStore::new(quality_config.clone()));
        Self {
            registry,
            strategy,
            weights,
            aliases,
            fallbacks,
            round_robin_counter: Arc::new(AtomicU64::new(0)),
            policy_matcher,
            budget_config: BudgetConfig::default(),
            budget_state: Arc::new(DashMap::new()),
            tokenizer_registry,
            quality_store,
            quality_config,
            queue_enabled: false,
        }
    }

    /// Create a new router with full configuration including budget
    #[allow(clippy::too_many_arguments)]
    pub fn with_full_config(
        registry: Arc<Registry>,
        strategy: RoutingStrategy,
        weights: ScoringWeights,
        aliases: HashMap<String, String>,
        fallbacks: HashMap<String, Vec<String>>,
        policy_matcher: PolicyMatcher,
        budget_config: BudgetConfig,
        budget_state: Arc<DashMap<String, BudgetMetrics>>,
    ) -> Self {
        let tokenizer_registry =
            Arc::new(TokenizerRegistry::new().expect("Failed to initialize tokenizer registry"));
        let quality_config = QualityConfig::default();
        let quality_store = Arc::new(QualityMetricsStore::new(quality_config.clone()));
        Self {
            registry,
            strategy,
            weights,
            aliases,
            fallbacks,
            round_robin_counter: Arc::new(AtomicU64::new(0)),
            policy_matcher,
            budget_config,
            budget_state,
            tokenizer_registry,
            quality_store,
            quality_config,
            queue_enabled: false,
        }
    }

    /// Resolve model aliases with chaining support (max 3 levels)
    fn resolve_alias(&self, model: &str) -> String {
        let mut current = model.to_string();
        let mut depth = 0;
        const MAX_DEPTH: usize = 3;

        while depth < MAX_DEPTH {
            match self.aliases.get(&current) {
                Some(target) => {
                    tracing::debug!(
                        from = %current,
                        to = %target,
                        depth = depth + 1,
                        "Resolved alias"
                    );
                    current = target.clone();
                    depth += 1;
                }
                None => break,
            }
        }

        if depth > 0 {
            tracing::debug!(
                original = %model,
                resolved = %current,
                chain_depth = depth,
                "Alias resolution complete"
            );
        }

        current
    }

    /// Get fallback chain for a model
    fn get_fallbacks(&self, model: &str) -> Vec<String> {
        self.fallbacks.get(model).cloned().unwrap_or_default()
    }

    /// Build a reconciler pipeline for the given model
    /// Order: RequestAnalyzer → LifecycleReconciler → PrivacyReconciler → BudgetReconciler
    ///        → TierReconciler → QualityReconciler → SchedulerReconciler
    fn build_pipeline(&self, model_aliases: HashMap<String, String>) -> ReconcilerPipeline {
        let analyzer = RequestAnalyzer::new(model_aliases, Arc::clone(&self.registry));
        let lifecycle = LifecycleReconciler::new(Arc::clone(&self.registry));
        let privacy =
            PrivacyReconciler::new(Arc::clone(&self.registry), self.policy_matcher.clone());
        let budget = BudgetReconciler::new(
            Arc::clone(&self.registry),
            self.budget_config.clone(),
            Arc::clone(&self.tokenizer_registry),
            Arc::clone(&self.budget_state),
        );
        let tier = TierReconciler::new(Arc::clone(&self.registry), self.policy_matcher.clone());
        let quality =
            QualityReconciler::new(Arc::clone(&self.quality_store), self.quality_config.clone());
        let scheduler = SchedulerReconciler::new(
            Arc::clone(&self.registry),
            self.strategy,
            self.weights,
            Arc::clone(&self.round_robin_counter),
            Arc::clone(&self.quality_store),
            self.quality_config.clone(),
        );
        ReconcilerPipeline::with_queue(
            vec![
                Box::new(analyzer),
                Box::new(lifecycle),
                Box::new(privacy),
                Box::new(budget),
                Box::new(tier),
                Box::new(quality),
                Box::new(scheduler),
            ],
            self.queue_enabled,
        )
    }

    /// Run the pipeline for a specific model (primary or fallback) and return the decision
    fn run_pipeline_for_model(
        &self,
        requirements: &RequestRequirements,
        model: &str,
        tier_enforcement_mode: Option<crate::routing::reconciler::intent::TierEnforcementMode>,
    ) -> Result<RoutingDecision, RoutingError> {
        // Build a pipeline with aliases that will resolve to the target model directly
        // (alias resolution already happened in the caller)
        let mut intent = RoutingIntent::new(
            format!(
                "req-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ),
            model.to_string(),
            model.to_string(), // pre-resolved
            requirements.clone(),
            vec![], // will be populated by RequestAnalyzer
        );

        // Set tier enforcement mode from request headers (T032)
        if let Some(mode) = tier_enforcement_mode {
            intent.tier_enforcement_mode = mode;
        }

        // Build pipeline with empty aliases (model is already resolved)
        let mut pipeline = self.build_pipeline(HashMap::new());
        pipeline.execute(&mut intent)
    }

    /// Select the best backend for the given requirements
    ///
    /// # Arguments
    ///
    /// * `requirements` - The extracted requirements from the request
    /// * `tier_enforcement_mode` - Optional tier enforcement mode from request headers (T032)
    ///
    /// # Returns
    ///
    /// Returns a `RoutingResult` with the selected backend and routing metadata,
    /// or a `RoutingError` if no suitable backend is available.
    pub fn select_backend(
        &self,
        requirements: &RequestRequirements,
        tier_enforcement_mode: Option<crate::routing::reconciler::intent::TierEnforcementMode>,
    ) -> Result<RoutingResult, RoutingError> {
        // Step 1: Resolve alias using existing logic (reused by pipeline too)
        let model = self.resolve_alias(&requirements.model);

        // Step 2: Check if model exists at all (for proper error messages)
        let all_backends = self.registry.get_backends_for_model(&model);
        let model_exists = !all_backends.is_empty();

        // Step 3: Run pipeline for primary model
        let decision = self.run_pipeline_for_model(requirements, &model, tier_enforcement_mode)?;

        if let RoutingDecision::Route {
            agent_id,
            model: resolved_model,
            reason,
            cost_estimate,
        } = decision
        {
            let backend = self.registry.get_backend(&agent_id).ok_or_else(|| {
                RoutingError::NoHealthyBackend {
                    model: model.clone(),
                }
            })?;

            tracing::debug!(
                backend = %backend.name,
                backend_type = ?backend.backend_type,
                model = %resolved_model,
                route_reason = %reason,
                "routing decision made"
            );

            // Calculate budget utilization from current state
            let (budget_status, budget_utilization, budget_remaining) =
                self.get_budget_status_and_utilization();

            return Ok(RoutingResult {
                backend: Arc::new(backend),
                actual_model: resolved_model,
                fallback_used: false,
                route_reason: reason,
                cost_estimated: Some(cost_estimate.cost_usd),
                budget_status,
                budget_utilization,
                budget_remaining,
            });
        }

        // T026: Handle Queue decision — propagate as RoutingError::Queue
        if let RoutingDecision::Queue {
            reason,
            estimated_wait_ms,
            ..
        } = &decision
        {
            return Err(RoutingError::Queue {
                reason: reason.clone(),
                estimated_wait_ms: *estimated_wait_ms,
            });
        }

        // Step 4: Try fallback chain
        let fallbacks = self.get_fallbacks(&model);
        for fallback_model in &fallbacks {
            let decision =
                self.run_pipeline_for_model(requirements, fallback_model, tier_enforcement_mode)?;

            if let RoutingDecision::Route {
                agent_id,
                cost_estimate,
                reason,
                ..
            } = decision
            {
                let backend = self.registry.get_backend(&agent_id).ok_or_else(|| {
                    RoutingError::NoHealthyBackend {
                        model: fallback_model.clone(),
                    }
                })?;

                let route_reason = format!("fallback:{}:{}", model, reason);

                tracing::warn!(
                    requested_model = %model,
                    fallback_model = %fallback_model,
                    backend = %backend.name,
                    "Using fallback model"
                );

                // Calculate budget utilization from current state
                let (budget_status, budget_utilization, budget_remaining) =
                    self.get_budget_status_and_utilization();

                return Ok(RoutingResult {
                    backend: Arc::new(backend),
                    actual_model: fallback_model.clone(),
                    fallback_used: true,
                    route_reason,
                    cost_estimated: Some(cost_estimate.cost_usd),
                    budget_status,
                    budget_utilization,
                    budget_remaining,
                });
            }
        }

        // Step 5: All attempts failed - produce the right error
        if !fallbacks.is_empty() {
            let mut chain = vec![model.clone()];
            chain.extend(fallbacks);
            Err(RoutingError::FallbackChainExhausted { chain })
        } else if model_exists {
            Err(RoutingError::NoHealthyBackend {
                model: model.clone(),
            })
        } else {
            Err(RoutingError::ModelNotFound {
                model: requirements.model.clone(),
            })
        }
    }

    /// Select backend using smart scoring
    #[allow(dead_code)] // Retained for potential direct use; scoring now in SchedulerReconciler
    fn select_smart(&self, candidates: &[Backend]) -> Backend {
        let best = candidates
            .iter()
            .max_by_key(|backend| {
                let priority = backend.priority as u32;
                let pending = backend
                    .pending_requests
                    .load(std::sync::atomic::Ordering::Relaxed);
                let latency = backend
                    .avg_latency_ms
                    .load(std::sync::atomic::Ordering::Relaxed);
                score_backend(priority, pending, latency, &self.weights)
            })
            .unwrap();

        // Create a new Backend by copying all fields (atomics are cloned by their current values)
        Backend {
            id: best.id.clone(),
            name: best.name.clone(),
            url: best.url.clone(),
            backend_type: best.backend_type,
            status: best.status,
            last_health_check: best.last_health_check,
            last_error: best.last_error.clone(),
            models: best.models.clone(),
            priority: best.priority,
            pending_requests: AtomicU32::new(
                best.pending_requests
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            total_requests: AtomicU64::new(
                best.total_requests
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            avg_latency_ms: AtomicU32::new(
                best.avg_latency_ms
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            discovery_source: best.discovery_source,
            metadata: best.metadata.clone(),
            current_operation: best.current_operation.clone(),
        }
    }

    /// Select backend using priority-only
    #[allow(dead_code)] // Retained for potential direct use; scoring now in SchedulerReconciler
    fn select_priority_only(&self, candidates: &[Backend]) -> Backend {
        let best = candidates
            .iter()
            .min_by_key(|backend| backend.priority)
            .unwrap();

        // Create a new Backend snapshot
        Backend {
            id: best.id.clone(),
            name: best.name.clone(),
            url: best.url.clone(),
            backend_type: best.backend_type,
            status: best.status,
            last_health_check: best.last_health_check,
            last_error: best.last_error.clone(),
            models: best.models.clone(),
            priority: best.priority,
            pending_requests: AtomicU32::new(
                best.pending_requests
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            total_requests: AtomicU64::new(
                best.total_requests
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            avg_latency_ms: AtomicU32::new(
                best.avg_latency_ms
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            discovery_source: best.discovery_source,
            metadata: best.metadata.clone(),
            current_operation: best.current_operation.clone(),
        }
    }

    /// Select backend using random
    #[allow(dead_code)] // Retained for potential direct use; scoring now in SchedulerReconciler
    fn select_random(&self, candidates: &[Backend]) -> Backend {
        use std::collections::hash_map::RandomState;
        use std::hash::BuildHasher;

        // Use RandomState to generate a random index
        let random_state = RandomState::new();
        let random_value = random_state.hash_one(std::time::SystemTime::now());
        let index = (random_value as usize) % candidates.len();
        let best = &candidates[index];

        // Create a new Backend snapshot
        Backend {
            id: best.id.clone(),
            name: best.name.clone(),
            url: best.url.clone(),
            backend_type: best.backend_type,
            status: best.status,
            last_health_check: best.last_health_check,
            last_error: best.last_error.clone(),
            models: best.models.clone(),
            priority: best.priority,
            pending_requests: AtomicU32::new(
                best.pending_requests
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            total_requests: AtomicU64::new(
                best.total_requests
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            avg_latency_ms: AtomicU32::new(
                best.avg_latency_ms
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            discovery_source: best.discovery_source,
            metadata: best.metadata.clone(),
            current_operation: best.current_operation.clone(),
        }
    }

    /// Filter candidates by model, health, and capabilities
    #[allow(dead_code)] // Legacy pre-pipeline impl; capability filtering now in SchedulerReconciler
    fn filter_candidates(&self, model: &str, requirements: &RequestRequirements) -> Vec<Backend> {
        // Get all backends that have this model
        let mut candidates = self.registry.get_backends_for_model(model);

        // Filter by health status
        candidates.retain(|backend| backend.status == BackendStatus::Healthy);

        // Filter by capabilities
        candidates.retain(|backend| {
            // Find the model in this backend
            if let Some(model_info) = backend.models.iter().find(|m| m.id == model) {
                // Check vision capability
                if requirements.needs_vision && !model_info.supports_vision {
                    return false;
                }

                // Check tools capability
                if requirements.needs_tools && !model_info.supports_tools {
                    return false;
                }

                // Check JSON mode capability
                if requirements.needs_json_mode && !model_info.supports_json_mode {
                    return false;
                }

                // Check context length
                if requirements.estimated_tokens > model_info.context_length {
                    return false;
                }

                true
            } else {
                // Model not found in this backend (shouldn't happen)
                false
            }
        });

        candidates
    }

    /// Get reference to the budget configuration (F14).
    pub fn budget_config(&self) -> &BudgetConfig {
        &self.budget_config
    }

    /// Get reference to the budget state (F14).
    pub fn budget_state(&self) -> &Arc<DashMap<String, BudgetMetrics>> {
        &self.budget_state
    }

    /// Get reference to the quality metrics store.
    pub fn quality_store(&self) -> &Arc<QualityMetricsStore> {
        &self.quality_store
    }

    /// Set whether request queuing is enabled (T026).
    pub fn set_queue_enabled(&mut self, enabled: bool) {
        self.queue_enabled = enabled;
    }

    /// Get current budget status and utilization percentage (F14).
    ///
    /// Returns (BudgetStatus, Option<f64>, Option<f64>) where:
    /// - First f64 is utilization percentage
    /// - Second f64 is remaining budget in USD
    ///
    /// Returns None for both if no monthly limit is configured.
    fn get_budget_status_and_utilization(
        &self,
    ) -> (reconciler::intent::BudgetStatus, Option<f64>, Option<f64>) {
        use reconciler::budget::GLOBAL_BUDGET_KEY;
        use reconciler::intent::BudgetStatus;

        let monthly_limit = match self.budget_config.monthly_limit_usd {
            Some(limit) if limit > 0.0 => limit,
            _ => return (BudgetStatus::Normal, None, None),
        };

        let current_spending = self
            .budget_state
            .get(GLOBAL_BUDGET_KEY)
            .map(|m| m.current_month_spending)
            .unwrap_or(0.0);

        let utilization_percent = (current_spending / monthly_limit) * 100.0;
        let remaining = (monthly_limit - current_spending).max(0.0);
        let soft_threshold = self.budget_config.soft_limit_percent;

        let status = if utilization_percent >= 100.0 {
            BudgetStatus::HardLimit
        } else if utilization_percent >= soft_threshold {
            BudgetStatus::SoftLimit
        } else {
            BudgetStatus::Normal
        };

        (status, Some(utilization_percent), Some(remaining))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_strategy_default_is_smart() {
        assert_eq!(RoutingStrategy::default(), RoutingStrategy::Smart);
    }

    #[test]
    fn routing_strategy_from_str() {
        assert_eq!(
            "smart".parse::<RoutingStrategy>().unwrap(),
            RoutingStrategy::Smart
        );
        assert_eq!(
            "round_robin".parse::<RoutingStrategy>().unwrap(),
            RoutingStrategy::RoundRobin
        );
        assert_eq!(
            "priority_only".parse::<RoutingStrategy>().unwrap(),
            RoutingStrategy::PriorityOnly
        );
        assert_eq!(
            "random".parse::<RoutingStrategy>().unwrap(),
            RoutingStrategy::Random
        );
    }

    #[test]
    fn routing_strategy_from_str_case_insensitive() {
        assert_eq!(
            "Smart".parse::<RoutingStrategy>().unwrap(),
            RoutingStrategy::Smart
        );
        assert_eq!(
            "ROUND_ROBIN".parse::<RoutingStrategy>().unwrap(),
            RoutingStrategy::RoundRobin
        );
    }

    #[test]
    fn routing_strategy_from_str_invalid() {
        assert!("invalid".parse::<RoutingStrategy>().is_err());
    }

    #[test]
    fn routing_strategy_display() {
        assert_eq!(RoutingStrategy::Smart.to_string(), "smart");
        assert_eq!(RoutingStrategy::RoundRobin.to_string(), "round_robin");
        assert_eq!(RoutingStrategy::PriorityOnly.to_string(), "priority_only");
        assert_eq!(RoutingStrategy::Random.to_string(), "random");
    }

    #[test]
    fn routing_strategy_display_roundtrips() {
        for strategy in &[
            RoutingStrategy::Smart,
            RoutingStrategy::RoundRobin,
            RoutingStrategy::PriorityOnly,
            RoutingStrategy::Random,
        ] {
            let s = strategy.to_string();
            let parsed: RoutingStrategy = s.parse().unwrap();
            assert_eq!(*strategy, parsed);
        }
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;
    use crate::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model};
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, AtomicU64};

    fn create_test_backend(
        id: &str,
        name: &str,
        status: BackendStatus,
        models: Vec<Model>,
    ) -> Backend {
        Backend {
            id: id.to_string(),
            name: name.to_string(),
            url: format!("http://{}", name),
            backend_type: BackendType::Ollama,
            status,
            last_health_check: Utc::now(),
            last_error: None,
            models,
            priority: 1,
            pending_requests: AtomicU32::new(0),
            total_requests: AtomicU64::new(0),
            avg_latency_ms: AtomicU32::new(50),
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
            current_operation: None,
        }
    }

    fn create_test_model(
        id: &str,
        context_length: u32,
        supports_vision: bool,
        supports_tools: bool,
    ) -> Model {
        Model {
            id: id.to_string(),
            name: id.to_string(),
            context_length,
            supports_vision,
            supports_tools,
            supports_json_mode: false,
            max_output_tokens: None,
        }
    }

    fn create_test_router(backends: Vec<Backend>) -> Router {
        let registry = Arc::new(Registry::new());
        for backend in backends {
            registry.add_backend(backend).unwrap();
        }

        Router::new(registry, RoutingStrategy::Smart, ScoringWeights::default())
    }

    #[test]
    fn filters_by_model_name() {
        let backends = vec![
            create_test_backend(
                "backend_a",
                "Backend A",
                BackendStatus::Healthy,
                vec![create_test_model("llama3:8b", 4096, false, false)],
            ),
            create_test_backend(
                "backend_b",
                "Backend B",
                BackendStatus::Healthy,
                vec![create_test_model("mistral:7b", 4096, false, false)],
            ),
        ];

        let router = create_test_router(backends);
        let requirements = RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let candidates = router.filter_candidates("llama3:8b", &requirements);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "Backend A");
    }

    #[test]
    fn filters_out_unhealthy_backends() {
        let backends = vec![
            create_test_backend(
                "backend_a",
                "Backend A",
                BackendStatus::Healthy,
                vec![create_test_model("llama3:8b", 4096, false, false)],
            ),
            create_test_backend(
                "backend_b",
                "Backend B",
                BackendStatus::Unhealthy,
                vec![create_test_model("llama3:8b", 4096, false, false)],
            ),
        ];

        let router = create_test_router(backends);
        let requirements = RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let candidates = router.filter_candidates("llama3:8b", &requirements);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "Backend A");
    }

    #[test]
    fn filters_by_vision_capability() {
        let backends = vec![
            create_test_backend(
                "backend_a",
                "Backend A",
                BackendStatus::Healthy,
                vec![create_test_model("llama3:8b", 4096, false, false)],
            ),
            create_test_backend(
                "backend_b",
                "Backend B",
                BackendStatus::Healthy,
                vec![create_test_model("llama3:8b", 4096, true, false)],
            ),
        ];

        let router = create_test_router(backends);
        let requirements = RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 100,
            needs_vision: true,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let candidates = router.filter_candidates("llama3:8b", &requirements);
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].models[0].supports_vision);
    }

    #[test]
    fn filters_by_context_length() {
        let backends = vec![
            create_test_backend(
                "backend_a",
                "Backend A",
                BackendStatus::Healthy,
                vec![create_test_model("llama3:8b", 4096, false, false)],
            ),
            create_test_backend(
                "backend_b",
                "Backend B",
                BackendStatus::Healthy,
                vec![create_test_model("llama3:8b", 128000, false, false)],
            ),
        ];

        let router = create_test_router(backends);
        let requirements = RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 10000,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let candidates = router.filter_candidates("llama3:8b", &requirements);
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].models[0].context_length >= 10000);
    }

    #[test]
    fn returns_empty_when_no_match() {
        let backends = vec![create_test_backend(
            "backend_a",
            "Backend A",
            BackendStatus::Healthy,
            vec![create_test_model("llama3:8b", 4096, false, false)],
        )];

        let router = create_test_router(backends);
        let requirements = RequestRequirements {
            model: "nonexistent".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let candidates = router.filter_candidates("nonexistent", &requirements);
        assert!(candidates.is_empty());
    }

    #[test]
    fn filters_by_tools_capability() {
        let backends = vec![
            create_test_backend(
                "backend_a",
                "Backend A",
                BackendStatus::Healthy,
                vec![create_test_model("llama3:8b", 4096, false, false)],
            ),
            create_test_backend(
                "backend_b",
                "Backend B",
                BackendStatus::Healthy,
                vec![create_test_model("llama3:8b", 4096, false, true)],
            ),
        ];

        let router = create_test_router(backends);
        let requirements = RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: true,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let candidates = router.filter_candidates("llama3:8b", &requirements);
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].models[0].supports_tools);
    }

    #[test]
    fn filters_by_json_mode_capability() {
        let model_no_json = Model {
            id: "llama3:8b".to_string(),
            name: "llama3:8b".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        };
        let model_with_json = Model {
            id: "llama3:8b".to_string(),
            name: "llama3:8b".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: true,
            max_output_tokens: None,
        };

        let backends = vec![
            create_test_backend(
                "backend_a",
                "Backend A",
                BackendStatus::Healthy,
                vec![model_no_json],
            ),
            create_test_backend(
                "backend_b",
                "Backend B",
                BackendStatus::Healthy,
                vec![model_with_json],
            ),
        ];

        let router = create_test_router(backends);
        let requirements = RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: true,
            prefers_streaming: false,
        };

        let candidates = router.filter_candidates("llama3:8b", &requirements);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "Backend B");
    }

    #[test]
    fn filters_by_multiple_capabilities() {
        let full_model = Model {
            id: "llama3:8b".to_string(),
            name: "llama3:8b".to_string(),
            context_length: 128000,
            supports_vision: true,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: None,
        };

        let backends = vec![
            create_test_backend(
                "backend_a",
                "Backend A",
                BackendStatus::Healthy,
                vec![create_test_model("llama3:8b", 4096, false, false)],
            ),
            create_test_backend(
                "backend_b",
                "Backend B",
                BackendStatus::Healthy,
                vec![full_model],
            ),
        ];

        let router = create_test_router(backends);
        let requirements = RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 50000,
            needs_vision: true,
            needs_tools: true,
            needs_json_mode: true,
            prefers_streaming: false,
        };

        let candidates = router.filter_candidates("llama3:8b", &requirements);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "Backend B");
    }
}

#[cfg(test)]
mod smart_strategy_tests {
    use super::*;
    use crate::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model};
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, AtomicU64};

    fn create_test_backend_with_state(
        id: &str,
        name: &str,
        priority: i32,
        pending_requests: u32,
        avg_latency_ms: u32,
    ) -> Backend {
        Backend {
            id: id.to_string(),
            name: name.to_string(),
            url: format!("http://{}", name),
            backend_type: BackendType::Ollama,
            status: BackendStatus::Healthy,
            last_health_check: Utc::now(),
            last_error: None,
            models: vec![Model {
                id: "llama3:8b".to_string(),
                name: "llama3:8b".to_string(),
                context_length: 4096,
                supports_vision: false,
                supports_tools: false,
                supports_json_mode: false,
                max_output_tokens: None,
            }],
            priority,
            pending_requests: AtomicU32::new(pending_requests),
            total_requests: AtomicU64::new(0),
            avg_latency_ms: AtomicU32::new(avg_latency_ms),
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
            current_operation: None,
        }
    }

    fn create_test_router(backends: Vec<Backend>) -> Router {
        let registry = Arc::new(Registry::new());
        for backend in backends {
            registry.add_backend(backend).unwrap();
        }

        Router::new(registry, RoutingStrategy::Smart, ScoringWeights::default())
    }

    #[test]
    fn smart_selects_highest_score() {
        let backends = vec![
            // Backend A: high priority (1), no load, low latency → high score
            create_test_backend_with_state("backend_a", "Backend A", 1, 0, 50),
            // Backend B: low priority (10), high load, high latency → low score
            create_test_backend_with_state("backend_b", "Backend B", 10, 50, 500),
        ];

        let router = create_test_router(backends);
        let requirements = RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let result = router.select_backend(&requirements, None).unwrap();
        assert_eq!(result.backend.name, "Backend A");
    }

    #[test]
    fn smart_considers_load() {
        let backends = vec![
            // Both same priority and latency, but different load
            create_test_backend_with_state("backend_a", "Backend A", 5, 0, 100),
            create_test_backend_with_state("backend_b", "Backend B", 5, 50, 100),
        ];

        let router = create_test_router(backends);
        let requirements = RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let result = router.select_backend(&requirements, None).unwrap();
        assert_eq!(result.backend.name, "Backend A"); // Lower load
    }

    #[test]
    fn smart_considers_latency() {
        let backends = vec![
            // Same priority and load, but different latency
            create_test_backend_with_state("backend_a", "Backend A", 5, 10, 50),
            create_test_backend_with_state("backend_b", "Backend B", 5, 10, 500),
        ];

        let router = create_test_router(backends);
        let requirements = RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let result = router.select_backend(&requirements, None).unwrap();
        assert_eq!(result.backend.name, "Backend A"); // Lower latency
    }

    #[test]
    fn returns_error_when_no_candidates() {
        let backends = vec![create_test_backend_with_state(
            "backend_a",
            "Backend A",
            1,
            0,
            50,
        )];

        let router = create_test_router(backends);
        let requirements = RequestRequirements {
            model: "nonexistent".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let result = router.select_backend(&requirements, None);
        assert!(matches!(result, Err(RoutingError::ModelNotFound { .. })));
    }
}

#[cfg(test)]
mod other_strategies_tests {
    use super::*;
    use crate::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model};
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, AtomicU64};

    fn create_test_backend_simple(id: &str, name: &str, priority: i32) -> Backend {
        Backend {
            id: id.to_string(),
            name: name.to_string(),
            url: format!("http://{}", name),
            backend_type: BackendType::Ollama,
            status: BackendStatus::Healthy,
            last_health_check: Utc::now(),
            last_error: None,
            models: vec![Model {
                id: "llama3:8b".to_string(),
                name: "llama3:8b".to_string(),
                context_length: 4096,
                supports_vision: false,
                supports_tools: false,
                supports_json_mode: false,
                max_output_tokens: None,
            }],
            priority,
            pending_requests: AtomicU32::new(0),
            total_requests: AtomicU64::new(0),
            avg_latency_ms: AtomicU32::new(50),
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
            current_operation: None,
        }
    }

    fn create_test_router_with_strategy(
        backends: Vec<Backend>,
        strategy: RoutingStrategy,
    ) -> Router {
        let registry = Arc::new(Registry::new());
        for backend in backends {
            registry.add_backend(backend).unwrap();
        }

        Router::new(registry, strategy, ScoringWeights::default())
    }

    #[test]
    fn round_robin_cycles_through_backends() {
        let backends = vec![
            create_test_backend_simple("backend_a", "Backend A", 1),
            create_test_backend_simple("backend_b", "Backend B", 1),
            create_test_backend_simple("backend_c", "Backend C", 1),
        ];

        let router = create_test_router_with_strategy(backends, RoutingStrategy::RoundRobin);
        let requirements = RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        // Should cycle through: A, B, C, A, B, C
        let names: Vec<String> = (0..6)
            .map(|_| {
                router
                    .select_backend(&requirements, None)
                    .unwrap()
                    .backend
                    .name
                    .clone()
            })
            .collect();

        // Verify round-robin pattern
        assert_eq!(names[0], "Backend A");
        assert_eq!(names[1], "Backend B");
        assert_eq!(names[2], "Backend C");
        assert_eq!(names[3], "Backend A");
        assert_eq!(names[4], "Backend B");
        assert_eq!(names[5], "Backend C");
    }

    #[test]
    fn priority_only_selects_lowest_priority() {
        let backends = vec![
            create_test_backend_simple("backend_a", "Backend A", 10),
            create_test_backend_simple("backend_b", "Backend B", 1),
            create_test_backend_simple("backend_c", "Backend C", 5),
        ];

        let router = create_test_router_with_strategy(backends, RoutingStrategy::PriorityOnly);
        let requirements = RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        // Should always select Backend B (priority 1)
        for _ in 0..5 {
            let result = router.select_backend(&requirements, None).unwrap();
            assert_eq!(result.backend.name, "Backend B");
        }
    }

    #[test]
    fn random_selects_from_candidates() {
        let backends = vec![
            create_test_backend_simple("backend_a", "Backend A", 1),
            create_test_backend_simple("backend_b", "Backend B", 1),
            create_test_backend_simple("backend_c", "Backend C", 1),
        ];

        let router = create_test_router_with_strategy(backends, RoutingStrategy::Random);
        let requirements = RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        // Should select from all three backends over many iterations
        let mut selected = HashMap::new();
        for _ in 0..30 {
            let result = router.select_backend(&requirements, None).unwrap();
            *selected.entry(result.backend.name.clone()).or_insert(0) += 1;
        }

        // All three backends should be selected at least once
        assert!(selected.contains_key("Backend A"));
        assert!(selected.contains_key("Backend B"));
        assert!(selected.contains_key("Backend C"));
    }
}

#[cfg(test)]
mod alias_and_fallback_tests {
    use super::*;
    use crate::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model};
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, AtomicU64};

    fn create_test_backend_with_model(id: &str, name: &str, model_id: &str) -> Backend {
        Backend {
            id: id.to_string(),
            name: name.to_string(),
            url: format!("http://{}", name),
            backend_type: BackendType::Ollama,
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
            current_operation: None,
        }
    }

    #[test]
    fn resolves_alias_transparently() {
        let backends = vec![create_test_backend_with_model(
            "backend_a",
            "Backend A",
            "llama3:70b",
        )];

        let registry = Arc::new(Registry::new());
        for backend in backends {
            registry.add_backend(backend).unwrap();
        }

        let mut aliases = HashMap::new();
        aliases.insert("gpt-4".to_string(), "llama3:70b".to_string());

        let router = Router::with_aliases_and_fallbacks(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            aliases,
            HashMap::new(),
        );

        let requirements = RequestRequirements {
            model: "gpt-4".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let result = router.select_backend(&requirements, None).unwrap();
        assert_eq!(result.backend.name, "Backend A");
    }

    #[test]
    fn uses_fallback_when_primary_unavailable() {
        let backends = vec![create_test_backend_with_model(
            "backend_a",
            "Backend A",
            "mistral:7b",
        )];

        let registry = Arc::new(Registry::new());
        for backend in backends {
            registry.add_backend(backend).unwrap();
        }

        let mut fallbacks = HashMap::new();
        fallbacks.insert(
            "llama3:70b".to_string(),
            vec!["llama3:8b".to_string(), "mistral:7b".to_string()],
        );

        let router = Router::with_aliases_and_fallbacks(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            HashMap::new(),
            fallbacks,
        );

        let requirements = RequestRequirements {
            model: "llama3:70b".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let result = router.select_backend(&requirements, None).unwrap();
        assert_eq!(result.backend.name, "Backend A");
    }

    #[test]
    fn exhausts_fallback_chain() {
        let backends = vec![create_test_backend_with_model(
            "backend_a",
            "Backend A",
            "some-other-model",
        )];

        let registry = Arc::new(Registry::new());
        for backend in backends {
            registry.add_backend(backend).unwrap();
        }

        let mut fallbacks = HashMap::new();
        fallbacks.insert(
            "llama3:70b".to_string(),
            vec!["llama3:8b".to_string(), "mistral:7b".to_string()],
        );

        let router = Router::with_aliases_and_fallbacks(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            HashMap::new(),
            fallbacks,
        );

        let requirements = RequestRequirements {
            model: "llama3:70b".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let result = router.select_backend(&requirements, None);
        assert!(matches!(
            result,
            Err(RoutingError::FallbackChainExhausted { .. })
        ));
    }

    #[test]
    fn alias_then_fallback() {
        let backends = vec![create_test_backend_with_model(
            "backend_a",
            "Backend A",
            "mistral:7b",
        )];

        let registry = Arc::new(Registry::new());
        for backend in backends {
            registry.add_backend(backend).unwrap();
        }

        let mut aliases = HashMap::new();
        aliases.insert("gpt-4".to_string(), "llama3:70b".to_string());

        let mut fallbacks = HashMap::new();
        fallbacks.insert("llama3:70b".to_string(), vec!["mistral:7b".to_string()]);

        let router = Router::with_aliases_and_fallbacks(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            aliases,
            fallbacks,
        );

        let requirements = RequestRequirements {
            model: "gpt-4".to_string(), // Alias → llama3:70b → mistral:7b
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let result = router.select_backend(&requirements, None).unwrap();
        assert_eq!(result.backend.name, "Backend A");
    }

    // T01: Alias Chaining Tests (TDD RED Phase)
    #[test]
    fn alias_chain_two_levels() {
        // Given aliases: "gpt-4" → "llama-large", "llama-large" → "llama3:70b"
        let backends = vec![create_test_backend_with_model(
            "backend_a",
            "Backend A",
            "llama3:70b",
        )];

        let registry = Arc::new(Registry::new());
        for backend in backends {
            registry.add_backend(backend).unwrap();
        }

        let mut aliases = HashMap::new();
        aliases.insert("gpt-4".to_string(), "llama-large".to_string());
        aliases.insert("llama-large".to_string(), "llama3:70b".to_string());

        let router = Router::with_aliases_and_fallbacks(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            aliases,
            HashMap::new(),
        );

        let requirements = RequestRequirements {
            model: "gpt-4".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        // When resolving "gpt-4"
        // Then should resolve through chain to "llama3:70b"
        let result = router.select_backend(&requirements, None).unwrap();
        assert_eq!(result.backend.name, "Backend A");
    }

    #[test]
    fn alias_chain_three_levels() {
        // Given aliases: "a" → "b", "b" → "c", "c" → "final-model"
        let backends = vec![create_test_backend_with_model(
            "backend_a",
            "Backend A",
            "final-model",
        )];

        let registry = Arc::new(Registry::new());
        for backend in backends {
            registry.add_backend(backend).unwrap();
        }

        let mut aliases = HashMap::new();
        aliases.insert("a".to_string(), "b".to_string());
        aliases.insert("b".to_string(), "c".to_string());
        aliases.insert("c".to_string(), "final-model".to_string());

        let router = Router::with_aliases_and_fallbacks(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            aliases,
            HashMap::new(),
        );

        let requirements = RequestRequirements {
            model: "a".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        // When resolving "a"
        // Then should resolve through 3-level chain to "final-model"
        let result = router.select_backend(&requirements, None).unwrap();
        assert_eq!(result.backend.name, "Backend A");
    }

    #[test]
    fn alias_chain_stops_at_max_depth() {
        // Given aliases: "a" → "b", "b" → "c", "c" → "d", "d" → "e"
        // Chain has 4 levels, but we should stop at 3
        let backends = vec![
            create_test_backend_with_model("backend_d", "Backend D", "d"),
            create_test_backend_with_model("backend_e", "Backend E", "e"),
        ];

        let registry = Arc::new(Registry::new());
        for backend in backends {
            registry.add_backend(backend).unwrap();
        }

        let mut aliases = HashMap::new();
        aliases.insert("a".to_string(), "b".to_string());
        aliases.insert("b".to_string(), "c".to_string());
        aliases.insert("c".to_string(), "d".to_string());
        aliases.insert("d".to_string(), "e".to_string());

        let router = Router::with_aliases_and_fallbacks(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            aliases,
            HashMap::new(),
        );

        let requirements = RequestRequirements {
            model: "a".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        // When resolving "a" (4-level chain)
        // Then should stop at 3 levels and resolve to "d"
        let result = router.select_backend(&requirements, None).unwrap();
        assert_eq!(result.backend.name, "Backend D");
    }

    #[test]
    fn alias_preserves_existing_single_level_behavior() {
        // Ensure single-level aliases still work after chaining implementation
        let backends = vec![create_test_backend_with_model(
            "backend_a",
            "Backend A",
            "llama3:70b",
        )];

        let registry = Arc::new(Registry::new());
        for backend in backends {
            registry.add_backend(backend).unwrap();
        }

        let mut aliases = HashMap::new();
        aliases.insert("gpt-4".to_string(), "llama3:70b".to_string());

        let router = Router::with_aliases_and_fallbacks(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            aliases,
            HashMap::new(),
        );

        let requirements = RequestRequirements {
            model: "gpt-4".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let result = router.select_backend(&requirements, None).unwrap();
        assert_eq!(result.backend.name, "Backend A");
    }

    // T07: RoutingResult struct tests (TDD RED phase)
    #[test]
    fn routing_result_contains_fallback_info() {
        // Given router with fallback "primary" → ["fallback"]
        let backends = vec![create_test_backend_with_model(
            "backend_fallback",
            "Backend Fallback",
            "fallback",
        )];

        let registry = Arc::new(Registry::new());
        for backend in backends {
            registry.add_backend(backend).unwrap();
        }

        let mut fallbacks = HashMap::new();
        fallbacks.insert("primary".to_string(), vec!["fallback".to_string()]);

        let router = Router::with_aliases_and_fallbacks(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            HashMap::new(),
            fallbacks,
        );

        // And only "fallback" is available (no primary backend)
        let requirements = RequestRequirements {
            model: "primary".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        // When select_backend("primary")
        let result = router.select_backend(&requirements, None).unwrap();

        // Then result.fallback_used == true
        assert!(result.fallback_used, "Expected fallback_used to be true");
        // And result.actual_model == "fallback"
        assert_eq!(result.actual_model, "fallback");
        // And result.backend is the fallback backend
        assert_eq!(result.backend.name, "Backend Fallback");
    }

    #[test]
    fn routing_result_no_fallback_when_primary_used() {
        // Given router with fallback "primary" → ["fallback"]
        let backends = vec![
            create_test_backend_with_model("backend_primary", "Backend Primary", "primary"),
            create_test_backend_with_model("backend_fallback", "Backend Fallback", "fallback"),
        ];

        let registry = Arc::new(Registry::new());
        for backend in backends {
            registry.add_backend(backend).unwrap();
        }

        let mut fallbacks = HashMap::new();
        fallbacks.insert("primary".to_string(), vec!["fallback".to_string()]);

        let router = Router::with_aliases_and_fallbacks(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            HashMap::new(),
            fallbacks,
        );

        // And "primary" is available
        let requirements = RequestRequirements {
            model: "primary".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        // When select_backend("primary")
        let result = router.select_backend(&requirements, None).unwrap();

        // Then result.fallback_used == false
        assert!(!result.fallback_used, "Expected fallback_used to be false");
        // And result.actual_model == "primary"
        assert_eq!(result.actual_model, "primary");
        // And result.backend is the primary backend
        assert_eq!(result.backend.name, "Backend Primary");
    }

    #[test]
    fn test_circular_alias_detection() {
        // Aliases form a cycle: a → b → c → a
        // resolve_alias should stop at MAX_DEPTH (3) and not loop infinitely
        let backends = vec![
            create_test_backend_with_model("backend_a", "Backend A", "a"),
            create_test_backend_with_model("backend_b", "Backend B", "b"),
            create_test_backend_with_model("backend_c", "Backend C", "c"),
        ];

        let registry = Arc::new(Registry::new());
        for backend in backends {
            registry.add_backend(backend).unwrap();
        }

        let mut aliases = HashMap::new();
        aliases.insert("a".to_string(), "b".to_string());
        aliases.insert("b".to_string(), "c".to_string());
        aliases.insert("c".to_string(), "a".to_string());

        let router = Router::with_aliases_and_fallbacks(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            aliases,
            HashMap::new(),
        );

        // After 3 hops: a → b → c → a, resolve_alias returns "a"
        let resolved = router.resolve_alias("a");
        assert_eq!(resolved, "a", "Circular alias should stop at MAX_DEPTH");

        // The model "a" exists, so select_backend should succeed
        let requirements = RequestRequirements {
            model: "a".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };
        let result = router.select_backend(&requirements, None);
        assert!(
            result.is_ok(),
            "Should route to model 'a' after circular alias resolution"
        );
    }

    #[test]
    fn test_alias_max_depth() {
        // 4-level chain: x → y → z → w → final
        // Should stop at 3 hops, resolving to "w" (not "final")
        let backends = vec![
            create_test_backend_with_model("backend_w", "Backend W", "w"),
            create_test_backend_with_model("backend_final", "Backend Final", "final"),
        ];

        let registry = Arc::new(Registry::new());
        for backend in backends {
            registry.add_backend(backend).unwrap();
        }

        let mut aliases = HashMap::new();
        aliases.insert("x".to_string(), "y".to_string());
        aliases.insert("y".to_string(), "z".to_string());
        aliases.insert("z".to_string(), "w".to_string());
        aliases.insert("w".to_string(), "final".to_string());

        let router = Router::with_aliases_and_fallbacks(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            aliases,
            HashMap::new(),
        );

        // After 3 hops: x → y → z → w, stops at depth 3
        let resolved = router.resolve_alias("x");
        assert_eq!(resolved, "w", "Should stop at 3 levels, resolving to 'w'");

        let requirements = RequestRequirements {
            model: "x".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };
        let result = router.select_backend(&requirements, None).unwrap();
        assert_eq!(result.backend.name, "Backend W");
    }
}

#[cfg(test)]
mod constructor_tests {
    use super::*;
    use crate::config::{BudgetConfig, PolicyMatcher, QualityConfig};
    use crate::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model, Registry};
    use chrono::Utc;
    use dashmap::DashMap;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, AtomicU64};

    fn create_test_backend_with_model(id: &str, name: &str, model_id: &str) -> Backend {
        Backend {
            id: id.to_string(),
            name: name.to_string(),
            url: format!("http://{}", name),
            backend_type: BackendType::Ollama,
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
            current_operation: None,
        }
    }

    fn simple_requirements(model: &str) -> RequestRequirements {
        RequestRequirements {
            model: model.to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        }
    }

    #[test]
    fn test_with_full_config() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_test_backend_with_model(
                "b1",
                "Backend1",
                "llama3:8b",
            ))
            .unwrap();

        let budget_config = BudgetConfig {
            monthly_limit_usd: Some(100.0),
            soft_limit_percent: 75.0,
            ..BudgetConfig::default()
        };
        let budget_state = Arc::new(DashMap::new());

        let router = Router::with_full_config(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            HashMap::new(),
            HashMap::new(),
            PolicyMatcher::default(),
            budget_config,
            budget_state,
        );

        let result = router.select_backend(&simple_requirements("llama3:8b"), None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().backend.name, "Backend1");
    }

    #[test]
    fn test_with_aliases_fallbacks_and_policies() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_test_backend_with_model(
                "b1",
                "Backend1",
                "llama3:8b",
            ))
            .unwrap();

        let mut aliases = HashMap::new();
        aliases.insert("gpt-4".to_string(), "llama3:8b".to_string());

        let router = Router::with_aliases_fallbacks_and_policies(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            aliases,
            HashMap::new(),
            PolicyMatcher::default(),
            QualityConfig::default(),
        );

        // Alias "gpt-4" should resolve to "llama3:8b"
        let result = router.select_backend(&simple_requirements("gpt-4"), None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().backend.name, "Backend1");
    }
}

#[cfg(test)]
mod select_backend_error_tests {
    use super::*;
    use crate::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model, Registry};
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, AtomicU64};

    fn create_backend(id: &str, name: &str, status: BackendStatus, models: Vec<Model>) -> Backend {
        Backend {
            id: id.to_string(),
            name: name.to_string(),
            url: format!("http://{}", name),
            backend_type: BackendType::Ollama,
            status,
            last_health_check: Utc::now(),
            last_error: None,
            models,
            priority: 1,
            pending_requests: AtomicU32::new(0),
            total_requests: AtomicU64::new(0),
            avg_latency_ms: AtomicU32::new(50),
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
            current_operation: None,
        }
    }

    fn simple_requirements(model: &str) -> RequestRequirements {
        RequestRequirements {
            model: model.to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        }
    }

    #[test]
    fn test_select_backend_model_not_found() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend(
                "b1",
                "Backend1",
                BackendStatus::Healthy,
                vec![Model {
                    id: "llama3:8b".to_string(),
                    name: "llama3:8b".to_string(),
                    context_length: 4096,
                    supports_vision: false,
                    supports_tools: false,
                    supports_json_mode: false,
                    max_output_tokens: None,
                }],
            ))
            .unwrap();

        let router = Router::new(registry, RoutingStrategy::Smart, ScoringWeights::default());
        let result = router.select_backend(&simple_requirements("nonexistent-model"), None);

        assert!(result.is_err());
        match result.unwrap_err() {
            RoutingError::ModelNotFound { model } => {
                assert_eq!(model, "nonexistent-model");
            }
            other => panic!("Expected ModelNotFound, got: {:?}", other),
        }
    }

    #[test]
    fn test_select_backend_no_healthy_backend() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend(
                "b1",
                "Backend1",
                BackendStatus::Unhealthy,
                vec![Model {
                    id: "llama3:8b".to_string(),
                    name: "llama3:8b".to_string(),
                    context_length: 4096,
                    supports_vision: false,
                    supports_tools: false,
                    supports_json_mode: false,
                    max_output_tokens: None,
                }],
            ))
            .unwrap();

        let router = Router::new(registry, RoutingStrategy::Smart, ScoringWeights::default());
        let result = router.select_backend(&simple_requirements("llama3:8b"), None);

        assert!(result.is_err());
        match result.unwrap_err() {
            RoutingError::NoHealthyBackend { model } => {
                assert_eq!(model, "llama3:8b");
            }
            other => panic!("Expected NoHealthyBackend, got: {:?}", other),
        }
    }

    #[test]
    fn test_select_backend_capability_mismatch() {
        // Backend has the model but does NOT support vision
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend(
                "b1",
                "Backend1",
                BackendStatus::Healthy,
                vec![Model {
                    id: "llama3:8b".to_string(),
                    name: "llama3:8b".to_string(),
                    context_length: 4096,
                    supports_vision: false,
                    supports_tools: false,
                    supports_json_mode: false,
                    max_output_tokens: None,
                }],
            ))
            .unwrap();

        let router = Router::new(registry, RoutingStrategy::Smart, ScoringWeights::default());

        let requirements = RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 100,
            needs_vision: true,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let result = router.select_backend(&requirements, None);
        // The pipeline rejects when no backend meets capability requirements.
        // This may manifest as NoHealthyBackend or CapabilityMismatch depending
        // on how the scheduler reconciler reports it.
        assert!(
            result.is_err(),
            "Expected error for vision capability mismatch"
        );
        let err = result.unwrap_err();
        match &err {
            RoutingError::NoHealthyBackend { .. }
            | RoutingError::CapabilityMismatch { .. }
            | RoutingError::Reject { .. } => {
                // Any of these is acceptable — the model exists but can't serve the request
            }
            other => panic!(
                "Expected NoHealthyBackend, CapabilityMismatch, or Reject, got: {:?}",
                other
            ),
        }
    }
}

#[cfg(test)]
mod budget_status_tests {
    use super::*;
    use crate::config::BudgetConfig;
    use crate::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model, Registry};
    use crate::routing::reconciler::budget::{BudgetMetrics, GLOBAL_BUDGET_KEY};
    use crate::routing::reconciler::intent::BudgetStatus;
    use chrono::Utc;
    use dashmap::DashMap;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, AtomicU64};

    fn create_backend_with_model(model_id: &str) -> Backend {
        Backend {
            id: "b1".to_string(),
            name: "Backend1".to_string(),
            url: "http://backend1".to_string(),
            backend_type: BackendType::Ollama,
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
            current_operation: None,
        }
    }

    fn make_router_with_budget(
        monthly_limit: Option<f64>,
        soft_limit_percent: f64,
        spending: f64,
    ) -> Router {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend_with_model("llama3:8b"))
            .unwrap();

        let budget_config = BudgetConfig {
            monthly_limit_usd: monthly_limit,
            soft_limit_percent,
            ..BudgetConfig::default()
        };
        let budget_state = Arc::new(DashMap::new());
        if spending > 0.0 {
            budget_state.insert(
                GLOBAL_BUDGET_KEY.to_string(),
                BudgetMetrics {
                    current_month_spending: spending,
                    last_reconciliation_time: Utc::now(),
                    month_key: Utc::now().format("%Y-%m").to_string(),
                },
            );
        }

        Router::with_full_config(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            HashMap::new(),
            HashMap::new(),
            PolicyMatcher::default(),
            budget_config,
            budget_state,
        )
    }

    #[test]
    fn budget_normal_when_no_limit() {
        let router = make_router_with_budget(None, 75.0, 0.0);
        let (status, utilization, remaining) = router.get_budget_status_and_utilization();

        assert!(matches!(status, BudgetStatus::Normal));
        assert!(utilization.is_none());
        assert!(remaining.is_none());
    }

    #[test]
    fn budget_normal_when_zero_limit() {
        let router = make_router_with_budget(Some(0.0), 75.0, 0.0);
        let (status, utilization, remaining) = router.get_budget_status_and_utilization();

        assert!(matches!(status, BudgetStatus::Normal));
        assert!(utilization.is_none());
        assert!(remaining.is_none());
    }

    #[test]
    fn budget_normal_when_below_soft_limit() {
        // $100 limit, 75% soft threshold, $50 spent (50%)
        let router = make_router_with_budget(Some(100.0), 75.0, 50.0);
        let (status, utilization, remaining) = router.get_budget_status_and_utilization();

        assert!(matches!(status, BudgetStatus::Normal));
        let util = utilization.unwrap();
        assert!((util - 50.0).abs() < 0.01);
        let rem = remaining.unwrap();
        assert!((rem - 50.0).abs() < 0.01);
    }

    #[test]
    fn budget_soft_limit_when_at_threshold() {
        // $100 limit, 75% soft threshold, $75 spent (75%)
        let router = make_router_with_budget(Some(100.0), 75.0, 75.0);
        let (status, utilization, remaining) = router.get_budget_status_and_utilization();

        assert!(matches!(status, BudgetStatus::SoftLimit));
        let util = utilization.unwrap();
        assert!((util - 75.0).abs() < 0.01);
        let rem = remaining.unwrap();
        assert!((rem - 25.0).abs() < 0.01);
    }

    #[test]
    fn budget_soft_limit_when_above_soft_below_hard() {
        // $100 limit, 75% soft threshold, $90 spent (90%)
        let router = make_router_with_budget(Some(100.0), 75.0, 90.0);
        let (status, utilization, remaining) = router.get_budget_status_and_utilization();

        assert!(matches!(status, BudgetStatus::SoftLimit));
        let util = utilization.unwrap();
        assert!((util - 90.0).abs() < 0.01);
        let rem = remaining.unwrap();
        assert!((rem - 10.0).abs() < 0.01);
    }

    #[test]
    fn budget_hard_limit_when_at_100_percent() {
        // $100 limit, 75% soft threshold, $100 spent (100%)
        let router = make_router_with_budget(Some(100.0), 75.0, 100.0);
        let (status, utilization, remaining) = router.get_budget_status_and_utilization();

        assert!(matches!(status, BudgetStatus::HardLimit));
        let util = utilization.unwrap();
        assert!((util - 100.0).abs() < 0.01);
        let rem = remaining.unwrap();
        assert!((rem - 0.0).abs() < 0.01);
    }

    #[test]
    fn budget_hard_limit_when_over_budget() {
        // $100 limit, 75% soft threshold, $150 spent (150%)
        let router = make_router_with_budget(Some(100.0), 75.0, 150.0);
        let (status, utilization, remaining) = router.get_budget_status_and_utilization();

        assert!(matches!(status, BudgetStatus::HardLimit));
        let util = utilization.unwrap();
        assert!((util - 150.0).abs() < 0.01);
        // Remaining clamped to 0
        let rem = remaining.unwrap();
        assert!((rem - 0.0).abs() < 0.01);
    }

    #[test]
    fn budget_normal_when_no_spending_recorded() {
        // $100 limit, but no spending has been recorded yet
        let router = make_router_with_budget(Some(100.0), 75.0, 0.0);
        let (status, utilization, remaining) = router.get_budget_status_and_utilization();

        assert!(matches!(status, BudgetStatus::Normal));
        let util = utilization.unwrap();
        assert!((util - 0.0).abs() < 0.01);
        let rem = remaining.unwrap();
        assert!((rem - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_select_backend_queue_decision() {
        // Enable queue in router, register a model with an unhealthy backend
        // so that the pipeline produces RoutingDecision::Queue
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend_with_model("llama3:8b"))
            .unwrap();
        // Make backend unhealthy so no candidates remain
        registry
            .update_status(
                "b1",
                crate::registry::BackendStatus::Unhealthy,
                Some("test".to_string()),
            )
            .unwrap();

        let mut router = Router::new(registry, RoutingStrategy::Smart, ScoringWeights::default());
        router.set_queue_enabled(true);

        let requirements = RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let result = router.select_backend(&requirements, None);
        assert!(result.is_err());
        match result.unwrap_err() {
            RoutingError::Queue { reason, .. } => {
                assert!(
                    reason.contains("capacity"),
                    "Expected capacity reason, got: {}",
                    reason
                );
            }
            other => panic!("Expected Queue, got: {:?}", other),
        }
    }

    #[test]
    fn test_select_backend_with_fallback_chain_all_unhealthy() {
        let registry = Arc::new(Registry::new());

        // Create backend with both primary and fallback models, but unhealthy
        let mut backend = create_backend_with_model("primary-model");
        backend.models.push(Model {
            id: "fallback-model".to_string(),
            name: "fallback-model".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        });
        registry.add_backend(backend).unwrap();
        registry
            .update_status(
                "b1",
                crate::registry::BackendStatus::Unhealthy,
                Some("down".to_string()),
            )
            .unwrap();

        let mut fallbacks = HashMap::new();
        fallbacks.insert(
            "primary-model".to_string(),
            vec!["fallback-model".to_string()],
        );

        let router = Router::with_aliases_and_fallbacks(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            HashMap::new(),
            fallbacks,
        );

        let requirements = RequestRequirements {
            model: "primary-model".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let result = router.select_backend(&requirements, None);
        assert!(result.is_err());
        match result.unwrap_err() {
            RoutingError::FallbackChainExhausted { chain } => {
                assert!(chain.contains(&"primary-model".to_string()));
                assert!(chain.contains(&"fallback-model".to_string()));
            }
            other => panic!("Expected FallbackChainExhausted, got: {:?}", other),
        }
    }

    #[test]
    fn test_select_backend_fallback_succeeds() {
        // Primary model's backend is unhealthy, but fallback model's backend is healthy
        let registry = Arc::new(Registry::new());

        // Backend with primary model (unhealthy)
        let mut primary_backend = create_backend_with_model("primary-model");
        primary_backend.id = "b-primary".to_string();
        primary_backend.name = "PrimaryBackend".to_string();
        registry.add_backend(primary_backend).unwrap();
        registry
            .update_status(
                "b-primary",
                crate::registry::BackendStatus::Unhealthy,
                Some("down".to_string()),
            )
            .unwrap();

        // Backend with fallback model (healthy)
        let mut fallback_backend = create_backend_with_model("fallback-model");
        fallback_backend.id = "b-fallback".to_string();
        fallback_backend.name = "FallbackBackend".to_string();
        registry.add_backend(fallback_backend).unwrap();

        let mut fallbacks = HashMap::new();
        fallbacks.insert(
            "primary-model".to_string(),
            vec!["fallback-model".to_string()],
        );

        let router = Router::with_aliases_and_fallbacks(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            HashMap::new(),
            fallbacks,
        );

        let requirements = RequestRequirements {
            model: "primary-model".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let result = router.select_backend(&requirements, None);
        assert!(result.is_ok(), "Fallback should succeed");
        let routing_result = result.unwrap();
        assert!(routing_result.fallback_used);
        assert_eq!(routing_result.actual_model, "fallback-model");
        assert!(routing_result.route_reason.starts_with("fallback:"));
    }

    #[test]
    fn test_select_backend_success_returns_budget_fields() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend_with_model("llama3:8b"))
            .unwrap();

        let budget_config = BudgetConfig {
            monthly_limit_usd: Some(100.0),
            soft_limit_percent: 75.0,
            ..BudgetConfig::default()
        };
        let budget_state = Arc::new(DashMap::new());
        budget_state.insert(
            crate::routing::reconciler::budget::GLOBAL_BUDGET_KEY.to_string(),
            BudgetMetrics {
                current_month_spending: 50.0,
                last_reconciliation_time: chrono::Utc::now(),
                month_key: chrono::Utc::now().format("%Y-%m").to_string(),
            },
        );

        let router = Router::with_full_config(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            HashMap::new(),
            HashMap::new(),
            PolicyMatcher::default(),
            budget_config,
            budget_state,
        );

        let requirements = RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let result = router.select_backend(&requirements, None);
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.budget_status, BudgetStatus::Normal);
        assert!(r.budget_utilization.is_some());
        assert!(r.budget_remaining.is_some());
        assert!(!r.fallback_used);
        assert!(r.cost_estimated.is_some());
    }

    #[test]
    fn test_select_smart_selects_best_scored_backend() {
        let registry = Arc::new(Registry::new());
        // b1: good priority, low latency
        let mut b1 = create_backend_with_model("llama3:8b");
        b1.id = "b1".to_string();
        b1.name = "B1".to_string();
        b1.priority = 1;
        registry.add_backend(b1).unwrap();
        // b2: worse priority, higher latency
        let mut b2 = create_backend_with_model("llama3:8b");
        b2.id = "b2".to_string();
        b2.name = "B2".to_string();
        b2.priority = 10;
        registry.add_backend(b2).unwrap();

        let router = Router::new(registry, RoutingStrategy::Smart, ScoringWeights::default());
        let result = router.select_backend(
            &RequestRequirements {
                model: "llama3:8b".to_string(),
                estimated_tokens: 100,
                needs_vision: false,
                needs_tools: false,
                needs_json_mode: false,
                prefers_streaming: false,
            },
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_select_priority_only_via_router() {
        let registry = Arc::new(Registry::new());
        // b1: priority 10
        let mut b1 = create_backend_with_model("llama3:8b");
        b1.id = "b1".to_string();
        b1.name = "B1".to_string();
        b1.priority = 10;
        registry.add_backend(b1).unwrap();
        // b2: priority 1 (better)
        let mut b2 = create_backend_with_model("llama3:8b");
        b2.id = "b2".to_string();
        b2.name = "B2".to_string();
        b2.priority = 1;
        registry.add_backend(b2).unwrap();

        let router = Router::new(
            registry,
            RoutingStrategy::PriorityOnly,
            ScoringWeights::default(),
        );
        let result = router.select_backend(
            &RequestRequirements {
                model: "llama3:8b".to_string(),
                estimated_tokens: 100,
                needs_vision: false,
                needs_tools: false,
                needs_json_mode: false,
                prefers_streaming: false,
            },
            None,
        );
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.backend.name, "B2");
    }

    #[test]
    fn test_select_round_robin_via_router() {
        let registry = Arc::new(Registry::new());
        let mut b1 = create_backend_with_model("llama3:8b");
        b1.id = "b1".to_string();
        b1.name = "B1".to_string();
        registry.add_backend(b1).unwrap();
        let mut b2 = create_backend_with_model("llama3:8b");
        b2.id = "b2".to_string();
        b2.name = "B2".to_string();
        registry.add_backend(b2).unwrap();

        let router = Router::new(
            registry,
            RoutingStrategy::RoundRobin,
            ScoringWeights::default(),
        );
        let reqs = RequestRequirements {
            model: "llama3:8b".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        };

        let r1 = router.select_backend(&reqs, None).unwrap();
        let r2 = router.select_backend(&reqs, None).unwrap();
        // Different backends should be selected
        assert_ne!(r1.backend.name, r2.backend.name);
    }

    #[test]
    fn test_select_random_via_router() {
        let registry = Arc::new(Registry::new());
        let mut b1 = create_backend_with_model("llama3:8b");
        b1.id = "b1".to_string();
        b1.name = "B1".to_string();
        registry.add_backend(b1).unwrap();

        let router = Router::new(registry, RoutingStrategy::Random, ScoringWeights::default());
        let result = router.select_backend(
            &RequestRequirements {
                model: "llama3:8b".to_string(),
                estimated_tokens: 100,
                needs_vision: false,
                needs_tools: false,
                needs_json_mode: false,
                prefers_streaming: false,
            },
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_select_backend_with_tier_enforcement() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend_with_model("llama3:8b"))
            .unwrap();

        let router = Router::new(registry, RoutingStrategy::Smart, ScoringWeights::default());
        let result = router.select_backend(
            &RequestRequirements {
                model: "llama3:8b".to_string(),
                estimated_tokens: 100,
                needs_vision: false,
                needs_tools: false,
                needs_json_mode: false,
                prefers_streaming: false,
            },
            Some(crate::routing::reconciler::intent::TierEnforcementMode::Strict),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_filter_candidates_legacy() {
        let registry = Arc::new(Registry::new());
        let mut backend = create_backend_with_model("llama3:8b");
        backend.models[0].supports_vision = true;
        backend.models[0].supports_tools = true;
        backend.models[0].supports_json_mode = true;
        registry.add_backend(backend).unwrap();

        let router = Router::new(registry, RoutingStrategy::Smart, ScoringWeights::default());

        // All requirements met
        let candidates = router.filter_candidates(
            "llama3:8b",
            &RequestRequirements {
                model: "llama3:8b".to_string(),
                estimated_tokens: 100,
                needs_vision: true,
                needs_tools: true,
                needs_json_mode: true,
                prefers_streaming: false,
            },
        );
        assert_eq!(candidates.len(), 1);
    }

    #[test]
    fn test_filter_candidates_vision_mismatch() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend_with_model("llama3:8b"))
            .unwrap();

        let router = Router::new(registry, RoutingStrategy::Smart, ScoringWeights::default());

        let candidates = router.filter_candidates(
            "llama3:8b",
            &RequestRequirements {
                model: "llama3:8b".to_string(),
                estimated_tokens: 100,
                needs_vision: true,
                needs_tools: false,
                needs_json_mode: false,
                prefers_streaming: false,
            },
        );
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_filter_candidates_tools_mismatch() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend_with_model("llama3:8b"))
            .unwrap();

        let router = Router::new(registry, RoutingStrategy::Smart, ScoringWeights::default());

        let candidates = router.filter_candidates(
            "llama3:8b",
            &RequestRequirements {
                model: "llama3:8b".to_string(),
                estimated_tokens: 100,
                needs_vision: false,
                needs_tools: true,
                needs_json_mode: false,
                prefers_streaming: false,
            },
        );
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_filter_candidates_json_mode_mismatch() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend_with_model("llama3:8b"))
            .unwrap();

        let router = Router::new(registry, RoutingStrategy::Smart, ScoringWeights::default());

        let candidates = router.filter_candidates(
            "llama3:8b",
            &RequestRequirements {
                model: "llama3:8b".to_string(),
                estimated_tokens: 100,
                needs_vision: false,
                needs_tools: false,
                needs_json_mode: true,
                prefers_streaming: false,
            },
        );
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_filter_candidates_context_length_exceeded() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend_with_model("llama3:8b"))
            .unwrap();

        let router = Router::new(registry, RoutingStrategy::Smart, ScoringWeights::default());

        let candidates = router.filter_candidates(
            "llama3:8b",
            &RequestRequirements {
                model: "llama3:8b".to_string(),
                estimated_tokens: 999999,
                needs_vision: false,
                needs_tools: false,
                needs_json_mode: false,
                prefers_streaming: false,
            },
        );
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_filter_candidates_unhealthy_excluded() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend_with_model("llama3:8b"))
            .unwrap();
        registry
            .update_status(
                "b1",
                crate::registry::BackendStatus::Unhealthy,
                Some("down".to_string()),
            )
            .unwrap();

        let router = Router::new(registry, RoutingStrategy::Smart, ScoringWeights::default());

        let candidates = router.filter_candidates(
            "llama3:8b",
            &RequestRequirements {
                model: "llama3:8b".to_string(),
                estimated_tokens: 100,
                needs_vision: false,
                needs_tools: false,
                needs_json_mode: false,
                prefers_streaming: false,
            },
        );
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_filter_candidates_model_not_found() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend_with_model("llama3:8b"))
            .unwrap();

        let router = Router::new(registry, RoutingStrategy::Smart, ScoringWeights::default());

        let candidates = router.filter_candidates(
            "nonexistent",
            &RequestRequirements {
                model: "nonexistent".to_string(),
                estimated_tokens: 100,
                needs_vision: false,
                needs_tools: false,
                needs_json_mode: false,
                prefers_streaming: false,
            },
        );
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_select_smart_with_candidates() {
        let registry = Arc::new(Registry::new());
        let mut b1 = create_backend_with_model("llama3:8b");
        b1.id = "b1".to_string();
        b1.name = "B1".to_string();
        b1.priority = 1;
        registry.add_backend(b1).unwrap();
        let mut b2 = create_backend_with_model("llama3:8b");
        b2.id = "b2".to_string();
        b2.name = "B2".to_string();
        b2.priority = 10;
        registry.add_backend(b2).unwrap();

        let router = Router::new(
            registry.clone(),
            RoutingStrategy::Smart,
            ScoringWeights::default(),
        );
        let candidates: Vec<Backend> = registry
            .get_backends_for_model("llama3:8b")
            .into_iter()
            .filter(|b| b.status == BackendStatus::Healthy)
            .collect();
        let selected = router.select_smart(&candidates);
        // b1 should win (higher priority = lower number = better)
        assert_eq!(selected.id, "b1");
    }

    #[test]
    fn test_select_priority_only_with_candidates() {
        let registry = Arc::new(Registry::new());
        let mut b1 = create_backend_with_model("llama3:8b");
        b1.id = "b1".to_string();
        b1.name = "B1".to_string();
        b1.priority = 10;
        registry.add_backend(b1).unwrap();
        let mut b2 = create_backend_with_model("llama3:8b");
        b2.id = "b2".to_string();
        b2.name = "B2".to_string();
        b2.priority = 1;
        registry.add_backend(b2).unwrap();

        let router = Router::new(
            registry.clone(),
            RoutingStrategy::PriorityOnly,
            ScoringWeights::default(),
        );
        let candidates: Vec<Backend> = registry
            .get_backends_for_model("llama3:8b")
            .into_iter()
            .filter(|b| b.status == BackendStatus::Healthy)
            .collect();
        let selected = router.select_priority_only(&candidates);
        assert_eq!(selected.id, "b2");
    }

    #[test]
    fn test_select_random_with_candidates() {
        let registry = Arc::new(Registry::new());
        let mut b1 = create_backend_with_model("llama3:8b");
        b1.id = "b1".to_string();
        b1.name = "B1".to_string();
        registry.add_backend(b1).unwrap();

        let router = Router::new(
            registry.clone(),
            RoutingStrategy::Random,
            ScoringWeights::default(),
        );
        let candidates: Vec<Backend> = registry
            .get_backends_for_model("llama3:8b")
            .into_iter()
            .filter(|b| b.status == BackendStatus::Healthy)
            .collect();
        let selected = router.select_random(&candidates);
        assert_eq!(selected.id, "b1");
    }

    #[test]
    fn test_budget_config_and_state_accessors() {
        let registry = Arc::new(Registry::new());
        let budget_config = BudgetConfig {
            monthly_limit_usd: Some(200.0),
            soft_limit_percent: 80.0,
            ..BudgetConfig::default()
        };
        let budget_state = Arc::new(DashMap::new());
        let router = Router::with_full_config(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            HashMap::new(),
            HashMap::new(),
            PolicyMatcher::default(),
            budget_config,
            budget_state,
        );
        assert_eq!(router.budget_config().monthly_limit_usd, Some(200.0));
        assert!(router.budget_state().is_empty());
        assert!(router.quality_store().get_all_metrics().is_empty());
    }
}
