//! RequestAnalyzer reconciler
//!
//! Resolves model aliases (max 3 levels), and populates candidate agents
//! from the registry for the resolved model.

use super::{intent::RoutingIntent, Reconciler};
use crate::registry::Registry;
use crate::routing::error::RoutingError;
use std::collections::HashMap;
use std::sync::Arc;

const MAX_ALIAS_DEPTH: usize = 3;

/// RequestAnalyzer resolves aliases and populates initial candidate list
pub struct RequestAnalyzer {
    /// Model alias mappings (alias → target)
    model_aliases: HashMap<String, String>,

    /// Registry for looking up backends by model
    registry: Arc<Registry>,
}

impl RequestAnalyzer {
    /// Create a new RequestAnalyzer
    pub fn new(model_aliases: HashMap<String, String>, registry: Arc<Registry>) -> Self {
        Self {
            model_aliases,
            registry,
        }
    }

    /// Resolve model aliases with chaining support (max 3 levels).
    /// Reuses the same algorithm as Router::resolve_alias.
    fn resolve_alias(&self, model: &str) -> String {
        let mut current = model.to_string();
        let mut depth = 0;

        while depth < MAX_ALIAS_DEPTH {
            match self.model_aliases.get(&current) {
                Some(target) => {
                    tracing::debug!(
                        from = %current,
                        to = %target,
                        depth = depth + 1,
                        "RequestAnalyzer: resolved alias"
                    );
                    current = target.clone();
                    depth += 1;
                }
                None => break,
            }
        }

        current
    }
}

impl Reconciler for RequestAnalyzer {
    fn name(&self) -> &'static str {
        "RequestAnalyzer"
    }

    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        // T021: Resolve model aliases (max 3 levels)
        let resolved = self.resolve_alias(&intent.requested_model);
        intent.resolved_model = resolved.clone();

        // T022: Requirements are already populated in the intent from construction
        // (RequestRequirements is set when RoutingIntent::new is called)

        // T023: Populate candidate_agents with all backend IDs that serve this model
        let backends = self.registry.get_backends_for_model(&resolved);
        intent.candidate_agents = backends.iter().map(|b| b.id.clone()).collect();

        if intent.candidate_agents.is_empty() {
            tracing::debug!(
                model = %resolved,
                "RequestAnalyzer: no backends found for model"
            );
        } else {
            tracing::debug!(
                model = %resolved,
                candidates = intent.candidate_agents.len(),
                "RequestAnalyzer: populated candidates"
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model};
    use crate::routing::reconciler::intent::RoutingIntent;
    use crate::routing::RequestRequirements;
    use chrono::Utc;
    use std::sync::atomic::{AtomicU32, AtomicU64};

    fn create_test_backend(id: &str, model_id: &str) -> Backend {
        Backend {
            id: id.to_string(),
            name: id.to_string(),
            url: format!("http://{}", id),
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

    fn create_requirements(model: &str) -> RequestRequirements {
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
    fn resolves_single_alias() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_test_backend("b1", "llama3:70b"))
            .unwrap();

        let mut aliases = HashMap::new();
        aliases.insert("gpt-4".to_string(), "llama3:70b".to_string());

        let analyzer = RequestAnalyzer::new(aliases, registry);

        let mut intent = RoutingIntent::new(
            "req-1".to_string(),
            "gpt-4".to_string(),
            "gpt-4".to_string(),
            create_requirements("gpt-4"),
            vec![],
        );

        analyzer.reconcile(&mut intent).unwrap();
        assert_eq!(intent.resolved_model, "llama3:70b");
        assert_eq!(intent.candidate_agents, vec!["b1"]);
    }

    #[test]
    fn resolves_chained_aliases_max_3() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_test_backend("b1", "d"))
            .unwrap();

        let mut aliases = HashMap::new();
        aliases.insert("a".to_string(), "b".to_string());
        aliases.insert("b".to_string(), "c".to_string());
        aliases.insert("c".to_string(), "d".to_string());
        aliases.insert("d".to_string(), "e".to_string()); // 4th level, should be ignored

        let analyzer = RequestAnalyzer::new(aliases, registry);

        let mut intent = RoutingIntent::new(
            "req-1".to_string(),
            "a".to_string(),
            "a".to_string(),
            create_requirements("a"),
            vec![],
        );

        analyzer.reconcile(&mut intent).unwrap();
        // Stops at depth 3: a → b → c → d (not e)
        assert_eq!(intent.resolved_model, "d");
        assert_eq!(intent.candidate_agents, vec!["b1"]);
    }

    #[test]
    fn populates_all_backend_ids_for_model() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_test_backend("b1", "llama3:8b"))
            .unwrap();
        registry
            .add_backend(create_test_backend("b2", "llama3:8b"))
            .unwrap();

        let analyzer = RequestAnalyzer::new(HashMap::new(), registry);

        let mut intent = RoutingIntent::new(
            "req-1".to_string(),
            "llama3:8b".to_string(),
            "llama3:8b".to_string(),
            create_requirements("llama3:8b"),
            vec![],
        );

        analyzer.reconcile(&mut intent).unwrap();
        assert_eq!(intent.candidate_agents.len(), 2);
    }

    #[test]
    fn no_alias_passes_through() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_test_backend("b1", "llama3:8b"))
            .unwrap();

        let analyzer = RequestAnalyzer::new(HashMap::new(), registry);

        let mut intent = RoutingIntent::new(
            "req-1".to_string(),
            "llama3:8b".to_string(),
            "llama3:8b".to_string(),
            create_requirements("llama3:8b"),
            vec![],
        );

        analyzer.reconcile(&mut intent).unwrap();
        assert_eq!(intent.resolved_model, "llama3:8b");
        assert_eq!(intent.candidate_agents, vec!["b1"]);
    }

    #[test]
    fn empty_candidates_for_unknown_model() {
        let registry = Arc::new(Registry::new());
        let analyzer = RequestAnalyzer::new(HashMap::new(), registry);

        let mut intent = RoutingIntent::new(
            "req-1".to_string(),
            "nonexistent".to_string(),
            "nonexistent".to_string(),
            create_requirements("nonexistent"),
            vec![],
        );

        analyzer.reconcile(&mut intent).unwrap();
        assert!(intent.candidate_agents.is_empty());
    }
}
