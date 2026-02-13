//! Benchmarks for Request Metrics (F09).
//!
//! Validates acceptance criterion: metric recording overhead < 0.1ms.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use nexus::api::AppState;
use nexus::config::NexusConfig;
use nexus::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model, Registry};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::Arc;

fn create_test_backend(id: &str, model_id: &str) -> Backend {
    Backend {
        id: id.to_string(),
        name: id.to_string(),
        url: format!("http://{}", id),
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
        priority: 1,
        pending_requests: AtomicU32::new(0),
        total_requests: AtomicU64::new(100),
        avg_latency_ms: AtomicU32::new(50),
        discovery_source: DiscoverySource::Static,
        metadata: HashMap::new(),
    }
}

fn create_populated_state() -> Arc<AppState> {
    let registry = Arc::new(Registry::new());
    for i in 0..5 {
        registry
            .add_backend(create_test_backend(
                &format!("backend-{}", i),
                &format!("model-{}", i),
            ))
            .unwrap();
    }
    let config = Arc::new(NexusConfig::default());
    Arc::new(AppState::new(registry, config))
}

/// T063: Benchmark for metric recording overhead.
///
/// Measures the cost of recording a counter + histogram (the hot path during
/// every request). Target: < 0.1ms (100µs).
fn bench_metric_recording_overhead(c: &mut Criterion) {
    let state = create_populated_state();

    c.bench_function("metric_recording_overhead", |b| {
        b.iter(|| {
            let model = state.metrics_collector.sanitize_label("llama3:8b");
            let backend = state.metrics_collector.sanitize_label("backend-0");

            // Simulate the metrics recorded on each successful request
            metrics::counter!(
                "nexus_requests_total",
                "model" => model.clone(),
                "backend" => backend.clone(),
                "status" => "200"
            )
            .increment(1);

            metrics::histogram!(
                "nexus_request_duration_seconds",
                "model" => model.clone(),
                "backend" => backend.clone()
            )
            .record(black_box(1.5));

            metrics::histogram!(
                "nexus_tokens_total",
                "model" => model,
                "backend" => backend,
                "type" => "prompt"
            )
            .record(black_box(256.0));
        });
    });
}

/// T064: Benchmark for /metrics endpoint latency.
///
/// Measures the cost of rendering Prometheus text output with fleet gauges.
fn bench_metrics_endpoint(c: &mut Criterion) {
    let state = create_populated_state();

    c.bench_function("metrics_endpoint_render", |b| {
        b.iter(|| {
            state.metrics_collector.update_fleet_gauges();
            black_box(state.metrics_collector.render_metrics());
        });
    });
}

/// T065: Benchmark for /v1/stats endpoint latency.
///
/// Measures the cost of computing JSON stats from registry.
fn bench_stats_endpoint(c: &mut Criterion) {
    let state = create_populated_state();

    c.bench_function("stats_endpoint_compute", |b| {
        b.iter(|| {
            state.metrics_collector.update_fleet_gauges();
            let uptime = state.metrics_collector.uptime_seconds();
            let _backends = state.metrics_collector.registry().get_all_backends();
            black_box(uptime);
        });
    });
}

/// Benchmark label sanitization (cached vs uncached).
fn bench_label_sanitization(c: &mut Criterion) {
    let state = create_populated_state();

    // Warm the cache
    state.metrics_collector.sanitize_label("ollama-local:11434");

    c.bench_function("label_sanitize_cached", |b| {
        b.iter(|| {
            black_box(state.metrics_collector.sanitize_label("ollama-local:11434"));
        });
    });

    c.bench_function("label_sanitize_uncached", |b| {
        let mut i = 0u64;
        b.iter(|| {
            let label = format!("dynamic-label-{}", i);
            i += 1;
            black_box(state.metrics_collector.sanitize_label(&label));
        });
    });
}

criterion_group!(
    benches,
    bench_metric_recording_overhead,
    bench_metrics_endpoint,
    bench_stats_endpoint,
    bench_label_sanitization,
);
criterion_main!(benches);
