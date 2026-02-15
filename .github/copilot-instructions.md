# Copilot Instructions for Nexus

You are an expert Rust developer building **Nexus**, a distributed LLM orchestrator — the control plane for heterogeneous LLM inference. It unifies local and cloud inference backends (Ollama, vLLM, llama.cpp, exo, LM Studio, OpenAI) behind a single OpenAI-compatible API gateway with mDNS auto-discovery, intelligent capability-aware routing, and privacy-zone enforcement.

> **Note**: The project constitution (`.specify/memory/constitution.md`) is the authoritative source for principles, constraints, and standards. This file provides implementation guidance.

## Product Vision

**"Local first, cloud when needed."** Nexus solves the capacity and availability problem for developers with heterogeneous LLM backends — not the speed problem. It pools multiple machines and providers into one endpoint with zero configuration.

### Core Value Proposition
- **One URL, zero config**: mDNS discovers backends automatically
- **Capability-aware routing**: Routes by model features (vision, tools, context length), not just load
- **Backend-agnostic**: Ollama, vLLM, llama.cpp, exo, LM Studio, OpenAI — all normalized to one API
- **Honest failures**: 503 with actionable context over silent quality downgrades

### Product Roadmap

| Version | Theme | Status |
|---------|-------|--------|
| v0.1 | Foundation — Registry, Health, Router, mDNS, CLI, Aliases, Fallbacks | ✅ Released |
| v0.2 | Observability — Prometheus metrics, Web Dashboard, Structured logging | ✅ Released |
| v0.3 | Cloud Hybrid — Cloud backends, Privacy zones, Budget management | Next |
| v0.4 | Intelligence — Speculative router, Quality tracking, Embeddings, Queuing | Planned |
| v0.5 | Orchestration — Pre-warming, Model lifecycle, Multi-tenant, Rate limiting | Planned |
| v1.0 | Complete Product — Management UI, full web-based control plane | Planned |

See `docs/FEATURES.md` for detailed feature specs (F01-F23).

## Build, Test, and Lint

```bash
cargo build                                        # Build
cargo test                                         # Run all tests
cargo test <test_name>                             # Single test
cargo test <module>::                              # Module tests
cargo clippy --all-targets -- -D warnings          # Lint
cargo fmt --all -- --check                         # Format check
RUST_LOG=debug cargo run -- serve                  # Run with debug logging
```

## Tech Stack & Patterns

- **Runtime**: `tokio` (full features)
- **Web Framework**: `axum` for all HTTP layers
- **HTTP Client**: `reqwest` with connection pooling
- **State Management**: `Arc<T>` for shared state, `DashMap` for concurrent maps, `AtomicU32`/`AtomicU64` for counters
- **Error Handling**: `thiserror` for internal errors; HTTP responses match OpenAI error format exactly
- **Logging**: `tracing` crate only — no `println!`
- **Configuration**: TOML format (see `nexus.example.toml`), precedence: CLI args > env vars > config file > defaults
- **CLI**: `clap` with derive feature

## Architecture

### Module Structure

```
src/
├── api/          # Axum HTTP server (completions, models, health)
├── cli/          # Clap CLI (serve, backends, models, health, config, completions)
├── config/       # TOML config loading with env override (NEXUS_*)
├── discovery/    # mDNS auto-discovery via mdns-sd
├── health/       # Background health checker with backend-specific endpoints
├── registry/     # In-memory backend/model registry (DashMap, source of truth)
├── routing/      # Intelligent request routing (strategies, scoring, requirements)
├── lib.rs        # Module declarations
└── main.rs       # Entry point
```

### API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/v1/chat/completions` | Chat completion (streaming and non-streaming) |
| `GET` | `/v1/models` | List available models from healthy backends |
| `GET` | `/health` | System health with backend/model counts |

### Key Types

| Type | Location | Description |
|------|----------|-------------|
| `BackendType` | `registry/backend.rs` | Ollama, VLLM, LlamaCpp, Exo, OpenAI, LMStudio, Generic |
| `BackendStatus` | `registry/backend.rs` | Healthy, Unhealthy, Unknown, Draining |
| `DiscoverySource` | `registry/backend.rs` | Static (config), MDNS (auto), Manual (CLI) |
| `Backend` | `registry/backend.rs` | Thread-safe registry entry with atomic counters (pending, total, latency EMA) |
| `Model` | `registry/backend.rs` | id, name, context_length, vision, tools, json_mode, max_output_tokens |
| `Registry` | `registry/mod.rs` | DashMap-based concurrent storage with model-to-backend index |
| `RoutingStrategy` | `routing/strategies.rs` | Smart (default), RoundRobin, PriorityOnly, Random |
| `Router` | `routing/mod.rs` | Alias resolution (max 3-level chaining), fallback chains, scoring |
| `RequestRequirements` | `routing/requirements.rs` | Vision, tools, context window requirements extracted from request |
| `AppState` | `api/mod.rs` | Registry, config, HTTP client, router, startup time |
| `NexusConfig` | `config/mod.rs` | Aggregates server, discovery, health_check, routing, backends, logging |

### Key Patterns

**Atomic Metrics**: Backend load tracking uses lock-free atomics, not mutexes:
```rust
// Latency tracked via EMA: new = (sample + 4×old) / 5 (α=0.2)
backend.update_latency(elapsed_ms);
backend.increment_pending();
backend.decrement_pending();
```

**View Models for Output**: Separate internal types from serialization types:
```rust
pub struct Backend { /* atomics, DashMap, business logic */ }
pub struct BackendView { /* simple, serializable, no atomics */ }
impl From<&Backend> for BackendView { /* ... */ }
```

**Graceful Shutdown**: Use `CancellationToken` from `tokio_util`:
```rust
let cancel_token = CancellationToken::new();
let handle = health_checker.start(cancel_token.clone());
shutdown_signal(cancel_token).await;
handle.await?;
```

**Capability Routing**: Router reads request payload to match backend capabilities:
```rust
// Request with image_url → requires vision-capable backend
// Request with tools[] → requires tool-use-capable backend
// Token count estimate → requires sufficient context window
```

## Architectural Principles

1. **Registry Source of Truth**: All backend status and model metadata live in `src/registry/mod.rs`
2. **Zero-Config Philosophy**: Prioritize mDNS discovery and auto-detection over manual input
3. **OpenAI Compatibility**: Strict adherence to OpenAI API; metadata in `X-Nexus-*` headers only (never modify response JSON body)
4. **Intelligent Routing**: Match capabilities to request requirements before considering load or latency
5. **Stateless by Design**: Route requests, not sessions — clients own conversation history
6. **Explicit Contracts**: 503 with actionable context is preferred over silent quality downgrades
7. **Precise Measurement**: Per-backend tokenizer registry, VRAM-aware pre-warming, sub-ms payload inspection

### What Nexus Is

- A **control plane** for heterogeneous LLM inference
- A **stateless request router** with capability matching
- A **policy enforcement layer** (privacy zones, budgets, capability tiers)

### What Nexus Is NOT

- **Not an inference engine** — routes to backends, doesn't run models
- **Not a GPU scheduler** — backends manage their own VRAM/compute
- **Not for training** — inference routing only
- **Not a session manager** — no KV-cache, no conversation state

### Evolving Scope

These capabilities were originally out of scope but are now planned:

| Capability | Version | Rationale |
|-----------|---------|-----------|
| Cloud backends | v0.3 | Hybrid local+cloud gateway with privacy enforcement |
| Multi-tenant auth | v0.5 | API keys and quotas for team/enterprise use |
| Model lifecycle | v0.5 | Load/unload/migrate via API (orchestration, not management) |
| Rate limiting | v0.5 | Per-backend and per-tenant protection |

## Git Workflow

**IMPORTANT**: Always use feature branches and Pull Requests for feature implementations.

### Feature Development Lifecycle

```
+-------------------------------------------------------------------------+
|  1. SPEC PHASE                                                          |
|     a) Write spec.md, plan.md, tasks.md                                 |
|     b) Copy requirements-validation.md to feature folder                |
|     c) Complete requirements-quality checklist (spec quality gate)       |
|     d) Run speckit.analyze (early check for spec issues)                |
|     e) Create GitHub issues via speckit.taskstoissues                   |
+-------------------------------------------------------------------------+
|  2. IMPLEMENTATION PHASE                                                |
|     a) Create feature branch                                            |
|     b) Copy verification.md template to feature folder                  |
|     c) Implement with TDD (tests first) - use speckit.implement         |
|     d) Check off acceptance criteria in tasks.md as you go              |
+-------------------------------------------------------------------------+
|  3. VERIFICATION PHASE                                                  |
|     a) Run speckit.analyze - verify spec/implementation alignment       |
|     b) Complete verification.md checklist (mark items [x] or [-])       |
|     c) Create walkthrough.md for code documentation                     |
|     d) Ensure 0 unchecked items in tasks.md and verification.md         |
+-------------------------------------------------------------------------+
|  4. MERGE PHASE                                                         |
|     a) Push feature branch                                              |
|     b) Create PR with verification summary                              |
|     c) Merge (closes issues automatically)                              |
+-------------------------------------------------------------------------+
```

### Quality Checklists

Use the three-checklist system for quality assurance:

| Phase | Checklist | Purpose |
|-------|-----------|---------|
| After spec, before coding | `requirements-validation.md` | Validate spec quality (quality gate) |
| During/after coding | `verification.md` | Verify implementation correctness |
| Reference | `tasks.md` | Track acceptance criteria completion |

```bash
# 1. SPEC PHASE: Copy requirements validation template
cp .specify/templates/requirements-validation.md specs/XXX-feature/requirements-validation.md

# Complete requirements validation checklist before implementation:
# - Mark [x] for items that pass
# - Mark [-] for items not applicable (N/A)
# - Fix any [ ] items before proceeding to implementation

# Verify spec is ready (should be 0 unchecked items)
grep -c "\- \[ \]" specs/XXX-feature/requirements-validation.md  # Should be 0

# 2. IMPLEMENTATION PHASE: Copy verification template
cp .specify/templates/implementation-verification.md specs/XXX-feature/verification.md

# 3. VERIFICATION PHASE: Complete both checklists
grep -c "\- \[ \]" specs/XXX-feature/verification.md  # Should be 0
grep -c "\- \[ \]" specs/XXX-feature/tasks.md         # Should be 0
```

**Quick Reference**: See `.specify/QUICK-REFERENCE.md` for top critical items.

### Commit Message Format

Use conventional commits with issue references:
```
feat: implement Feature Name (FXX)

Description of changes...

Closes #123
Closes #124
Closes #125
```

**Note**: Use separate `Closes #X` lines for each issue.

```bash
# 1. Create feature branch BEFORE implementing
git checkout -b feature/f09-request-metrics

# 2. Implement the feature (commits go to feature branch)
git add .
git commit -m "feat: implement Request Metrics (F09)"

# 3. Push feature branch and create PR
git push -u origin feature/f09-request-metrics
gh pr create --title "feat: Request Metrics (F09)" --body "..." --label "enhancement"

# 4. Merge PR (closes linked issues automatically)
gh pr merge --squash
```

### Task Completion Checklist

When completing a task, **always update the tasks.md file**:

1. Check off acceptance criteria as you verify them
2. Ensure all `- [ ]` items are now `- [x]` before committing
3. Run `speckit.analyze` before PR to catch gaps

```bash
# Verify no unchecked items remain
grep -c "\- \[ \]" specs/XXX-feature/tasks.md         # Should be 0
grep -c "\- \[ \]" specs/XXX-feature/verification.md  # Should be 0
```

## Coding Standards

- Line width: 100 characters (see `rustfmt.toml`)
- Routing decision: < 1ms; total request overhead: < 5ms (see constitution latency budget)
- Memory baseline: < 50 MB; per backend: < 10 KB
- Prefer memory-safe Rust patterns; minimize `unsafe` blocks
- `Model` struct must include context_length and feature flags (vision, tools, json_mode)
- No `println!` — use `tracing` macros for all output
- No panics on backend errors — always return proper HTTP response
- Comment the "why", not the "what"; no commented-out code in main branch

## Testing Strategy

### TDD Workflow (Non-Negotiable)

When asked to implement a feature, **always suggest the test cases first**:
1. Write tests → 2. Confirm they fail (Red) → 3. Implement (Green) → 4. Refactor

### Test Structure

| Scope | Location | Focus |
|-------|----------|-------|
| Unit tests | `mod tests` in each logic file | Registry data integrity, Router scoring |
| Integration tests | `tests/` directory | End-to-end API flows with mock backends |
| Property tests | `proptest` in scoring modules | Router `score()` function edge cases |
| Doc tests | Doc comments on public types | Executable examples stay accurate |

### Rules

- Every logic file must contain a `mod tests` block guarded by `#[cfg(test)]`
- Focus unit tests on the Registry's data integrity and the Router's scoring logic
- Use mock HTTP backends to simulate OpenAI-compatible responses
- For complex scoring logic, prefer property-based testing (`proptest`) over static values
- Write executable examples in doc comments for public traits and structs
