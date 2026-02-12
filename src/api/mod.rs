//! # Core API Gateway
//!
//! OpenAI-compatible HTTP endpoints for the Nexus LLM orchestrator.
//!
//! This module implements the HTTP API server that provides OpenAI-compatible
//! endpoints for chat completions, model listing, and health checks.
//!
//! ## Endpoints
//!
//! - `POST /v1/chat/completions` - Chat completion (non-streaming)
//! - `GET /v1/models` - List available models from healthy backends
//! - `GET /health` - System health status with backend counts
//!
//! ## Example
//!
//! ```no_run
//! use nexus::api::{AppState, create_router};
//! use nexus::config::NexusConfig;
//! use nexus::registry::Registry;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create registry and config
//! let registry = Arc::new(Registry::new());
//! let config = Arc::new(NexusConfig::default());
//!
//! // Create application state
//! let state = Arc::new(AppState::new(registry, config));
//!
//! // Create router with all endpoints
//! let app = create_router(state);
//!
//! // Start server
//! let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await?;
//! axum::serve(listener, app).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Architecture
//!
//! The API Gateway follows a simple request flow:
//! 1. Request received and parsed into OpenAI-compatible types
//! 2. Registry queried for backends supporting the requested model
//! 3. Request proxied to first healthy backend
//! 4. On failure, retry with next healthy backend (up to max_retries)
//! 5. Response returned in OpenAI format or error returned
//!
//! ## Error Handling
//!
//! All errors are returned in OpenAI-compatible format:
//! ```json
//! {
//!   "error": {
//!     "message": "Model 'gpt-4' not found",
//!     "type": "invalid_request_error",
//!     "param": "model",
//!     "code": "model_not_found"
//!   }
//! }
//! ```

mod completions;
mod health;
mod models;
pub mod types;

pub use types::*;

use crate::config::NexusConfig;
use crate::metrics::MetricsCollector;
use crate::registry::Registry;
use crate::routing;
use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::limit::RequestBodyLimitLayer;

use std::time::Instant;

/// Maximum request body size (10 MB).
const MAX_BODY_SIZE: usize = 10 * 1024 * 1024;

/// Shared application state accessible to all handlers.
pub struct AppState {
    pub registry: Arc<Registry>,
    pub config: Arc<NexusConfig>,
    pub http_client: reqwest::Client,
    pub router: Arc<routing::Router>,
    /// Server startup time for uptime tracking
    pub start_time: Instant,
    /// Metrics collector for observability
    pub metrics_collector: Arc<MetricsCollector>,
}

impl AppState {
    /// Create new application state with the given registry and configuration.
    pub fn new(registry: Arc<Registry>, config: Arc<NexusConfig>) -> Self {
        let timeout_secs = config.server.request_timeout_seconds;

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .pool_max_idle_per_host(10)
            .build()
            .expect("Failed to create HTTP client");

        let start_time = Instant::now();

        // Create router from config
        let router = Arc::new(routing::Router::with_aliases_and_fallbacks(
            Arc::clone(&registry),
            config.routing.strategy.into(),
            config.routing.weights.clone().into(),
            config.routing.aliases.clone(),
            config.routing.fallbacks.clone(),
        ));

        // Initialize metrics (safe to call multiple times - will reuse existing if already set)
        let prometheus_handle = crate::metrics::setup_metrics().unwrap_or_else(|e| {
            // If metrics are already initialized (e.g., in tests), create a new handle
            // by building a recorder without installing it globally
            tracing::debug!("Metrics already initialized, creating new handle: {}", e);
            crate::metrics::PrometheusBuilder::new()
                .build_recorder()
                .handle()
        });
        
        let metrics_collector = Arc::new(MetricsCollector::new(
            Arc::clone(&registry),
            start_time,
            prometheus_handle,
        ));

        Self {
            registry,
            config,
            http_client,
            router,
            start_time,
            metrics_collector,
        }
    }
}

/// Create the main API router with all endpoints configured.
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(completions::handle))
        .route("/v1/models", get(models::handle))
        .route("/health", get(health::handle))
        .route(
            "/metrics",
            get(crate::metrics::handler::metrics_handler),
        )
        .route("/v1/stats", get(crate::metrics::handler::stats_handler))
        .layer(RequestBodyLimitLayer::new(MAX_BODY_SIZE))
        .with_state(state)
}
