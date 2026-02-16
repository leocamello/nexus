# nexus Development Guidelines

Auto-generated from all feature plans. Last updated: 2026-02-14

## Active Technologies
- Rust 1.87+ (stable) + axum 0.7, tokio 1.x (full features), tracing 0.1, tracing-subscriber 0.3 (with json feature) (011-structured-logging)
- N/A (in-memory only, stateless by design) (011-structured-logging)
- Rust 1.87 (stable toolchain) + Tokio (async runtime), Axum (HTTP framework), DashMap (concurrent state), thiserror (error handling) (014-control-plane-reconciler)
- In-memory only (no persistence required) (014-control-plane-reconciler)

- Rust 1.87+ (stable toolchain) (010-web-dashboard)

## Project Structure

```text
backend/
frontend/
tests/
```

## Commands

cargo test [ONLY COMMANDS FOR ACTIVE TECHNOLOGIES][ONLY COMMANDS FOR ACTIVE TECHNOLOGIES] cargo clippy

## Code Style

Rust 1.87+ (stable toolchain): Follow standard conventions

## Recent Changes
- 014-control-plane-reconciler: Added Rust 1.87 (stable toolchain) + Tokio (async runtime), Axum (HTTP framework), DashMap (concurrent state), thiserror (error handling)
- 011-structured-logging: Added Rust 1.87+ (stable) + axum 0.7, tokio 1.x (full features), tracing 0.1, tracing-subscriber 0.3 (with json feature)

- 010-web-dashboard: Added Rust 1.87+ (stable toolchain)

<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
