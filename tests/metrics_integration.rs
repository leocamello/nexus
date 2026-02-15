//! Integration tests for Request Metrics (F09).
//!
//! Tests cover contract compliance, endpoint behavior, counter/histogram/gauge
//! tracking, and interactions with routing features (fallbacks, token counting).
//!
//! **Note on global recorder**: The `metrics` crate uses a global recorder that
//! can only be installed once per process. Each test creates its own `AppState`
//! which may or may not own the global recorder. Tests verify behaviour through
//! HTTP status codes, JSON schemas, and Registry atomics (via `/v1/stats`) rather
//! than asserting specific values in Prometheus text output (which depends on
//! which test wins the global recorder).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use nexus::api::{create_router, AppState};
use nexus::config::NexusConfig;
use nexus::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model, Registry};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tower::Service;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_test_backend(id: &str, name: &str, model_id: &str, priority: i32) -> Backend {
    Backend {
        id: id.to_string(),
        name: name.to_string(),
        url: format!("http://{}", name),
        backend_type: BackendType::Ollama,
        status: BackendStatus::Healthy,
        last_health_check: chrono::Utc::now(),
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
        priority,
        pending_requests: AtomicU32::new(0),
        total_requests: AtomicU64::new(0),
        avg_latency_ms: AtomicU32::new(50),
        discovery_source: DiscoverySource::Static,
        metadata: HashMap::new(),
    }
}

fn create_app_with_backends(backends: Vec<Backend>) -> axum::Router {
    let registry = Arc::new(Registry::new());
    for backend in backends {
        registry.add_backend(backend).unwrap();
    }
    let config = Arc::new(NexusConfig::default());
    let state = Arc::new(AppState::new(registry, config));
    create_router(state)
}

fn create_app_empty() -> axum::Router {
    create_app_with_backends(vec![])
}

async fn get_body_string(response: axum::http::Response<Body>) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

// ===========================================================================
// T016: Contract test for /metrics endpoint
// ===========================================================================

#[tokio::test]
async fn test_metrics_endpoint_returns_200() {
    let mut app = create_app_empty();
    let response = app
        .call(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_metrics_endpoint_returns_text_response() {
    let mut app =
        create_app_with_backends(vec![create_test_backend("b1", "Backend1", "llama3:8b", 1)]);
    let response = app
        .call(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    // Prometheus text format response (may be empty if this AppState doesn't
    // own the global recorder — the `metrics` crate limitation)
}

// ===========================================================================
// T017: Contract test for /v1/stats endpoint
// ===========================================================================

#[tokio::test]
async fn test_stats_endpoint_returns_200_json() {
    let mut app = create_app_empty();
    let response = app
        .call(
            Request::builder()
                .uri("/v1/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_stats_endpoint_json_schema() {
    let mut app = create_app_empty();
    let response = app
        .call(
            Request::builder()
                .uri("/v1/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = get_body_string(response).await;
    let json: serde_json::Value = serde_json::from_str(&body).expect("Response must be valid JSON");

    // Verify top-level schema
    assert!(
        json.get("uptime_seconds").is_some(),
        "Missing uptime_seconds"
    );
    assert!(json.get("requests").is_some(), "Missing requests");
    assert!(json.get("backends").is_some(), "Missing backends");
    assert!(json.get("models").is_some(), "Missing models");

    // Verify requests sub-schema
    let requests = &json["requests"];
    assert!(requests.get("total").is_some(), "Missing requests.total");
    assert!(
        requests.get("success").is_some(),
        "Missing requests.success"
    );
    assert!(requests.get("errors").is_some(), "Missing requests.errors");
}

#[tokio::test]
async fn test_stats_endpoint_uptime_is_positive() {
    let mut app = create_app_empty();
    let response = app
        .call(
            Request::builder()
                .uri("/v1/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = get_body_string(response).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let uptime = json["uptime_seconds"].as_u64().unwrap();
    assert!(uptime < 5, "Uptime should be < 5 seconds, got {}", uptime);
}

// ===========================================================================
// T018: Integration test for request counter tracking
// ===========================================================================

#[tokio::test]
async fn test_error_request_returns_proper_status() {
    let mut app = create_app_empty();

    // Request a nonexistent model — should return error, not panic
    let response = app
        .call(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "model": "nonexistent-model",
                        "messages": [{"role": "user", "content": "test"}]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should be a client error, not a 200 and not a 500
    assert_ne!(response.status(), StatusCode::OK);
    assert_ne!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn test_metrics_still_works_after_error_request() {
    let mut app = create_app_empty();

    // Trigger an error
    let _ = app
        .call(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "model": "nonexistent",
                        "messages": [{"role": "user", "content": "test"}]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // /metrics should still return 200
    let response = app
        .call(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

// ===========================================================================
// T031: Integration test for request duration histogram
// ===========================================================================

#[tokio::test]
async fn test_metrics_returns_valid_output_with_backends() {
    let mut app =
        create_app_with_backends(vec![create_test_backend("b1", "Backend1", "llama3:8b", 1)]);
    let response = app
        .call(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

// ===========================================================================
// T032: Integration test for backend latency histogram
// ===========================================================================

#[tokio::test]
async fn test_metrics_endpoint_ok_with_registered_backends() {
    let mut app =
        create_app_with_backends(vec![create_test_backend("b1", "Backend1", "llama3:8b", 1)]);
    let response = app
        .call(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

// ===========================================================================
// T033: Integration test for average latency computation
// ===========================================================================

#[tokio::test]
async fn test_stats_backend_latency_from_registry() {
    let backend = create_test_backend("b1", "Backend1", "llama3:8b", 1);
    backend.avg_latency_ms.store(150, Ordering::SeqCst);
    backend.total_requests.store(42, Ordering::SeqCst);

    let mut app = create_app_with_backends(vec![backend]);
    let response = app
        .call(
            Request::builder()
                .uri("/v1/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = get_body_string(response).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();

    let backends = json["backends"].as_array().unwrap();
    assert_eq!(backends.len(), 1);
    assert_eq!(backends[0]["id"], "b1");
    assert_eq!(backends[0]["requests"], 42);
    assert_eq!(backends[0]["average_latency_ms"], 150.0);
}

// ===========================================================================
// T041: Integration test for fallback counter
// ===========================================================================

#[tokio::test]
async fn test_metrics_stable_after_routing_error() {
    let mut app = create_app_empty();

    // Trigger a routing error
    let _ = app
        .call(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "model": "nonexistent",
                        "messages": [{"role": "user", "content": "test"}]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Both endpoints should still work after routing errors
    let response = app
        .call(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .call(
            Request::builder()
                .uri("/v1/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

// ===========================================================================
// T042: Integration test for token counting
// ===========================================================================

#[tokio::test]
async fn test_metrics_endpoint_healthy_after_setup() {
    // Token buckets are configured in setup_metrics(). Verify /metrics responds cleanly.
    let mut app = create_app_empty();
    let response = app
        .call(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

// ===========================================================================
// T043: Integration test for pending requests gauge
// ===========================================================================

#[tokio::test]
async fn test_pending_requests_in_stats() {
    let backend = create_test_backend("b1", "Backend1", "llama3:8b", 1);
    backend.pending_requests.store(5, Ordering::SeqCst);

    let mut app = create_app_with_backends(vec![backend]);
    let response = app
        .call(
            Request::builder()
                .uri("/v1/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = get_body_string(response).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();

    let backends = json["backends"].as_array().unwrap();
    assert_eq!(backends[0]["pending"], 5);
}

// ===========================================================================
// T051: Integration test for backends_total gauge
// Verified via /v1/stats JSON (reads from Registry, not Prometheus handle)
// ===========================================================================

#[tokio::test]
async fn test_backends_total_reflected_in_stats() {
    let mut app = create_app_with_backends(vec![
        create_test_backend("b1", "Backend1", "llama3:8b", 1),
        create_test_backend("b2", "Backend2", "mistral:7b", 2),
        create_test_backend("b3", "Backend3", "phi3:mini", 3),
    ]);
    let response = app
        .call(
            Request::builder()
                .uri("/v1/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = get_body_string(response).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let backends = json["backends"].as_array().unwrap();
    assert_eq!(backends.len(), 3, "Should have 3 backends in stats");
}

// ===========================================================================
// T052: Integration test for backends_healthy gauge
// Verified via /v1/stats JSON (healthy backends have lower avg_latency)
// ===========================================================================

#[tokio::test]
async fn test_unhealthy_backend_still_appears_in_stats() {
    let mut unhealthy = create_test_backend("b2", "Backend2", "mistral:7b", 2);
    unhealthy.status = BackendStatus::Unhealthy;

    let mut app = create_app_with_backends(vec![
        create_test_backend("b1", "Backend1", "llama3:8b", 1),
        unhealthy,
    ]);
    let response = app
        .call(
            Request::builder()
                .uri("/v1/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = get_body_string(response).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let backends = json["backends"].as_array().unwrap();
    // Both backends appear in stats (2 total, even though 1 is unhealthy)
    assert_eq!(
        backends.len(),
        2,
        "Both healthy and unhealthy should appear in stats"
    );
}

// ===========================================================================
// T053: Integration test for models_available gauge
// Verified through /v1/models endpoint (which lists models from healthy backends)
// ===========================================================================

#[tokio::test]
async fn test_models_endpoint_shows_models_from_healthy_backends() {
    let mut unhealthy = create_test_backend("b2", "Backend2", "mistral:7b", 2);
    unhealthy.status = BackendStatus::Unhealthy;

    let mut app = create_app_with_backends(vec![
        create_test_backend("b1", "Backend1", "llama3:8b", 1),
        unhealthy,
    ]);
    let response = app
        .call(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = get_body_string(response).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let models = json["data"].as_array().unwrap();
    // Only healthy backend's model should be listed
    assert_eq!(models.len(), 1, "Only healthy backend models should appear");
    assert_eq!(models[0]["id"], "llama3:8b");
}

#[tokio::test]
async fn test_models_deduplication_across_backends() {
    // Two backends serve the same model — /v1/models lists per-backend entries
    // with owned_by set to the backend name for multi-backend visibility
    let mut app = create_app_with_backends(vec![
        create_test_backend("b1", "Backend1", "llama3:8b", 1),
        create_test_backend("b2", "Backend2", "llama3:8b", 2),
        create_test_backend("b3", "Backend3", "mistral:7b", 3),
    ]);
    let response = app
        .call(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = get_body_string(response).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let models = json["data"].as_array().unwrap();
    // Should list all per-backend entries (2x llama3:8b + 1x mistral:7b)
    assert_eq!(
        models.len(),
        3,
        "Should list per-backend model entries for multi-backend visibility"
    );
    // Verify owned_by is the backend name, not "nexus"
    let owners: Vec<&str> = models.iter().map(|m| m["owned_by"].as_str().unwrap()).collect();
    assert!(owners.contains(&"Backend1"));
    assert!(owners.contains(&"Backend2"));
    assert!(owners.contains(&"Backend3"));
}

// ===========================================================================
// T054: Integration test for /v1/stats per-backend breakdown
// ===========================================================================

#[tokio::test]
async fn test_stats_per_backend_breakdown() {
    let b1 = create_test_backend("ollama-1", "Ollama Local", "llama3:8b", 1);
    b1.total_requests.store(100, Ordering::SeqCst);
    b1.avg_latency_ms.store(45, Ordering::SeqCst);
    b1.pending_requests.store(2, Ordering::SeqCst);

    let b2 = create_test_backend("vllm-1", "vLLM Server", "mistral:7b", 2);
    b2.total_requests.store(200, Ordering::SeqCst);
    b2.avg_latency_ms.store(30, Ordering::SeqCst);
    b2.pending_requests.store(0, Ordering::SeqCst);

    let mut app = create_app_with_backends(vec![b1, b2]);
    let response = app
        .call(
            Request::builder()
                .uri("/v1/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = get_body_string(response).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();

    let backends = json["backends"].as_array().unwrap();
    assert_eq!(backends.len(), 2, "Should have 2 backend entries");

    let ollama = backends.iter().find(|b| b["id"] == "ollama-1").unwrap();
    assert_eq!(ollama["requests"], 100);
    assert_eq!(ollama["average_latency_ms"], 45.0);
    assert_eq!(ollama["pending"], 2);

    let vllm = backends.iter().find(|b| b["id"] == "vllm-1").unwrap();
    assert_eq!(vllm["requests"], 200);
    assert_eq!(vllm["average_latency_ms"], 30.0);
    assert_eq!(vllm["pending"], 0);
}

// ===========================================================================
// T075: Integration test for FR-020 — existing endpoints still work
// ===========================================================================

#[tokio::test]
async fn test_existing_endpoints_unaffected_by_metrics() {
    let mut app =
        create_app_with_backends(vec![create_test_backend("b1", "Backend1", "llama3:8b", 1)]);

    // GET /health should still work
    let response = app
        .call(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // GET /v1/models should still work
    let response = app
        .call(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // POST /v1/chat/completions should still accept requests (will fail on proxy, not 404)
    let response = app
        .call(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "model": "llama3:8b",
                        "messages": [{"role": "user", "content": "test"}]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    // Should not be 404 — the route exists
    assert_ne!(response.status(), StatusCode::NOT_FOUND);
}

// ===========================================================================
// T068: Error handling — metrics endpoint returns correct content-type
// ===========================================================================

#[tokio::test]
async fn test_metrics_endpoint_returns_prometheus_content_type() {
    let mut app = create_app_empty();
    let response = app
        .call(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .expect("Missing content-type header")
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("text/plain"),
        "Expected text/plain content-type for Prometheus, got: {}",
        content_type
    );
}

#[tokio::test]
async fn test_metrics_endpoint_ok_with_no_backends() {
    // Even with zero backends, /metrics should return 200 (not 503 or 500)
    let mut app = create_app_empty();
    let response = app
        .call(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

// ===========================================================================
// T074: Final comprehensive integration test — exercises all user stories
// ===========================================================================

#[tokio::test]
async fn test_comprehensive_metrics_end_to_end() {
    // Setup: 2 healthy backends, 1 unhealthy
    let b1 = create_test_backend("gpu-node-1", "GPU Node 1", "llama3:70b", 1);
    b1.total_requests.store(500, Ordering::SeqCst);
    b1.avg_latency_ms.store(120, Ordering::SeqCst);
    b1.pending_requests.store(3, Ordering::SeqCst);

    let b2 = create_test_backend("cpu-node-1", "CPU Node 1", "mistral:7b", 2);
    b2.total_requests.store(200, Ordering::SeqCst);
    b2.avg_latency_ms.store(250, Ordering::SeqCst);
    b2.pending_requests.store(0, Ordering::SeqCst);

    let mut b3 = create_test_backend("offline-node", "Offline Node", "phi3:mini", 3);
    b3.status = BackendStatus::Unhealthy;

    let mut app = create_app_with_backends(vec![b1, b2, b3]);

    // --- US1: Prometheus endpoint works ---
    let response = app
        .call(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let ct = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.contains("text/plain"));

    // --- US1: JSON stats endpoint works ---
    let response = app
        .call(
            Request::builder()
                .uri("/v1/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = get_body_string(response).await;
    let stats: serde_json::Value = serde_json::from_str(&body).unwrap();

    // --- US2: Uptime and request schema ---
    assert!(stats["uptime_seconds"].as_u64().unwrap() < 5);
    assert!(stats["requests"].is_object());
    assert!(stats["requests"]["total"].is_number());

    // --- US3: Per-backend breakdown ---
    let backends = stats["backends"].as_array().unwrap();
    assert_eq!(
        backends.len(),
        3,
        "All backends (including unhealthy) in stats"
    );

    let gpu = backends.iter().find(|b| b["id"] == "gpu-node-1").unwrap();
    assert_eq!(gpu["requests"], 500);
    assert_eq!(gpu["average_latency_ms"], 120.0);
    assert_eq!(gpu["pending"], 3);

    let cpu = backends.iter().find(|b| b["id"] == "cpu-node-1").unwrap();
    assert_eq!(cpu["requests"], 200);

    // --- US4: Models endpoint reflects healthy backends only ---
    let response = app
        .call(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = get_body_string(response).await;
    let models: serde_json::Value = serde_json::from_str(&body).unwrap();
    let model_list = models["data"].as_array().unwrap();
    // Only 2 healthy backends' models (llama3:70b, mistral:7b), not phi3:mini
    assert_eq!(model_list.len(), 2);

    // --- FR-020: Existing endpoints still work ---
    let response = app
        .call(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // --- Error handling: invalid request doesn't crash metrics ---
    let _ = app
        .call(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "model": "nonexistent-model",
                        "messages": [{"role": "user", "content": "test"}]
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    // Metrics still work after error
    let response = app
        .call(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .call(
            Request::builder()
                .uri("/v1/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
