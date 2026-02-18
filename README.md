# Nexus

<!-- Project, Distribution, Rust Ecosystem, & Quality -->
[![Rust](https://img.shields.io/badge/rust-1.87%2B-blue.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://github.com/leocamello/nexus/blob/main/LICENSE)
[![GitHub Release](https://img.shields.io/github/v/release/leocamello/nexus)](https://github.com/leocamello/nexus/releases/latest)
[![Docker](https://img.shields.io/docker/v/leocamello/nexus?label=docker&sort=semver)](https://hub.docker.com/r/leocamello/nexus)
[![Crates.io](https://img.shields.io/crates/v/nexus-orchestrator.svg)](https://crates.io/crates/nexus-orchestrator)
[![docs.rs](https://docs.rs/nexus-orchestrator/badge.svg)](https://docs.rs/nexus-orchestrator)
[![codecov](https://codecov.io/gh/leocamello/nexus/branch/main/graph/badge.svg)](https://codecov.io/gh/leocamello/nexus)
[![CI](https://github.com/leocamello/nexus/actions/workflows/ci.yml/badge.svg)](https://github.com/leocamello/nexus/actions/workflows/ci.yml)

**One API endpoint. Any backend. Zero configuration.**

Nexus is a distributed LLM orchestrator that unifies heterogeneous inference backends behind a single, intelligent API gateway. Local first, cloud when needed.

## Features

- 🔍 **Auto-Discovery** — Finds LLM backends on your network via mDNS
- 🎯 **Intelligent Routing** — Routes by model capabilities, load, and latency
- 🔄 **Transparent Failover** — Retries with fallback backends automatically
- 🔌 **OpenAI-Compatible** — Works with any OpenAI API client
- ⚡ **Zero Config** — Just run it — works out of the box with Ollama
- 🔒 **Privacy Zones** — Structural enforcement prevents data from reaching cloud backends
- 💰 **Budget Management** — Token-aware cost tracking with automatic spend limits
- 📊 **Real-time Dashboard** — Monitor backends, models, and requests in your browser
- 🧠 **Quality Tracking** — Profiles backend response quality to inform routing decisions
- 📐 **Embeddings API** — OpenAI-compatible `/v1/embeddings` with capability-aware routing
- 📋 **Request Queuing** — Holds requests when backends are busy, with priority support

## Supported Backends

| Backend | Status | Discovery |
|---------|--------|-----------|
| [Ollama](https://ollama.ai) | ✅ Supported | mDNS (auto) |
| [LM Studio](https://lmstudio.ai) | ✅ Supported | Static config |
| [vLLM](https://github.com/vllm-project/vllm) | ✅ Supported | Static config |
| [llama.cpp](https://github.com/ggerganov/llama.cpp) | ✅ Supported | Static config |
| [exo](https://github.com/exo-explore/exo) | ✅ Supported | mDNS (auto) |
| [OpenAI](https://openai.com) | ✅ Supported | Static config |

## Quick Start

```bash
# Install from source
cargo install --path .

# Start with auto-discovery (zero config)
nexus serve

# Or with Docker
docker run -d -p 8000:8000 leocamello/nexus
```

Once running, send your first request:

```bash
curl http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "llama3:70b", "messages": [{"role": "user", "content": "Hello!"}]}'
```

Point any OpenAI-compatible client to `http://localhost:8000/v1` — Claude Code, Continue.dev, OpenAI SDK, or plain curl.

→ **[Full setup guide](docs/getting-started.md)** — installation, configuration, CLI reference, and more.

## Architecture

```
┌──────────────────────────────────────────────────┐
│              Nexus Orchestrator                   │
│  - Discovers backends via mDNS                   │
│  - Tracks model capabilities & quality           │
│  - Routes to best available backend              │
│  - Queues requests when backends are busy        │
│  - OpenAI-compatible API + Embeddings            │
└──────────────────────────────────────────────────┘
        │           │           │           │
        ▼           ▼           ▼           ▼
   ┌────────┐  ┌────────┐  ┌────────┐  ┌────────┐
   │ Ollama │  │  vLLM  │  │  exo   │  │ OpenAI │
   │  7B    │  │  70B   │  │  32B   │  │ cloud  │
   └────────┘  └────────┘  └────────┘  └────────┘
```

## Documentation

| | Document | What you'll find |
|---|---------|-----------------|
| 🚀 | [Getting Started](docs/getting-started.md) | Installation, configuration, CLI, environment variables |
| 📖 | [REST API](docs/api/rest.md) | HTTP endpoints, X-Nexus-* headers, error responses |
| 🔌 | [WebSocket API](docs/api/websocket.md) | Real-time dashboard protocol |
| 🏗️ | [Architecture](docs/architecture.md) | System design, module structure, data flows |
| 🗺️ | [Roadmap](docs/roadmap.md) | Feature index (F01–F23), version history, future plans |
| 🔧 | [Troubleshooting](docs/troubleshooting.md) | Common errors, debugging tips |
| ❓ | [FAQ](docs/faq.md) | What Nexus is (and isn't), common questions |
| 🤝 | [Contributing](.github/CONTRIBUTING.md) | Dev workflow, coding standards, PR guidelines |
| 📋 | [Changelog](CHANGELOG.md) | Release history |
| 🔒 | [Security](.github/SECURITY.md) | Vulnerability reporting |

## License

Apache License 2.0 — see [LICENSE](LICENSE) for details.

## Related Projects

- [exo](https://github.com/exo-explore/exo) — Distributed AI inference
- [LM Studio](https://lmstudio.ai) — Desktop app for local LLMs
- [Ollama](https://ollama.ai) — Easy local LLM serving
- [vLLM](https://github.com/vllm-project/vllm) — High-throughput LLM serving
- [LiteLLM](https://github.com/BerriAI/litellm) — Cloud LLM API router
