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

# Generate a configuration file
nexus config init

# Run with auto-discovery
nexus serve

# Or with a custom config file
nexus serve --config nexus.toml
```

## CLI Commands

```bash
# Start the server
nexus serve [--config FILE] [--port PORT] [--host HOST]

# List backends
nexus backends list [--json] [--status healthy|unhealthy|unknown]

# Add a backend manually (auto-detects type)
nexus backends add http://localhost:11434 [--name NAME] [--type ollama|vllm|llamacpp]

# Remove a backend
nexus backends remove <ID>

# List available models
nexus models [--json] [--backend ID]

# Show system health
nexus health [--json]

# Generate config file
nexus config init [--output FILE] [--force] [--minimal]

# Generate shell completions
nexus completions bash > ~/.bash_completion.d/nexus
nexus completions zsh > ~/.zsh/completions/_nexus
nexus completions fish > ~/.config/fish/completions/nexus.fish
```

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `NEXUS_CONFIG` | Config file path | `nexus.toml` |
| `NEXUS_PORT` | Listen port | `8000` |
| `NEXUS_HOST` | Listen address | `0.0.0.0` |
| `NEXUS_LOG_LEVEL` | Log level (trace/debug/info/warn/error) | `info` |
| `NEXUS_LOG_FORMAT` | Log format (pretty/json) | `pretty` |
| `NEXUS_DISCOVERY` | Enable mDNS discovery | `true` |
| `NEXUS_HEALTH_CHECK` | Enable health checking | `true` |

**Precedence**: CLI args > Environment variables > Config file > Defaults

## API Usage

Once running, Nexus exposes an OpenAI-compatible API:

```bash
# Health check
curl http://localhost:8000/health

# List available models
curl http://localhost:8000/v1/models

# Chat completion (non-streaming)
curl http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:70b",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'

# Chat completion (streaming)
curl http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:70b",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": true
  }'
```

### With Claude Code / Continue.dev

Point your AI coding assistant to `http://localhost:8000` as the API endpoint.

### With OpenAI SDK

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:8000/v1",
    api_key="not-needed"
)

response = client.chat.completions.create(
    model="llama3:70b",
    messages=[{"role": "user", "content": "Hello!"}]
)
print(response.choices[0].message.content)
```

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
