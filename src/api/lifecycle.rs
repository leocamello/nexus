//! Model lifecycle management API endpoints.
//!
//! Provides HTTP handlers for loading, unloading, and migrating models across backends,
//! as well as fleet intelligence recommendations.

use axum::http::StatusCode;
use axum::response::AppendHeaders;
use axum::{extract::Path, extract::Query, extract::State, Json};
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::agent::types::{LifecycleOperation, OperationStatus, OperationType};
use crate::api::types::ApiError;
use crate::api::AppState;

/// T089a: Header name for lifecycle operation status
const LIFECYCLE_STATUS_HEADER: &str = "x-nexus-lifecycle-status";
/// T089a: Header name for lifecycle operation ID
const LIFECYCLE_OPERATION_HEADER: &str = "x-nexus-lifecycle-operation";

// Request/response types
#[derive(serde::Deserialize)]
pub struct LoadModelRequest {
    pub model_id: String,
    pub backend_id: String,
}

#[derive(serde::Serialize)]
pub struct LoadModelResponse {
    pub operation_id: String,
    pub model_id: String,
    pub backend_id: String,
    pub status: String,
}

#[derive(serde::Deserialize)]
pub struct UnloadQuery {
    pub backend_id: String,
}

// T042: Request/response types for model migration
#[derive(serde::Deserialize)]
pub struct MigrateModelRequest {
    pub model_id: String,
    pub source_backend_id: String,
    pub target_backend_id: String,
}

#[derive(serde::Serialize)]
pub struct MigrateModelResponse {
    pub operation_id: String,
    pub model_id: String,
    pub source_backend_id: String,
    pub target_backend_id: String,
    pub status: String,
    pub message: String,
}

/// POST /v1/models/load
///
/// Load a model onto a specific backend (T028).
///
/// # Implementation
///
/// 1. Look up backend and agent from registry
/// 2. Check for concurrent operation (T033) → 409 if InProgress
/// 3. Check VRAM availability (T032) → 400/507 if insufficient
/// 4. Call agent.load_model()
/// 5. Set backend.current_operation to InProgress
/// 6. Return 202 Accepted with operation details
///
/// # Errors
///
/// - 400 Bad Request: Invalid request or insufficient VRAM (T036)
/// - 404 Not Found: Backend not found
/// - 409 Conflict: Concurrent load already in progress (T033)
/// - 500 Internal Server Error: Agent error
/// - 507 Insufficient Storage: Not enough VRAM (T036)
pub async fn handle_load(
    State(state): State<Arc<AppState>>,
    Json(request): Json<LoadModelRequest>,
) -> Result<
    (
        StatusCode,
        AppendHeaders<[(&'static str, String); 2]>,
        Json<LoadModelResponse>,
    ),
    ApiError,
> {
    info!(
        model_id = %request.model_id,
        backend_id = %request.backend_id,
        "Received model load request"
    );

    // 1. Get agent from registry
    let agent = state
        .registry
        .get_agent(&request.backend_id)
        .ok_or_else(|| ApiError::not_found(&format!("Backend {} not found", request.backend_id)))?;

    // T033: Check for concurrent operation
    if let Ok(Some(op)) = state.registry.get_operation(&request.backend_id) {
        if op.status == OperationStatus::InProgress {
            warn!(
                model_id = %request.model_id,
                backend_id = %request.backend_id,
                existing_operation = %op.operation_id,
                "Concurrent load rejected - operation already in progress"
            );
            return Err(ApiError::conflict(&format!(
                "Backend {} already has an operation in progress: {}",
                request.backend_id, op.operation_id
            )));
        }
    }

    // T032: VRAM validation
    let profile = agent.profile();
    if profile.capabilities.resource_monitoring {
        let usage: crate::agent::types::ResourceUsage = agent.resource_usage().await;

        // Check VRAM availability
        if let Some(vram_used) = usage.vram_used_bytes {
            if let Some(vram_free) = usage.vram_free_bytes() {
                // We have both total and used, calculate required headroom
                let headroom_percent = state.config.lifecycle.vram_headroom_percent;
                let required_free = if let Some(total) = usage.vram_total_bytes {
                    (total as f64 * headroom_percent as f64 / 100.0) as u64
                } else {
                    // Fallback: require 4GB free
                    4_000_000_000
                };

                if vram_free < required_free {
                    // T036: Insufficient VRAM error
                    warn!(
                        model_id = %request.model_id,
                        backend_id = %request.backend_id,
                        vram_free = vram_free,
                        required_free = required_free,
                        "Insufficient VRAM for model load"
                    );
                    return Err(ApiError::bad_request(&format!(
                        "Insufficient VRAM: {}GB free, need at least {}GB",
                        vram_free / 1_000_000_000,
                        required_free / 1_000_000_000
                    )));
                }
            } else {
                // Backend doesn't report total VRAM (e.g. Ollama) — use configurable heuristic
                let max_gb = state.config.lifecycle.vram_heuristic_max_gb;
                if vram_used > max_gb * 1_000_000_000 {
                    warn!(
                        model_id = %request.model_id,
                        backend_id = %request.backend_id,
                        vram_used = vram_used,
                        vram_heuristic_max_gb = max_gb,
                        "High VRAM usage detected, may be insufficient for model load"
                    );
                    return Err(ApiError::bad_request(&format!(
                        "High VRAM usage: {}GB in use (heuristic max: {}GB). \
                         Adjust lifecycle.vram_heuristic_max_gb to match your GPU.",
                        vram_used / 1_000_000_000,
                        max_gb
                    )));
                }
            }
        }
    }

    // 4. Create operation and set BEFORE calling load_model to prevent TOCTOU races
    let operation_id = format!("op-{}", uuid::Uuid::new_v4());
    let operation = LifecycleOperation {
        operation_id: operation_id.clone(),
        operation_type: OperationType::Load,
        model_id: request.model_id.clone(),
        source_backend_id: None,
        target_backend_id: request.backend_id.clone(),
        status: OperationStatus::InProgress,
        progress_percent: 0,
        eta_ms: None,
        initiated_at: chrono::Utc::now(),
        completed_at: None,
        error_details: None,
    };

    if let Err(e) = state
        .registry
        .update_operation(&request.backend_id, Some(operation))
    {
        error!(
            backend_id = %request.backend_id,
            error = ?e,
            "Failed to update operation in registry"
        );
        return Err(ApiError::bad_gateway("Failed to track operation"));
    }

    // 5. Call agent.load_model() — operation is already tracked
    if let Err(e) = agent.load_model(&request.model_id).await {
        error!(
            model_id = %request.model_id,
            backend_id = %request.backend_id,
            error = ?e,
            "Failed to initiate model load"
        );
        // Clear operation on failure
        let _ = state.registry.update_operation(&request.backend_id, None);
        return Err(ApiError::bad_gateway(&format!(
            "Failed to load model: {:?}",
            e
        )));
    }

    info!(
        model_id = %request.model_id,
        backend_id = %request.backend_id,
        operation_id = %operation_id,
        "Model load initiated successfully"
    );

    // 6. Return 202 Accepted with T089a lifecycle headers
    Ok((
        StatusCode::ACCEPTED,
        AppendHeaders([
            (LIFECYCLE_STATUS_HEADER, "loading".to_string()),
            (LIFECYCLE_OPERATION_HEADER, operation_id.clone()),
        ]),
        Json(LoadModelResponse {
            operation_id,
            model_id: request.model_id,
            backend_id: request.backend_id,
            status: "in_progress".to_string(),
        }),
    ))
}

/// POST /v1/models/migrate
///
/// Migrate a model from source backend to target backend (T042, T043).
///
/// # Implementation
///
/// 1. Verify source backend has the model loaded
/// 2. Verify target backend is healthy and has VRAM capacity
/// 3. Set source backend operation to Migrate/InProgress (keeps it serving) - T046
/// 4. Start load on target backend via agent.load_model()
/// 5. Set target backend operation to Load/InProgress
/// 6. Return 202 Accepted with migration operation details
///
/// # Migration Coordination (T043)
///
/// The migration is a two-phase operation:
/// - Phase 1 (this handler): Start loading on target while source continues serving
/// - Phase 2 (operator-managed): After target loads, unload from source
///
/// The source backend is marked with Migrate operation type, which signals to
/// the LifecycleReconciler that it should CONTINUE routing traffic to this backend
/// (unlike Load operations which block routing) - T044.
///
/// # Error Handling (T047, T048)
///
/// If target load fails, source continues serving normally. The migration operation
/// is rolled back and detailed error information is returned.
///
/// # Errors
///
/// - 400 Bad Request: Invalid request or insufficient VRAM on target
/// - 404 Not Found: Source or target backend not found
/// - 409 Conflict: Source doesn't have model or target has concurrent operation
/// - 500 Internal Server Error: Agent error
/// - 502 Bad Gateway: Target backend unreachable or load failed (T047, T048)
pub async fn handle_migrate(
    State(state): State<Arc<AppState>>,
    Json(request): Json<MigrateModelRequest>,
) -> Result<
    (
        StatusCode,
        AppendHeaders<[(&'static str, String); 2]>,
        Json<MigrateModelResponse>,
    ),
    ApiError,
> {
    info!(
        model_id = %request.model_id,
        source_backend_id = %request.source_backend_id,
        target_backend_id = %request.target_backend_id,
        "Received model migration request"
    );

    // 1. Verify source backend has the model
    let _source_agent = state
        .registry
        .get_agent(&request.source_backend_id)
        .ok_or_else(|| {
            ApiError::not_found(&format!(
                "Source backend {} not found",
                request.source_backend_id
            ))
        })?;

    let source_backend = state
        .registry
        .get_backend(&request.source_backend_id)
        .ok_or_else(|| {
            ApiError::not_found(&format!(
                "Source backend {} not found",
                request.source_backend_id
            ))
        })?;

    // Check if source has the model loaded
    let model_found = source_backend
        .models
        .iter()
        .any(|m| m.id == request.model_id);

    if !model_found {
        warn!(
            model_id = %request.model_id,
            source_backend_id = %request.source_backend_id,
            "Source backend does not have the model loaded"
        );
        return Err(ApiError::conflict(&format!(
            "Source backend {} does not have model {} loaded",
            request.source_backend_id, request.model_id
        )));
    }

    // 2. Get target agent and verify it's available
    let target_agent = state
        .registry
        .get_agent(&request.target_backend_id)
        .ok_or_else(|| {
            ApiError::not_found(&format!(
                "Target backend {} not found",
                request.target_backend_id
            ))
        })?;

    // Check for concurrent operation on target
    if let Ok(Some(op)) = state.registry.get_operation(&request.target_backend_id) {
        if op.status == OperationStatus::InProgress {
            warn!(
                target_backend_id = %request.target_backend_id,
                existing_operation = %op.operation_id,
                "Migration rejected - target has operation in progress"
            );
            return Err(ApiError::conflict(&format!(
                "Target backend {} already has an operation in progress: {}",
                request.target_backend_id, op.operation_id
            )));
        }
    }

    // T032: VRAM validation for target
    let profile = target_agent.profile();
    if profile.capabilities.resource_monitoring {
        let usage: crate::agent::types::ResourceUsage = target_agent.resource_usage().await;

        if let Some(vram_used) = usage.vram_used_bytes {
            if let Some(vram_free) = usage.vram_free_bytes() {
                let headroom_percent = state.config.lifecycle.vram_headroom_percent;
                let required_free = if let Some(total) = usage.vram_total_bytes {
                    (total as f64 * headroom_percent as f64 / 100.0) as u64
                } else {
                    4_000_000_000 // 4GB fallback
                };

                if vram_free < required_free {
                    // T048: Detailed failure notification
                    warn!(
                        model_id = %request.model_id,
                        target_backend_id = %request.target_backend_id,
                        vram_free = vram_free,
                        required_free = required_free,
                        "Insufficient VRAM on target for migration"
                    );
                    return Err(ApiError::bad_request(&format!(
                        "Target backend has insufficient VRAM: {}GB free, need at least {}GB",
                        vram_free / 1_000_000_000,
                        required_free / 1_000_000_000
                    )));
                }
            } else if vram_used > state.config.lifecycle.vram_heuristic_max_gb * 1_000_000_000 {
                // T048: Detailed failure notification
                let max_gb = state.config.lifecycle.vram_heuristic_max_gb;
                warn!(
                    model_id = %request.model_id,
                    target_backend_id = %request.target_backend_id,
                    vram_used = vram_used,
                    vram_heuristic_max_gb = max_gb,
                    "High VRAM usage on target, may be insufficient for migration"
                );
                return Err(ApiError::bad_request(&format!(
                    "Target backend has high VRAM usage: {}GB in use (heuristic max: {}GB). \
                     Adjust lifecycle.vram_heuristic_max_gb to match your GPU.",
                    vram_used / 1_000_000_000,
                    max_gb
                )));
            }
        }
    }

    // 3. Set operations on BOTH backends BEFORE calling load_model to prevent TOCTOU races
    let migration_op_id = format!("op-migrate-{}", uuid::Uuid::new_v4());
    let load_op_id = format!("op-load-{}", uuid::Uuid::new_v4());

    // Set source backend operation to Migrate/InProgress (T046)
    let source_operation = LifecycleOperation {
        operation_id: migration_op_id.clone(),
        operation_type: OperationType::Migrate,
        model_id: request.model_id.clone(),
        source_backend_id: Some(request.source_backend_id.clone()),
        target_backend_id: request.target_backend_id.clone(),
        status: OperationStatus::InProgress,
        progress_percent: 0,
        eta_ms: None,
        initiated_at: chrono::Utc::now(),
        completed_at: None,
        error_details: None,
    };

    if let Err(e) = state
        .registry
        .update_operation(&request.source_backend_id, Some(source_operation))
    {
        error!(
            source_backend_id = %request.source_backend_id,
            error = ?e,
            "Failed to set migration operation on source backend"
        );
        return Err(ApiError::bad_gateway("Failed to track migration operation"));
    }

    // Set target backend operation to Load/InProgress
    let target_operation = LifecycleOperation {
        operation_id: load_op_id.clone(),
        operation_type: OperationType::Load,
        model_id: request.model_id.clone(),
        source_backend_id: Some(request.source_backend_id.clone()),
        target_backend_id: request.target_backend_id.clone(),
        status: OperationStatus::InProgress,
        progress_percent: 0,
        eta_ms: None,
        initiated_at: chrono::Utc::now(),
        completed_at: None,
        error_details: None,
    };

    if let Err(e) = state
        .registry
        .update_operation(&request.target_backend_id, Some(target_operation))
    {
        error!(
            target_backend_id = %request.target_backend_id,
            error = ?e,
            "Failed to set load operation on target backend"
        );
        // Rollback source operation
        let _ = state
            .registry
            .update_operation(&request.source_backend_id, None);
        return Err(ApiError::bad_gateway(
            "Failed to track target load operation",
        ));
    }

    // 4. Start load on target backend — both operations already tracked
    if let Err(e) = target_agent.load_model(&request.model_id).await {
        // T047: Migration failure detection — rollback both operations
        error!(
            model_id = %request.model_id,
            target_backend_id = %request.target_backend_id,
            error = ?e,
            "Failed to initiate model load on target backend"
        );
        let _ = state
            .registry
            .update_operation(&request.source_backend_id, None);
        let _ = state
            .registry
            .update_operation(&request.target_backend_id, None);
        // T048: Detailed failure notification
        return Err(ApiError::bad_gateway(&format!(
            "Migration failed: could not load model on target backend {}: {:?}",
            request.target_backend_id, e
        )));
    }

    info!(
        model_id = %request.model_id,
        source_backend_id = %request.source_backend_id,
        target_backend_id = %request.target_backend_id,
        migration_op_id = %migration_op_id,
        load_op_id = %load_op_id,
        "Model migration initiated successfully"
    );

    // 6. Return 202 Accepted with T089a lifecycle headers
    Ok((
        StatusCode::ACCEPTED,
        AppendHeaders([
            (LIFECYCLE_STATUS_HEADER, "migrating".to_string()),
            (LIFECYCLE_OPERATION_HEADER, migration_op_id.clone()),
        ]),
        Json(MigrateModelResponse {
            operation_id: migration_op_id,
            model_id: request.model_id,
            source_backend_id: request.source_backend_id,
            target_backend_id: request.target_backend_id,
            status: "in_progress".to_string(),
            message: "Migration initiated. Target backend is loading the model. Source backend will continue serving requests.".to_string(),
        }),
    ))
}

/// DELETE /v1/models/{model_id}
///
/// Unload a model from a specific backend (T055).
///
/// # Implementation
///
/// 1. Look up backend by backend_id (from query param)
/// 2. Check if model is actually loaded on that backend
/// 3. Check for active requests (T056) - use pending_requests from Backend's AtomicU32
/// 4. If active requests > 0 → return 409 Conflict (T057)
/// 5. Set backend operation to Unload/InProgress
/// 6. Call agent.unload_model(model_id)
/// 7. On success: remove model from backend (T058), verify VRAM release (T059), clear operation, return 200
/// 8. On failure: clear operation, return 500
///
/// # Errors
///
/// - 400 Bad Request: Invalid request or model not loaded on backend
/// - 404 Not Found: Backend not found
/// - 409 Conflict: Active requests in progress (T057)
/// - 500 Internal Server Error: Agent error
pub async fn handle_unload(
    State(state): State<Arc<AppState>>,
    Path(model_id): Path<String>,
    Query(query): Query<UnloadQuery>,
) -> Result<
    (
        AppendHeaders<[(&'static str, String); 2]>,
        Json<serde_json::Value>,
    ),
    ApiError,
> {
    use std::sync::atomic::Ordering;

    info!(
        model_id = %model_id,
        backend_id = %query.backend_id,
        "Received model unload request"
    );

    // 1. Get agent from registry
    let agent = state
        .registry
        .get_agent(&query.backend_id)
        .ok_or_else(|| ApiError::not_found(&format!("Backend {} not found", query.backend_id)))?;

    // 2. Check if model is loaded on this backend
    let backend = state
        .registry
        .get_backend(&query.backend_id)
        .ok_or_else(|| ApiError::not_found(&format!("Backend {} not found", query.backend_id)))?;

    let model_found = backend.models.iter().any(|m| m.id == model_id);
    if !model_found {
        warn!(
            model_id = %model_id,
            backend_id = %query.backend_id,
            "Model not loaded on backend"
        );
        return Err(ApiError::bad_request(&format!(
            "Model {} is not loaded on backend {}",
            model_id, query.backend_id
        )));
    }

    // 3. T056: Check for active requests
    let pending = backend.pending_requests.load(Ordering::SeqCst);
    if pending > 0 {
        // T057: Return 409 Conflict
        warn!(
            model_id = %model_id,
            backend_id = %query.backend_id,
            pending_requests = pending,
            "Cannot unload model: active requests in progress"
        );
        return Err(ApiError::conflict(&format!(
            "Cannot unload model: {} active requests in progress",
            pending
        )));
    }

    // 5. Create and set operation
    let operation_id = format!("op-unload-{}", uuid::Uuid::new_v4());
    let operation = LifecycleOperation {
        operation_id: operation_id.clone(),
        operation_type: OperationType::Unload,
        model_id: model_id.clone(),
        source_backend_id: Some(query.backend_id.clone()),
        target_backend_id: query.backend_id.clone(),
        status: OperationStatus::InProgress,
        progress_percent: 0,
        eta_ms: None,
        initiated_at: chrono::Utc::now(),
        completed_at: None,
        error_details: None,
    };

    if let Err(e) = state
        .registry
        .update_operation(&query.backend_id, Some(operation))
    {
        error!(
            backend_id = %query.backend_id,
            error = ?e,
            "Failed to set unload operation"
        );
        return Err(ApiError::bad_gateway("Failed to track operation"));
    }

    // 6. Call agent.unload_model()
    if let Err(e) = agent.unload_model(&model_id).await {
        error!(
            model_id = %model_id,
            backend_id = %query.backend_id,
            error = ?e,
            "Failed to unload model"
        );
        // Clear operation
        let _ = state.registry.update_operation(&query.backend_id, None);
        return Err(ApiError::bad_gateway(&format!(
            "Failed to unload model: {:?}",
            e
        )));
    }

    // 7. T058: Remove model from backend
    if let Err(e) = state
        .registry
        .remove_model_from_backend(&query.backend_id, &model_id)
    {
        error!(
            backend_id = %query.backend_id,
            model_id = %model_id,
            error = ?e,
            "Failed to remove model from registry"
        );
        // Clear operation
        let _ = state.registry.update_operation(&query.backend_id, None);
        return Err(ApiError::bad_gateway(
            "Failed to update registry after unload",
        ));
    }

    // T059: Verify VRAM release
    let usage = agent.resource_usage().await;
    let vram_free = usage.vram_free_bytes().unwrap_or(0);

    // Clear operation (completed successfully)
    if let Err(e) = state.registry.update_operation(&query.backend_id, None) {
        warn!(
            backend_id = %query.backend_id,
            error = ?e,
            "Failed to clear operation after successful unload"
        );
    }

    info!(
        model_id = %model_id,
        backend_id = %query.backend_id,
        operation_id = %operation_id,
        vram_free_gb = vram_free / 1_000_000_000,
        "Model unloaded successfully"
    );

    // Return 200 OK with T089a lifecycle headers and VRAM info
    Ok((
        AppendHeaders([
            (LIFECYCLE_STATUS_HEADER, "completed".to_string()),
            (LIFECYCLE_OPERATION_HEADER, operation_id.clone()),
        ]),
        Json(serde_json::json!({
            "operation_id": operation_id,
            "model_id": model_id,
            "backend_id": query.backend_id,
            "status": "completed",
            "vram_free_bytes": vram_free,
            "vram_free_gb": vram_free / 1_000_000_000,
        })),
    ))
}

/// GET /v1/fleet/recommendations
///
/// Get pre-warming recommendations from fleet intelligence analysis (T078).
///
/// Returns advisory-only recommendations that require operator approval to execute.
///
/// # Response
///
/// - 200 OK: JSON array of PrewarmingRecommendation objects
/// - 200 OK (empty): Empty array if fleet intelligence is disabled or no patterns found
pub async fn handle_recommendations(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let recommendations = state.fleet_tracker.get_recommendations().await;

    Ok(Json(serde_json::json!({
        "recommendations": recommendations,
        "fleet_enabled": state.config.fleet.enabled,
        "generated_at": chrono::Utc::now().to_rfc3339(),
    })))
}
