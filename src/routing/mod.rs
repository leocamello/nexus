//! Intelligent routing system for selecting optimal backends
//!
//! This module implements the routing logic that selects the best backend
//! for each request based on model requirements, backend capabilities, and
//! current system state.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::Arc;

pub mod error;
pub mod requirements;
pub mod scoring;
pub mod strategies;

pub use error::RoutingError;
pub use requirements::RequestRequirements;
pub use scoring::{score_backend, ScoringWeights};
pub use strategies::RoutingStrategy;

use crate::registry::{Backend, BackendStatus, Registry};

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

    /// Round-robin counter for round-robin strategy
    round_robin_counter: AtomicU64,
}

impl Router {
    /// Create a new router with the given configuration
    pub fn new(
        registry: Arc<Registry>,
        strategy: RoutingStrategy,
        weights: ScoringWeights,
    ) -> Self {
        Self {
            registry,
            strategy,
            weights,
            aliases: HashMap::new(),
            fallbacks: HashMap::new(),
            round_robin_counter: AtomicU64::new(0),
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
        Self {
            registry,
            strategy,
            weights,
            aliases,
            fallbacks,
            round_robin_counter: AtomicU64::new(0),
        }
    }

    /// Resolve model aliases (single-level only)
    fn resolve_alias(&self, model: &str) -> String {
        self.aliases
            .get(model)
            .cloned()
            .unwrap_or_else(|| model.to_string())
    }

    /// Get fallback chain for a model
    fn get_fallbacks(&self, model: &str) -> Vec<String> {
        self.fallbacks.get(model).cloned().unwrap_or_default()
    }

    /// Select the best backend for the given requirements
    pub fn select_backend(
        &self,
        requirements: &RequestRequirements,
    ) -> Result<Arc<Backend>, RoutingError> {
        // Resolve alias first
        let model = self.resolve_alias(&requirements.model);

        // Try to find backend for the primary model
        let candidates = self.filter_candidates(&model, requirements);

        if !candidates.is_empty() {
            // Apply routing strategy
            let selected = match self.strategy {
                RoutingStrategy::Smart => self.select_smart(&candidates),
                RoutingStrategy::RoundRobin => self.select_round_robin(&candidates),
                RoutingStrategy::PriorityOnly => self.select_priority_only(&candidates),
                RoutingStrategy::Random => self.select_random(&candidates),
            };
            return Ok(Arc::new(selected));
        }

        // Try fallback chain
        let fallbacks = self.get_fallbacks(&model);
        for fallback_model in &fallbacks {
            let candidates = self.filter_candidates(fallback_model, requirements);
            if !candidates.is_empty() {
                let selected = match self.strategy {
                    RoutingStrategy::Smart => self.select_smart(&candidates),
                    RoutingStrategy::RoundRobin => self.select_round_robin(&candidates),
                    RoutingStrategy::PriorityOnly => self.select_priority_only(&candidates),
                    RoutingStrategy::Random => self.select_random(&candidates),
                };
                return Ok(Arc::new(selected));
            }
        }

        // All attempts failed
        if !fallbacks.is_empty() {
            // Build chain for error message
            let mut chain = vec![model.clone()];
            chain.extend(fallbacks);
            Err(RoutingError::FallbackChainExhausted { chain })
        } else {
            Err(RoutingError::ModelNotFound {
                model: requirements.model.clone(),
            })
        }
    }

    /// Select backend using smart scoring
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
        }
    }

    /// Select backend using round-robin
    fn select_round_robin(&self, candidates: &[Backend]) -> Backend {
        let counter = self
            .round_robin_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let index = (counter as usize) % candidates.len();
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
        }
    }

    /// Select backend using priority-only
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
        }
    }

    /// Select backend using random
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
        }
    }

    /// Filter candidates by model, health, and capabilities
    #[allow(dead_code)] // Will be used when select_backend is implemented
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
        };

        let candidates = router.filter_candidates("nonexistent", &requirements);
        assert!(candidates.is_empty());
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
        };

        let backend = router.select_backend(&requirements).unwrap();
        assert_eq!(backend.name, "Backend A");
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
        };

        let backend = router.select_backend(&requirements).unwrap();
        assert_eq!(backend.name, "Backend A"); // Lower load
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
        };

        let backend = router.select_backend(&requirements).unwrap();
        assert_eq!(backend.name, "Backend A"); // Lower latency
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
        };

        let result = router.select_backend(&requirements);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RoutingError::ModelNotFound { .. }
        ));
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
        };

        // Should cycle through: A, B, C, A, B, C
        let names: Vec<String> = (0..6)
            .map(|_| router.select_backend(&requirements).unwrap().name.clone())
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
        };

        // Should always select Backend B (priority 1)
        for _ in 0..5 {
            let backend = router.select_backend(&requirements).unwrap();
            assert_eq!(backend.name, "Backend B");
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
        };

        // Should select from all three backends over many iterations
        let mut selected = HashMap::new();
        for _ in 0..30 {
            let backend = router.select_backend(&requirements).unwrap();
            *selected.entry(backend.name.clone()).or_insert(0) += 1;
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
        };

        let backend = router.select_backend(&requirements).unwrap();
        assert_eq!(backend.name, "Backend A");
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
        };

        let backend = router.select_backend(&requirements).unwrap();
        assert_eq!(backend.name, "Backend A");
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
        };

        let result = router.select_backend(&requirements);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RoutingError::FallbackChainExhausted { .. }
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
        };

        let backend = router.select_backend(&requirements).unwrap();
        assert_eq!(backend.name, "Backend A");
    }
}
