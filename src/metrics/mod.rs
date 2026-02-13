//! # Metrics Collection Module
//!
//! Provides request metrics tracking, Prometheus export, and JSON stats API.
//!
//! ## Overview
//!
//! This module exposes two endpoints:
//! - `GET /metrics` - Prometheus text format metrics
//! - `GET /v1/stats` - JSON format statistics
//!
//! ## Metrics Tracked
//!
//! **Counters:**
//! - `nexus_requests_total{model, backend, status}` - Total requests
//! - `nexus_errors_total{error_type, model}` - Total errors by type
//! - `nexus_fallbacks_total{from_model, to_model}` - Routing fallbacks
//!
//! **Histograms:**
//! - `nexus_request_duration_seconds{model, backend}` - Request duration
//! - `nexus_backend_latency_seconds{backend}` - Health check latency
//! - `nexus_tokens_total{model, backend, type}` - Token counts
//!
//! **Gauges:**
//! - `nexus_backends_total` - Total registered backends
//! - `nexus_backends_healthy` - Healthy backends count
//! - `nexus_models_available` - Unique models available
//! - `nexus_pending_requests{backend}` - Pending requests per backend

pub mod handler;
pub mod types;

pub use types::*;

// Re-export PrometheusBuilder for test compatibility
pub use metrics_exporter_prometheus::PrometheusBuilder;

use crate::registry::{BackendStatus, Registry};
use dashmap::DashMap;
use std::sync::Arc;
use std::time::Instant;

/// Central coordinator for metrics collection and gauge computation.
pub struct MetricsCollector {
    /// Reference to backend registry for computing gauges
    registry: Arc<Registry>,
    /// Gateway startup time for uptime calculation
    start_time: Instant,
    /// Thread-safe cache for sanitized Prometheus labels
    label_cache: DashMap<String, String>,
    /// Prometheus handle for rendering metrics
    prometheus_handle: metrics_exporter_prometheus::PrometheusHandle,
}

impl MetricsCollector {
    /// Create a new MetricsCollector.
    pub fn new(
        registry: Arc<Registry>,
        start_time: Instant,
        prometheus_handle: metrics_exporter_prometheus::PrometheusHandle,
    ) -> Self {
        Self {
            registry,
            start_time,
            label_cache: DashMap::new(),
            prometheus_handle,
        }
    }

    /// Get sanitized Prometheus label (cached for performance).
    ///
    /// Prometheus label names must match regex: `[a-zA-Z_][a-zA-Z0-9_]*`
    /// This function replaces invalid characters with underscores.
    pub fn sanitize_label(&self, label: &str) -> String {
        // Check cache first
        if let Some(cached) = self.label_cache.get(label) {
            return cached.clone();
        }

        // Sanitize: replace non-alphanumeric (except underscore) with underscore
        let mut sanitized = label
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();

        // Ensure first character is not a digit
        if sanitized.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            sanitized.insert(0, '_');
        }

        // Cache and return
        self.label_cache
            .insert(label.to_string(), sanitized.clone());
        sanitized
    }

    /// Update fleet state gauges from Registry.
    ///
    /// This computes current state metrics that are derived from the Registry:
    /// - Total backends
    /// - Healthy backends
    /// - Available models
    pub fn update_fleet_gauges(&self) {
        let backends = self.registry.get_all_backends();

        // Total backends
        metrics::gauge!("nexus_backends_total").set(backends.len() as f64);

        // Healthy backends count
        let healthy_count = backends
            .iter()
            .filter(|b| b.status == BackendStatus::Healthy)
            .count();
        metrics::gauge!("nexus_backends_healthy").set(healthy_count as f64);

        // Unique models available across healthy backends
        let unique_models: std::collections::HashSet<String> = backends
            .iter()
            .filter(|b| b.status == BackendStatus::Healthy)
            .flat_map(|b| b.models.iter().map(|m| m.id.clone()))
            .collect();
        metrics::gauge!("nexus_models_available").set(unique_models.len() as f64);
    }

    /// Get uptime in seconds since gateway startup.
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Get reference to the registry.
    pub fn registry(&self) -> &Arc<Registry> {
        &self.registry
    }

    /// Render Prometheus metrics in text format.
    pub fn render_metrics(&self) -> String {
        self.prometheus_handle.render()
    }
}

/// Initialize Prometheus metrics exporter with custom histogram buckets.
///
/// Buckets are optimized for LLM inference latency patterns (seconds, not milliseconds).
/// Buckets: [0.1, 0.25, 0.5, 1, 2.5, 5, 10, 30, 60, 120, 300] seconds for durations.
/// Token buckets: [10, 50, 100, 500, 1000, 2000, 4000, 8000, 16000, 32000, 64000, 128000].
///
/// Returns a PrometheusHandle that can be used to render metrics.
pub fn setup_metrics(
) -> Result<metrics_exporter_prometheus::PrometheusHandle, Box<dyn std::error::Error>> {
    use metrics_exporter_prometheus::{Matcher, PrometheusBuilder};

    let duration_buckets = &[
        0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0,
    ];

    let token_buckets = &[
        10.0, 50.0, 100.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0, 32000.0, 64000.0,
        128000.0,
    ];

    let handle = PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("nexus_request_duration_seconds".to_string()),
            duration_buckets,
        )?
        .set_buckets_for_metric(
            Matcher::Full("nexus_backend_latency_seconds".to_string()),
            duration_buckets,
        )?
        .set_buckets_for_metric(
            Matcher::Full("nexus_tokens_total".to_string()),
            token_buckets,
        )?
        .install_recorder()?;

    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, Once};

    static INIT: Once = Once::new();
    static TEST_HANDLE: Mutex<Option<metrics_exporter_prometheus::PrometheusHandle>> =
        Mutex::new(None);

    fn get_test_handle() -> metrics_exporter_prometheus::PrometheusHandle {
        INIT.call_once(|| {
            // Use build_recorder which doesn't need a runtime
            let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();

            // Get the handle from the recorder
            let handle = recorder.handle();
            *TEST_HANDLE.lock().unwrap() = Some(handle);

            // Install the recorder globally (only once for all tests)
            metrics::set_global_recorder(Box::new(recorder)).ok();
        });

        // Return a clone of the handle
        TEST_HANDLE.lock().unwrap().as_ref().unwrap().clone()
    }

    #[test]
    fn test_metrics_collector_construction() {
        let registry = Arc::new(Registry::new());
        let start_time = Instant::now();
        let handle = get_test_handle();

        let collector = MetricsCollector::new(Arc::clone(&registry), start_time, handle);

        assert!(collector.uptime_seconds() < 1); // Should be very small
    }

    #[test]
    fn test_label_sanitization_valid_names() {
        let registry = Arc::new(Registry::new());
        let handle = get_test_handle();
        let collector = MetricsCollector::new(registry, Instant::now(), handle);

        assert_eq!(collector.sanitize_label("valid_name"), "valid_name");
        assert_eq!(collector.sanitize_label("ValidName123"), "ValidName123");
        assert_eq!(collector.sanitize_label("_underscore"), "_underscore");
    }

    #[test]
    fn test_label_sanitization_special_chars() {
        let registry = Arc::new(Registry::new());
        let handle = get_test_handle();
        let collector = MetricsCollector::new(registry, Instant::now(), handle);

        assert_eq!(
            collector.sanitize_label("ollama-local:11434"),
            "ollama_local_11434"
        );
        assert_eq!(collector.sanitize_label("model/gpt-4"), "model_gpt_4");
        assert_eq!(collector.sanitize_label("backend@host"), "backend_host");
    }

    #[test]
    fn test_label_sanitization_leading_digit() {
        let registry = Arc::new(Registry::new());
        let handle = get_test_handle();
        let collector = MetricsCollector::new(registry, Instant::now(), handle);

        assert_eq!(collector.sanitize_label("123backend"), "_123backend");
        assert_eq!(collector.sanitize_label("4o"), "_4o");
    }

    #[test]
    fn test_label_sanitization_caching() {
        let registry = Arc::new(Registry::new());
        let handle = get_test_handle();
        let collector = MetricsCollector::new(registry, Instant::now(), handle);

        let first = collector.sanitize_label("test-label");
        let second = collector.sanitize_label("test-label");

        assert_eq!(first, second);
        assert_eq!(first, "test_label");
    }

    mod proptests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            /// T067: Property test — sanitized labels always match Prometheus label regex.
            #[test]
            fn prop_sanitized_label_is_valid_prometheus(input in "[\\x00-\\x7F]{1,50}") {
                let registry = Arc::new(Registry::new());
                let handle = get_test_handle();
                let collector = MetricsCollector::new(registry, Instant::now(), handle);

                let sanitized = collector.sanitize_label(&input);

                // Must not be empty
                prop_assert!(!sanitized.is_empty(), "Sanitized label should never be empty");

                // First character must be letter or underscore
                let first = sanitized.chars().next().unwrap();
                prop_assert!(
                    first.is_ascii_alphabetic() || first == '_',
                    "First char '{}' must be letter or underscore",
                    first
                );

                // All characters must be alphanumeric or underscore
                for c in sanitized.chars() {
                    prop_assert!(
                        c.is_alphanumeric() || c == '_',
                        "Character '{}' is invalid in Prometheus label",
                        c
                    );
                }
            }

            /// Property: sanitize_label is idempotent.
            #[test]
            fn prop_sanitize_is_idempotent(input in "[a-zA-Z0-9_:\\-\\./@]{1,30}") {
                let registry = Arc::new(Registry::new());
                let handle = get_test_handle();
                let collector = MetricsCollector::new(registry, Instant::now(), handle);

                let once = collector.sanitize_label(&input);
                let twice = collector.sanitize_label(&once);
                prop_assert_eq!(once, twice, "Sanitization should be idempotent");
            }
        }
    }
}
