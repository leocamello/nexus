//! Intelligent routing system for selecting optimal backends
//!
//! This module implements the routing logic that selects the best backend
//! for each request based on model requirements, backend capabilities, and
//! current system state.

use std::sync::atomic::AtomicU64;
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

    /// Round-robin counter for round-robin strategy
    round_robin_counter: AtomicU64,
}

impl Router {
    /// Create a new router with the given configuration
    pub fn new(registry: Arc<Registry>, strategy: RoutingStrategy, weights: ScoringWeights) -> Self {
        Self {
            registry,
            strategy,
            weights,
            round_robin_counter: AtomicU64::new(0),
        }
    }

    /// Select the best backend for the given requirements
    pub fn select_backend(
        &self,
        _requirements: &RequestRequirements,
    ) -> Result<Arc<Backend>, RoutingError> {
        // Implementation will be added in subsequent tasks
        todo!("select_backend implementation")
    }

    /// Filter candidates by model, health, and capabilities
    #[allow(dead_code)] // Will be used when select_backend is implemented
    fn filter_candidates(
        &self,
        model: &str,
        requirements: &RequestRequirements,
    ) -> Vec<Backend> {
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

        Router::new(
            registry,
            RoutingStrategy::Smart,
            ScoringWeights::default(),
        )
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
