//! Health checking module for backend health monitoring.
//!
//! This module provides background health checking for registered backends,
//! including automatic model discovery and status tracking.

mod config;
mod error;
mod parser;
mod state;

#[cfg(test)]
mod tests;

pub use config::*;
pub use error::*;
pub use state::*;

// Re-export for convenience
pub use state::HealthCheckResult;

use crate::registry::{Backend, BackendType, Registry};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Background service that periodically checks backend health.
pub struct HealthChecker {
    /// Reference to the backend registry
    registry: Arc<Registry>,
    /// HTTP client with connection pooling
    client: reqwest::Client,
    /// Health check configuration
    config: HealthCheckConfig,
    /// Per-backend health tracking state
    state: DashMap<String, BackendHealthState>,
}

impl HealthChecker {
    /// Create a new health checker with default HTTP client.
    pub fn new(registry: Arc<Registry>, config: HealthCheckConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            registry,
            client,
            config,
            state: DashMap::new(),
        }
    }

    /// Create a health checker with custom HTTP client (for testing).
    pub fn with_client(
        registry: Arc<Registry>,
        config: HealthCheckConfig,
        client: reqwest::Client,
    ) -> Self {
        Self {
            registry,
            client,
            config,
            state: DashMap::new(),
        }
    }

    /// Get the health check endpoint for a backend type.
    pub fn get_health_endpoint(backend_type: BackendType) -> &'static str {
        match backend_type {
            BackendType::Ollama => "/api/tags",
            BackendType::LlamaCpp => "/health",
            BackendType::VLLM
            | BackendType::Exo
            | BackendType::OpenAI
            | BackendType::LMStudio
            | BackendType::Generic => "/v1/models",
        }
    }

    /// Check a single backend's health.
    pub async fn check_backend(&self, backend: &Backend) -> HealthCheckResult {
        let endpoint = Self::get_health_endpoint(backend.backend_type);
        let url = format!("{}{}", backend.url, endpoint);

        let start = Instant::now();

        match self
            .client
            .get(&url)
            .timeout(Duration::from_secs(self.config.timeout_seconds))
            .send()
            .await
        {
            Ok(response) => {
                let latency_ms = start.elapsed().as_millis() as u32;
                
                // Record backend latency histogram (convert ms to seconds for Prometheus)
                let latency_seconds = latency_ms as f64 / 1000.0;
                metrics::histogram!("nexus_backend_latency_seconds",
                    "backend" => backend.id.clone()
                )
                .record(latency_seconds);

                if !response.status().is_success() {
                    return HealthCheckResult::Failure {
                        error: HealthCheckError::HttpError(response.status().as_u16()),
                    };
                }

                // Parse response based on backend type
                match response.text().await {
                    Ok(body) => self.parse_response(backend.backend_type, &body, latency_ms),
                    Err(e) => HealthCheckResult::Failure {
                        error: HealthCheckError::ParseError(e.to_string()),
                    },
                }
            }
            Err(e) => HealthCheckResult::Failure {
                error: Self::classify_error(e, self.config.timeout_seconds),
            },
        }
    }

    /// Parse response based on backend type.
    fn parse_response(
        &self,
        backend_type: BackendType,
        body: &str,
        latency_ms: u32,
    ) -> HealthCheckResult {
        match backend_type {
            BackendType::Ollama => match parser::parse_ollama_response(body) {
                Ok(models) => HealthCheckResult::Success { latency_ms, models },
                Err(error) => {
                    // Backend returned 200 but invalid JSON - treat as healthy, preserve models
                    tracing::warn!(
                        backend_type = ?backend_type,
                        error = %error,
                        "Backend returned 200 but invalid JSON, treating as healthy"
                    );
                    HealthCheckResult::SuccessWithParseError {
                        latency_ms,
                        parse_error: error.to_string(),
                    }
                }
            },
            BackendType::LlamaCpp => {
                match parser::parse_llamacpp_response(body) {
                    Ok(healthy) if healthy => HealthCheckResult::Success {
                        latency_ms,
                        models: vec![], // LlamaCpp doesn't return models
                    },
                    Ok(_) => HealthCheckResult::Failure {
                        error: HealthCheckError::HttpError(500),
                    },
                    Err(error) => {
                        // Backend returned 200 but invalid JSON - treat as healthy
                        tracing::warn!(
                            backend_type = ?backend_type,
                            error = %error,
                            "Backend returned 200 but invalid JSON, treating as healthy"
                        );
                        HealthCheckResult::SuccessWithParseError {
                            latency_ms,
                            parse_error: error.to_string(),
                        }
                    }
                }
            }
            BackendType::VLLM
            | BackendType::Exo
            | BackendType::OpenAI
            | BackendType::LMStudio
            | BackendType::Generic => {
                match parser::parse_openai_response(body) {
                    Ok(models) => HealthCheckResult::Success { latency_ms, models },
                    Err(error) => {
                        // Backend returned 200 but invalid JSON - treat as healthy, preserve models
                        tracing::warn!(
                            backend_type = ?backend_type,
                            error = %error,
                            "Backend returned 200 but invalid JSON, treating as healthy"
                        );
                        HealthCheckResult::SuccessWithParseError {
                            latency_ms,
                            parse_error: error.to_string(),
                        }
                    }
                }
            }
        }
    }

    /// Classify reqwest error into HealthCheckError.
    fn classify_error(e: reqwest::Error, timeout_seconds: u64) -> HealthCheckError {
        if e.is_timeout() {
            HealthCheckError::Timeout(timeout_seconds)
        } else {
            // All other errors treated as connection failures
            HealthCheckError::ConnectionFailed(e.to_string())
        }
    }

    /// Apply health check result to registry and update state.
    pub fn apply_result(&self, backend_id: &str, result: HealthCheckResult) {
        // Get or create backend state
        let mut state = self.state.entry(backend_id.to_string()).or_default();

        // Determine if status should transition
        let new_status = state.apply_result(&result, &self.config);
        state.last_check_time = Some(chrono::Utc::now());

        // Update registry based on result
        match &result {
            HealthCheckResult::Success { latency_ms, models } => {
                // Update latency
                let _ = self.registry.update_latency(backend_id, *latency_ms);

                // Update models (or preserve on empty for LlamaCpp)
                if !models.is_empty() {
                    if self
                        .registry
                        .update_models(backend_id, models.clone())
                        .is_ok()
                    {
                        state.last_models = models.clone();
                    }
                } else if !state.last_models.is_empty() {
                    // Preserve last known models for backends that don't report them
                    let _ = self
                        .registry
                        .update_models(backend_id, state.last_models.clone());
                }
            }
            HealthCheckResult::SuccessWithParseError { latency_ms, .. } => {
                // Update latency (backend is responding)
                let _ = self.registry.update_latency(backend_id, *latency_ms);

                // Preserve last known models (don't update with empty/invalid data)
                if !state.last_models.is_empty() {
                    let _ = self
                        .registry
                        .update_models(backend_id, state.last_models.clone());
                }
            }
            HealthCheckResult::Failure { .. } => {
                // Models preserved in state.last_models
            }
        }

        // Update status if transition occurred
        if let Some(status) = new_status {
            let error = match &result {
                HealthCheckResult::Failure { error } => Some(error.to_string()),
                _ => None,
            };

            if self
                .registry
                .update_status(backend_id, status, error)
                .is_ok()
            {
                tracing::info!(
                    backend_id = backend_id,
                    old_status = ?state.last_status,
                    new_status = ?status,
                    "Backend status changed"
                );
                state.last_status = status;
            }
        }
    }

    /// Check all registered backends once.
    pub async fn check_all_backends(&self) -> Vec<(String, HealthCheckResult)> {
        let backends: Vec<_> = self
            .registry
            .get_all_backends()
            .into_iter()
            .map(|b| (b.id.clone(), b))
            .collect();

        let mut results = Vec::with_capacity(backends.len());

        for (id, backend) in backends {
            let result = self.check_backend(&backend).await;
            self.apply_result(&id, result.clone());
            results.push((id, result));
        }

        results
    }

    /// Start the health checker background task.
    /// Returns a JoinHandle that resolves when the checker stops.
    pub fn start(self, cancel_token: CancellationToken) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(Duration::from_secs(self.config.interval_seconds));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            tracing::info!(
                interval_seconds = self.config.interval_seconds,
                "Health checker started"
            );

            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        tracing::info!("Health checker shutting down");
                        break;
                    }
                    _ = interval.tick() => {
                        let results = self.check_all_backends().await;
                        tracing::debug!(
                            backends_checked = results.len(),
                            "Health check cycle completed"
                        );
                    }
                }
            }
        })
    }
}
