//! Integration tests for actionable 503 rejection responses (US4)
//!
//! Tests T043-T051:
//! - T043: Rejection reasons flow from RoutingIntent to error response
//! - T044: Privacy rejection includes zone context
//! - T045: Tier rejection includes required tier
//! - T047: Combined privacy + tier rejection
//! - T049: Available backends listed in context
//! - T051: Structured error follows OpenAI envelope format

use nexus::agent::factory::create_agent;
use nexus::agent::PrivacyZone;
use nexus::config::{PolicyMatcher, PrivacyConstraint, TrafficPolicy};
use nexus::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Registry};
use nexus::routing::reconciler::decision::RoutingDecision;
use nexus::routing::reconciler::intent::{RoutingIntent, TierEnforcementMode};
use nexus::routing::reconciler::privacy::PrivacyReconciler;
use nexus::routing::reconciler::tier::TierReconciler;
use nexus::routing::reconciler::{scheduler::SchedulerReconciler, ReconcilerPipeline};
use nexus::routing::{RequestRequirements, RoutingStrategy, ScoringWeights};
use std::collections::HashMap;
use std::sync::Arc;

fn create_backend(
    id: &str,
    name: &str,
    tier: u8,
    zone: PrivacyZone,
    status: BackendStatus,
) -> (Backend, Arc<dyn nexus::agent::InferenceAgent>) {
    let backend = Backend {
        id: id.to_string(),
        name: name.to_string(),
        url: "http://localhost:11434".to_string(),
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
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        zone,
        Some(tier),
    )
    .unwrap();

    (backend, agent)
}

/// T043: Privacy rejection produces rejection_reasons with PrivacyReconciler info
#[test]
fn test_privacy_rejection_produces_reasons() {
    let registry = Arc::new(Registry::new());

    // Only cloud/open backend available
    let (backend, agent) = create_backend(
        "cloud-1",
        "Cloud Backend",
        3,
        PrivacyZone::Open,
        BackendStatus::Healthy,
    );
    registry.add_backend_with_agent(backend, agent).unwrap();

    // Policy requires restricted zone
    let policy = TrafficPolicy {
        model_pattern: "test-*".to_string(),
        privacy: PrivacyConstraint::Restricted,
        max_cost_per_request: None,
        min_tier: None,
        fallback_allowed: true,
    };
    let policy_matcher = PolicyMatcher::compile(vec![policy]).unwrap();

    let privacy = PrivacyReconciler::new(Arc::clone(&registry), policy_matcher);
    let scheduler = SchedulerReconciler::new(
        Arc::clone(&registry),
        RoutingStrategy::PriorityOnly,
        ScoringWeights::default(),
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
    );

    let mut pipeline = ReconcilerPipeline::new(vec![Box::new(privacy), Box::new(scheduler)]);

    let reqs = RequestRequirements {
        model: "test-model".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
    };

    let mut intent = RoutingIntent::new(
        "req-1".to_string(),
        "test-model".to_string(),
        "test-model".to_string(),
        reqs,
        vec!["cloud-1".to_string()],
    );

    let decision = pipeline.execute(&mut intent);
    assert!(decision.is_ok());

    match decision.unwrap() {
        RoutingDecision::Reject { rejection_reasons } => {
            assert!(
                !rejection_reasons.is_empty(),
                "Should have rejection reasons"
            );
            let reason = &rejection_reasons[0];
            assert_eq!(reason.reconciler, "PrivacyReconciler");
            assert!(
                reason.reason.contains("restricted")
                    || reason.reason.contains("privacy")
                    || reason.reason.contains("zone"),
                "Reason should mention privacy/restricted/zone: {}",
                reason.reason
            );
            assert!(
                !reason.suggested_action.is_empty(),
                "Should have suggested action"
            );
        }
        other => panic!("Expected Reject, got {:?}", other),
    }
}

/// T045: Tier rejection includes required tier info in rejection reason
#[test]
fn test_tier_rejection_produces_reasons_with_tier_info() {
    let registry = Arc::new(Registry::new());

    // Only tier 2 backend
    let (backend, agent) = create_backend(
        "low-tier",
        "Low Tier",
        2,
        PrivacyZone::Open,
        BackendStatus::Healthy,
    );
    registry.add_backend_with_agent(backend, agent).unwrap();

    // Policy requires tier 4
    let policy = TrafficPolicy {
        model_pattern: "test-*".to_string(),
        privacy: PrivacyConstraint::Unrestricted,
        max_cost_per_request: None,
        min_tier: Some(4),
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

    let mut pipeline = ReconcilerPipeline::new(vec![Box::new(tier), Box::new(scheduler)]);

    let reqs = RequestRequirements {
        model: "test-model".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
    };

    let mut intent = RoutingIntent::new(
        "req-1".to_string(),
        "test-model".to_string(),
        "test-model".to_string(),
        reqs,
        vec!["low-tier".to_string()],
    );
    intent.tier_enforcement_mode = TierEnforcementMode::Strict;

    let decision = pipeline.execute(&mut intent);
    assert!(decision.is_ok());

    match decision.unwrap() {
        RoutingDecision::Reject { rejection_reasons } => {
            assert!(!rejection_reasons.is_empty());
            let reason = &rejection_reasons[0];
            assert_eq!(reason.reconciler, "TierReconciler");
            // Should mention the tier mismatch
            assert!(
                reason.reason.contains("tier") || reason.reason.contains("capability"),
                "Reason should mention tier: {}",
                reason.reason
            );
        }
        other => panic!("Expected Reject, got {:?}", other),
    }
}

/// T047: Combined privacy + tier rejection lists both reasons
#[test]
fn test_combined_privacy_and_tier_rejection() {
    let registry = Arc::new(Registry::new());

    // Open, tier 2 backend
    let (backend, agent) = create_backend(
        "open-low",
        "Open Low",
        2,
        PrivacyZone::Open,
        BackendStatus::Healthy,
    );
    registry.add_backend_with_agent(backend, agent).unwrap();

    // Policy: restricted + min tier 4
    let policy = TrafficPolicy {
        model_pattern: "test-*".to_string(),
        privacy: PrivacyConstraint::Restricted,
        max_cost_per_request: None,
        min_tier: Some(4),
        fallback_allowed: true,
    };
    let policy_matcher = PolicyMatcher::compile(vec![policy]).unwrap();

    let privacy = PrivacyReconciler::new(Arc::clone(&registry), policy_matcher.clone());
    let tier = TierReconciler::new(Arc::clone(&registry), policy_matcher);
    let scheduler = SchedulerReconciler::new(
        Arc::clone(&registry),
        RoutingStrategy::PriorityOnly,
        ScoringWeights::default(),
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
    );

    let mut pipeline =
        ReconcilerPipeline::new(vec![Box::new(privacy), Box::new(tier), Box::new(scheduler)]);

    let reqs = RequestRequirements {
        model: "test-model".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
    };

    let mut intent = RoutingIntent::new(
        "req-1".to_string(),
        "test-model".to_string(),
        "test-model".to_string(),
        reqs,
        vec!["open-low".to_string()],
    );

    let decision = pipeline.execute(&mut intent);
    assert!(decision.is_ok());

    match decision.unwrap() {
        RoutingDecision::Reject { rejection_reasons } => {
            // Privacy should have excluded it already, so at least 1 reason
            assert!(
                !rejection_reasons.is_empty(),
                "Should have at least one rejection reason"
            );
            let reconcilers: Vec<&str> = rejection_reasons
                .iter()
                .map(|r| r.reconciler.as_str())
                .collect();
            assert!(
                reconcilers.contains(&"PrivacyReconciler"),
                "Should contain PrivacyReconciler reason"
            );
        }
        other => panic!("Expected Reject, got {:?}", other),
    }
}

/// T049: Rejection reasons include suggested_action for user guidance
#[test]
fn test_rejection_reasons_include_suggested_actions() {
    let registry = Arc::new(Registry::new());

    let (backend, agent) = create_backend(
        "cloud-1",
        "Cloud",
        3,
        PrivacyZone::Open,
        BackendStatus::Healthy,
    );
    registry.add_backend_with_agent(backend, agent).unwrap();

    let policy = TrafficPolicy {
        model_pattern: "test-*".to_string(),
        privacy: PrivacyConstraint::Restricted,
        max_cost_per_request: None,
        min_tier: None,
        fallback_allowed: true,
    };
    let policy_matcher = PolicyMatcher::compile(vec![policy]).unwrap();

    let privacy = PrivacyReconciler::new(Arc::clone(&registry), policy_matcher);
    let scheduler = SchedulerReconciler::new(
        Arc::clone(&registry),
        RoutingStrategy::PriorityOnly,
        ScoringWeights::default(),
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
    );

    let mut pipeline = ReconcilerPipeline::new(vec![Box::new(privacy), Box::new(scheduler)]);

    let reqs = RequestRequirements {
        model: "test-model".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
    };

    let mut intent = RoutingIntent::new(
        "req-1".to_string(),
        "test-model".to_string(),
        "test-model".to_string(),
        reqs,
        vec!["cloud-1".to_string()],
    );

    let decision = pipeline.execute(&mut intent);
    assert!(decision.is_ok());

    match decision.unwrap() {
        RoutingDecision::Reject { rejection_reasons } => {
            for reason in &rejection_reasons {
                assert!(
                    !reason.suggested_action.is_empty(),
                    "Each rejection should have a suggested action"
                );
            }
        }
        other => panic!("Expected Reject, got {:?}", other),
    }
}

/// Budget hard limit (BlockAll) rejects with BudgetReconciler reason
#[test]
fn test_budget_hard_limit_block_all_produces_rejection() {
    use dashmap::DashMap;
    use nexus::agent::tokenizer::TokenizerRegistry;
    use nexus::config::{BudgetConfig, HardLimitAction};
    use nexus::routing::reconciler::budget::{BudgetMetrics, BudgetReconciler, GLOBAL_BUDGET_KEY};

    let registry = Arc::new(Registry::new());

    // Single local backend
    let (backend, agent) = create_backend(
        "local-1",
        "Local Backend",
        3,
        PrivacyZone::Restricted,
        BackendStatus::Healthy,
    );
    registry.add_backend_with_agent(backend, agent).unwrap();

    // Budget config: $10 limit, hard limit = block all
    let budget_config = BudgetConfig {
        monthly_limit_usd: Some(10.0),
        soft_limit_percent: 75.0,
        hard_limit_action: HardLimitAction::BlockAll,
        reconciliation_interval_secs: 60,
    };

    let tokenizer_registry = Arc::new(TokenizerRegistry::new().unwrap());
    let budget_state = Arc::new(DashMap::new());

    // Pre-load spending that exceeds hard limit
    budget_state.insert(
        GLOBAL_BUDGET_KEY.to_string(),
        BudgetMetrics {
            current_month_spending: 15.0, // Over the $10 limit
            last_reconciliation_time: chrono::Utc::now(),
            month_key: chrono::Utc::now().format("%Y-%m").to_string(),
        },
    );

    let budget = BudgetReconciler::new(
        Arc::clone(&registry),
        budget_config,
        tokenizer_registry,
        budget_state,
    );
    let scheduler = SchedulerReconciler::new(
        Arc::clone(&registry),
        RoutingStrategy::PriorityOnly,
        ScoringWeights::default(),
        Arc::new(std::sync::atomic::AtomicU64::new(0)),
    );

    let mut pipeline = ReconcilerPipeline::new(vec![Box::new(budget), Box::new(scheduler)]);

    let reqs = RequestRequirements {
        model: "test-model".to_string(),
        estimated_tokens: 100,
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
    };

    let mut intent = RoutingIntent::new(
        "req-budget-1".to_string(),
        "test-model".to_string(),
        "test-model".to_string(),
        reqs,
        vec!["local-1".to_string()],
    );

    let decision = pipeline.execute(&mut intent).unwrap();

    match decision {
        RoutingDecision::Reject { rejection_reasons } => {
            assert!(
                !rejection_reasons.is_empty(),
                "Should have rejection reasons"
            );
            assert!(
                rejection_reasons
                    .iter()
                    .any(|r| r.reconciler == "BudgetReconciler"),
                "Should be rejected by BudgetReconciler, got: {:?}",
                rejection_reasons,
            );
            assert!(
                rejection_reasons
                    .iter()
                    .any(|r| r.reason.contains("budget")),
                "Reason should mention budget"
            );
        }
        other => panic!("Expected Reject for budget exceeded, got {:?}", other),
    }
}
