# Copilot Instructions for Nexus

You are an expert Rust developer building **Nexus**, a distributed LLM orchestrator that unifies heterogeneous inference backends (Ollama, vLLM, llama.cpp, exo) behind an OpenAI-compatible API gateway. Your goal is to maintain a high-performance, single-binary service.

> **Note**: The project constitution (`.specify/memory/constitution.md`) is the authoritative source for principles, constraints, and standards. This file provides implementation guidance.

## Build, Test, and Lint

```bash
# Build
cargo build

# Run tests
cargo test

# Run a single test
cargo test <test_name>

# Run tests in a specific module
cargo test <module>::

# Lint (all-targets catches test and example code too)
cargo clippy --all-targets -- -D warnings

# Format check
cargo fmt --all -- --check

# Run with debug logging
RUST_LOG=debug cargo run -- serve
```

## Tech Stack & Patterns

- **Runtime**: Use `tokio` for the async runtime (full features).
- **Web Framework**: Use `axum` for all HTTP layers.
- **Client**: Use `reqwest` for backend communication with connection pooling.
- **State Management**: Use `Arc<T>` for shared state and `DashMap` for concurrent maps.
- **Error Handling**: Use `thiserror` for internal errors. HTTP responses must match the OpenAI Error format.
- **Logging**: Use the `tracing` crate for structured logging. Avoid `println!`.
- **Configuration**: TOML format (see `nexus.example.toml`).

## Architecture

### Core Components

1. **API Layer** (`src/api/`) - Axum-based HTTP server exposing:
   - `POST /v1/chat/completions` - Chat completion (streaming supported)
   - `GET /v1/models` - List available models
   - `GET /health` - System health

2. **Router Layer** (`src/routing/`) - Intelligent request routing:
   - Matches models to capable backends
   - Scores backends by priority, load, and latency
   - Supports strategies: `smart`, `round_robin`, `priority_only`, `random`
   - **Always consider context length, vision support, and tool-use capabilities before load or latency**

3. **Backend Registry** (`src/registry/`) - In-memory storage (source of truth):
   - Tracks backends and their health status
   - Maintains model-to-backend index
   - Uses `DashMap` for concurrent access
   - All backend status and model metadata live here

4. **Health Checker** (`src/health/`) - Background service:
   - Polls backends every 30s
   - Updates model capabilities on each check
   - Uses backend-specific endpoints (Ollama: `/api/tags`, others: `/v1/models`)

5. **mDNS Discovery** (`src/discovery/`) - Auto-discovery:
   - Listens for `_ollama._tcp.local` and `_llm._tcp.local`
   - Automatically registers/removes backends

### Request Flow

1. Request arrives at API layer
2. Router selects best healthy backend for the requested model
3. Request is proxied to backend
4. On failure: retry with next backend (configurable max retries)
5. Stream response back to client

### Key Types

- `BackendType`: Ollama, VLLM, LlamaCpp, Exo, OpenAI, Generic
- `BackendStatus`: Healthy, Unhealthy, Unknown
- `DiscoverySource`: Static (config), MDNS (auto), Manual (CLI)
- `Model`: Must include context length and feature flags (vision, tools)

### Key Patterns

**View Models for Output**: Separate internal types from display/serialization types:
```rust
// Internal type (complex, atomics, business logic)
pub struct Backend { /* ... */ }

// Display type (simple, serializable, no atomics)
pub struct BackendView { /* ... */ }

impl From<&Backend> for BackendView { /* ... */ }
```

**Graceful Shutdown**: Use `CancellationToken` from `tokio_util` for clean shutdown:
```rust
let cancel_token = CancellationToken::new();
// Pass to background tasks
let handle = health_checker.start(cancel_token.clone());
// Wait for shutdown signal
shutdown_signal(cancel_token).await;
// Cleanup
handle.await?;
```

## Architectural Rules

- **Registry Source of Truth**: All backend status and model metadata live in `src/registry/mod.rs`.
- **Zero-Config Philosophy**: Prioritize mDNS discovery (`mdns-sd`) and auto-detection over manual user input.
- **OpenAI Compatibility**: Strict adherence to the OpenAI Chat Completions API (streaming and non-streaming) is mandatory.
- **Intelligent Routing**: When implementing the Router, always consider context length, vision support, and tool-use capabilities before load or latency.

## Git Workflow

**IMPORTANT**: Always use feature branches and Pull Requests for feature implementations.

### Feature Development Lifecycle

```
┌─────────────────────────────────────────────────────────────────────┐
│  1. SPEC PHASE                                                      │
│     - Write spec.md, plan.md, tasks.md                              │
│     - Validate against requirements-quality.md checklist            │
│     - Create GitHub issues from tasks                               │
├─────────────────────────────────────────────────────────────────────┤
│  2. IMPLEMENTATION PHASE                                            │
│     - Create feature branch                                         │
│     - Copy implementation-verification.md to feature folder         │
│     - Implement with TDD (tests first)                              │
│     - Check off acceptance criteria as you go                       │
├─────────────────────────────────────────────────────────────────────┤
│  3. VERIFICATION PHASE                                              │
│     - Run speckit.analyze                                           │
│     - Verify all checklists complete                                │
│     - Create walkthrough.md                                         │
├─────────────────────────────────────────────────────────────────────┤
│  4. MERGE PHASE                                                     │
│     - Push feature branch                                           │
│     - Create PR with verification summary                           │
│     - Merge (closes issues automatically)                           │
└─────────────────────────────────────────────────────────────────────┘
```

### Quality Checklists

Use the two-checklist system for quality assurance:

| Phase | Checklist | Command |
|-------|-----------|---------|
| Before coding | Validate spec quality | Review `.specify/checklists/requirements-quality.md` |
| After coding | Verify implementation | Check `.specify/templates/implementation-verification.md` |

```bash
# Copy verification template to your feature
cp .specify/templates/implementation-verification.md specs/XXX-feature/verification.md

# Verify all items checked before PR
grep -c "\- \[ \]" specs/XXX-feature/verification.md  # Should be 0
grep -c "\- \[ \]" specs/XXX-feature/tasks.md         # Should be 0
```

**Quick Reference**: See `.specify/QUICK-REFERENCE.md` for top critical items.

### Feature Branch Process

```bash
# 1. Create feature branch BEFORE implementing
git checkout -b feature/f05-intelligent-router

# 2. Implement the feature (commits go to feature branch)
git add .
git commit -m "feat: implement Intelligent Router (F05)"

# 3. Push feature branch
git push -u origin feature/f05-intelligent-router

# 4. Create Pull Request
gh pr create \
  --title "feat: Intelligent Router (F05)" \
  --body "..." \
  --label "enhancement"

# 5. Merge PR (this closes linked issues automatically)
gh pr merge --squash
```

### Commit Message Format

Use conventional commits with issue references:
```
feat: implement Feature Name (FXX)

Description of changes...

Closes #123
Closes #124
Closes #125
```

**Note**: Use separate `Closes #X` lines for each issue. Comma-separated syntax (`Closes #1, #2, #3`) may not close all issues reliably.

### Why This Matters

- PRs provide a review checkpoint before merging to main
- PR history documents feature implementations (see [closed PRs](https://github.com/leocamello/nexus/pulls?q=is%3Apr+is%3Aclosed))
- Issues are automatically closed when PR is merged
- Easier to revert if needed (single PR vs hunting commits)

### Task Completion Checklist

When completing a task, **always update the tasks.md file**:

1. **After implementing each task**: Check off acceptance criteria as you verify them
2. **Before committing**: Ensure all `- [ ]` items for the task are now `- [x]`
3. **Use speckit.analyze**: Run analysis before PR to catch gaps

```bash
# Verify no unchecked items remain in ALL feature docs
grep -c "\- \[ \]" specs/XXX-feature/tasks.md         # Should be 0
grep -c "\- \[ \]" specs/XXX-feature/verification.md  # Should be 0
```

**Why**: Acceptance criteria checkboxes document what was actually delivered. Unchecked boxes create confusion about implementation status.

## Coding Standards

- Line width: 100 characters (see `rustfmt.toml`)
- Routing decision: < 1ms; total request overhead: < 5ms (see constitution for full latency budget)
- Prefer memory-safe Rust patterns; minimize `unsafe` blocks
- Ensure the `Model` struct includes context length and feature flags (vision, tools)
- No `println!` - use `tracing` macros for all output
- No panics on backend errors - always return proper HTTP response

## Testing Strategy

### TDD Workflow

When asked to implement a feature, **always suggest the test cases first**.

### Unit Tests

- Place in the same file or a `tests.rs` module for registry and routing logic
- Every logic file (e.g., `src/routing/mod.rs`) must contain a `mod tests` block at the bottom guarded by `#[cfg(test)]`
- Focus unit tests on the Registry's data integrity and the Router's scoring logic

### Integration Tests

- Use the `tests/` directory for end-to-end API flows
- Use mock HTTP backends to simulate OpenAI-compatible responses

### Property Testing

- For complex logic like the `score()` function in `src/routing/mod.rs`, prefer property-based testing (via `proptest`) over static values

### Documentation Tests

- Write executable examples in doc comments for public traits and structs to ensure documentation stays accurate
