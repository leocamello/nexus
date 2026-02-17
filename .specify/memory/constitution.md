# Nexus Constitution

A distributed LLM orchestrator that unifies heterogeneous inference backends behind an OpenAI-compatible API gateway.

## Core Principles

### I. Zero Configuration
Nexus works out of the box with no mandatory configuration:
- mDNS discovery automatically finds Ollama and other backends on the local network
- Sensible defaults for all settings
- Static configuration is optional, not required
- "Just run it" philosophy: `nexus serve` should work immediately

### II. Single Binary
Nexus ships as one self-contained executable:
- Written in Rust for performance and safety
- No runtime dependencies (no Python, Node.js, Docker required)
- All assets (dashboard, config templates) embedded in binary
- Cross-platform: Linux, macOS, Windows from same codebase
- Target binary size: < 20 MB

### III. OpenAI-Compatible
Strict adherence to the OpenAI Chat Completions API:
- `POST /v1/chat/completions` - streaming and non-streaming
- `GET /v1/models` - list available models
- Error responses match OpenAI format exactly
- Works with Claude Code, Continue.dev, and any OpenAI client without modification
- API compatibility is non-negotiable; do not deviate from the spec
- **Nexus-Transparent outputs**: routing metadata in `X-Nexus-*` response headers (never in JSON body)
  - `X-Nexus-Backend-Type`: `local` | `cloud`
  - `X-Nexus-Route-Reason`: `capacity-overflow` | `privacy-requirement` | `capability-match`
  - `X-Nexus-Cost-Estimated`: per-request cost from tokenizer registry
  - `X-Nexus-Privacy-Zone`: `restricted` | `open`
- **Actionable errors**: 503 responses include `context` object with `available_nodes`, `eta_seconds`, `required_tier` — inside the standard OpenAI error envelope

### IV. Backend Agnostic
Nexus treats all inference backends equally:
- Supports: Ollama, vLLM, llama.cpp, exo, OpenAI, any OpenAI-compatible server
- No preference for any particular backend
- Backend-specific quirks handled in adapters, not core logic
- New backends can be added without changing the router

### V. Intelligent Routing
Route requests based on capabilities, not just load:
- Match model capabilities to request requirements (context length, vision, tools)
- Consider backend health, priority, current load, and latency
- Support model aliases (e.g., "gpt-4" → "llama3:70b")
- Support fallback chains for resilience
- Routing decision must complete in < 1ms

### VI. Resilient
Graceful handling of failures:
- Automatic failover when backends go down
- Retry with next-best backend on failure (configurable max retries)
- Health checks detect and remove unhealthy backends
- Grace periods prevent flapping on transient issues
- Never crash on backend errors; always return proper HTTP response

### VII. Local-First
Designed for home labs and small teams, not cloud:
- No authentication required (trusted network assumed)
- No external dependencies or cloud services
- All state is in-memory (no database)
- Works fully offline once backends are discovered
- Privacy: no telemetry, no external calls

### VIII. Stateless by Design
Nexus routes requests, not sessions:
- Clients own conversation history (OpenAI API contract)
- No context checkpointing, no KV-cache management
- Backend affinity (sticky routing) is a hint, not a guarantee
- Operational state only: backend health, metrics, load — never user data

### IX. Explicit Contracts
Never silently degrade the user experience:
- Privacy zones are structural (backend property), not opt-in (client header)
- A 503 is preferred over unpredictable quality — never silently downgrade
- Cross-zone overflow: scrub-or-block, never silently forward context
- Budget limits degrade gracefully: warn → shift to local → queue, never hard-cut

### X. Precise Measurement
Measure what matters, with accuracy:
- Per-backend tokenizer registry for audit-grade token counting
- VRAM is zero-sum — pre-warming respects headroom budgets, never cannibalizes active workloads
- Routing decisions use payload inspection only (sub-ms), not inference

## Technical Constraints

### Language & Runtime
- **Language**: Rust (stable toolchain)
- **Async Runtime**: Tokio with full features
- **HTTP Framework**: Axum
- **HTTP Client**: reqwest with connection pooling

### State Management
- **Registry**: DashMap for concurrent access
- **No Persistence**: All state is in-memory
- **Thread Safety**: Arc<T> for shared state, atomic operations for counters

### Discovery & Health
- **mDNS**: mdns-sd crate for cross-platform discovery
- **Health Checks**: Periodic polling, backend-specific endpoints
- **Status Transitions**: Configurable thresholds for healthy/unhealthy

### Configuration
- **Format**: TOML for config files
- **Precedence**: CLI args > Environment variables > Config file > Defaults
- **CLI**: clap with derive feature

### Logging & Observability
- **Logging**: tracing crate for structured async-friendly logs
- **Format**: Human-readable (pretty) or JSON
- **No println!**: All output through tracing macros

### Error Handling
- **Internal Errors**: thiserror for typed errors
- **HTTP Errors**: OpenAI-compatible error format
- **No Panics**: All errors handled gracefully

## Constitution Gates

These gates must be checked before implementation begins. They align with the plan template's "Constitution Check" section.

### Simplicity Gate
- [ ] Using ≤3 main modules for initial implementation?
- [ ] No speculative "might need" features?
- [ ] No premature optimization?
- [ ] Start with simplest approach that could work?

### Anti-Abstraction Gate
- [ ] Using Axum/Tokio/reqwest directly (no wrapper layers)?
- [ ] Single representation for each data type?
- [ ] No "framework on top of framework" patterns?
- [ ] Abstractions justified by actual (not theoretical) needs?

### Integration-First Gate
- [ ] API contracts defined before implementation?
- [ ] Integration tests planned with real/mock backends?
- [ ] End-to-end flow testable?

### Performance Gate
- [ ] Routing decision target: < 1ms?
- [ ] Total overhead target: < 5ms?
- [ ] Memory baseline target: < 50MB?

> If any gate fails, document the justification in the plan's "Complexity Tracking" section.

## Performance Standards

### Latency Budget
| Operation | Target | Maximum |
|-----------|--------|---------|
| Request parsing | 0.1ms | 1ms |
| Backend selection | 0.5ms | 2ms |
| Proxy overhead | 1ms | 5ms |
| **Total overhead** | **< 2ms** | **< 10ms** |

### Resource Limits
| Resource | Target | Maximum |
|----------|--------|---------|
| Memory (baseline) | 30 MB | 50 MB |
| Memory (per backend) | 5 KB | 10 KB |
| Binary size | 15 MB | 20 MB |
| Concurrent requests | 1000+ | - |

### Health Check Performance
- Health check timeout: 5 seconds
- Health check interval: 30 seconds (configurable)
- Status transition: < 3 check cycles

## Testing Standards

### Test-First Development (NON-NEGOTIABLE)

This is **mandatory** - all implementation MUST follow Test-Driven Development:

1. Tests are written first
2. Tests are reviewed and approved
3. Tests are confirmed to **FAIL** (Red phase)
4. Implementation is written to make tests pass (Green phase)
5. Code is refactored while keeping tests green (Refactor phase)

No implementation code shall be written before tests exist and fail.

### Test Structure

- Every logic module has a `mod tests` block with `#[cfg(test)]`
- Property-based testing (proptest) for complex scoring logic
- Integration tests use mock backends
- Test file creation order: contract → integration → e2e → unit

| Component | Required Coverage |
|-----------|-------------------|
| Registry operations | Unit tests for all methods |
| Router scoring | Property-based tests |
| API endpoints | Integration tests |
| Health checker | Mock HTTP response tests |

### CI Requirements
- `cargo test` must pass
- `cargo clippy --all-features -- -D warnings` must pass
- `cargo fmt --all -- --check` must pass

### Acceptance Criteria Tracking
- Each task in `tasks.md` has explicit acceptance criteria
- Criteria are checked off (`- [x]`) as they are verified
- All criteria must be checked before a feature is considered complete
- Use `grep -c "\- \[ \]" specs/*/tasks.md` to verify no unchecked items

### Three-Checklist Quality System
The project uses three checklists to ensure quality:
1. **Requirements Validation** (65 items) - Before implementation
2. **Tasks** - During implementation  
3. **Implementation Verification** (210 items) - After implementation

Templates are in `.specify/templates/`. See `.specify/INDEX.md` for details.

## Code Style

### Formatting
- Line width: 100 characters (see `rustfmt.toml`)
- Use `cargo fmt` before committing

### Naming
- Types: PascalCase
- Functions/methods: snake_case
- Constants: SCREAMING_SNAKE_CASE
- Modules: snake_case

### Documentation
- Public items must have doc comments
- Include examples in doc comments for complex APIs
- Document all error conditions

### Comments
- Comment the "why", not the "what"
- No commented-out code in main branch

## What Nexus Is

1. **A control plane** for heterogeneous LLM inference — routes, observes, enforces policy
2. **A stateless request router** with capability matching and privacy enforcement
3. **A policy enforcement layer** for privacy zones, budgets, and capability tiers
4. **Backend-agnostic** — the value exists above any single inference engine

## What Nexus Is NOT

These are permanently out of scope:

1. **Not an inference engine** - Nexus routes to backends; it doesn't run models
2. **Not a GPU scheduler** - Backends manage their own VRAM and compute resources
3. **Not for training** - Inference routing only
4. **Not a session manager** - No KV-cache, no conversation state (clients own history)

### Evolving Scope

These capabilities were originally out of scope but are now planned features:

| Capability | Version | Rationale |
|-----------|---------|-----------|
| Cloud backends | v0.3 | Hybrid local+cloud gateway with privacy enforcement |
| Multi-tenant auth | v0.5 | API keys and quotas for team/enterprise use |
| Model lifecycle | v0.5 | Load/unload/migrate via API (orchestration, not storage) |
| Rate limiting | v0.5 | Per-backend and per-tenant protection |

## Governance

### Constitution Authority
- This constitution supersedes all other development practices
- All code changes must comply with these principles
- Complexity must be justified against simplicity principle

### Amendments
- Amendments require documentation of rationale
- Performance regressions require explicit justification
- Breaking API changes require migration path

### Development Guidance
- Refer to `.github/copilot-instructions.md` for implementation details
- Refer to `docs/architecture.md` for system design
- Refer to `docs/roadmap.md` for feature specifications

**Version**: 1.1.0 | **Ratified**: 2026-02-01 | **Last Amended**: 2026-02-10
