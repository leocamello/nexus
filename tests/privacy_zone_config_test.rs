//! Integration tests for privacy zone and capability tier configuration (US3)
//!
//! Tests T014-T017:
//! - T014: Test configuration parsing with explicit zone="restricted" and tier=3
//! - T015: Test configuration parsing with missing zone field (defaults to backend type)
//! - T016: Test configuration parsing with missing tier field (defaults to 1)
//! - T017: Test startup validation rejects tier=10 with clear error message

use nexus::agent::PrivacyZone;
use nexus::config::backend::BackendConfig;
use nexus::registry::{BackendType, Registry};
use std::sync::Arc;

#[test]
fn test_explicit_zone_and_tier() {
    // T014: Parse explicit zone="Restricted" and tier=3
    let config_toml = r#"
        [[backends]]
        name = "test-explicit"
        url = "http://localhost:11434"
        type = "ollama"
        zone = "Restricted"
        tier = 3
    "#;

    #[derive(serde::Deserialize)]
    struct TestConfig {
        backends: Vec<BackendConfig>,
    }

    let config: TestConfig = toml::from_str(config_toml).expect("Failed to parse config");
    assert_eq!(config.backends.len(), 1);

    let backend = &config.backends[0];
    assert_eq!(backend.name, "test-explicit");
    assert_eq!(backend.effective_privacy_zone(), PrivacyZone::Restricted);
    assert_eq!(backend.effective_tier(), 3);
    assert!(backend.validate().is_ok());
}

#[test]
fn test_missing_zone_defaults_to_backend_type() {
    // T015: Missing zone field defaults to backend type default
    let config_toml = r#"
        [[backends]]
        name = "test-default-zone-ollama"
        url = "http://localhost:11434"
        type = "ollama"
        # zone omitted
    "#;

    #[derive(serde::Deserialize)]
    struct TestConfig {
        backends: Vec<BackendConfig>,
    }

    let config: TestConfig = toml::from_str(config_toml).expect("Failed to parse config");
    let backend = &config.backends[0];

    // Ollama defaults to Restricted
    assert_eq!(backend.effective_privacy_zone(), PrivacyZone::Restricted);
    assert!(backend.validate().is_ok());
}

#[test]
fn test_missing_zone_defaults_to_backend_type_cloud() {
    // T015: Cloud backends default to Open
    let config_toml = r#"
        [[backends]]
        name = "test-default-zone-openai"
        url = "https://api.openai.com/v1"
        type = "openai"
        api_key_env = "OPENAI_API_KEY"
        # zone omitted
    "#;

    #[derive(serde::Deserialize)]
    struct TestConfig {
        backends: Vec<BackendConfig>,
    }

    let config: TestConfig = toml::from_str(config_toml).expect("Failed to parse config");
    let backend = &config.backends[0];

    // OpenAI defaults to Open
    assert_eq!(backend.effective_privacy_zone(), PrivacyZone::Open);
    assert!(backend.validate().is_ok());
}

#[test]
fn test_missing_tier_defaults_to_one() {
    // T016: Missing tier field defaults to 1 (FR-022)
    let config_toml = r#"
        [[backends]]
        name = "test-default-tier"
        url = "http://localhost:11434"
        type = "ollama"
        zone = "Open"
        # tier omitted
    "#;

    #[derive(serde::Deserialize)]
    struct TestConfig {
        backends: Vec<BackendConfig>,
    }

    let config: TestConfig = toml::from_str(config_toml).expect("Failed to parse config");
    let backend = &config.backends[0];

    assert_eq!(backend.effective_tier(), 1); // FR-022: default is 1
    assert!(backend.validate().is_ok());
}

#[test]
fn test_invalid_tier_rejected_by_validation() {
    // T017: Startup validation rejects tier=10 with clear error
    let config_toml = r#"
        [[backends]]
        name = "test-invalid-tier"
        url = "http://localhost:11434"
        type = "ollama"
        tier = 10
    "#;

    #[derive(serde::Deserialize)]
    struct TestConfig {
        backends: Vec<BackendConfig>,
    }

    let config: TestConfig = toml::from_str(config_toml).expect("Failed to parse config");
    let backend = &config.backends[0];

    // Validation should reject tier=10
    let result = backend.validate();
    assert!(result.is_err());

    let err_msg = result.unwrap_err();
    assert!(err_msg.contains("invalid tier"));
    assert!(err_msg.contains("10"));
    assert!(err_msg.contains("1-5"));
}

#[test]
fn test_tier_range_boundaries() {
    // Test boundary conditions for tier validation
    for tier in 1..=5 {
        let config_toml = format!(
            r#"
            [[backends]]
            name = "test-tier-{}"
            url = "http://localhost:11434"
            type = "ollama"
            tier = {}
        "#,
            tier, tier
        );

        #[derive(serde::Deserialize)]
        struct TestConfig {
            backends: Vec<BackendConfig>,
        }

        let config: TestConfig = toml::from_str(&config_toml).expect("Failed to parse config");
        let backend = &config.backends[0];

        assert_eq!(backend.effective_tier(), tier);
        assert!(
            backend.validate().is_ok(),
            "tier {} should be valid",
            tier
        );
    }

    // Test invalid tiers
    for tier in [0, 6, 100, 255] {
        let config_toml = format!(
            r#"
            [[backends]]
            name = "test-invalid-tier-{}"
            url = "http://localhost:11434"
            type = "ollama"
            tier = {}
        "#,
            tier, tier
        );

        #[derive(serde::Deserialize)]
        struct TestConfig {
            backends: Vec<BackendConfig>,
        }

        let config: TestConfig = toml::from_str(&config_toml).expect("Failed to parse config");
        let backend = &config.backends[0];

        assert!(
            backend.validate().is_err(),
            "tier {} should be invalid",
            tier
        );
    }
}

#[test]
fn test_zone_tier_flow_to_agent_profile() {
    // T011: Verify AgentProfile population from BackendConfig
    let registry = Arc::new(Registry::new());
    let client = Arc::new(reqwest::Client::new());

    // Create backend config with explicit zone and tier
    let backend_config = BackendConfig {
        name: "test-backend".to_string(),
        url: "http://localhost:11434".to_string(),
        backend_type: BackendType::Ollama,
        priority: 50,
        api_key_env: None,
        zone: Some(PrivacyZone::Restricted),
        tier: Some(4),
    };

    // Create agent using the factory (simulating serve.rs behavior)
    let privacy_zone = backend_config.effective_privacy_zone();
    let capability_tier = Some(backend_config.effective_tier());

    let agent = nexus::agent::factory::create_agent(
        "test-backend".to_string(),
        backend_config.name.clone(),
        backend_config.url.clone(),
        backend_config.backend_type,
        client,
        std::collections::HashMap::new(),
        privacy_zone,
        capability_tier,
    )
    .expect("Failed to create agent");

    // Verify agent profile has correct zone and tier
    let profile = agent.profile();
    assert_eq!(profile.privacy_zone, PrivacyZone::Restricted);
    assert_eq!(profile.capability_tier, Some(4));
}
