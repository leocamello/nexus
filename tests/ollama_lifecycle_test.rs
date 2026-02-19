//! Integration tests for Ollama agent lifecycle operations.
//!
//! Tests OllamaAgent.load_model() and resource_usage() with mock Ollama backend.

use nexus::agent::factory::create_agent;
use nexus::agent::types::PrivacyZone;
use nexus::registry::BackendType;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// T021: Integration test for OllamaAgent.load_model() with wiremock
#[tokio::test]
async fn test_ollama_load_model_sends_pull_request() {
    let mock_server = MockServer::start().await;

    // Mock /api/pull endpoint
    Mock::given(method("POST"))
        .and(path("/api/pull"))
        .and(body_json(json!({"name": "llama3:8b"})))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = Arc::new(reqwest::Client::new());
    let agent = create_agent(
        "test-backend".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    // Call load_model
    let result = agent.load_model("llama3:8b").await;

    // Should succeed
    assert!(result.is_ok());
}

// T023: Integration test for VRAM validation before load
#[tokio::test]
async fn test_ollama_resource_usage_vram_calculation() {
    let mock_server = MockServer::start().await;

    // Mock /api/ps endpoint with multiple models
    Mock::given(method("GET"))
        .and(path("/api/ps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": [
                {
                    "name": "llama3:8b",
                    "model": "llama3:8b",
                    "size": 4661224676u64,
                    "size_vram": 4661224676u64,
                    "details": {}
                },
                {
                    "name": "mistral:7b",
                    "model": "mistral:7b",
                    "size": 4109865159u64,
                    "size_vram": 4109865159u64,
                    "details": {}
                }
            ]
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = Arc::new(reqwest::Client::new());
    let agent = create_agent(
        "test-backend".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    // Call resource_usage
    let usage = agent.resource_usage().await;

    // Should have VRAM data
    assert!(usage.vram_used_bytes.is_some());
    let expected_vram = 4661224676u64 + 4109865159u64;
    assert_eq!(usage.vram_used_bytes.unwrap(), expected_vram);

    // Should have loaded models
    assert_eq!(usage.loaded_models.len(), 2);
    assert!(usage.loaded_models.contains(&"llama3:8b".to_string()));
    assert!(usage.loaded_models.contains(&"mistral:7b".to_string()));
}

// T021: Test load_model handles network errors
#[tokio::test]
async fn test_ollama_load_model_network_error() {
    let mock_server = MockServer::start().await;

    // Mock /api/pull to return error
    Mock::given(method("POST"))
        .and(path("/api/pull"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = Arc::new(reqwest::Client::new());
    let agent = create_agent(
        "test-backend".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    // Call load_model
    let result = agent.load_model("llama3:8b").await;

    // Should fail with error
    assert!(result.is_err());
}

// T023: Test resource_usage with no models loaded
#[tokio::test]
async fn test_ollama_resource_usage_empty() {
    let mock_server = MockServer::start().await;

    // Mock /api/ps endpoint with no models
    Mock::given(method("GET"))
        .and(path("/api/ps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": []
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = Arc::new(reqwest::Client::new());
    let agent = create_agent(
        "test-backend".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    // Call resource_usage
    let usage = agent.resource_usage().await;

    // Should have zero VRAM usage
    assert_eq!(usage.vram_used_bytes.unwrap(), 0);
    assert_eq!(usage.loaded_models.len(), 0);
}

// T051: Integration test for OllamaAgent.unload_model() with wiremock
#[tokio::test]
async fn test_ollama_unload_model_sends_keepalive_zero() {
    let mock_server = MockServer::start().await;

    // Mock /api/generate endpoint with keep_alive=0 to unload
    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .and(body_json(json!({"model": "llama3:8b", "keep_alive": 0})))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = Arc::new(reqwest::Client::new());
    let agent = create_agent(
        "test-backend".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    // Call unload_model
    let result = agent.unload_model("llama3:8b").await;

    // Should succeed
    assert!(result.is_ok());
}

// T051: Test unload_model handles network errors
#[tokio::test]
async fn test_ollama_unload_model_network_error() {
    let mock_server = MockServer::start().await;

    // Mock /api/generate to return error
    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = Arc::new(reqwest::Client::new());
    let agent = create_agent(
        "test-backend".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    // Call unload_model
    let result = agent.unload_model("llama3:8b").await;

    // Should fail with error
    assert!(result.is_err());
}

// T052: Integration test for VRAM release verification
#[tokio::test]
async fn test_ollama_vram_release_after_unload() {
    let mock_server = MockServer::start().await;

    // First call to /api/ps shows model loaded (only once)
    Mock::given(method("GET"))
        .and(path("/api/ps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": [{
                "name": "llama3:8b",
                "model": "llama3:8b",
                "size": 4661224676u64,
                "size_vram": 4661224676u64,
                "details": {}
            }]
        })))
        .up_to_n_times(1)
        .mount(&mock_server)
        .await;

    let client = Arc::new(reqwest::Client::new());
    let agent = create_agent(
        "test-backend".to_string(),
        "Test Backend".to_string(),
        mock_server.uri(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    // Check resource usage before unload
    let usage_before = agent.resource_usage().await;
    assert!(usage_before.vram_used_bytes.is_some());
    assert_eq!(usage_before.vram_used_bytes.unwrap(), 4661224676u64);
    assert_eq!(usage_before.loaded_models.len(), 1);
    assert!(usage_before
        .loaded_models
        .contains(&"llama3:8b".to_string()));

    // Mock unload operation
    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .and(body_json(json!({"model": "llama3:8b", "keep_alive": 0})))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Second call to /api/ps shows model unloaded (after first mock expired)
    Mock::given(method("GET"))
        .and(path("/api/ps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": []
        })))
        .mount(&mock_server)
        .await;

    // Call unload_model
    let unload_result = agent.unload_model("llama3:8b").await;
    assert!(unload_result.is_ok());

    // Check resource usage after unload
    let usage_after = agent.resource_usage().await;
    assert_eq!(usage_after.vram_used_bytes.unwrap(), 0);
    assert_eq!(usage_after.loaded_models.len(), 0);
}
