//! Serve command implementation

use crate::api::{create_router, AppState};
use crate::cli::ServeArgs;
use crate::config::{LogFormat, NexusConfig};
use crate::health::HealthChecker;
use crate::registry::{Backend, DiscoverySource, Registry};
use crate::routing::reconciler::budget::{BudgetMetrics, BudgetReconciliationLoop};
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Load configuration with CLI overrides
pub fn load_config_with_overrides(
    args: &ServeArgs,
) -> Result<NexusConfig, Box<dyn std::error::Error>> {
    // Load from file if it exists, otherwise use defaults
    let mut config = if args.config.exists() {
        NexusConfig::load(Some(&args.config))?
    } else {
        tracing::debug!("Config file not found, using defaults");
        NexusConfig::default()
    };

    // Apply environment variable overrides
    config = config.with_env_overrides();

    // Apply CLI overrides (highest priority)
    if let Some(port) = args.port {
        config.server.port = port;
    }
    if let Some(ref host) = args.host {
        config.server.host = host.clone();
    }
    if let Some(ref log_level) = args.log_level {
        config.logging.level = log_level.clone();
    }
    if args.no_discovery {
        config.discovery.enabled = false;
    }
    if args.no_health_check {
        config.health_check.enabled = false;
    }

    Ok(config)
}

/// Initialize tracing based on configuration
pub fn init_tracing(
    config: &crate::config::LoggingConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    // Build filter directives using helper function
    let filter_str = crate::logging::build_filter_directives(config);

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&filter_str));

    // Warn if content logging is enabled
    if config.enable_content_logging {
        eprintln!(
            "WARNING: Content logging is enabled. Request/response message content will be logged."
        );
        eprintln!("         This may include sensitive data. Use only for debugging.");
    }

    match config.format {
        LogFormat::Pretty => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().pretty())
                .try_init()?;
        }
        LogFormat::Json => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().json())
                .try_init()?;
        }
    }

    Ok(())
}

/// Load backends from configuration into registry
pub fn load_backends_from_config(
    config: &NexusConfig,
    registry: &Registry,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create shared HTTP client for all agents (T027)
    let client = Arc::new(
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?,
    );

    for backend_config in &config.backends {
        let id = uuid::Uuid::new_v4().to_string();

        // Build metadata from backend config (T027)
        let mut metadata = HashMap::new();
        if let Some(api_key_env) = &backend_config.api_key_env {
            metadata.insert("api_key_env".to_string(), api_key_env.clone());
        }

        let backend = Backend::new(
            id.clone(),
            backend_config.name.clone(),
            backend_config.url.clone(),
            backend_config.backend_type,
            vec![], // Models will be discovered by health checker
            DiscoverySource::Static,
            metadata.clone(),
        );

        // Create agent for this backend (T027)
        let agent = crate::agent::factory::create_agent(
            id.clone(),
            backend_config.name.clone(),
            backend_config.url.clone(),
            backend_config.backend_type,
            Arc::clone(&client),
            metadata,
            backend_config.effective_privacy_zone(),
            Some(backend_config.effective_tier()),
        )?;

        // Register both backend and agent
        registry.add_backend_with_agent(backend, agent)?;

        tracing::info!(
            name = %backend_config.name,
            url = %backend_config.url,
            backend_type = ?backend_config.backend_type,
            "Loaded static backend from config"
        );
    }

    Ok(())
}

/// Build API router with all endpoints
fn build_api_router(
    registry: Arc<Registry>,
    config: Arc<NexusConfig>,
) -> (axum::Router, Arc<AppState>) {
    let app_state = Arc::new(AppState::new(registry, config));
    let router = create_router(Arc::clone(&app_state));
    (router, app_state)
}

/// Wait for shutdown signal (SIGINT or SIGTERM)
async fn shutdown_signal(cancel_token: CancellationToken) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received SIGINT, shutting down...");
        }
        _ = terminate => {
            tracing::info!("Received SIGTERM, shutting down...");
        }
    }

    cancel_token.cancel();
}

/// Main serve command handler
pub async fn run_serve(args: ServeArgs) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load and merge configuration
    let config = load_config_with_overrides(&args)?;

    // Validate configuration
    config.validate()?;

    // 2. Initialize tracing
    init_tracing(&config.logging)?;

    tracing::info!("Starting Nexus server");
    tracing::debug!(?config, "Loaded configuration");

    // 3. Create registry and load static backends
    let registry = Arc::new(Registry::new());
    load_backends_from_config(&config, &registry)?;

    // 4. Build API router and get AppState (to access ws_broadcast)
    let config_arc = Arc::new(config.clone());
    let (app, app_state) = build_api_router(registry.clone(), config_arc);

    // 5. Start health checker (if enabled) with broadcast sender
    let cancel_token = CancellationToken::new();
    let health_handle = if config.health_check.enabled {
        tracing::info!("Starting health checker");
        let checker = HealthChecker::new(registry.clone(), config.health_check.clone())
            .with_broadcast(app_state.ws_broadcast.clone());
        Some(checker.start(cancel_token.clone()))
    } else {
        tracing::info!("Health checking disabled");
        None
    };

    // 4.5. Start mDNS discovery (if enabled)
    let discovery_handle = if config.discovery.enabled {
        tracing::info!("Starting mDNS discovery");
        let discovery =
            crate::discovery::MdnsDiscovery::new(config.discovery.clone(), registry.clone());
        Some(discovery.start(cancel_token.clone()))
    } else {
        tracing::info!("mDNS discovery disabled");
        None
    };

    // 4.6. Start budget reconciliation loop (if budget is configured)
    let budget_handle = if config.routing.budget.monthly_limit_usd.is_some() {
        tracing::info!("Starting budget reconciliation loop");
        let budget_state: Arc<DashMap<String, BudgetMetrics>> = Arc::new(DashMap::new());
        let budget_loop = BudgetReconciliationLoop::new(
            Arc::clone(&budget_state),
            config.routing.budget.clone(),
            config.routing.budget.reconciliation_interval_secs,
        );
        Some(budget_loop.start(cancel_token.clone()))
    } else {
        tracing::debug!("Budget management disabled (no monthly limit configured)");
        None
    };

    // 6. Bind and serve
    let addr = format!("{}:{}", config.server.host, config.server.port);
    tracing::info!(addr = %addr, "Nexus API server listening");

    let listener = tokio::net::TcpListener::bind(&addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(cancel_token.clone()))
        .await?;

    // 7. Cleanup
    if let Some(handle) = health_handle {
        tracing::info!("Waiting for health checker to stop");
        handle.await?;
    }

    if let Some(handle) = discovery_handle {
        tracing::info!("Waiting for mDNS discovery to stop");
        handle.await?;
    }

    if let Some(handle) = budget_handle {
        tracing::info!("Waiting for budget reconciliation loop to stop");
        handle.await?;
    }

    tracing::info!("Nexus server stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BackendConfig;
    use crate::registry::BackendType;
    use std::path::PathBuf;
    use std::time::Duration;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_serve_config_loading() {
        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "[server]\nport = 8080").unwrap();

        let args = ServeArgs {
            config: temp.path().to_path_buf(),
            port: None,
            host: None,
            log_level: None,
            no_discovery: false,
            no_health_check: false,
        };

        let config = load_config_with_overrides(&args).unwrap();
        assert_eq!(config.server.port, 8080);
    }

    #[tokio::test]
    async fn test_serve_cli_overrides_config() {
        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "[server]\nport = 8080").unwrap();

        let args = ServeArgs {
            config: temp.path().to_path_buf(),
            port: Some(9000), // Override
            host: None,
            log_level: None,
            no_discovery: false,
            no_health_check: false,
        };

        let config = load_config_with_overrides(&args).unwrap();
        assert_eq!(config.server.port, 9000); // CLI wins
    }

    #[tokio::test]
    async fn test_serve_works_without_config_file() {
        let args = ServeArgs {
            config: PathBuf::from("nonexistent.toml"),
            port: None,
            host: None,
            log_level: None,
            no_discovery: false,
            no_health_check: false,
        };

        let config = load_config_with_overrides(&args).unwrap();
        assert_eq!(config.server.port, 8000); // Default
    }

    #[tokio::test]
    async fn test_backends_loaded_from_config() {
        let mut config = NexusConfig::default();
        config.backends.push(BackendConfig {
            name: "test".to_string(),
            url: "http://localhost:11434".to_string(),
            backend_type: BackendType::Ollama,
            priority: 1,
            api_key_env: None,
            zone: None,
            tier: None,
        });

        let registry = Arc::new(Registry::new());
        load_backends_from_config(&config, &registry).unwrap();

        assert_eq!(registry.backend_count(), 1);
    }

    #[tokio::test]
    async fn test_shutdown_signal_triggers_cancel() {
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        let handle = tokio::spawn(async move {
            // Simulate shutdown after 100ms
            tokio::time::sleep(Duration::from_millis(100)).await;
            cancel_clone.cancel();
        });

        // This should return when cancelled
        tokio::select! {
            _ = cancel.cancelled() => {}
            _ = tokio::time::sleep(Duration::from_secs(5)) => {
                panic!("Shutdown didn't trigger");
            }
        }

        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_health_checker_stops_on_shutdown() {
        let registry = Arc::new(Registry::new());
        let config = crate::health::HealthCheckConfig::default();
        let checker = HealthChecker::new(registry, config);

        let cancel = CancellationToken::new();
        let handle = checker.start(cancel.clone());

        // Let it run briefly
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Trigger shutdown
        cancel.cancel();

        // Should complete quickly
        let result = tokio::time::timeout(Duration::from_secs(1), handle).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_backends_loaded_with_agents() {
        let mut config = NexusConfig::default();
        config.backends.push(BackendConfig {
            name: "ollama-test".to_string(),
            url: "http://localhost:11434".to_string(),
            backend_type: BackendType::Ollama,
            priority: 1,
            api_key_env: None,
            zone: None,
            tier: None,
        });
        config.backends.push(BackendConfig {
            name: "vllm-test".to_string(),
            url: "http://localhost:8000".to_string(),
            backend_type: BackendType::VLLM,
            priority: 2,
            api_key_env: None,
            zone: None,
            tier: None,
        });

        let registry = Arc::new(Registry::new());
        load_backends_from_config(&config, &registry).unwrap();

        assert_eq!(registry.backend_count(), 2);

        // Verify agents are created for each backend
        let backends = registry.get_all_backends();
        for backend in &backends {
            let agent = registry.get_agent(&backend.id);
            assert!(
                agent.is_some(),
                "Agent should exist for backend {}",
                backend.name
            );
        }
    }

    #[tokio::test]
    async fn test_backends_loaded_with_api_key_env() {
        // Set a test API key in the environment
        std::env::set_var("NEXUS_TEST_OPENAI_KEY", "sk-test-key-12345");

        let mut config = NexusConfig::default();
        config.backends.push(BackendConfig {
            name: "openai-test".to_string(),
            url: "https://api.openai.com".to_string(),
            backend_type: BackendType::OpenAI,
            priority: 1,
            api_key_env: Some("NEXUS_TEST_OPENAI_KEY".to_string()),
            zone: None,
            tier: None,
        });

        let registry = Arc::new(Registry::new());
        load_backends_from_config(&config, &registry).unwrap();

        assert_eq!(registry.backend_count(), 1);

        let backends = registry.get_all_backends();
        let backend = &backends[0];
        let agent = registry.get_agent(&backend.id);
        assert!(agent.is_some(), "Agent should exist for OpenAI backend");

        // Verify agent profile has correct type
        let profile = agent.unwrap().profile();
        assert_eq!(profile.backend_type, "openai");

        // Clean up
        std::env::remove_var("NEXUS_TEST_OPENAI_KEY");
    }

    #[tokio::test]
    async fn test_build_api_router_returns_app_state() {
        let registry = Arc::new(Registry::new());
        let config = Arc::new(NexusConfig::default());
        let (_router, app_state) = build_api_router(registry.clone(), config.clone());

        // AppState should reference the same registry
        assert_eq!(app_state.registry.backend_count(), 0);
    }

    #[tokio::test]
    async fn test_cli_overrides_host_and_log_level() {
        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), "[server]\nhost = \"0.0.0.0\"\nport = 8000").unwrap();

        let args = ServeArgs {
            config: temp.path().to_path_buf(),
            port: None,
            host: Some("127.0.0.1".to_string()),
            log_level: Some("debug".to_string()),
            no_discovery: false,
            no_health_check: false,
        };

        let config = load_config_with_overrides(&args).unwrap();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.logging.level, "debug");
    }

    #[tokio::test]
    async fn test_cli_overrides_disable_discovery_and_health() {
        let args = ServeArgs {
            config: PathBuf::from("nonexistent.toml"),
            port: None,
            host: None,
            log_level: None,
            no_discovery: true,
            no_health_check: true,
        };

        let config = load_config_with_overrides(&args).unwrap();
        assert!(!config.discovery.enabled);
        assert!(!config.health_check.enabled);
    }

    #[tokio::test]
    async fn test_empty_config_loads_zero_backends() {
        let config = NexusConfig::default();
        let registry = Arc::new(Registry::new());
        load_backends_from_config(&config, &registry).unwrap();
        assert_eq!(registry.backend_count(), 0);
    }

    #[tokio::test]
    async fn test_budget_loop_starts_and_stops() {
        let budget_state: Arc<DashMap<String, BudgetMetrics>> = Arc::new(DashMap::new());
        let budget_config = BudgetConfig {
            monthly_limit_usd: Some(100.0),
            soft_limit_percent: 75.0,
            hard_limit_action: HardLimitAction::Warn,
            reconciliation_interval_secs: 60,
        };
        let budget_loop = BudgetReconciliationLoop::new(Arc::clone(&budget_state), budget_config, 1);

        let cancel = CancellationToken::new();
        let handle = budget_loop.start(cancel.clone());

        // Let it run briefly
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Trigger shutdown
        cancel.cancel();

        // Should complete quickly
        let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_build_api_router_creates_valid_state() {
        let registry = Arc::new(Registry::new());
        let mut config = NexusConfig::default();
        config.backends.push(BackendConfig {
            name: "test".to_string(),
            url: "http://localhost:11434".to_string(),
            backend_type: BackendType::Ollama,
            priority: 1,
            api_key_env: None,
            zone: None,
            tier: None,
        });

        load_backends_from_config(&config, &registry).unwrap();
        let config_arc = Arc::new(config);
        let (_router, app_state) = build_api_router(registry.clone(), config_arc);

        assert_eq!(app_state.registry.backend_count(), 1);
    }
}
