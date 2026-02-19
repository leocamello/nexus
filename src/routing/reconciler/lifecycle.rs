//! LifecycleReconciler - excludes agents with active model lifecycle operations
//!
//! Filters out backends that are currently loading, unloading, or migrating models.
//! These backends should not receive new inference requests until the operation completes.

use super::{intent::RoutingIntent, Reconciler};
use crate::agent::types::OperationStatus;
use crate::registry::Registry;
use crate::routing::error::RoutingError;
use std::sync::Arc;

/// LifecycleReconciler filters candidates by active lifecycle operations.
///
/// # Pipeline Position
/// RequestAnalyzer → **LifecycleReconciler** → PrivacyReconciler → BudgetReconciler → TierReconciler → Scheduler
///
/// # Behavior
/// 1. For each candidate agent, check if its backend has an active lifecycle operation
/// 2. If backend.current_operation exists with status InProgress, exclude the agent
/// 3. Add rejection reason explaining the backend is loading a model
///
/// # Rationale
/// Backends undergoing model lifecycle operations (load/unload/migrate) should not
/// receive new inference requests. Loading operations can consume significant resources
/// and may cause requests to fail or time out.
pub struct LifecycleReconciler {
    registry: Arc<Registry>,
}

impl LifecycleReconciler {
    /// Create a new LifecycleReconciler with the given registry.
    pub fn new(registry: Arc<Registry>) -> Self {
        Self { registry }
    }
}

impl Reconciler for LifecycleReconciler {
    fn name(&self) -> &'static str {
        "LifecycleReconciler"
    }

    fn reconcile(&self, intent: &mut RoutingIntent) -> Result<(), RoutingError> {
        // Get the list of candidate agent IDs before filtering
        let candidate_ids: Vec<String> = intent.candidate_agents.clone();

        tracing::trace!(
            request_id = %intent.request_id,
            candidates = candidate_ids.len(),
            "LifecycleReconciler: checking for active operations"
        );

        // Check each candidate for active lifecycle operations
        for agent_id in &candidate_ids {
            // Look up the backend for this agent
            if let Some(backend) = self.registry.get_backend(agent_id) {
                // Check if backend has an active lifecycle operation
                if let Some(operation) = &backend.current_operation {
                    if operation.status == OperationStatus::InProgress {
                        // T044: Special handling for Migrate operations
                        // If this backend is the SOURCE of a migration, it should CONTINUE serving
                        // Only filter if it's a Load/Unload, or if it's the TARGET of a migration
                        let should_exclude = match operation.operation_type {
                            crate::agent::types::OperationType::Migrate => {
                                // Check if this backend is the source or target
                                // If source_backend_id matches this agent_id, it's the SOURCE - don't exclude
                                // If target_backend_id matches, it's the TARGET - exclude
                                operation.source_backend_id.as_deref() != Some(agent_id)
                            }
                            crate::agent::types::OperationType::Load
                            | crate::agent::types::OperationType::Unload => {
                                // Always exclude for Load/Unload
                                true
                            }
                        };

                        if should_exclude {
                            // Exclude this agent - backend is busy with lifecycle operation
                            intent.exclude_agent(
                                agent_id.clone(),
                                "LifecycleReconciler",
                                format!(
                                    "Backend is currently {} model '{}' ({}% complete)",
                                    match operation.operation_type {
                                        crate::agent::types::OperationType::Load => "loading",
                                        crate::agent::types::OperationType::Unload => "unloading",
                                        crate::agent::types::OperationType::Migrate => "migrating to (target)",
                                    },
                                    operation.model_id,
                                    operation.progress_percent
                                ),
                                format!(
                                    "Wait for the {} operation to complete (ETA: {})",
                                    match operation.operation_type {
                                        crate::agent::types::OperationType::Load => "load",
                                        crate::agent::types::OperationType::Unload => "unload",
                                        crate::agent::types::OperationType::Migrate => "migration",
                                    },
                                    operation.eta_ms.map(|eta| format!("{}s", eta / 1000)).unwrap_or_else(|| "unknown".to_string())
                                ),
                            );

                            tracing::debug!(
                                request_id = %intent.request_id,
                                agent_id = %agent_id,
                                operation_type = ?operation.operation_type,
                                model_id = %operation.model_id,
                                progress = operation.progress_percent,
                                "LifecycleReconciler: excluded agent with active operation"
                            );
                        } else {
                            // This is a source backend in migration - continue serving
                            tracing::debug!(
                                request_id = %intent.request_id,
                                agent_id = %agent_id,
                                operation_type = ?operation.operation_type,
                                model_id = %operation.model_id,
                                "LifecycleReconciler: keeping migration source backend in rotation"
                            );
                        }
                    }
                }
            }
        }

        tracing::trace!(
            request_id = %intent.request_id,
            remaining = intent.candidate_agents.len(),
            excluded = intent.excluded_agents.len(),
            "LifecycleReconciler: filtering complete"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{LifecycleOperation, OperationStatus, OperationType};
    use crate::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model};
    use crate::routing::reconciler::intent::RoutingIntent;
    use crate::routing::RequestRequirements;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, AtomicU64};

    fn create_backend(id: &str, model_id: &str, current_operation: Option<LifecycleOperation>) -> Backend {
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
            current_operation,
        }
    }

    fn create_intent(model: &str, candidates: Vec<String>) -> RoutingIntent {
        RoutingIntent::new(
            "req-1".to_string(),
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
            candidates,
        )
    }

    fn create_in_progress_operation(model_id: &str) -> LifecycleOperation {
        LifecycleOperation {
            operation_id: "op-123".to_string(),
            operation_type: OperationType::Load,
            model_id: model_id.to_string(),
            source_backend_id: None,
            target_backend_id: "backend-1".to_string(),
            status: OperationStatus::InProgress,
            progress_percent: 45,
            eta_ms: Some(30000),
            initiated_at: Utc::now(),
            completed_at: None,
            error_details: None,
        }
    }

    #[test]
    fn no_operations_passes_all_through() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend("b1", "llama3:8b", None))
            .unwrap();
        registry
            .add_backend(create_backend("b2", "llama3:8b", None))
            .unwrap();

        let reconciler = LifecycleReconciler::new(Arc::clone(&registry));

        let mut intent = create_intent("llama3:8b", vec!["b1".into(), "b2".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents.len(), 2);
        assert!(intent.excluded_agents.is_empty());
    }

    #[test]
    fn excludes_agent_with_in_progress_operation() {
        let registry = Arc::new(Registry::new());
        
        // Backend with active load operation
        registry
            .add_backend(create_backend(
                "loading",
                "llama3:8b",
                Some(create_in_progress_operation("llama3:70b")),
            ))
            .unwrap();
        
        // Normal backend
        registry
            .add_backend(create_backend("idle", "llama3:8b", None))
            .unwrap();

        let reconciler = LifecycleReconciler::new(Arc::clone(&registry));

        let mut intent = create_intent("llama3:8b", vec!["loading".into(), "idle".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents, vec!["idle"]);
        assert_eq!(intent.excluded_agents, vec!["loading"]);
    }

    #[test]
    fn completed_operation_does_not_exclude() {
        let registry = Arc::new(Registry::new());
        
        // Backend with completed operation
        let mut completed_op = create_in_progress_operation("llama3:70b");
        completed_op.status = OperationStatus::Completed;
        
        registry
            .add_backend(create_backend("completed", "llama3:8b", Some(completed_op)))
            .unwrap();

        let reconciler = LifecycleReconciler::new(Arc::clone(&registry));

        let mut intent = create_intent("llama3:8b", vec!["completed".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents, vec!["completed"]);
        assert!(intent.excluded_agents.is_empty());
    }

    #[test]
    fn failed_operation_does_not_exclude() {
        let registry = Arc::new(Registry::new());
        
        // Backend with failed operation
        let mut failed_op = create_in_progress_operation("llama3:70b");
        failed_op.status = OperationStatus::Failed;
        
        registry
            .add_backend(create_backend("failed", "llama3:8b", Some(failed_op)))
            .unwrap();

        let reconciler = LifecycleReconciler::new(Arc::clone(&registry));

        let mut intent = create_intent("llama3:8b", vec!["failed".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.candidate_agents, vec!["failed"]);
        assert!(intent.excluded_agents.is_empty());
    }

    #[test]
    fn rejection_reason_includes_required_fields() {
        let registry = Arc::new(Registry::new());
        registry
            .add_backend(create_backend(
                "loading",
                "llama3:8b",
                Some(create_in_progress_operation("llama3:70b")),
            ))
            .unwrap();

        let reconciler = LifecycleReconciler::new(Arc::clone(&registry));

        let mut intent = create_intent("llama3:8b", vec!["loading".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert_eq!(intent.rejection_reasons.len(), 1);
        let reason = &intent.rejection_reasons[0];
        assert_eq!(reason.agent_id, "loading");
        assert_eq!(reason.reconciler, "LifecycleReconciler");
        assert!(reason.reason.contains("loading"));
        assert!(reason.reason.contains("llama3:70b"));
        assert!(reason.reason.contains("45%"));
        assert!(!reason.suggested_action.is_empty());
    }

    #[test]
    fn multiple_operations_all_excluded() {
        let registry = Arc::new(Registry::new());
        
        registry
            .add_backend(create_backend(
                "b1",
                "llama3:8b",
                Some(create_in_progress_operation("model-a")),
            ))
            .unwrap();
        
        registry
            .add_backend(create_backend(
                "b2",
                "llama3:8b",
                Some(create_in_progress_operation("model-b")),
            ))
            .unwrap();

        let reconciler = LifecycleReconciler::new(Arc::clone(&registry));

        let mut intent = create_intent("llama3:8b", vec!["b1".into(), "b2".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert!(intent.candidate_agents.is_empty());
        assert_eq!(intent.rejection_reasons.len(), 2);
    }

    #[test]
    fn unload_operation_also_blocks() {
        let registry = Arc::new(Registry::new());
        
        let mut unload_op = create_in_progress_operation("llama3:70b");
        unload_op.operation_type = OperationType::Unload;
        
        registry
            .add_backend(create_backend("unloading", "llama3:8b", Some(unload_op)))
            .unwrap();

        let reconciler = LifecycleReconciler::new(Arc::clone(&registry));

        let mut intent = create_intent("llama3:8b", vec!["unloading".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        assert!(intent.candidate_agents.is_empty());
        assert_eq!(intent.excluded_agents, vec!["unloading"]);
        
        let reason = &intent.rejection_reasons[0];
        assert!(reason.reason.contains("unloading"));
    }

    #[test]
    fn migrate_operation_source_continues_serving() {
        // T044: Migration source should NOT be blocked
        let registry = Arc::new(Registry::new());
        
        let mut migrate_op = create_in_progress_operation("llama3:70b");
        migrate_op.operation_type = OperationType::Migrate;
        migrate_op.source_backend_id = Some("migrating-source".to_string());
        migrate_op.target_backend_id = "migrating-target".to_string();
        
        registry
            .add_backend(create_backend("migrating-source", "llama3:8b", Some(migrate_op)))
            .unwrap();

        let reconciler = LifecycleReconciler::new(Arc::clone(&registry));

        let mut intent = create_intent("llama3:8b", vec!["migrating-source".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        // Source backend should NOT be excluded - it continues serving during migration
        assert_eq!(intent.candidate_agents, vec!["migrating-source"]);
        assert!(intent.excluded_agents.is_empty());
    }

    #[test]
    fn migrate_operation_target_is_blocked() {
        // T044: Migration target SHOULD be blocked (it's loading)
        let registry = Arc::new(Registry::new());
        
        let mut migrate_op = create_in_progress_operation("llama3:70b");
        migrate_op.operation_type = OperationType::Migrate;
        migrate_op.source_backend_id = Some("migrating-source".to_string());
        migrate_op.target_backend_id = "migrating-target".to_string();
        
        // Note: This is a Load operation on the target, not Migrate
        // The target has a Load operation, source has Migrate operation
        let mut load_op = create_in_progress_operation("llama3:70b");
        load_op.operation_type = OperationType::Load;
        load_op.target_backend_id = "migrating-target".to_string();
        
        registry
            .add_backend(create_backend("migrating-target", "llama3:8b", Some(load_op)))
            .unwrap();

        let reconciler = LifecycleReconciler::new(Arc::clone(&registry));

        let mut intent = create_intent("llama3:8b", vec!["migrating-target".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        // Target backend SHOULD be excluded - it's loading
        assert!(intent.candidate_agents.is_empty());
        assert_eq!(intent.excluded_agents, vec!["migrating-target"]);
        
        let reason = &intent.rejection_reasons[0];
        assert!(reason.reason.contains("loading"));
    }

    #[test]
    fn unknown_backend_does_not_crash() {
        let registry = Arc::new(Registry::new());
        // Don't add "ghost" backend

        let reconciler = LifecycleReconciler::new(Arc::clone(&registry));

        let mut intent = create_intent("llama3:8b", vec!["ghost".into()]);
        reconciler.reconcile(&mut intent).unwrap();

        // Unknown backend is not excluded (no backend to check)
        assert_eq!(intent.candidate_agents, vec!["ghost"]);
        assert!(intent.excluded_agents.is_empty());
    }
}
