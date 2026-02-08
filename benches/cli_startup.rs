//! Benchmark for CLI startup performance (NFR-002: < 100ms)

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::process::Command;
use std::time::Instant;

fn bench_cli_help_startup(c: &mut Criterion) {
    // Build the binary first (not part of benchmark)
    let _ = Command::new("cargo")
        .args(["build", "--release", "--quiet"])
        .status();

    c.bench_function("cli_help_startup", |b| {
        b.iter(|| {
            let start = Instant::now();
            let output = Command::new("./target/release/nexus")
                .arg("--help")
                .output()
                .expect("Failed to execute command");
            let elapsed = start.elapsed();
            assert!(output.status.success());
            black_box(elapsed)
        });
    });
}

fn bench_cli_version_startup(c: &mut Criterion) {
    c.bench_function("cli_version_startup", |b| {
        b.iter(|| {
            let start = Instant::now();
            let output = Command::new("./target/release/nexus")
                .arg("--version")
                .output()
                .expect("Failed to execute command");
            let elapsed = start.elapsed();
            assert!(output.status.success());
            black_box(elapsed)
        });
    });
}

fn bench_cli_backends_help(c: &mut Criterion) {
    c.bench_function("cli_backends_help_startup", |b| {
        b.iter(|| {
            let start = Instant::now();
            let output = Command::new("./target/release/nexus")
                .args(["backends", "--help"])
                .output()
                .expect("Failed to execute command");
            let elapsed = start.elapsed();
            assert!(output.status.success());
            black_box(elapsed)
        });
    });
}

fn bench_cli_models_help(c: &mut Criterion) {
    c.bench_function("cli_models_help_startup", |b| {
        b.iter(|| {
            let start = Instant::now();
            let output = Command::new("./target/release/nexus")
                .args(["models", "--help"])
                .output()
                .expect("Failed to execute command");
            let elapsed = start.elapsed();
            assert!(output.status.success());
            black_box(elapsed)
        });
    });
}

criterion_group!(
    benches,
    bench_cli_help_startup,
    bench_cli_version_startup,
    bench_cli_backends_help,
    bench_cli_models_help
);
criterion_main!(benches);
