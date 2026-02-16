//! Integration tests for tier enforcement with strict/flexible modes (US2)
//!
//! Tests T037-T042:
//! - T037: No headers → strict mode (default) → only exact/higher tier accepted
//! - T038: X-Nexus-Strict: true → exact model matching enforced
//! - T039: X-Nexus-Flexible: true → higher tier substitution allowed
//! - T040: Tier 3 backend offline, tier 2 available, flexible mode → 503 (never downgrade)
//! - T041: Tier 3 backend offline, tier 4 available, flexible mode → routes to tier 4
//! - T042: Conflicting headers (both strict and flexible) → strict wins

use axum::http::{HeaderMap, HeaderValue};
use nexus::agent::factory::create_agent;
use nexus::agent::PrivacyZone;
use nexus::config::{PolicyMatcher, TrafficPolicy};
use nexus::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Registry};
use nexus::routing::reconciler::decision::RoutingDecision;
use nexus::routing::reconciler::intent::{RoutingIntent, TierEnforcementMode};
use nexus::routing::reconciler::tier::TierReconciler;
use nexus::routing::reconciler::{ReconcilerPipeline, request_analyzer::RequestAnalyzer, scheduler::SchedulerReconciler};
use nexus::routing::{RequestRequirements, RoutingStrategy, ScoringWeights};
use std::collections::HashMap;
use std::sync::Arc;

/// Helper to create a test backend with specific tier
fn create_test_backend_with_tier(
    id: &str,
    name: &str,
    tier: u8,
    status: BackendStatus,
) -> (Backend, Arc<dyn nexus::agent::InferenceAgent>) {
    let backend = Backend {
        id: id.to_string(),
        name: name.to_string(),
        url: format!("http://localhost:{}", 11434),
        backend_type: BackendType::Ollama,
        status,
        models: vec![nexus::registry::Model {
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
    };

    let client = Arc::new(reqwest::Client::new());
    let agent = create_agent(
        id.to_string(),
        name.to_string(),
        format!("http://localhost:{}", 11434),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        Some(tier),
    )
    .unwrap();

    (backend, agent)
}

#[test]
fn test_no_headers_defaults_to_strict_mode() {
    // T037: No headers → strict mode → only exact/higher tier accepted
    let registry = Arc::new(Registry::new());

    // Create backends with different tiers
    let (backend_tier2, agent_tier2) =
        create_test_backend_with_tier("backend-tier2", "Tier 2", 2, BackendStatus::Healthy);
    let (backend_tier3, agent_tier3) =
        create_test_backend_with_tier("backend-tier3", "Tier 3", 3, BackendStatus::Healthy);

    registry
        .add_backend_with_agent(backend_tier2, agent_tier2)
        .unwrap();
    registry
        .add_backend_with_agent(backend_tier3, agent_tier3)
        .unwrap();

    // Create traffic policy requiring tier 3
    let policy = TrafficPolicy {
        model_pattern: "test-*".to_string(),
        privacy: nexus::config::PrivacyConstraint::Unrestricted,
        max_cost_per_request: None,
        min_tier: Some(3),
        fallback_allowed: true,
    };

    let policy_matcher = PolicyMatcher::compile(vec![policy]).unwrap();

    // Build reconciler pipeline
    let tier = TierReconciler::new(Arc::clone(&registry), policy_matcher);
    let scheduler = SchedulerReconciler::new(
        Arc::clone(&registry),
        RoutingStrategy::PriorityOnly,
        ScoringWeights::default(),
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
    );

    let mut pipeline = ReconcilerPipeline::new(vec![
        Box::new(RequestAnalyzer::new(HashMap::new(), Arc::clone(&registry))),
        Box::new(tier),
        Box::new(scheduler),
    ]);

    let requirements = RequestRequirements {
        model: "test-model".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
    };

    let mut intent = RoutingIntent::new(
        "test-request".to_string(),
        "test-model".to_string(),
        "test-model".to_string(),
        requirements,
        vec![],
    );

    // Default mode is Strict (no headers)
    assert_eq!(intent.tier_enforcement_mode, TierEnforcementMode::Strict);

    let result = pipeline.execute(&mut intent);

    // Should route to tier 3 backend (tier 2 is excluded in strict mode)
    assert!(result.is_ok());
    if let Ok(RoutingDecision::Route { agent_id, .. }) = result {
        assert_eq!(agent_id, "backend-tier3");
    } else {
        panic!("Expected Route to tier 3 backend, got: {:?}", result);
    }
}

#[test]
fn test_strict_header_enforces_exact_tier_matching() {
    // T038: X-Nexus-Strict: true → exact tier matching
    // Same as T037 - strict mode excludes lower tiers
    let registry = Arc::new(Registry::new());

    let (backend_tier2, agent_tier2) =
        create_test_backend_with_tier("backend-tier2", "Tier 2", 2, BackendStatus::Healthy);
    let (backend_tier3, agent_tier3) =
        create_test_backend_with_tier("backend-tier3", "Tier 3", 3, BackendStatus::Healthy);

    registry
        .add_backend_with_agent(backend_tier2, agent_tier2)
        .unwrap();
    registry
        .add_backend_with_agent(backend_tier3, agent_tier3)
        .unwrap();

    let policy = TrafficPolicy {
        model_pattern: "test-*".to_string(),
        privacy: nexus::config::PrivacyConstraint::Unrestricted,
        max_cost_per_request: None,
        min_tier: Some(3),
        fallback_allowed: true,
    };

    let policy_matcher = PolicyMatcher::compile(vec![policy]).unwrap();
    let tier = TierReconciler::new(Arc::clone(&registry), policy_matcher);
    let scheduler = SchedulerReconciler::new(
        Arc::clone(&registry),
        RoutingStrategy::PriorityOnly,
        ScoringWeights::default(),
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
    );

    let mut pipeline = ReconcilerPipeline::new(vec![
        Box::new(RequestAnalyzer::new(HashMap::new(), Arc::clone(&registry))),
        Box::new(tier),
        Box::new(scheduler),
    ]);

    let requirements = RequestRequirements {
        model: "test-model".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
    };

    let mut intent = RoutingIntent::new(
        "test-request".to_string(),
        "test-model".to_string(),
        "test-model".to_string(),
        requirements,
        vec![],
    );

    // Explicitly set Strict mode
    intent.tier_enforcement_mode = TierEnforcementMode::Strict;

    let result = pipeline.execute(&mut intent);

    // Should route to tier 3, excluding tier 2
    assert!(result.is_ok());
    if let Ok(RoutingDecision::Route { agent_id, .. }) = result {
        assert_eq!(agent_id, "backend-tier3");

        // Verify tier 2 was excluded
        let tier2_rejected = intent
            .rejection_reasons
            .iter()
            .any(|r| r.agent_id == "backend-tier2" && r.reconciler == "TierReconciler");
        assert!(tier2_rejected, "Tier 2 backend should be rejected in strict mode");
    } else {
        panic!("Expected Route decision, got: {:?}", result);
    }
}

#[test]
fn test_flexible_header_allows_higher_tier_substitution() {
    // T039: X-Nexus-Flexible: true → higher tier substitution allowed
    let registry = Arc::new(Registry::new());

    // Only tier 4 available (higher than required tier 3)
    let (backend_tier4, agent_tier4) =
        create_test_backend_with_tier("backend-tier4", "Tier 4", 4, BackendStatus::Healthy);

    registry
        .add_backend_with_agent(backend_tier4, agent_tier4)
        .unwrap();

    let policy = TrafficPolicy {
        model_pattern: "test-*".to_string(),
        privacy: nexus::config::PrivacyConstraint::Unrestricted,
        max_cost_per_request: None,
        min_tier: Some(3),
        fallback_allowed: true,
    };

    let policy_matcher = PolicyMatcher::compile(vec![policy]).unwrap();
    let tier = TierReconciler::new(Arc::clone(&registry), policy_matcher);
    let scheduler = SchedulerReconciler::new(
        Arc::clone(&registry),
        RoutingStrategy::PriorityOnly,
        ScoringWeights::default(),
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
    );

    let mut pipeline = ReconcilerPipeline::new(vec![
        Box::new(RequestAnalyzer::new(HashMap::new(), Arc::clone(&registry))),
        Box::new(tier),
        Box::new(scheduler),
    ]);

    let requirements = RequestRequirements {
        model: "test-model".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
    };

    let mut intent = RoutingIntent::new(
        "test-request".to_string(),
        "test-model".to_string(),
        "test-model".to_string(),
        requirements,
        vec![],
    );

    // Set Flexible mode
    intent.tier_enforcement_mode = TierEnforcementMode::Flexible;

    let result = pipeline.execute(&mut intent);

    // Should route to tier 4 (higher tier substitution allowed)
    assert!(result.is_ok());
    if let Ok(RoutingDecision::Route { agent_id, .. }) = result {
        assert_eq!(agent_id, "backend-tier4");
    } else {
        panic!(
            "Expected Route to higher tier backend in flexible mode, got: {:?}",
            result
        );
    }
}

#[test]
fn test_flexible_mode_never_downgrades() {
    // T040: Tier 3 offline, tier 2 available, flexible mode → 503 (never downgrade)
    let registry = Arc::new(Registry::new());

    // Tier 3 UNHEALTHY, tier 2 HEALTHY
    let (backend_tier3, agent_tier3) =
        create_test_backend_with_tier("backend-tier3", "Tier 3", 3, BackendStatus::Unhealthy);
    let (backend_tier2, agent_tier2) =
        create_test_backend_with_tier("backend-tier2", "Tier 2", 2, BackendStatus::Healthy);

    registry
        .add_backend_with_agent(backend_tier3, agent_tier3)
        .unwrap();
    registry
        .add_backend_with_agent(backend_tier2, agent_tier2)
        .unwrap();

    let policy = TrafficPolicy {
        model_pattern: "test-*".to_string(),
        privacy: nexus::config::PrivacyConstraint::Unrestricted,
        max_cost_per_request: None,
        min_tier: Some(3),
        fallback_allowed: true,
    };

    let policy_matcher = PolicyMatcher::compile(vec![policy]).unwrap();
    let tier = TierReconciler::new(Arc::clone(&registry), policy_matcher);
    let scheduler = SchedulerReconciler::new(
        Arc::clone(&registry),
        RoutingStrategy::PriorityOnly,
        ScoringWeights::default(),
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
    );

    let mut pipeline = ReconcilerPipeline::new(vec![
        Box::new(RequestAnalyzer::new(HashMap::new(), Arc::clone(&registry))),
        Box::new(tier),
        Box::new(scheduler),
    ]);

    let requirements = RequestRequirements {
        model: "test-model".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
    };

    let mut intent = RoutingIntent::new(
        "test-request".to_string(),
        "test-model".to_string(),
        "test-model".to_string(),
        requirements,
        vec![],
    );

    // Set Flexible mode
    intent.tier_enforcement_mode = TierEnforcementMode::Flexible;

    let result = pipeline.execute(&mut intent);

    // Should REJECT - flexible mode allows upgrades but NEVER downgrades
    assert!(result.is_ok());
    match result {
        Ok(RoutingDecision::Reject { rejection_reasons }) => {
            // Success: downgrade was correctly blocked
            assert!(!rejection_reasons.is_empty());
            let tier_rejection = rejection_reasons
                .iter()
                .any(|r| r.reconciler == "TierReconciler");
            assert!(
                tier_rejection,
                "Expected tier rejection in flexible mode when only lower tiers available"
            );
        }
        Ok(RoutingDecision::Route { agent_id, .. }) => {
            panic!(
                "FAILURE: Downgrade happened! Routed to {} despite requiring tier 3+",
                agent_id
            );
        }
        _ => {
            panic!("Unexpected result: {:?}", result);
        }
    }
}

#[test]
fn test_flexible_mode_upgrades_to_available_higher_tier() {
    // T041: Tier 3 offline, tier 4 available, flexible mode → routes to tier 4
    let registry = Arc::new(Registry::new());

    // Tier 3 UNHEALTHY, tier 4 HEALTHY
    let (backend_tier3, agent_tier3) =
        create_test_backend_with_tier("backend-tier3", "Tier 3", 3, BackendStatus::Unhealthy);
    let (backend_tier4, agent_tier4) =
        create_test_backend_with_tier("backend-tier4", "Tier 4", 4, BackendStatus::Healthy);

    registry
        .add_backend_with_agent(backend_tier3, agent_tier3)
        .unwrap();
    registry
        .add_backend_with_agent(backend_tier4, agent_tier4)
        .unwrap();

    let policy = TrafficPolicy {
        model_pattern: "test-*".to_string(),
        privacy: nexus::config::PrivacyConstraint::Unrestricted,
        max_cost_per_request: None,
        min_tier: Some(3),
        fallback_allowed: true,
    };

    let policy_matcher = PolicyMatcher::compile(vec![policy]).unwrap();
    let tier = TierReconciler::new(Arc::clone(&registry), policy_matcher);
    let scheduler = SchedulerReconciler::new(
        Arc::clone(&registry),
        RoutingStrategy::PriorityOnly,
        ScoringWeights::default(),
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
    );

    let mut pipeline = ReconcilerPipeline::new(vec![
        Box::new(RequestAnalyzer::new(HashMap::new(), Arc::clone(&registry))),
        Box::new(tier),
        Box::new(scheduler),
    ]);

    let requirements = RequestRequirements {
        model: "test-model".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
    };

    let mut intent = RoutingIntent::new(
        "test-request".to_string(),
        "test-model".to_string(),
        "test-model".to_string(),
        requirements,
        vec![],
    );

    // Set Flexible mode
    intent.tier_enforcement_mode = TierEnforcementMode::Flexible;

    let result = pipeline.execute(&mut intent);

    // Should route to tier 4 (flexible mode allows upgrade)
    assert!(result.is_ok());
    if let Ok(RoutingDecision::Route { agent_id, .. }) = result {
        assert_eq!(agent_id, "backend-tier4");
    } else {
        panic!(
            "Expected Route to tier 4 in flexible mode, got: {:?}",
            result
        );
    }
}

#[test]
fn test_conflicting_headers_strict_wins() {
    // T042: Both strict and flexible headers → strict takes precedence
    // This behavior is verified by the extract_tier_enforcement_mode() tests in completions.rs
    // Here we verify that when Strict mode is set, it behaves correctly
    
    let registry = Arc::new(Registry::new());
    
    // Create tier 2 and tier 3 backends
    let (backend_tier2, agent_tier2) =
        create_test_backend_with_tier("backend-tier2", "Tier 2", 2, BackendStatus::Healthy);
    let (backend_tier3, agent_tier3) =
        create_test_backend_with_tier("backend-tier3", "Tier 3", 3, BackendStatus::Healthy);
    
    registry.add_backend_with_agent(backend_tier2, agent_tier2).unwrap();
    registry.add_backend_with_agent(backend_tier3, agent_tier3).unwrap();

    let policy = TrafficPolicy {
        model_pattern: "test-*".to_string(),
        privacy: nexus::config::PrivacyConstraint::Unrestricted,
        max_cost_per_request: None,
        min_tier: Some(3),
        fallback_allowed: true,
    };

    let policy_matcher = PolicyMatcher::compile(vec![policy]).unwrap();
    let tier = TierReconciler::new(Arc::clone(&registry), policy_matcher);
    let scheduler = SchedulerReconciler::new(
        Arc::clone(&registry),
        RoutingStrategy::PriorityOnly,
        ScoringWeights::default(),
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
    );
    
    let mut pipeline = ReconcilerPipeline::new(vec![
        Box::new(RequestAnalyzer::new(HashMap::new(), Arc::clone(&registry))),
        Box::new(tier),
        Box::new(scheduler),
    ]);
    
    let requirements = RequestRequirements {
        model: "test-model".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
    };

    let mut intent = RoutingIntent::new(
        "test-request".to_string(),
        "test-model".to_string(),
        "test-model".to_string(),
        requirements,
        vec![],
    );

    // When both headers are present, extract_tier_enforcement_mode() returns Strict
    // We verify that Strict mode correctly filters out tier 2
    intent.tier_enforcement_mode = TierEnforcementMode::Strict;

    let result = pipeline.execute(&mut intent);

    // Should route to tier 3, excluding tier 2 (strict mode behavior)
    assert!(result.is_ok());
    if let Ok(RoutingDecision::Route { agent_id, .. }) = result {
        assert_eq!(agent_id, "backend-tier3");
        
        // Verify tier 2 was rejected
        let tier2_rejected = intent
            .rejection_reasons
            .iter()
            .any(|r| r.agent_id == "backend-tier2");
        assert!(tier2_rejected, "Tier 2 should be rejected when Strict mode is active");
    } else {
        panic!("Expected Route decision, got: {:?}", result);
    }
}
