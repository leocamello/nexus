# Contributing to Nexus

Thank you for your interest in contributing to Nexus! This document covers the
development workflow, coding standards, and project conventions.

## Getting Started

### Prerequisites

- **Rust 1.87+** (stable)
- **Git** with conventional commit knowledge
- **cargo-clippy** and **rustfmt** (included with Rust)

### Build & Test

```bash
# Build
cargo build

# Run all tests (unit + integration)
cargo test

# Run a specific test
cargo test test_name

# Run module tests
cargo test routing::

# Lint (must pass with zero warnings)
cargo clippy --all-targets -- -D warnings

# Format check
cargo fmt --all -- --check

# Run with debug logging
RUST_LOG=debug cargo run -- serve
```

## Project Structure

```
src/
├── agent/        # NII: InferenceAgent trait and backend implementations
├── api/          # Axum HTTP server (completions, models, health, stats)
├── cli/          # Clap CLI (serve, backends, models, health, config)
├── config/       # TOML config loading with env overrides (NEXUS_*)
├── dashboard/    # Embedded web dashboard (rust-embed, WebSocket)
├── discovery/    # mDNS auto-discovery via mdns-sd
├── health/       # Background health checker
├── logging/      # Structured logging middleware
├── metrics/      # Prometheus metrics and JSON stats API
├── registry/     # In-memory backend/model registry (DashMap)
├── routing/      # Request routing (strategies, scoring, reconcilers)
├── lib.rs        # Module declarations
└── main.rs       # Entry point
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for detailed component descriptions
and [docs/RFC-001.md](docs/RFC-001.md) for the platform architecture RFC.

## Development Workflow

We follow a strict **Feature Development Lifecycle**:

1. **Spec Phase**: Write spec.md, plan.md, tasks.md → validate → create issues
2. **Implementation Phase**: Feature branch → TDD (tests first) → check off criteria
3. **Verification Phase**: Run analyze → complete checklists → write walkthrough
4. **Merge Phase**: Push → PR → merge (closes issues)

### Branching

- `main` — stable, all tests pass
- `feature/<name>` — feature development branches
- Always use Pull Requests for merging to main

### Commit Messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: implement Feature Name (FXX)
fix: correct routing score calculation
docs: update ARCHITECTURE.md for v0.3
test: add privacy reconciler edge cases
refactor: extract tokenizer into separate module
```

Reference issues with separate `Closes #X` lines:

```
feat: implement Privacy Zones (F13)

Structural enforcement of privacy boundaries.

Closes #42
Closes #43
```

## Coding Standards

### General

- **Line width**: 100 characters (see `rustfmt.toml`)
- **Logging**: `tracing` macros only — no `println!`
- **Errors**: `thiserror` for internal errors; HTTP responses match OpenAI format
- **Concurrency**: `Arc<T>` for shared state, `DashMap` for concurrent maps, atomics for counters
- **No panics** on backend errors — always return proper HTTP responses
- **Comment the "why"**, not the "what"; no commented-out code

### Performance Budgets

| Metric | Budget |
|--------|--------|
| Routing decision | < 1ms |
| Total request overhead | < 5ms |
| Memory baseline | < 50MB |
| Per-backend memory | < 10KB |

### Testing

Every logic file must contain a `#[cfg(test)] mod tests` block. We follow TDD:

1. Write tests → 2. Confirm they fail → 3. Implement → 4. Refactor

| Scope | Location | Focus |
|-------|----------|-------|
| Unit tests | `mod tests` in each file | Data integrity, scoring logic |
| Integration tests | `tests/` directory | End-to-end API flows with mock backends |
| Property tests | `proptest` in scoring modules | Edge cases |
| Doc tests | Doc comments on public types | Executable examples |

### Configuration

- API keys **must** use `api_key_env` (environment variable name), never store actual keys
- Configuration example in `nexus.example.toml` must stay in sync with config structs
- Precedence: CLI args > env vars (`NEXUS_*`) > config file > defaults

## Pull Request Guidelines

1. All CI checks must pass (clippy, fmt, tests, coverage)
2. Link related issues in the PR body
3. Include a brief summary of changes
4. For features: reference the spec and verification checklist

## Resources

- [Architecture](docs/ARCHITECTURE.md) — System design and module structure
- [Features](docs/FEATURES.md) — Feature specifications (F01–F23)
- [RFC-001](docs/RFC-001.md) — Platform architecture RFC
- [Manual Testing Guide](docs/MANUAL_TESTING_GUIDE.md) — How to test manually
