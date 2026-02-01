//! Integration tests for health checker with mock HTTP servers.

use nexus::health::{HealthCheckConfig, HealthCheckResult, HealthChecker};
use nexus::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Registry};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Helper to create a test registry with a backend
async fn setup_test_registry_with_backend(
    backend_type: BackendType,
    url: String,
) -> (Arc<Registry>, String) {
    let registry = Arc::new(Registry::new());
    let backend = Backend::new(
        "test-backend".to_string(),
        "Test Backend".to_string(),
        url,
        backend_type,
        vec![],
        DiscoverySource::Manual,
        HashMap::new(),
    );
    let backend_id = backend.id.clone();
    registry.add_backend(backend).unwrap();
    (registry, backend_id)
}

#[tokio::test]
async fn test_full_health_check_cycle_ollama() {
    // Setup mock Ollama server
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "models": [
                {"name": "llama3:70b"},
                {"name": "mistral:7b"}
            ]
        })))
        .mount(&mock_server)
        .await;

    let (registry, backend_id) =
        setup_test_registry_with_backend(BackendType::Ollama, mock_server.uri()).await;

    let config = HealthCheckConfig {
        interval_seconds: 1,
        timeout_seconds: 5,
        failure_threshold: 3,
        recovery_threshold: 2,
        enabled: true,
    };

    let checker = HealthChecker::new(registry.clone(), config);

    // Run a single check cycle
    let results = checker.check_all_backends().await;

    assert_eq!(results.len(), 1);
    assert!(matches!(results[0].1, HealthCheckResult::Success { .. }));

    // Verify registry was updated
    let backend = registry.get_backend(&backend_id).unwrap();
    assert_eq!(backend.status, BackendStatus::Healthy);
    assert_eq!(backend.models.len(), 2);
    assert_eq!(backend.models[0].id, "llama3:70b");
    assert_eq!(backend.models[1].id, "mistral:7b");
}

#[tokio::test]
async fn test_status_transition_thresholds() {
    // Setup mock server that will fail
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let (registry, backend_id) =
        setup_test_registry_with_backend(BackendType::Ollama, mock_server.uri()).await;

    let config = HealthCheckConfig {
        interval_seconds: 1,
        timeout_seconds: 5,
        failure_threshold: 3,
        recovery_threshold: 2,
        enabled: true,
    };

    let checker = HealthChecker::new(registry.clone(), config);

    // Backend starts as Unknown
    let backend = registry.get_backend(&backend_id).unwrap();
    assert_eq!(backend.status, BackendStatus::Unknown);

    // First failure: Unknown -> Unhealthy
    checker.check_all_backends().await;
    let backend = registry.get_backend(&backend_id).unwrap();
    assert_eq!(backend.status, BackendStatus::Unhealthy);

    // Now setup success responses
    mock_server.reset().await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "models": [{"name": "test"}]
        })))
        .mount(&mock_server)
        .await;

    // First success: still Unhealthy (need 2 for recovery)
    checker.check_all_backends().await;
    let backend = registry.get_backend(&backend_id).unwrap();
    assert_eq!(backend.status, BackendStatus::Unhealthy);

    // Second success: Unhealthy -> Healthy
    checker.check_all_backends().await;
    let backend = registry.get_backend(&backend_id).unwrap();
    assert_eq!(backend.status, BackendStatus::Healthy);
}

#[tokio::test]
async fn test_model_discovery_openai_format() {
    // Setup mock vLLM server (uses OpenAI format)
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                {"id": "gpt-4"},
                {"id": "gpt-3.5-turbo"}
            ]
        })))
        .mount(&mock_server)
        .await;

    let (registry, backend_id) =
        setup_test_registry_with_backend(BackendType::VLLM, mock_server.uri()).await;

    let config = HealthCheckConfig::default();
    let checker = HealthChecker::new(registry.clone(), config);

    // Run check
    checker.check_all_backends().await;

    // Verify models were discovered
    let backend = registry.get_backend(&backend_id).unwrap();
    assert_eq!(backend.models.len(), 2);
    assert_eq!(backend.models[0].id, "gpt-4");
    assert_eq!(backend.models[1].id, "gpt-3.5-turbo");
}

#[tokio::test]
async fn test_graceful_shutdown() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "models": []
        })))
        .mount(&mock_server)
        .await;

    let (registry, _) =
        setup_test_registry_with_backend(BackendType::Ollama, mock_server.uri()).await;

    let config = HealthCheckConfig {
        interval_seconds: 1,
        ..Default::default()
    };

    let checker = HealthChecker::new(registry, config);
    let cancel_token = CancellationToken::new();

    // Start the health checker
    let handle = checker.start(cancel_token.clone());

    // Let it run for a bit
    sleep(Duration::from_millis(500)).await;

    // Cancel and wait for shutdown
    cancel_token.cancel();

    // Should complete within a reasonable time
    let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
    assert!(result.is_ok(), "Health checker should shutdown gracefully");
}

#[tokio::test]
async fn test_timeout_handling() {
    // Create a server that never responds
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(10)))
        .mount(&mock_server)
        .await;

    let (registry, backend_id) =
        setup_test_registry_with_backend(BackendType::Ollama, mock_server.uri()).await;

    let config = HealthCheckConfig {
        timeout_seconds: 1, // Short timeout
        failure_threshold: 1,
        ..Default::default()
    };

    let checker = HealthChecker::new(registry.clone(), config);

    // Run check - should timeout
    let results = checker.check_all_backends().await;

    assert_eq!(results.len(), 1);
    assert!(matches!(results[0].1, HealthCheckResult::Failure { .. }));

    // Backend should be marked unhealthy
    let backend = registry.get_backend(&backend_id).unwrap();
    assert_eq!(backend.status, BackendStatus::Unhealthy);
}

#[tokio::test]
async fn test_latency_tracking() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "models": [{"name": "test"}]
        })))
        .mount(&mock_server)
        .await;

    let (registry, backend_id) =
        setup_test_registry_with_backend(BackendType::Ollama, mock_server.uri()).await;

    let config = HealthCheckConfig::default();
    let checker = HealthChecker::new(registry.clone(), config);

    // Initial latency should be 0
    let backend = registry.get_backend(&backend_id).unwrap();
    assert_eq!(
        backend
            .avg_latency_ms
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );

    // Run check
    let results = checker.check_all_backends().await;

    // Verify check succeeded
    assert!(matches!(results[0].1, HealthCheckResult::Success { .. }));
    if let HealthCheckResult::Success { latency_ms, .. } = &results[0].1 {
        // Latency should be recorded (even if very small)
        assert!(
            *latency_ms < 1000,
            "Latency should be reasonable for local mock"
        );
    }

    // Registry latency should be updated (might be 0 for very fast responses)
    let backend = registry.get_backend(&backend_id).unwrap();
    let latency = backend
        .avg_latency_ms
        .load(std::sync::atomic::Ordering::SeqCst);
    // Just verify it's been set (can be 0 for sub-millisecond responses)
    assert!(latency < 1000, "Latency should be reasonable");
}
