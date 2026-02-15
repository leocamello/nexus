# nexus Development Guidelines

Auto-generated from all feature plans. Last updated: 2026-02-14

## Active Technologies
- Rust 1.87+ (stable) + axum 0.7, tokio 1.x (full features), tracing 0.1, tracing-subscriber 0.3 (with json feature) (011-structured-logging)
- N/A (in-memory only, stateless by design) (011-structured-logging)
- Rust 1.75 (stable toolchain) (013-cloud-backend)
- N/A (all state in-memory via DashMap) (013-cloud-backend)

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
- 014-control-plane: Added [if applicable, e.g., PostgreSQL, CoreData, files or N/A]
- 013-cloud-backend: Added Rust 1.75 (stable toolchain)
- 011-structured-logging: Added Rust 1.87+ (stable) + axum 0.7, tokio 1.x (full features), tracing 0.1, tracing-subscriber 0.3 (with json feature)


<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
