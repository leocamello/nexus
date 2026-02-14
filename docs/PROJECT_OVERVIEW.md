# Nexus — Project Overview

A distributed LLM orchestrator that unifies heterogeneous inference backends behind a single, intelligent API gateway. The smart load balancer for the generative era.

---

## Vision

**One API endpoint. Any backend. Local first, cloud when needed. Zero configuration.**

Nexus automatically discovers LLM inference servers on your network, understands their capabilities, and intelligently routes requests to the best available backend. When local capacity is exhausted, it seamlessly overflows to cloud APIs — with structural privacy guarantees and cost-aware budget management.

Nexus is a **control plane**, not a data plane. It routes requests and enforces policies; backends handle the heavy lifting.

---

## Problem Statement

### The Current Pain

Running local LLMs at home or in a small team involves:

1. **Multiple machines with different capabilities**
   - Gaming PC with RTX 4090 running Llama 70B
   - Laptop with GTX 1080 running Mistral 7B
   - Raspberry Pi cluster with exo running distributed models

2. **Multiple inference servers**
   - Ollama on one machine
   - vLLM on another
   - llama.cpp server on the Pi
   - Each with different APIs, ports, configurations

3. **Manual routing decisions**
   - "Which machine should I use for this request?"
   - "Is the 70B model available or is someone else using it?"
   - "This request needs 100K context, which model supports that?"

4. **No unified interface**
   - Claude Code needs one endpoint
   - Continue.dev needs configuration per model
   - Every tool needs manual setup

5. **Cloud overflow is all-or-nothing**
   - Local cluster handles 80% of requests, but spikes need cloud
   - No way to transparently overflow without switching endpoints
   - No cost controls when cloud APIs are involved
   - No privacy guarantees about which data leaves the network

### The Solution

Nexus provides:
- **Single endpoint** for all clients (OpenAI-compatible API)
- **Auto-discovery** of backends via mDNS
- **Intelligent routing** based on model capabilities, load, and request requirements
- **Transparent failover** when backends go down
- **Local-first, cloud-overflow** with privacy zones and budget management
- **Zero configuration** for basic usage
- **Nexus-transparent outputs** — `X-Nexus-*` headers reveal routing decisions without breaking compatibility

---

## Target Users

### Primary: Home Lab Enthusiasts
- Multiple machines with GPUs
- Want to pool resources
- Technical but don't want complexity

### Secondary: Small Teams / Startups
- Shared GPU resources across team
- Need reliability and failover
- Cost-conscious (avoiding cloud APIs)

### Tertiary: Edge Deployments
- Multiple office locations
- Local inference preferred
- Cloud fallback for capacity

### Emerging: Compliance-Sensitive Teams
- Need privacy guarantees (PII never leaves local network)
- Cost controls on cloud API spend
- Audit-grade token tracking

---

## Key Differentiators

| Feature | Nexus | LiteLLM | Ollama | Ray Serve |
|---------|-------|---------|--------|-----------|
| Local-first | ✅ | ❌ Cloud-focused | ✅ | ✅ |
| Zero-config | ✅ mDNS discovery | ❌ Config required | ✅ | ❌ Complex |
| Multi-backend | ✅ Ollama, vLLM, etc. | ✅ Cloud APIs | ❌ Ollama only | ✅ |
| Capability-aware | ✅ | ❌ | ❌ | ❌ |
| Hybrid local+cloud | ✅ Planned (v0.3) | ✅ Cloud-native | ❌ | ❌ |
| Privacy zones | ✅ Planned (v0.3) | ❌ | ❌ | ❌ |
| Lightweight | ✅ Single binary | ❌ Python | ✅ | ❌ |

---

## Core Capabilities

### 1. Backend Discovery
- mDNS/Bonjour for local network
- Static configuration fallback
- Periodic health checks

### 2. Model Registry
- Track all available models across backends
- Capability metadata (context length, vision, tools, etc.)
- Real-time availability status

### 3. Intelligent Routing
- Match request requirements to model capabilities
- Load balancing across capable models
- Priority/preference rules
- Latency-aware selection
- Model aliases and fallback chains

### 4. Unified API
- OpenAI Chat Completions API (streaming and non-streaming)
- Model listing endpoint
- Health/status endpoints
- Nexus-Transparent Protocol (`X-Nexus-*` response headers)

### 5. Resilience
- Automatic failover on backend failure
- Request retry with fallback chains
- Graceful degradation (503 with actionable context, never silent downgrade)

### 6. Hybrid Cloud Gateway (Planned — v0.3)
- Local-first routing with cloud overflow
- Privacy zones: structural enforcement (backend property, not client header)
- Capability tiers: overflow only to same-or-higher tier
- Inference budget management with graceful degradation

### 7. Fleet Intelligence (Planned — v0.5)
- Model pre-warming based on demand prediction
- VRAM headroom awareness
- Model lifecycle management (load, unload, migrate)

---

## Technical Stack

| Component | Technology | Rationale |
|-----------|------------|-----------|
| Language | Rust | Performance, single binary, memory safety |
| HTTP Server | Axum | Modern, async, great ergonomics |
| Discovery | mdns-sd | Cross-platform mDNS |
| Async Runtime | Tokio | Industry standard |
| Serialization | Serde | Fast, flexible |
| Config | TOML | Human-readable, Rust-native |
| Logging | tracing | Structured, async-friendly |

---

## Supported Backends

| Backend | API Type | Discovery | Status |
|---------|----------|-----------|--------|
| Ollama | Ollama API | mDNS | ✅ Supported |
| vLLM | OpenAI-compatible | Static | ✅ Supported |
| llama.cpp server | OpenAI-compatible | Static | ✅ Supported |
| exo | OpenAI-compatible | mDNS | ✅ Supported |
| LM Studio | OpenAI-compatible | Static | ✅ Supported |
| OpenAI (cloud) | OpenAI API | Static | ✅ Supported (via Generic) |
| LocalAI | OpenAI-compatible | Static | 🔜 Planned (v0.3) |
| Anthropic (cloud) | Anthropic API | Static | 🔜 Planned (v0.3) |
| Google AI (cloud) | Google API | Static | 🔜 Planned (v0.3) |
| Llamafile | OpenAI-compatible | Static | 🔜 Planned |
| PowerInfer | Needs wrapper | Static | 🔬 Research |

---

## Non-Goals

These remain permanently out of scope:

1. **Distributed inference** — That's exo's job; Nexus routes to backends that handle this
2. **Model serving** — Nexus doesn't run models, it routes to servers that do
3. **Model management** — No model downloads, conversions, or storage
4. **GPU scheduling** — Backends manage their own resources
5. **Training/fine-tuning** — Inference only
6. **Stateful session management** — Clients own conversation history (OpenAI API contract)

### Evolving Scope

These were originally non-goals but are now planned for future versions:

| Capability | Originally | Now | Version |
|-----------|-----------|-----|---------|
| Authentication | Out of scope | Multi-tenant API keys | v0.5 |
| Cloud backends | Fallback only | Full hybrid gateway | v0.3 |
| Metrics | Prometheus + dashboard | Prometheus + dashboard | ✅ v0.2 |
| Rate limiting | Out of scope | Per-backend and per-tenant | v0.5 |

---

## Success Metrics

### User Experience
- Time from install to first request: < 5 minutes
- Zero configuration required for Ollama backends
- Single binary, no dependencies

### Technical
- Request routing latency: < 5ms overhead
- Backend health check: < 100ms
- Memory footprint: < 50MB

### Adoption
- Works with Claude Code out of the box
- Works with Continue.dev out of the box
- Works with any OpenAI-compatible client

---

## Product Roadmap

| Version | Theme | Features | Status |
|---------|-------|----------|--------|
| **v0.1** | Foundation | Registry, Health, Router, mDNS, CLI, Aliases, Fallbacks, LM Studio | ✅ Released |
| **v0.2** | Observability | Prometheus metrics, Web Dashboard, Structured request logging | ✅ Released |
| **v0.3** | Cloud Hybrid | Cloud backends, Privacy zones, Capability tiers, Budget management, Nexus-Transparent Protocol | 🎯 Next |
| **v0.4** | Intelligence | Speculative router, Quality tracking, Embeddings API, Request queuing | Planned |
| **v0.5** | Orchestration | Pre-warming, Model lifecycle, Multi-tenant, Rate limiting | Planned |

See [FEATURES.md](FEATURES.md) for detailed feature specifications (F01-F22).

---

## Related Projects

- **exo** — Distributed inference across Apple Silicon (Nexus can route TO exo)
- **LiteLLM** — Cloud API router (inspiration for API design)
- **LocalAI** — Multi-backend local inference server (planned as supported backend)
- **PowerInfer** — "Hot neuron" preloading for large models on consumer GPUs (research)
- **Llamafile** — Single-file LLM executable with built-in OpenAI API (planned backend)
- **Traefik/Nginx** — Reverse proxy patterns (architectural inspiration)
- **Consul/etcd** — Service discovery patterns

---

## Architectural Principles

Nexus follows 10 core principles defined in the [Constitution](.specify/memory/constitution.md) (v1.1.0). Key highlights:

1. **Zero Configuration** — mDNS discovery, sensible defaults, "just run it"
2. **Single Binary** — Rust, no runtime dependencies, < 20 MB
3. **OpenAI-Compatible** — Strict API adherence; Nexus-transparent via response headers only
4. **Stateless by Design** — Route requests, not sessions; clients own conversation history
5. **Explicit Contracts** — Never silently downgrade; 503 is preferred over unpredictable quality
6. **Precise Measurement** — Per-backend tokenizer registry; VRAM-aware fleet management

---

## Document Index

| Document | Purpose |
|----------|---------|
| [PROJECT_OVERVIEW.md](PROJECT_OVERVIEW.md) | This file — vision, roadmap, and positioning |
| [FEATURES.md](FEATURES.md) | Feature specifications (F01-F22) |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Technical architecture and design |
| [SPEC_KIT_PROMPTS.md](SPEC_KIT_PROMPTS.md) | Spec-kit workflow and feature prompts |
| [constitution.md](../.specify/memory/constitution.md) | Core principles and constraints |
