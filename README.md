# Nexus

**One API endpoint. Any backend. Zero configuration.**

Nexus is a distributed LLM model serving orchestrator that unifies heterogeneous inference backends behind a single, intelligent API gateway.

## Features

- 🔍 **Auto-Discovery**: Automatically finds LLM backends on your network via mDNS
- 🎯 **Intelligent Routing**: Routes requests based on model capabilities and load
- 🔄 **Transparent Failover**: Automatically retries with fallback backends
- 🔌 **OpenAI-Compatible**: Works with any OpenAI API client
- ⚡ **Zero Config**: Just run it - works out of the box with Ollama

## Supported Backends

| Backend | Status | Notes |
|---------|--------|-------|
| Ollama | ✅ Supported | Auto-discovery via mDNS |
| vLLM | ✅ Supported | Static configuration |
| llama.cpp server | ✅ Supported | Static configuration |
| exo | ✅ Supported | Auto-discovery via mDNS |
| LocalAI | 🔜 Planned | |
| OpenAI (fallback) | 🔜 Planned | Cloud fallback |

## Quick Start

```bash
# Install (from source)
cargo install --path .

# Run with auto-discovery
nexus serve

# Or with a config file
nexus serve --config nexus.toml
```

## Usage

Once running, Nexus exposes an OpenAI-compatible API:

```bash
# List available models
curl http://localhost:8000/v1/models

# Chat completion
curl http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:70b",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

### With Claude Code / Continue.dev

Point your AI coding assistant to `http://localhost:8000` as the API endpoint.

## Configuration

```toml
# nexus.toml

[server]
host = "0.0.0.0"
port = 8000

[discovery]
enabled = true

[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
priority = 1

[[backends]]
name = "gpu-server"
url = "http://192.168.1.100:8000"
type = "vllm"
priority = 2
```

## Architecture

```
┌─────────────────────────────────────────────┐
│           Nexus Orchestrator                │
│  - Discovers backends via mDNS              │
│  - Tracks model capabilities                │
│  - Routes to best available backend         │
│  - OpenAI-compatible API                    │
└─────────────────────────────────────────────┘
        │           │           │
        ▼           ▼           ▼
   ┌────────┐  ┌────────┐  ┌────────┐
   │ Ollama │  │  vLLM  │  │  exo   │
   │  7B    │  │  70B   │  │  32B   │
   └────────┘  └────────┘  └────────┘
```

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run -- serve

# Check formatting
cargo fmt --check

# Lint
cargo clippy
```

## License

Apache License 2.0 - see [LICENSE](LICENSE) for details.

## Related Projects

- [exo](https://github.com/exo-explore/exo) - Distributed AI inference
- [Ollama](https://ollama.ai) - Easy local LLM serving
- [vLLM](https://github.com/vllm-project/vllm) - High-throughput LLM serving
- [LiteLLM](https://github.com/BerriAI/litellm) - Cloud LLM API router
