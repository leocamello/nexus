//! Benchmarks for routing latency with varying backend counts.
//!
//! Validates constitution requirement: routing decision < 1ms, total overhead < 5ms.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use nexus::registry::{Backend, BackendStatus, BackendType, DiscoverySource, Model, Registry};
use nexus::routing::{RequestRequirements, Router, RoutingStrategy, ScoringWeights};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::Arc;

fn create_backend(id: usize, model_count: usize) -> Backend {
    let models: Vec<Model> = (0..model_count)
        .map(|m| Model {
            id: format!("model-{}", m),
            name: format!("model-{}", m),
            context_length: 4096 + (m * 1024) as u32,
            supports_vision: m % 3 == 0,
            supports_tools: m % 2 == 0,
            supports_json_mode: m % 4 == 0,
            max_output_tokens: Some(2048),
        })
        .collect();

    Backend {
        id: format!("backend-{}", id),
        name: format!("backend-{}", id),
        url: format!("http://backend-{}:11434", id),
        backend_type: BackendType::Ollama,
        status: BackendStatus::Healthy,
        last_health_check: chrono::Utc::now(),
        last_error: None,
        models,
        priority: (id % 5 + 1) as i32,
        pending_requests: AtomicU32::new((id % 10) as u32),
        total_requests: AtomicU64::new(100 + id as u64),
        avg_latency_ms: AtomicU32::new(20 + (id * 5) as u32),
        discovery_source: DiscoverySource::Static,
        metadata: HashMap::new(),
    }
}

fn create_router(backend_count: usize, models_per_backend: usize) -> Router {
    let registry = Arc::new(Registry::new());
    for i in 0..backend_count {
        registry
            .add_backend(create_backend(i, models_per_backend))
            .unwrap();
    }

    Router::new(
        registry,
        RoutingStrategy::Smart,
        ScoringWeights {
            priority: 50,
            load: 30,
            latency: 20,
        },
    )
}

fn create_round_robin_router(backend_count: usize) -> Router {
    let registry = Arc::new(Registry::new());
    for i in 0..backend_count {
        registry.add_backend(create_backend(i, 3)).unwrap();
    }

    Router::new(
        registry,
        RoutingStrategy::RoundRobin,
        ScoringWeights::default(),
    )
}

/// Benchmark smart routing with varying backend counts.
/// All backends serve model-0, so the router must score all candidates.
fn bench_smart_routing_by_backend_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("smart_routing");

    for count in [1, 5, 10, 25, 50] {
        let router = create_router(count, 3);
        let requirements = RequestRequirements {
            model: "model-0".to_string(),
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            estimated_tokens: 100,
            prefers_streaming: false,
        };

        group.bench_with_input(BenchmarkId::new("backends", count), &count, |b, _| {
            b.iter(|| {
                black_box(router.select_backend(&requirements, None).unwrap());
            });
        });
    }

    group.finish();
}

/// Benchmark round-robin routing (should be O(1) regardless of backend count).
fn bench_round_robin_routing(c: &mut Criterion) {
    let mut group = c.benchmark_group("round_robin_routing");

    for count in [5, 25, 50] {
        let router = create_round_robin_router(count);
        let requirements = RequestRequirements {
            model: "model-0".to_string(),
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
            estimated_tokens: 100,
            prefers_streaming: false,
        };

        group.bench_with_input(BenchmarkId::new("backends", count), &count, |b, _| {
            b.iter(|| {
                black_box(router.select_backend(&requirements, None).unwrap());
            });
        });
    }

    group.finish();
}

/// Benchmark routing with capability filtering (vision requirement).
/// Only ~1/3 of backends support vision, so the router must filter first.
fn bench_capability_filtered_routing(c: &mut Criterion) {
    let router = create_router(25, 5);
    let requirements = RequestRequirements {
        model: "model-0".to_string(),
        needs_vision: true,
        needs_tools: false,
        needs_json_mode: false,
        estimated_tokens: 100,
        prefers_streaming: false,
    };

    c.bench_function("capability_filtered_25_backends", |b| {
        b.iter(|| {
            black_box(router.select_backend(&requirements, None).unwrap());
        });
    });
}

/// Benchmark routing with fallback chain.
/// Requests a non-existent model to trigger fallback resolution.
fn bench_routing_with_fallback(c: &mut Criterion) {
    let registry = Arc::new(Registry::new());
    for i in 0..10 {
        registry.add_backend(create_backend(i, 3)).unwrap();
    }

    let mut fallbacks = HashMap::new();
    fallbacks.insert(
        "premium-model".to_string(),
        vec!["model-0".to_string(), "model-1".to_string()],
    );

    let router = Router::with_aliases_and_fallbacks(
        registry,
        RoutingStrategy::Smart,
        ScoringWeights {
            priority: 50,
            load: 30,
            latency: 20,
        },
        HashMap::new(),
        fallbacks,
    );

    let requirements = RequestRequirements {
        model: "premium-model".to_string(),
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
        estimated_tokens: 100,
        prefers_streaming: false,
    };

    c.bench_function("routing_with_fallback_10_backends", |b| {
        b.iter(|| {
            black_box(router.select_backend(&requirements, None).unwrap());
        });
    });
}

/// Benchmark alias resolution + routing.
fn bench_routing_with_alias(c: &mut Criterion) {
    let registry = Arc::new(Registry::new());
    for i in 0..10 {
        registry.add_backend(create_backend(i, 3)).unwrap();
    }

    let mut aliases = HashMap::new();
    aliases.insert("gpt4".to_string(), "model-0".to_string());

    let router = Router::with_aliases_and_fallbacks(
        registry,
        RoutingStrategy::Smart,
        ScoringWeights {
            priority: 50,
            load: 30,
            latency: 20,
        },
        aliases,
        HashMap::new(),
    );

    let requirements = RequestRequirements {
        model: "gpt4".to_string(),
        needs_vision: false,
        needs_tools: false,
        needs_json_mode: false,
        estimated_tokens: 100,
        prefers_streaming: false,
    };

    c.bench_function("routing_with_alias_10_backends", |b| {
        b.iter(|| {
            black_box(router.select_backend(&requirements, None).unwrap());
        });
    });
}

// === Reconciler Pipeline Benchmarks ===

use nexus::agent::tokenizer::TokenizerRegistry;
use nexus::config::{BudgetConfig, PolicyMatcher};
use nexus::routing::reconciler::budget::{BudgetMetrics, BudgetReconciler};
use nexus::routing::reconciler::intent::RoutingIntent;
use nexus::routing::reconciler::privacy::PrivacyReconciler;
use nexus::routing::reconciler::quality::QualityReconciler;
use nexus::routing::reconciler::request_analyzer::RequestAnalyzer;
use nexus::routing::reconciler::scheduler::SchedulerReconciler;
use nexus::routing::reconciler::tier::TierReconciler;
use nexus::routing::reconciler::ReconcilerPipeline;

fn create_pipeline_registry(backend_count: usize, models_per_backend: usize) -> Arc<Registry> {
    let registry = Arc::new(Registry::new());
    for i in 0..backend_count {
        registry
            .add_backend(create_backend(i, models_per_backend))
            .unwrap();
    }
    registry
}

/// Benchmark full reconciler pipeline execution (FR-036: <1ms p95).
fn bench_full_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("pipeline");

    for count in [5, 10, 25, 50] {
        let registry = create_pipeline_registry(count, 3);
        let budget_state = Arc::new(dashmap::DashMap::new());
        budget_state.insert("global".to_string(), BudgetMetrics::new());
        let tokenizer_registry = Arc::new(TokenizerRegistry::new().expect("tokenizer registry"));

        group.bench_with_input(BenchmarkId::new("backends", count), &count, |b, _| {
            b.iter(|| {
                let mut pipeline = ReconcilerPipeline::new(vec![
                    Box::new(RequestAnalyzer::new(HashMap::new(), Arc::clone(&registry))),
                    Box::new(PrivacyReconciler::new(
                        Arc::clone(&registry),
                        PolicyMatcher::default(),
                    )),
                    Box::new(BudgetReconciler::new(
                        Arc::clone(&registry),
                        BudgetConfig::default(),
                        Arc::clone(&tokenizer_registry),
                        Arc::clone(&budget_state),
                    )),
                    Box::new(TierReconciler::new(
                        Arc::clone(&registry),
                        PolicyMatcher::default(),
                    )),
                    Box::new({
                        let qcfg = nexus::config::QualityConfig::default();
                        let qstore = std::sync::Arc::new(
                            nexus::agent::quality::QualityMetricsStore::new(qcfg.clone()),
                        );
                        QualityReconciler::new(qstore, qcfg)
                    }),
                    Box::new({
                        let qcfg = nexus::config::QualityConfig::default();
                        let qstore = std::sync::Arc::new(
                            nexus::agent::quality::QualityMetricsStore::new(qcfg.clone()),
                        );
                        SchedulerReconciler::new(
                            Arc::clone(&registry),
                            RoutingStrategy::Smart,
                            ScoringWeights {
                                priority: 50,
                                load: 30,
                                latency: 20,
                            },
                            Arc::new(std::sync::atomic::AtomicU64::new(0)),
                            qstore,
                            qcfg,
                        )
                    }),
                ]);

                let mut intent = RoutingIntent::new(
                    "bench-req".to_string(),
                    "model-0".to_string(),
                    "model-0".to_string(),
                    RequestRequirements {
                        model: "model-0".to_string(),
                        needs_vision: false,
                        needs_tools: false,
                        needs_json_mode: false,
                        estimated_tokens: 100,
                        prefers_streaming: false,
                    },
                    vec![],
                );

                black_box(pipeline.execute(&mut intent).unwrap());
            });
        });
    }

    group.finish();
}

/// Benchmark RequestAnalyzer alone (FR-009: <0.5ms).
fn bench_request_analyzer(c: &mut Criterion) {
    let mut group = c.benchmark_group("request_analyzer");

    for count in [5, 10, 25, 50] {
        let registry = create_pipeline_registry(count, 3);

        let mut aliases = HashMap::new();
        aliases.insert("gpt4".to_string(), "model-0".to_string());

        let analyzer = RequestAnalyzer::new(aliases.clone(), Arc::clone(&registry));

        group.bench_with_input(BenchmarkId::new("backends", count), &count, |b, _| {
            b.iter(|| {
                let mut intent = RoutingIntent::new(
                    "bench-req".to_string(),
                    "gpt4".to_string(),
                    "gpt4".to_string(),
                    RequestRequirements {
                        model: "gpt4".to_string(),
                        needs_vision: false,
                        needs_tools: false,
                        needs_json_mode: false,
                        estimated_tokens: 100,
                        prefers_streaming: false,
                    },
                    vec![],
                );

                nexus::routing::reconciler::Reconciler::reconcile(&analyzer, &mut intent).unwrap();
                black_box(&intent);
            });
        });
    }

    group.finish();
}

/// Benchmark tokenizer registry operations (SC-007: <200ms P95 overhead).
fn bench_tokenizer_counting(c: &mut Criterion) {
    let mut group = c.benchmark_group("tokenizer");
    let registry = TokenizerRegistry::new().expect("tokenizer registry");

    let short_text = "Hello, world!";
    let medium_text = "The quick brown fox jumps over the lazy dog. ".repeat(10);
    let long_text = "The quick brown fox jumps over the lazy dog. ".repeat(100);

    // Exact tokenizer (OpenAI models)
    for (label, text) in [
        ("exact_short", short_text),
        ("exact_medium", medium_text.as_str()),
        ("exact_long", long_text.as_str()),
    ] {
        group.bench_function(label, |b| {
            b.iter(|| {
                black_box(registry.count_tokens("gpt-4", text).unwrap());
            });
        });
    }

    // Approximation tokenizer (Anthropic models)
    group.bench_function("approx_medium", |b| {
        b.iter(|| {
            black_box(
                registry
                    .count_tokens("claude-3-sonnet-20240229", &medium_text)
                    .unwrap(),
            );
        });
    });

    // Heuristic tokenizer (unknown/local models)
    group.bench_function("heuristic_medium", |b| {
        b.iter(|| {
            black_box(registry.count_tokens("llama3:8b", &medium_text).unwrap());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_smart_routing_by_backend_count,
    bench_round_robin_routing,
    bench_capability_filtered_routing,
    bench_routing_with_fallback,
    bench_routing_with_alias,
    bench_full_pipeline,
    bench_request_analyzer,
    bench_tokenizer_counting,
);
criterion_main!(benches);
