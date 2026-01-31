# LLM Orchestrator Project Overview

## Project Codename: **Nexus** (working title)

A distributed model serving orchestrator that unifies heterogeneous LLM backends behind a single, intelligent API gateway.

---

## Vision

**One API endpoint. Any backend. Zero configuration.**

Nexus automatically discovers LLM inference servers on your network, understands their capabilities, and intelligently routes requests to the best available model based on the request requirements and current system load.

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

### The Solution

Nexus provides:
- **Single endpoint** for all clients (OpenAI-compatible API)
- **Auto-discovery** of backends via mDNS/libp2p
- **Intelligent routing** based on model capabilities and load
- **Transparent failover** when backends go down
- **Zero configuration** for basic usage

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

---

## Key Differentiators

| Feature | Nexus | LiteLLM | Ollama | Ray Serve |
|---------|-------|---------|--------|-----------|
| Local-first | ✅ | ❌ Cloud-focused | ✅ | ✅ |
| Zero-config | ✅ mDNS discovery | ❌ Config required | ✅ | ❌ Complex |
| Multi-backend | ✅ Ollama, vLLM, etc. | ✅ Cloud APIs | ❌ Ollama only | ✅ |
| Capability-aware | ✅ | ❌ | ❌ | ❌ |
| Lightweight | ✅ Single binary | ❌ Python | ✅ | ❌ |

---

## Core Capabilities

### 1. Backend Discovery
- mDNS/Bonjour for local network
- libp2p for P2P (future)
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

### 4. Unified API
- OpenAI Chat Completions API
- Streaming support
- Model listing endpoint
- Health/status endpoints

### 5. Resilience
- Automatic failover on backend failure
- Request retry with fallback
- Graceful degradation

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

## Supported Backends (Initial)

| Backend | API Type | Discovery | Priority |
|---------|----------|-----------|----------|
| Ollama | Ollama API | mDNS | P0 |
| vLLM | OpenAI-compatible | Static | P0 |
| llama.cpp server | OpenAI-compatible | Static | P1 |
| exo | OpenAI-compatible | mDNS | P1 |
| LocalAI | OpenAI-compatible | Static | P2 |
| OpenAI (cloud) | OpenAI API | Static | P2 (fallback) |

---

## Non-Goals (v1.0)

These are explicitly out of scope for the initial version:

1. **Distributed inference** - That's exo's job; Nexus routes to backends that handle this
2. **Model serving** - Nexus doesn't run models, it routes to servers that do
3. **Model management** - No model downloads, conversions, or storage
4. **Authentication/multi-tenancy** - Single-user/team assumed
5. **GPU scheduling** - Backends manage their own resources
6. **Training/fine-tuning** - Inference only

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

## Project Timeline (Rough Estimate)

| Phase | Duration | Outcome |
|-------|----------|---------|
| MVP | 3 weeks | Static routing, OpenAI API, basic health |
| Discovery | 2 weeks | mDNS auto-discovery |
| Smart Routing | 2 weeks | Capability-aware routing |
| Polish | 2 weeks | CLI, config, docs |
| **Total** | **~9 weeks** | Production-ready v1.0 |

---

## Related Projects

- **exo** - Distributed inference (Nexus can route TO exo)
- **LiteLLM** - Cloud API router (inspiration for API design)
- **Traefik/Nginx** - Reverse proxy patterns
- **Consul/etcd** - Service discovery patterns

---

## Open Questions

1. **Naming**: Is "Nexus" good? Other options: Relay, Prism, Arbiter, Forge
2. **Scope**: Should v1 include any authentication?
3. **Metrics**: Should we expose Prometheus metrics?
4. **UI**: Should there be a simple web dashboard?

---

## Next Steps

1. Review spec-kit prompts document
2. Run constitution phase with spec-kit
3. Create initial Rust project structure
4. Implement MVP (static routing)
5. Iterate based on real usage

---

## Document Index

| Document | Purpose |
|----------|---------|
| `orchestrator-project-overview.md` | This file - vision and goals |
| `orchestrator-spec-kit-prompts.md` | Prompts for GitHub spec-kit workflow |
| `orchestrator-architecture.md` | Technical architecture and design |
| `orchestrator-features.md` | Detailed feature specifications |
