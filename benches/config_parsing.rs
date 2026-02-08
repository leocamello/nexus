//! Benchmark for config parsing performance (NFR-001: < 10ms)

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::path::Path;

fn bench_config_load_from_file(c: &mut Criterion) {
    let config_path = Path::new("nexus.example.toml");

    c.bench_function("config_parse_from_file", |b| {
        b.iter(|| {
            let config = nexus::config::NexusConfig::load(Some(black_box(config_path)));
            black_box(config)
        });
    });
}

fn bench_config_load_defaults(c: &mut Criterion) {
    c.bench_function("config_parse_defaults_only", |b| {
        b.iter(|| {
            let config = nexus::config::NexusConfig::load(None);
            black_box(config)
        });
    });
}

fn bench_config_toml_parsing(c: &mut Criterion) {
    // Complex config with all sections
    let toml_content = r#"
[server]
host = "0.0.0.0"
port = 8000

[health_check]
enabled = true
interval_seconds = 30
timeout_seconds = 5
failure_threshold = 3
recovery_threshold = 2

[discovery]
enabled = true
grace_period_seconds = 60
service_types = ["_ollama._tcp.local", "_llm._tcp.local"]

[routing]
strategy = "smart"
max_retries = 2

[routing.weights]
priority = 50
load = 30
latency = 20

[routing.aliases]
"gpt-4" = "llama3:70b"
"gpt-3.5-turbo" = "mistral:7b"
"claude-3-opus" = "qwen2:72b"

[routing.fallbacks]
"llama3:70b" = ["qwen2:72b", "mixtral:8x7b", "llama3:8b"]

[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
priority = 1

[[backends]]
name = "gpu-server"
url = "http://192.168.1.100:8000"
type = "vllm"
priority = 2
"#;

    c.bench_function("config_parse_complex_toml", |b| {
        b.iter(|| {
            let config: nexus::config::NexusConfig =
                toml::from_str(black_box(toml_content)).unwrap();
            black_box(config)
        });
    });
}

criterion_group!(
    benches,
    bench_config_load_from_file,
    bench_config_load_defaults,
    bench_config_toml_parsing
);
criterion_main!(benches);
