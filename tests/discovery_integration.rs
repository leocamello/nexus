//! Integration tests for mDNS Discovery (F05)
//!
//! These tests verify the integration between the discovery module,
//! registry, and the overall system behavior.

use nexus::config::{DiscoveryConfig, NexusConfig};
use nexus::discovery::{DiscoveryEvent, MdnsDiscovery};
use nexus::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Registry};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

// ============================================================================
// Helper Functions
// ============================================================================

fn create_test_registry() -> Arc<Registry> {
    Arc::new(Registry::new())
}

fn create_service_found_event(
    instance: &str,
    ip: Ipv4Addr,
    port: u16,
    service_type: &str,
) -> DiscoveryEvent {
    DiscoveryEvent::ServiceFound {
        instance: instance.to_string(),
        service_type: service_type.to_string(),
        addresses: vec![IpAddr::V4(ip)],
        port,
        txt_records: HashMap::new(),
    }
}

fn create_service_removed_event(instance: &str, service_type: &str) -> DiscoveryEvent {
    DiscoveryEvent::ServiceRemoved {
        instance: instance.to_string(),
        service_type: service_type.to_string(),
    }
}

// ============================================================================
// Registry Integration Tests
// ============================================================================

/// Test that discovered backends are properly registered with correct metadata
#[tokio::test]
async fn discovery_registers_backend_with_correct_metadata() {
    let registry = create_test_registry();
    let config = DiscoveryConfig::default();
    let discovery = MdnsDiscovery::new(config, registry.clone());

    let event = DiscoveryEvent::ServiceFound {
        instance: "my-ollama-server._ollama._tcp.local".to_string(),
        service_type: "_ollama._tcp.local".to_string(),
        addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100))],
        port: 11434,
        txt_records: {
            let mut txt = HashMap::new();
            txt.insert("version".to_string(), "0.1.32".to_string());
            txt
        },
    };

    discovery.handle_event(event).await;

    // Verify backend was added
    assert_eq!(registry.backend_count(), 1);

    // Get the backend and verify all fields
    let backends = registry.get_all_backends();
    let backend = backends.first().unwrap();

    assert_eq!(backend.url, "http://192.168.1.100:11434");
    assert_eq!(backend.backend_type, BackendType::Ollama);
    assert_eq!(backend.discovery_source, DiscoverySource::MDNS);
    assert_eq!(backend.status, BackendStatus::Unknown); // Initial status
    assert!(backend.metadata.contains_key("mdns_instance"));
    assert_eq!(
        backend.metadata.get("mdns_instance").unwrap(),
        "my-ollama-server._ollama._tcp.local"
    );
}

/// Test that the registry correctly tracks backends by mDNS instance
#[tokio::test]
async fn registry_tracks_mdns_instance_for_lookup() {
    let registry = create_test_registry();
    let config = DiscoveryConfig::default();
    let discovery = MdnsDiscovery::new(config, registry.clone());

    let instance = "unique-server._ollama._tcp.local";
    let event = create_service_found_event(
        instance,
        Ipv4Addr::new(10, 0, 0, 50),
        11434,
        "_ollama._tcp.local",
    );

    discovery.handle_event(event).await;

    // Should be able to find by mDNS instance
    let id = registry.find_by_mdns_instance(instance);
    assert!(id.is_some());

    // Should return correct backend
    let backend = registry.get_backend(&id.unwrap()).unwrap();
    assert_eq!(backend.url, "http://10.0.0.50:11434");
}

/// Test URL normalization prevents duplicate backends
#[tokio::test]
async fn discovery_prevents_duplicate_urls_with_normalization() {
    let registry = create_test_registry();
    let config = DiscoveryConfig::default();
    let discovery = MdnsDiscovery::new(config, registry.clone());

    // Add first backend
    let event1 = create_service_found_event(
        "server1._ollama._tcp.local",
        Ipv4Addr::new(192, 168, 1, 10),
        11434,
        "_ollama._tcp.local",
    );
    discovery.handle_event(event1).await;

    // Try to add same URL from different mDNS instance
    let event2 = create_service_found_event(
        "server1-alias._ollama._tcp.local",
        Ipv4Addr::new(192, 168, 1, 10), // Same IP
        11434,                          // Same port
        "_ollama._tcp.local",
    );
    discovery.handle_event(event2).await;

    // Should still have only 1 backend
    assert_eq!(registry.backend_count(), 1);
}

/// Test static backends take precedence over mDNS discovered
#[tokio::test]
async fn static_backends_take_precedence_over_mdns() {
    let registry = create_test_registry();

    // Pre-add a static backend
    let static_backend = Backend::new(
        uuid::Uuid::new_v4().to_string(),
        "My Static Server".to_string(),
        "http://192.168.1.50:11434".to_string(),
        BackendType::Ollama,
        vec![],
        DiscoverySource::Static,
        HashMap::new(),
    );
    registry.add_backend(static_backend).unwrap();

    let config = DiscoveryConfig::default();
    let discovery = MdnsDiscovery::new(config, registry.clone());

    // Try to discover same URL via mDNS
    let event = create_service_found_event(
        "discovered._ollama._tcp.local",
        Ipv4Addr::new(192, 168, 1, 50), // Same IP as static
        11434,
        "_ollama._tcp.local",
    );
    discovery.handle_event(event).await;

    // Should still have only 1 backend (the static one)
    assert_eq!(registry.backend_count(), 1);

    let backends = registry.get_all_backends();
    let backend = backends.first().unwrap();
    assert_eq!(backend.discovery_source, DiscoverySource::Static);
    assert_eq!(backend.name, "My Static Server");
}

// ============================================================================
// Service Lifecycle Tests
// ============================================================================

/// Test complete service lifecycle: discover -> remove -> cleanup
#[tokio::test]
async fn service_lifecycle_discover_remove_cleanup() {
    let registry = create_test_registry();
    let config = DiscoveryConfig {
        enabled: true,
        grace_period_seconds: 1, // Short for testing
        ..Default::default()
    };
    let discovery = MdnsDiscovery::new(config, registry.clone());

    let instance = "lifecycle-test._ollama._tcp.local";

    // Step 1: Discover
    discovery
        .handle_event(create_service_found_event(
            instance,
            Ipv4Addr::new(172, 16, 0, 1),
            11434,
            "_ollama._tcp.local",
        ))
        .await;
    assert_eq!(registry.backend_count(), 1);

    // Step 2: Service disappears
    discovery
        .handle_event(create_service_removed_event(instance, "_ollama._tcp.local"))
        .await;

    // Backend still exists but status is Unknown
    let id = registry.find_by_mdns_instance(instance).unwrap();
    let backend = registry.get_backend(&id).unwrap();
    assert_eq!(backend.status, BackendStatus::Unknown);
    assert_eq!(registry.backend_count(), 1);

    // Step 3: Wait for grace period and cleanup
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    discovery.cleanup_stale_backends().await;

    // Backend should be removed
    assert_eq!(registry.backend_count(), 0);
}

/// Test service reappearance cancels pending removal
#[tokio::test]
async fn service_reappearance_cancels_removal() {
    let registry = create_test_registry();
    let config = DiscoveryConfig {
        enabled: true,
        grace_period_seconds: 60, // Long grace period
        ..Default::default()
    };
    let discovery = MdnsDiscovery::new(config, registry.clone());

    let instance = "flaky-server._ollama._tcp.local";

    // Discover
    discovery
        .handle_event(create_service_found_event(
            instance,
            Ipv4Addr::new(192, 168, 1, 99),
            11434,
            "_ollama._tcp.local",
        ))
        .await;

    // Remove (starts grace period)
    discovery
        .handle_event(create_service_removed_event(instance, "_ollama._tcp.local"))
        .await;

    // Reappear (should cancel removal)
    discovery
        .handle_event(create_service_found_event(
            instance,
            Ipv4Addr::new(192, 168, 1, 99),
            11434,
            "_ollama._tcp.local",
        ))
        .await;

    // Run cleanup - should NOT remove the backend
    discovery.cleanup_stale_backends().await;

    // Backend should still exist
    assert_eq!(registry.backend_count(), 1);
    let id = registry.find_by_mdns_instance(instance).unwrap();
    // Status might still be Unknown from the removal event, but backend exists
    assert!(registry.get_backend(&id).is_some());
}

// ============================================================================
// Multiple Service Types Tests
// ============================================================================

/// Test discovery of different service types
#[tokio::test]
async fn discovery_handles_multiple_service_types() {
    let registry = create_test_registry();
    let config = DiscoveryConfig {
        enabled: true,
        service_types: vec![
            "_ollama._tcp.local".to_string(),
            "_llm._tcp.local".to_string(),
        ],
        ..Default::default()
    };
    let discovery = MdnsDiscovery::new(config, registry.clone());

    // Discover Ollama service
    discovery
        .handle_event(create_service_found_event(
            "ollama-server._ollama._tcp.local",
            Ipv4Addr::new(192, 168, 1, 10),
            11434,
            "_ollama._tcp.local",
        ))
        .await;

    // Discover generic LLM service
    discovery
        .handle_event(create_service_found_event(
            "vllm-server._llm._tcp.local",
            Ipv4Addr::new(192, 168, 1, 20),
            8000,
            "_llm._tcp.local",
        ))
        .await;

    // Should have both backends
    assert_eq!(registry.backend_count(), 2);

    // Verify different backend types
    let backends = registry.get_all_backends();
    let types: Vec<_> = backends.iter().map(|b| &b.backend_type).collect();
    assert!(types.contains(&&BackendType::Ollama));
    assert!(types.contains(&&BackendType::Generic));
}

// ============================================================================
// Cancellation and Shutdown Tests
// ============================================================================

/// Test that discovery service responds to cancellation
#[tokio::test]
async fn discovery_responds_to_cancellation_token() {
    let registry = create_test_registry();
    let config = DiscoveryConfig {
        enabled: false, // Disable so we don't try real mDNS
        ..Default::default()
    };
    let discovery = MdnsDiscovery::new(config, registry);
    let cancel_token = CancellationToken::new();

    let handle = discovery.start(cancel_token.clone());

    // Give it a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Cancel
    cancel_token.cancel();

    // Should complete within reasonable time
    let result = tokio::time::timeout(std::time::Duration::from_secs(1), handle).await;
    assert!(result.is_ok(), "Discovery should respond to cancellation");
}

/// Test that disabled discovery completes immediately
#[tokio::test]
async fn disabled_discovery_returns_immediately() {
    let registry = create_test_registry();
    let config = DiscoveryConfig {
        enabled: false,
        ..Default::default()
    };
    let discovery = MdnsDiscovery::new(config, registry);
    let cancel_token = CancellationToken::new();

    let handle = discovery.start(cancel_token);

    // Should complete almost immediately
    let result = tokio::time::timeout(std::time::Duration::from_millis(100), handle).await;
    assert!(
        result.is_ok(),
        "Disabled discovery should return immediately"
    );
}

// ============================================================================
// Configuration Integration Tests
// ============================================================================

/// Test that discovery respects configuration settings
#[tokio::test]
async fn discovery_respects_config_settings() {
    let registry = create_test_registry();

    // Custom configuration
    let config = DiscoveryConfig {
        enabled: true,
        service_types: vec!["_custom._tcp.local".to_string()],
        grace_period_seconds: 120,
    };

    let discovery = MdnsDiscovery::new(config.clone(), registry.clone());

    // Discover via custom service type
    let event = DiscoveryEvent::ServiceFound {
        instance: "custom-server._custom._tcp.local".to_string(),
        service_type: "_custom._tcp.local".to_string(),
        addresses: vec![IpAddr::V4(Ipv4Addr::new(10, 10, 10, 10))],
        port: 9999,
        txt_records: HashMap::new(),
    };

    discovery.handle_event(event).await;

    // Should be registered as Generic type (unknown service type)
    assert_eq!(registry.backend_count(), 1);
    let backends = registry.get_all_backends();
    let backend = backends.first().unwrap();
    assert_eq!(backend.backend_type, BackendType::Generic);
}

/// Test configuration loads correctly from NexusConfig
#[test]
fn discovery_config_from_nexus_config() {
    let toml = r#"
[discovery]
enabled = true
service_types = ["_ollama._tcp.local", "_llm._tcp.local"]
grace_period_seconds = 90
"#;

    let config: NexusConfig = toml::from_str(toml).unwrap();

    assert!(config.discovery.enabled);
    assert_eq!(config.discovery.service_types.len(), 2);
    assert_eq!(config.discovery.grace_period_seconds, 90);
}

// ============================================================================
// Edge Cases
// ============================================================================

/// Test handling of service with no addresses
#[tokio::test]
async fn discovery_handles_service_without_addresses() {
    let registry = create_test_registry();
    let config = DiscoveryConfig::default();
    let discovery = MdnsDiscovery::new(config, registry.clone());

    // Event with empty addresses
    let event = DiscoveryEvent::ServiceFound {
        instance: "no-address._ollama._tcp.local".to_string(),
        service_type: "_ollama._tcp.local".to_string(),
        addresses: vec![], // No addresses!
        port: 11434,
        txt_records: HashMap::new(),
    };

    discovery.handle_event(event).await;

    // Should NOT add backend (can't connect without address)
    assert_eq!(registry.backend_count(), 0);
}

/// Test handling of removed service that was never discovered
#[tokio::test]
async fn discovery_handles_removal_of_unknown_service() {
    let registry = create_test_registry();
    let config = DiscoveryConfig::default();
    let discovery = MdnsDiscovery::new(config, registry.clone());

    // Try to remove service that was never discovered
    let event = create_service_removed_event("unknown._ollama._tcp.local", "_ollama._tcp.local");

    discovery.handle_event(event).await;

    // Should not panic or error, just do nothing
    assert_eq!(registry.backend_count(), 0);
}
