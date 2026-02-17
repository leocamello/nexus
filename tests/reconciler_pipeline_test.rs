//! Integration tests for the Reconciler Pipeline (Control Plane Phase 2)
//!
//! Tests the full pipeline end-to-end: RequestAnalyzer → Privacy → Budget
//! → Tier → Quality → Scheduler, verifying that reconcilers compose correctly.

mod common;

use dashmap::DashMap;
use nexus::agent::quality::QualityMetricsStore;
use nexus::agent::tokenizer::TokenizerRegistry;
use nexus::config::routing::{BudgetConfig, PolicyMatcher, PrivacyConstraint, TrafficPolicy};
use nexus::config::QualityConfig;
use nexus::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model, Registry};
use nexus::routing::reconciler::budget::BudgetReconciler;
use nexus::routing::reconciler::decision::RoutingDecision;
use nexus::routing::reconciler::intent::{BudgetStatus, RoutingIntent};
use nexus::routing::reconciler::privacy::PrivacyReconciler;
use nexus::routing::reconciler::quality::QualityReconciler;
use nexus::routing::reconciler::scheduler::SchedulerReconciler;
use nexus::routing::reconciler::tier::TierReconciler;
use nexus::routing::reconciler::Reconciler;
use nexus::routing::reconciler::ReconcilerPipeline;
use nexus::routing::scoring::ScoringWeights;
use nexus::routing::strategies::RoutingStrategy;
use nexus::routing::RequestRequirements;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::Arc;

fn create_backend(
    id: &str,
    name: &str,
    backend_type: BackendType,
    status: BackendStatus,
    model_id: &str,
    priority: i32,
) -> Backend {
    Backend {
        id: id.to_string(),
        name: name.to_string(),
        url: format!("http://{}", id),
        backend_type,
        status,
        last_health_check: chrono::Utc::now(),
        last_error: None,
        models: vec![Model {
            id: model_id.to_string(),
            name: model_id.to_string(),
            context_length: 128000,
            supports_vision: true,
            supports_tools: true,
            supports_json_mode: true,
            max_output_tokens: Some(4096),
        }],
        priority,
        pending_requests: AtomicU32::new(0),
        total_requests: AtomicU64::new(0),
        avg_latency_ms: AtomicU32::new(50),
        discovery_source: DiscoverySource::Static,
        metadata: HashMap::new(),
    }
}

fn create_intent(model: &str, candidates: Vec<&str>) -> RoutingIntent {
    RoutingIntent::new(
        "req-test".to_string(),
        model.to_string(),
        model.to_string(),
        RequestRequirements {
            model: model.to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            prefers_streaming: false,
        },
        candidates.into_iter().map(|s| s.to_string()).collect(),
    )
}

/// Test: Privacy reconciler excludes cloud agents when constraint is Restricted
#[test]
fn privacy_excludes_cloud_agents_with_restricted_constraint() {
    let registry = Arc::new(Registry::new());

    registry
        .add_backend(create_backend(
            "local-1",
            "Local Ollama",
            BackendType::Ollama,
            BackendStatus::Healthy,
            "llama3:8b",
            1,
        ))
        .unwrap();

    registry
        .add_backend(create_backend(
            "cloud-1",
            "OpenAI GPT-4",
            BackendType::OpenAI,
            BackendStatus::Healthy,
            "llama3:8b",
            2,
        ))
        .unwrap();

    let policies = vec![TrafficPolicy {
        model_pattern: "llama3:*".to_string(),
        privacy: PrivacyConstraint::Restricted,
        max_cost_per_request: None,
        min_tier: None,
        fallback_allowed: true,
    }];
    let matcher = PolicyMatcher::compile(policies).unwrap();

    let privacy = PrivacyReconciler::new(Arc::clone(&registry), matcher);
    let scheduler = {
        let qcfg = QualityConfig::default();
        let qstore = std::sync::Arc::new(QualityMetricsStore::new(qcfg.clone()));
        SchedulerReconciler::new(
            Arc::clone(&registry),
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            Arc::new(std::sync::atomic::AtomicU64::new(0)),
            qstore,
            qcfg,
        )
    };

    let mut pipeline = ReconcilerPipeline::new(vec![Box::new(privacy), Box::new(scheduler)]);

    let mut intent = create_intent("llama3:8b", vec!["local-1", "cloud-1"]);

    let decision = pipeline.execute(&mut intent).unwrap();

    match decision {
        RoutingDecision::Route { agent_id, .. } => {
            assert_eq!(agent_id, "local-1", "Should route to local backend");
        }
        _ => panic!("Expected Route decision, got {:?}", decision),
    }

    // Cloud agent should be in excluded list
    assert!(intent.excluded_agents.contains(&"cloud-1".to_string()));
}

/// Test: Full pipeline with no policies passes everything through
#[test]
fn full_pipeline_no_policies_routes_normally() {
    let registry = Arc::new(Registry::new());

    registry
        .add_backend(create_backend(
            "b1",
            "Backend 1",
            BackendType::Ollama,
            BackendStatus::Healthy,
            "llama3:8b",
            1,
        ))
        .unwrap();

    let empty_matcher = PolicyMatcher::compile(vec![]).unwrap();
    let budget_config = BudgetConfig::default();
    let budget_state = Arc::new(DashMap::new());
    let tokenizer_registry =
        Arc::new(TokenizerRegistry::new().expect("Failed to create TokenizerRegistry"));

    let privacy = PrivacyReconciler::new(Arc::clone(&registry), empty_matcher);
    let budget = BudgetReconciler::new(
        Arc::clone(&registry),
        budget_config,
        Arc::clone(&tokenizer_registry),
        Arc::clone(&budget_state),
    );
    let tier = TierReconciler::new(
        Arc::clone(&registry),
        PolicyMatcher::compile(vec![]).unwrap(),
    );
    let quality = {
        let qcfg = QualityConfig::default();
        let qstore = std::sync::Arc::new(QualityMetricsStore::new(qcfg.clone()));
        QualityReconciler::new(qstore, qcfg)
    };
    let scheduler = {
        let qcfg = QualityConfig::default();
        let qstore = std::sync::Arc::new(QualityMetricsStore::new(qcfg.clone()));
        SchedulerReconciler::new(
            Arc::clone(&registry),
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            Arc::new(std::sync::atomic::AtomicU64::new(0)),
            qstore,
            qcfg,
        )
    };

    let mut pipeline = ReconcilerPipeline::new(vec![
        Box::new(privacy),
        Box::new(budget),
        Box::new(tier),
        Box::new(quality),
        Box::new(scheduler),
    ]);

    let mut intent = create_intent("llama3:8b", vec!["b1"]);
    let decision = pipeline.execute(&mut intent).unwrap();

    match decision {
        RoutingDecision::Route { agent_id, .. } => {
            assert_eq!(agent_id, "b1");
        }
        _ => panic!("Expected Route decision"),
    }
}

/// Test: Pipeline produces actionable reject with all rejection reasons
#[test]
fn pipeline_produces_reject_with_reasons() {
    let registry = Arc::new(Registry::new());

    // Unhealthy backend — will be rejected by scheduler
    registry
        .add_backend(create_backend(
            "b1",
            "Unhealthy Backend",
            BackendType::Ollama,
            BackendStatus::Unhealthy,
            "llama3:8b",
            1,
        ))
        .unwrap();

    let scheduler = {
        let qcfg = QualityConfig::default();
        let qstore = std::sync::Arc::new(QualityMetricsStore::new(qcfg.clone()));
        SchedulerReconciler::new(
            Arc::clone(&registry),
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            Arc::new(std::sync::atomic::AtomicU64::new(0)),
            qstore,
            qcfg,
        )
    };

    let mut pipeline = ReconcilerPipeline::new(vec![Box::new(scheduler)]);

    let mut intent = create_intent("llama3:8b", vec!["b1"]);
    let decision = pipeline.execute(&mut intent).unwrap();

    match decision {
        RoutingDecision::Reject {
            rejection_reasons, ..
        } => {
            assert!(!rejection_reasons.is_empty());
            assert_eq!(rejection_reasons[0].agent_id, "b1");
            assert!(rejection_reasons[0].reason.contains("unhealthy"));
        }
        _ => panic!("Expected Reject decision"),
    }
}

/// Test: Tier reconciler with no policies does not filter
#[test]
fn tier_reconciler_no_policy_passes_through() {
    let registry = Arc::new(Registry::new());

    registry
        .add_backend(create_backend(
            "b1",
            "Low Tier",
            BackendType::Ollama,
            BackendStatus::Healthy,
            "llama3:8b",
            1,
        ))
        .unwrap();

    registry
        .add_backend(create_backend(
            "b2",
            "High Tier",
            BackendType::OpenAI,
            BackendStatus::Healthy,
            "llama3:8b",
            2,
        ))
        .unwrap();

    let tier = TierReconciler::new(
        Arc::clone(&registry),
        PolicyMatcher::compile(vec![]).unwrap(),
    );
    let scheduler = {
        let qcfg = QualityConfig::default();
        let qstore = std::sync::Arc::new(QualityMetricsStore::new(qcfg.clone()));
        SchedulerReconciler::new(
            Arc::clone(&registry),
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            Arc::new(std::sync::atomic::AtomicU64::new(0)),
            qstore,
            qcfg,
        )
    };

    let mut pipeline = ReconcilerPipeline::new(vec![Box::new(tier), Box::new(scheduler)]);

    let mut intent = create_intent("llama3:8b", vec!["b1", "b2"]);
    let decision = pipeline.execute(&mut intent).unwrap();

    match decision {
        RoutingDecision::Route { .. } => {} // Pass — routed successfully
        _ => panic!("Expected Route decision without tier constraint"),
    }
}

/// Test: Pipeline length and empty checks
#[test]
fn pipeline_metadata() {
    let empty_pipeline = ReconcilerPipeline::new(vec![]);
    assert!(empty_pipeline.is_empty());
    assert_eq!(empty_pipeline.len(), 0);

    let pipeline = ReconcilerPipeline::new(vec![
        Box::new({
            let qcfg = QualityConfig::default();
            let qstore = std::sync::Arc::new(QualityMetricsStore::new(qcfg.clone()));
            QualityReconciler::new(qstore, qcfg)
        }),
        Box::new({
            let qcfg = QualityConfig::default();
            let qstore = std::sync::Arc::new(QualityMetricsStore::new(qcfg.clone()));
            QualityReconciler::new(qstore, qcfg)
        }),
    ]);
    assert!(!pipeline.is_empty());
    assert_eq!(pipeline.len(), 2);
}

/// Test: Budget reconciler sets budget status on intent
#[test]
fn budget_reconciler_annotates_intent() {
    let registry = Arc::new(Registry::new());

    registry
        .add_backend(create_backend(
            "b1",
            "Backend",
            BackendType::Ollama,
            BackendStatus::Healthy,
            "llama3:8b",
            1,
        ))
        .unwrap();

    let budget_config = BudgetConfig::default();
    let budget_state = Arc::new(DashMap::new());
    let tokenizer_registry =
        Arc::new(TokenizerRegistry::new().expect("Failed to create TokenizerRegistry"));
    let budget = BudgetReconciler::new(
        Arc::clone(&registry),
        budget_config,
        Arc::clone(&tokenizer_registry),
        budget_state,
    );

    let mut intent = create_intent("llama3:8b", vec!["b1"]);
    budget.reconcile(&mut intent).unwrap();

    // With default config (no monthly limit), budget status stays Normal
    assert_eq!(intent.budget_status, BudgetStatus::Normal);
}
