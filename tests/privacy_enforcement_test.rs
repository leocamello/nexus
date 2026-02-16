//! Integration tests for privacy zone enforcement (US1)
//!
//! Tests T024-T027:
//! - T024: Restricted backend available → routes to it, returns X-Nexus-Privacy-Zone: restricted
//! - T025: Restricted backend offline, open available → 503 with privacy_zone_required
//! - T026: Response includes X-Nexus-Privacy-Zone header matching backend's configured zone
//! - T027: Verify cross-zone failover never happens (restricted never routes to open)

use chrono::Utc;
use nexus::agent::factory::create_agent;
use nexus::agent::PrivacyZone;
use nexus::config::{PolicyMatcher, TrafficPolicy};
use nexus::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Registry};
use nexus::routing::reconciler::decision::RoutingDecision;
use nexus::routing::reconciler::intent::RoutingIntent;
use nexus::routing::reconciler::privacy::PrivacyReconciler;
use nexus::routing::reconciler::request_analyzer::RequestAnalyzer;
use nexus::routing::reconciler::scheduler::SchedulerReconciler;
use nexus::routing::reconciler::{Reconciler, ReconcilerPipeline};
use nexus::routing::{RequestRequirements, RoutingStrategy, ScoringWeights};
use std::collections::HashMap;
use std::sync::Arc;

/// Helper to create a test backend
fn create_test_backend(
    id: &str,
    name: &str,
    backend_type: BackendType,
    zone: PrivacyZone,
    tier: Option<u8>,
    status: BackendStatus,
) -> (Backend, Arc<dyn nexus::agent::InferenceAgent>) {
    let mut metadata = HashMap::new();
    if backend_type == BackendType::OpenAI {
        metadata.insert("api_key".to_string(), "test-key".to_string());
    }

    let backend = Backend {
        id: id.to_string(),
        name: name.to_string(),
        url: format!("http://localhost:{}", 11434),
        backend_type,
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
        last_health_check: Utc::now(),
        last_error: None,
        pending_requests: std::sync::atomic::AtomicU32::new(0),
        total_requests: std::sync::atomic::AtomicU64::new(0),
        avg_latency_ms: std::sync::atomic::AtomicU32::new(0),
        discovery_source: DiscoverySource::Static,
        metadata: metadata.clone(),
    };

    let client = Arc::new(reqwest::Client::new());
    let agent = create_agent(
        id.to_string(),
        name.to_string(),
        format!("http://localhost:{}", 11434),
        backend_type,
        client,
        metadata,
        zone,
        tier,
    )
    .unwrap();

    (backend, agent)
}

#[test]
fn test_restricted_backend_available_routes_correctly() {
    // T024: Restricted backend available → routes to it
    let registry = Arc::new(Registry::new());

    // Create restricted backend (healthy)
    let (backend, agent) = create_test_backend(
        "ollama-local",
        "Local Ollama",
        BackendType::Ollama,
        PrivacyZone::Restricted,
        Some(2),
        BackendStatus::Healthy,
    );

    registry.add_backend_with_agent(backend, agent).unwrap();

    // Create traffic policy that requires restricted zone
    let policy = TrafficPolicy {
        model_pattern: "test-*".to_string(),
        privacy: nexus::config::PrivacyConstraint::Restricted,
        max_cost_per_request: None,
        min_tier: None,
        fallback_allowed: true,
    };

    let policy_matcher = PolicyMatcher::compile(vec![policy]).unwrap();

    // Build reconciler pipeline
    let privacy = PrivacyReconciler::new(Arc::clone(&registry), policy_matcher);
    let scheduler = SchedulerReconciler::new(
        Arc::clone(&registry),
        RoutingStrategy::PriorityOnly,
        ScoringWeights::default(),
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
    );

    let mut pipeline = ReconcilerPipeline::new(vec![
        Box::new(RequestAnalyzer::new(HashMap::new(), Arc::clone(&registry))),
        Box::new(privacy),
        Box::new(scheduler),
    ]);

    // Create routing intent
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

    // Execute pipeline
    let result = pipeline.execute(&mut intent);

    // Should succeed and route to restricted backend
    assert!(result.is_ok());
    if let Ok(RoutingDecision::Route { agent_id, .. }) = result {
        assert_eq!(agent_id, "ollama-local");

        // Verify agent has correct privacy zone
        let agent = registry.get_agent(&agent_id).unwrap();
        assert_eq!(agent.profile().privacy_zone, PrivacyZone::Restricted);
    } else {
        panic!("Expected Route decision, got: {:?}", result);
    }
}

#[test]
fn test_restricted_backend_offline_open_available_returns_503() {
    // T025: Restricted backend offline, open available → 503 with privacy context
    let registry = Arc::new(Registry::new());

    // Create restricted backend (UNHEALTHY)
    let (backend_restricted, agent_restricted) = create_test_backend(
        "ollama-local",
        "Local Ollama",
        BackendType::Ollama,
        PrivacyZone::Restricted,
        Some(2),
        BackendStatus::Unhealthy,
    );

    // Create open backend (HEALTHY)
    let (backend_open, agent_open) = create_test_backend(
        "openai-cloud",
        "OpenAI Cloud",
        BackendType::OpenAI,
        PrivacyZone::Open,
        Some(5),
        BackendStatus::Healthy,
    );

    registry
        .add_backend_with_agent(backend_restricted, agent_restricted)
        .unwrap();
    registry
        .add_backend_with_agent(backend_open, agent_open)
        .unwrap();

    // Create traffic policy that requires restricted zone
    let policy = TrafficPolicy {
        model_pattern: "test-*".to_string(),
        privacy: nexus::config::PrivacyConstraint::Restricted,
        max_cost_per_request: None,
        min_tier: None,
        fallback_allowed: true,
    };

    let policy_matcher = PolicyMatcher::compile(vec![policy]).unwrap();

    // Build reconciler pipeline
    let privacy = PrivacyReconciler::new(Arc::clone(&registry), policy_matcher);
    let scheduler = SchedulerReconciler::new(
        Arc::clone(&registry),
        RoutingStrategy::PriorityOnly,
        ScoringWeights::default(),
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
    );

    let mut pipeline = ReconcilerPipeline::new(vec![
        Box::new(RequestAnalyzer::new(HashMap::new(), Arc::clone(&registry))),
        Box::new(privacy),
        Box::new(scheduler),
    ]);

    // Create routing intent
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

    // Execute pipeline
    let result = pipeline.execute(&mut intent);

    // Should reject (no healthy restricted backends)
    assert!(result.is_ok());
    if let Ok(RoutingDecision::Reject { rejection_reasons }) = result {
        // Verify rejection includes privacy constraint info
        assert!(!rejection_reasons.is_empty());
        let has_privacy_rejection = rejection_reasons
            .iter()
            .any(|r| r.reconciler == "PrivacyReconciler" || r.reason.contains("privacy"));
        assert!(
            has_privacy_rejection,
            "Expected privacy rejection, got: {:?}",
            rejection_reasons
        );

        // Verify intent has privacy constraint set
        assert_eq!(intent.privacy_constraint, Some(PrivacyZone::Restricted));
    } else {
        panic!(
            "Expected Reject decision (cross-zone routing should be blocked), got: {:?}",
            result
        );
    }
}

#[test]
fn test_cross_zone_failover_never_happens() {
    // T027: Verify cross-zone failover never happens
    let registry = Arc::new(Registry::new());

    // Create only an open backend (no restricted backends at all)
    let (backend_open, agent_open) = create_test_backend(
        "openai-cloud",
        "OpenAI Cloud",
        BackendType::OpenAI,
        PrivacyZone::Open,
        Some(5),
        BackendStatus::Healthy,
    );

    registry
        .add_backend_with_agent(backend_open, agent_open)
        .unwrap();

    // Create traffic policy that requires restricted zone
    let policy = TrafficPolicy {
        model_pattern: "test-*".to_string(),
        privacy: nexus::config::PrivacyConstraint::Restricted,
        max_cost_per_request: None,
        min_tier: None,
        fallback_allowed: true,
    };

    let policy_matcher = PolicyMatcher::compile(vec![policy]).unwrap();

    // Build reconciler pipeline
    let privacy = PrivacyReconciler::new(Arc::clone(&registry), policy_matcher);
    let scheduler = SchedulerReconciler::new(
        Arc::clone(&registry),
        RoutingStrategy::PriorityOnly,
        ScoringWeights::default(),
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
    );

    let mut pipeline = ReconcilerPipeline::new(vec![
        Box::new(RequestAnalyzer::new(HashMap::new(), Arc::clone(&registry))),
        Box::new(privacy),
        Box::new(scheduler),
    ]);

    // Create routing intent
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

    // Execute pipeline
    let result = pipeline.execute(&mut intent);

    // Should REJECT - must never route to open backend for restricted request
    assert!(result.is_ok());
    match result {
        Ok(RoutingDecision::Reject { rejection_reasons }) => {
            // Success: cross-zone routing was correctly blocked
            assert!(!rejection_reasons.is_empty());
            assert_eq!(intent.privacy_constraint, Some(PrivacyZone::Restricted));
        }
        Ok(RoutingDecision::Route { agent_id, .. }) => {
            // This should NEVER happen - routing to open backend for restricted model!
            panic!("FAILURE: Cross-zone routing happened! Routed to {} (Open zone) despite Restricted requirement", agent_id);
        }
        Ok(RoutingDecision::Queue { .. }) => {
            panic!("Unexpected Queue decision");
        }
        Err(e) => {
            panic!("Unexpected error: {:?}", e);
        }
    }
}

#[test]
fn test_privacy_zone_header_in_response() {
    // T026: Response includes X-Nexus-Privacy-Zone header
    // This is tested at the header module level
    use nexus::api::headers::NexusTransparentHeaders;
    use nexus::api::headers::RouteReason;

    // Test restricted backend
    let headers_restricted = NexusTransparentHeaders::new(
        "ollama-local".to_string(),
        BackendType::Ollama,
        RouteReason::CapabilityMatch,
        PrivacyZone::Restricted,
        None,
    );

    let mut response = axum::http::Response::new(());
    headers_restricted.inject_into_response(&mut response);

    let privacy_zone_header = response
        .headers()
        .get("x-nexus-privacy-zone")
        .expect("X-Nexus-Privacy-Zone header should be present");

    assert_eq!(privacy_zone_header, "restricted");

    // Test open backend
    let headers_open = NexusTransparentHeaders::new(
        "openai-cloud".to_string(),
        BackendType::OpenAI,
        RouteReason::CapabilityMatch,
        PrivacyZone::Open,
        None,
    );

    let mut response = axum::http::Response::new(());
    headers_open.inject_into_response(&mut response);

    let privacy_zone_header = response
        .headers()
        .get("x-nexus-privacy-zone")
        .expect("X-Nexus-Privacy-Zone header should be present");

    assert_eq!(privacy_zone_header, "open");
}
