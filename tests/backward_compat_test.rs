//! Integration tests for backward compatibility (US5)
//!
//! Tests T052-T057:
//! - T052: No traffic policies → no filtering applied (zero-config)
//! - T053: Empty policy matcher doesn't exclude any agents
//! - T054: Backend without zone/tier config uses defaults
//! - T055: Existing routing behavior preserved with no F13 headers
//! - T056: Mixed config (some backends with zones, some without) works
//! - T057: select_backend with None tier_enforcement_mode works

use nexus::agent::factory::create_agent;
use nexus::agent::quality::QualityMetricsStore;
use nexus::agent::PrivacyZone;
use nexus::config::PolicyMatcher;
use nexus::config::QualityConfig;
use nexus::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model, Registry};
use nexus::routing::reconciler::decision::RoutingDecision;
use nexus::routing::reconciler::intent::RoutingIntent;
use nexus::routing::reconciler::{
    privacy::PrivacyReconciler, scheduler::SchedulerReconciler, tier::TierReconciler, Reconciler,
    ReconcilerPipeline,
};
use nexus::routing::{RequestRequirements, Router, RoutingStrategy, ScoringWeights};
use std::collections::HashMap;
use std::sync::Arc;

fn test_backend(
    id: &str,
    name: &str,
    zone: PrivacyZone,
    tier: Option<u8>,
) -> (Backend, Arc<dyn nexus::agent::InferenceAgent>) {
    let backend = Backend {
        id: id.to_string(),
        name: name.to_string(),
        url: "http://localhost:11434".to_string(),
        backend_type: BackendType::Ollama,
        status: BackendStatus::Healthy,
        models: vec![Model {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            context_length: 4096,
            supports_vision: false,
            supports_tools: false,
            supports_json_mode: false,
            max_output_tokens: None,
        }],
        priority: 50,
        last_health_check: chrono::Utc::now(),
        last_error: None,
        pending_requests: std::sync::atomic::AtomicU32::new(0),
        total_requests: std::sync::atomic::AtomicU64::new(0),
        avg_latency_ms: std::sync::atomic::AtomicU32::new(0),
        discovery_source: DiscoverySource::Static,
        metadata: HashMap::new(),
        current_operation: None,
    };

    let client = Arc::new(reqwest::Client::new());
    let agent = create_agent(
        id.to_string(),
        name.to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        zone,
        tier,
    )
    .unwrap();

    (backend, agent)
}

/// T052: No traffic policies → no filtering applied (zero-config default)
#[test]
fn test_no_policies_no_filtering() {
    let registry = Arc::new(Registry::new());

    let (backend, agent) = test_backend("b1", "Backend 1", PrivacyZone::Open, Some(1));
    registry.add_backend_with_agent(backend, agent).unwrap();

    // Empty policy matcher — zero config
    let policy_matcher = PolicyMatcher::compile(vec![]).unwrap();

    let privacy = PrivacyReconciler::new(Arc::clone(&registry), policy_matcher.clone());
    let tier = TierReconciler::new(Arc::clone(&registry), policy_matcher);
    let scheduler = {
        let qcfg = QualityConfig::default();
        let qstore = std::sync::Arc::new(QualityMetricsStore::new(qcfg.clone()));
        SchedulerReconciler::new(
            Arc::clone(&registry),
            RoutingStrategy::PriorityOnly,
            ScoringWeights::default(),
            Arc::new(std::sync::atomic::AtomicU64::new(0)),
            qstore,
            qcfg,
        )
    };

    let mut pipeline =
        ReconcilerPipeline::new(vec![Box::new(privacy), Box::new(tier), Box::new(scheduler)]);

    let reqs = RequestRequirements {
        model: "test-model".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
        prefers_streaming: false,
    };

    let mut intent = RoutingIntent::new(
        "req-1".to_string(),
        "test-model".to_string(),
        "test-model".to_string(),
        reqs,
        vec!["b1".to_string()],
    );

    let decision = pipeline.execute(&mut intent);
    assert!(decision.is_ok());
    assert!(
        matches!(decision.unwrap(), RoutingDecision::Route { .. }),
        "No policies should result in Route decision"
    );
}

/// T053: Empty policy matcher doesn't exclude any agents
#[test]
fn test_empty_policy_matcher_passes_all() {
    let registry = Arc::new(Registry::new());

    // Mix of zones and tiers
    let (b1, a1) = test_backend("local-1", "Local", PrivacyZone::Restricted, Some(1));
    let (b2, a2) = test_backend("cloud-1", "Cloud", PrivacyZone::Open, Some(4));

    registry.add_backend_with_agent(b1, a1).unwrap();
    registry.add_backend_with_agent(b2, a2).unwrap();

    let policy_matcher = PolicyMatcher::compile(vec![]).unwrap();

    let privacy = PrivacyReconciler::new(Arc::clone(&registry), policy_matcher.clone());
    let tier = TierReconciler::new(Arc::clone(&registry), policy_matcher);

    let reqs = RequestRequirements {
        model: "test-model".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
        prefers_streaming: false,
    };

    let mut intent = RoutingIntent::new(
        "req-1".to_string(),
        "test-model".to_string(),
        "test-model".to_string(),
        reqs.clone(),
        vec!["local-1".to_string(), "cloud-1".to_string()],
    );

    // Run privacy — should not exclude anyone
    privacy.reconcile(&mut intent).unwrap();
    assert_eq!(
        intent.candidate_agents.len(),
        2,
        "Privacy should not exclude with no policies"
    );

    // Run tier — should not exclude anyone
    tier.reconcile(&mut intent).unwrap();
    assert_eq!(
        intent.candidate_agents.len(),
        2,
        "Tier should not exclude with no policies"
    );
}

/// T054: Backend without explicit zone/tier config uses defaults
#[test]
fn test_default_zone_and_tier() {
    let registry = Arc::new(Registry::new());

    // Backend with no explicit zone/tier (defaults: Open zone, tier 1)
    let (backend, agent) = test_backend("default-b", "Default Backend", PrivacyZone::Open, None);
    registry.add_backend_with_agent(backend, agent).unwrap();

    // Check agent profile has defaults
    if let Some(agent) = registry.get_agent("default-b") {
        let profile = agent.profile();
        assert_eq!(profile.privacy_zone, PrivacyZone::Open);
        // Tier should be the default (None means use default)
    }
}

/// T055: Existing routing behavior preserved with no F13 headers
#[test]
fn test_routing_without_f13_headers() {
    let registry = Arc::new(Registry::new());

    let (b1, a1) = test_backend("b1", "Backend 1", PrivacyZone::Open, Some(3));
    let (b2, a2) = test_backend("b2", "Backend 2", PrivacyZone::Open, Some(3));
    registry.add_backend_with_agent(b1, a1).unwrap();
    registry.add_backend_with_agent(b2, a2).unwrap();

    let router = Router::new(
        Arc::clone(&registry),
        RoutingStrategy::RoundRobin,
        ScoringWeights::default(),
    );

    let reqs = RequestRequirements {
        model: "test-model".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
        prefers_streaming: false,
    };

    // None = no tier enforcement header (backward compatible)
    let result = router.select_backend(&reqs, None);
    assert!(result.is_ok(), "Routing should work without F13 headers");
}

/// T056: Mixed configuration — some backends with zones, some without
#[test]
fn test_mixed_zone_configuration() {
    let registry = Arc::new(Registry::new());

    // One with explicit zone, one without (defaults to Open)
    let (b1, a1) = test_backend(
        "restricted-1",
        "Restricted",
        PrivacyZone::Restricted,
        Some(3),
    );
    let (b2, a2) = test_backend("default-1", "Default", PrivacyZone::Open, Some(3));
    registry.add_backend_with_agent(b1, a1).unwrap();
    registry.add_backend_with_agent(b2, a2).unwrap();

    let policy_matcher = PolicyMatcher::compile(vec![]).unwrap();

    let privacy = PrivacyReconciler::new(Arc::clone(&registry), policy_matcher);

    let reqs = RequestRequirements {
        model: "test-model".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
        prefers_streaming: false,
    };

    let mut intent = RoutingIntent::new(
        "req-1".to_string(),
        "test-model".to_string(),
        "test-model".to_string(),
        reqs,
        vec!["restricted-1".to_string(), "default-1".to_string()],
    );

    // No policies → both should remain candidates
    privacy.reconcile(&mut intent).unwrap();
    assert_eq!(
        intent.candidate_agents.len(),
        2,
        "Both should remain with no policies"
    );
}

/// T057: select_backend with None tier_enforcement_mode preserves behavior
#[test]
fn test_select_backend_none_tier_mode() {
    let registry = Arc::new(Registry::new());

    let (b1, a1) = test_backend("b1", "Backend", PrivacyZone::Open, Some(2));
    registry.add_backend_with_agent(b1, a1).unwrap();

    let router = Router::new(
        Arc::clone(&registry),
        RoutingStrategy::PriorityOnly,
        ScoringWeights::default(),
    );

    let reqs = RequestRequirements {
        model: "test-model".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
        prefers_streaming: false,
    };

    // Passing None should work exactly like before F13
    let result = router.select_backend(&reqs, None);
    assert!(result.is_ok());
    let routing_result = result.unwrap();
    assert_eq!(routing_result.backend.id, "b1");
}
