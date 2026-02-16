//! Integration tests for the NII agent abstraction (RFC-001 Phase 1).
//!
//! Tests dual storage, agent-based health checking, and agent-based request forwarding.

use nexus::agent::factory::create_agent;
use nexus::agent::types::PrivacyZone;
use nexus::registry::{Backend, BackendType, DiscoverySource, Registry, RegistryError};
use std::collections::HashMap;
use std::sync::Arc;

fn test_client() -> Arc<reqwest::Client> {
    Arc::new(reqwest::Client::new())
}

fn test_backend(id: &str, backend_type: BackendType) -> Backend {
    Backend::new(
        id.to_string(),
        format!("Test {}", id),
        "http://localhost:11434".to_string(),
        backend_type,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    )
}

// T028a: Dual storage — add_backend_with_agent stores both Backend and agent
#[tokio::test]
async fn test_dual_storage_stores_both() {
    let registry = Registry::new();
    let client = test_client();

    let backend = test_backend("b1", BackendType::Ollama);
    let agent = create_agent(
        "b1".to_string(),
        "Test b1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry.add_backend_with_agent(backend, agent).unwrap();

    // Both Backend and agent are stored
    assert!(registry.get_backend("b1").is_some());
    assert!(registry.get_agent("b1").is_some());
    assert_eq!(registry.backend_count(), 1);
}

// T028a: BackendView unaffected by agent storage
#[tokio::test]
async fn test_dual_storage_backend_view_unaffected() {
    let registry = Registry::new();
    let client = test_client();

    let backend = test_backend("b1", BackendType::Ollama);
    let agent = create_agent(
        "b1".to_string(),
        "Test b1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        Arc::clone(&client),
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry.add_backend_with_agent(backend, agent).unwrap();

    // BackendView should serialize normally
    let backends = registry.get_all_backends();
    assert_eq!(backends.len(), 1);
    assert_eq!(backends[0].id, "b1");
    assert_eq!(backends[0].name, "Test b1");
    assert_eq!(backends[0].backend_type, BackendType::Ollama);
}

// T028a: Agent returns correct identity data
#[tokio::test]
async fn test_dual_storage_agent_identity() {
    let registry = Registry::new();
    let client = test_client();

    let agent = create_agent(
        "b1".to_string(),
        "Test b1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    let backend = test_backend("b1", BackendType::Ollama);
    registry.add_backend_with_agent(backend, agent).unwrap();

    let agent = registry.get_agent("b1").unwrap();
    assert_eq!(agent.id(), "b1");
    assert_eq!(agent.name(), "Test b1");
}

// T028a: Multiple backend types can coexist in dual storage
#[tokio::test]
async fn test_dual_storage_mixed_backends() {
    let registry = Registry::new();
    let client = test_client();

    let types = vec![
        ("ollama-1", BackendType::Ollama),
        ("openai-1", BackendType::LMStudio),
        ("vllm-1", BackendType::VLLM),
    ];

    for (id, bt) in &types {
        let backend = test_backend(id, *bt);
        let mut metadata = HashMap::new();
        if *bt == BackendType::OpenAI {
            metadata.insert("api_key".to_string(), "test-key".to_string());
        }
        let agent = create_agent(
            id.to_string(),
            format!("Test {}", id),
            "http://localhost:11434".to_string(),
            *bt,
            Arc::clone(&client),
            metadata,
            PrivacyZone::Restricted,
            None,
        )
        .unwrap();
        registry.add_backend_with_agent(backend, agent).unwrap();
    }

    assert_eq!(registry.backend_count(), 3);
    for (id, _) in &types {
        assert!(registry.get_backend(id).is_some());
        assert!(registry.get_agent(id).is_some());
    }
}

// T028a: Remove backend also removes agent
#[tokio::test]
async fn test_dual_storage_remove_cleans_agent() {
    let registry = Registry::new();
    let client = test_client();

    let backend = test_backend("b1", BackendType::Ollama);
    let agent = create_agent(
        "b1".to_string(),
        "Test b1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    registry.add_backend_with_agent(backend, agent).unwrap();
    assert!(registry.get_agent("b1").is_some());

    registry.remove_backend("b1").unwrap();
    assert!(registry.get_agent("b1").is_none());
    assert!(registry.get_backend("b1").is_none());
}

// T028a: get_all_agents returns all registered agents
#[tokio::test]
async fn test_get_all_agents() {
    let registry = Registry::new();
    let client = test_client();

    for i in 0..3 {
        let id = format!("b{}", i);
        let backend = test_backend(&id, BackendType::Ollama);
        let agent = create_agent(
            id.clone(),
            format!("Test {}", id),
            "http://localhost:11434".to_string(),
            BackendType::Ollama,
            Arc::clone(&client),
            HashMap::new(),
            PrivacyZone::Restricted,
            None,
        )
        .unwrap();
        registry.add_backend_with_agent(backend, agent).unwrap();
    }

    let agents = registry.get_all_agents();
    assert_eq!(agents.len(), 3);
}

// T028a: Duplicate backend with agent rejected
#[tokio::test]
async fn test_dual_storage_duplicate_rejected() {
    let registry = Registry::new();
    let client = test_client();

    let backend = test_backend("b1", BackendType::Ollama);
    let agent = create_agent(
        "b1".to_string(),
        "Test b1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        Arc::clone(&client),
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();
    registry.add_backend_with_agent(backend, agent).unwrap();

    // Second add with same ID should fail
    let backend2 = test_backend("b1", BackendType::LMStudio);
    let agent2 = create_agent(
        "b1".to_string(),
        "Test b1".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::LMStudio,
        Arc::clone(&client),
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();
    assert!(matches!(
        registry.add_backend_with_agent(backend2, agent2),
        Err(RegistryError::DuplicateBackend(_))
    ));
}

// T035a: HealthStatus maps to BackendStatus correctly (via agent profile)
#[tokio::test]
async fn test_agent_profile_reflects_backend_type() {
    let client = test_client();

    let ollama = create_agent(
        "a1".to_string(),
        "Ollama".to_string(),
        "http://localhost:11434".to_string(),
        BackendType::Ollama,
        Arc::clone(&client),
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();
    assert_eq!(ollama.profile().backend_type, "ollama");

    let lmstudio = create_agent(
        "a2".to_string(),
        "LMStudio".to_string(),
        "http://localhost:1234".to_string(),
        BackendType::LMStudio,
        Arc::clone(&client),
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();
    assert_eq!(lmstudio.profile().backend_type, "lmstudio");

    let vllm = create_agent(
        "a3".to_string(),
        "vLLM".to_string(),
        "http://localhost:8000".to_string(),
        BackendType::VLLM,
        Arc::clone(&client),
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();
    assert_eq!(vllm.profile().backend_type, "vllm");
}

// T022f: Cancellation safety — dropping chat_completion future mid-flight doesn't leak
#[tokio::test]
async fn test_cancellation_safety_chat_completion() {
    use nexus::api::types::ChatCompletionRequest;
    use std::time::Duration;

    // Start a mock server that delays its response
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Spawn a server that accepts connections but never responds
    let server_handle = tokio::spawn(async move {
        // Accept up to 2 connections
        for _ in 0..2 {
            if let Ok((mut _socket, _)) = listener.accept().await {
                // Hold the connection open for 10 seconds (simulating slow backend)
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        }
    });

    let client = Arc::new(
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap(),
    );

    let agent = create_agent(
        "cancel-test".to_string(),
        "Cancel Test".to_string(),
        format!("http://127.0.0.1:{}", addr.port()),
        BackendType::Generic,
        client,
        HashMap::new(),
        PrivacyZone::Restricted,
        None,
    )
    .unwrap();

    let request = ChatCompletionRequest {
        model: "test-model".to_string(),
        messages: vec![],
        stream: false,
        temperature: None,
        max_tokens: None,
        top_p: None,
        stop: None,
        presence_penalty: None,
        frequency_penalty: None,
        user: None,
        extra: std::collections::HashMap::new(),
    };

    // Drop the future after 100ms — it should not panic or leak
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        agent.chat_completion(request, None),
    )
    .await;

    // The timeout should fire (server never responds)
    assert!(result.is_err(), "Expected timeout, got {:?}", result);

    // If we get here without panic or hang, cancellation is safe
    // The agent's reqwest future was dropped and resources cleaned up

    server_handle.abort();
}
