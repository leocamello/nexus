# Roadmap

**One API endpoint. Any backend. Local first, cloud when needed. Zero configuration.**

Nexus is a control plane for heterogeneous LLM inference — it routes requests and enforces policies; backends handle the heavy lifting.

---

## Product Roadmap

| Version | Theme | Features | Status |
|---------|-------|----------|--------|
| **v0.1** | Foundation | Registry, Health, Router, mDNS, CLI, Aliases, Fallbacks | ✅ Released |
| **v0.2** | Observability | Prometheus metrics, Web Dashboard, Structured logging | ✅ Released |
| **v0.3** | Cloud Hybrid | Cloud backends, Privacy zones, Capability tiers, Budget management | ✅ Released |
| **v0.4** | Intelligence | Speculative router, Quality tracking, Embeddings, Queuing | Planned |
| **v0.5** | Orchestration | Pre-warming, Model lifecycle, Multi-tenant, Rate limiting | Planned |
| **v1.0** | Complete Product | Management UI — full web-based control plane | Planned |

---

## Feature Index

| ID | Feature | Version | Status | Spec |
|----|---------|---------|--------|------|
| F01 | Core API Gateway | v0.1 | ✅ Complete | [specs/004-api-gateway](../specs/004-api-gateway/) |
| F02 | Backend Registry | v0.1 | ✅ Complete | [specs/001-backend-registry](../specs/001-backend-registry/) |
| F03 | Health Checker | v0.1 | ✅ Complete | [specs/002-health-checker](../specs/002-health-checker/) |
| F04 | CLI and Configuration | v0.1 | ✅ Complete | [specs/003-cli-configuration](../specs/003-cli-configuration/) |
| F05 | mDNS Discovery | v0.1 | ✅ Complete | [specs/005-mdns-discovery](../specs/005-mdns-discovery/) |
| F06 | Intelligent Router | v0.1 | ✅ Complete | [specs/006-intelligent-router](../specs/006-intelligent-router/) |
| F07 | Model Aliases | v0.1 | ✅ Complete | [specs/007-model-aliases](../specs/007-model-aliases/) |
| F08 | Fallback Chains | v0.1 | ✅ Complete | [specs/008-fallback-chains](../specs/008-fallback-chains/) |
| F09 | Request Metrics | v0.2 | ✅ Complete | [specs/009-request-metrics](../specs/009-request-metrics/) |
| F10 | Web Dashboard | v0.2 | ✅ Complete | [specs/010-web-dashboard](../specs/010-web-dashboard/) |
| F11 | Structured Request Logging | v0.2 | ✅ Complete | [specs/011-structured-logging](../specs/011-structured-logging/) |
| F12 | Cloud Backend Support | v0.3 | ✅ Complete | [specs/013-cloud-backend-support](../specs/013-cloud-backend-support/) |
| F13 | Privacy Zones & Capability Tiers | v0.3 | ✅ Complete | [specs/015-privacy-zones-capability-tiers](../specs/015-privacy-zones-capability-tiers/) |
| F14 | Inference Budget Management | v0.3 | ✅ Complete | [specs/016-inference-budget-mgmt](../specs/016-inference-budget-mgmt/) |
| — | **Quality + Queuing (Phase 2.5)** | **v0.4** | Planned | - |
| F15 | Speculative Router | v0.4 | Planned | - |
| F16 | Quality Tracking & Backend Profiling | v0.4 | Planned | - |
| F17 | Embeddings API | v0.4 | Planned | - |
| F18 | Request Queuing & Prioritization | v0.4 | Planned | - |
| — | **Fleet Intelligence (Phase 3)** | **v0.5** | Planned | - |
| F19 | Pre-warming & Fleet Intelligence | v0.5 | Planned | - |
| F20 | Model Lifecycle Management | v0.5 | Planned | - |
| F21 | Multi-Tenant Support | v0.5 | Planned | - |
| F22 | Rate Limiting | v0.5 | Planned | - |
| F23 | Management UI | v1.0 | Planned | - |

---

## Version Details

### v0.1 — Foundation

The initial release established Nexus as a working LLM gateway. Core infrastructure includes an in-memory backend registry with concurrent access (DashMap), background health checking with configurable intervals, and mDNS auto-discovery of Ollama and exo backends. The intelligent router scores backends by capability match, load, and latency using an exponential moving average. Model aliases allow friendly names (e.g., `fast` → `mistral:7b`) with up to 3-level chaining, and fallback chains provide automatic failover when a preferred model is unavailable. A full CLI (clap) supports `serve`, `backends`, `models`, `health`, `config`, and `completions` subcommands.

### v0.2 — Observability

Added production-grade observability. Prometheus metrics (`/metrics`) expose request counters, latency histograms, and backend gauge metrics. A real-time web dashboard is embedded in the binary (rust-embed) with WebSocket-based live updates for backend status, model changes, and request history. Structured request logging provides per-request trace context with configurable output format (JSON or pretty-print) and per-component log levels.

### v0.3 — Cloud Hybrid

Extended Nexus from a local-only gateway to a hybrid local+cloud control plane. Cloud backend support (OpenAI, generic OpenAI-compatible) adds API key management and tiktoken-based token counting. Privacy zones enforce structural guarantees — backends are tagged as `local`, `restricted`, or `public`, and requests with sensitive data never route to public backends. Capability tiers ensure overflow only targets same-or-higher tier backends. Inference budget management tracks token spend with configurable limits and graceful degradation policies.

---

## What's Next (v0.4)

The **Intelligence** release will make Nexus smarter about routing decisions:

- **Speculative Router (F15)** — Pre-analyze requests to predict optimal backend before payload inspection
- **Quality Tracking (F16)** — Profile backend response quality and track success rates over time
- **Embeddings API (F17)** — Support for `/v1/embeddings` endpoint with capability-aware routing
- **Request Queuing (F18)** — Queue requests when all capable backends are at capacity instead of rejecting immediately

---

## Non-Goals

These remain permanently out of scope:

1. **Distributed inference** — That's exo's job; Nexus routes to backends that handle this
2. **Model serving** — Nexus doesn't run models, it routes to servers that do
3. **Model management** — No model downloads, conversions, or storage
4. **GPU scheduling** — Backends manage their own resources
5. **Training/fine-tuning** — Inference only
6. **Stateful session management** — Clients own conversation history (OpenAI API contract)

---

## Architectural Principles

1. **Zero Configuration** — mDNS discovery, sensible defaults, "just run it"
2. **Single Binary** — Rust, no runtime dependencies
3. **OpenAI-Compatible** — Strict API adherence; metadata in `X-Nexus-*` headers only
4. **Stateless by Design** — Route requests, not sessions; clients own conversation history
5. **Explicit Contracts** — 503 with actionable context over silent quality downgrades
6. **Precise Measurement** — Per-backend tokenizer registry, VRAM-aware fleet management

See [architecture.md](architecture.md) for full technical details.
