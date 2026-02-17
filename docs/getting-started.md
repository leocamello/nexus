# Getting Started with Nexus

This guide walks you through installing Nexus, starting the server, and making your first LLM request — all in about five minutes.

## Prerequisites

- **From source**: [Rust](https://www.rust-lang.org/tools/install) 1.87+ (with `cargo`)
- **Docker**: [Docker](https://docs.docker.com/get-docker/) installed and running
- **Pre-built binary**: No dependencies — just download and run

You'll also need at least one LLM backend running (e.g., [Ollama](https://ollama.ai) on `localhost:11434`).

---

## 1. Install Nexus

Choose one of the three installation methods:

### Option A: From Source

```bash
# Clone and install
git clone https://github.com/leocamello/nexus.git
cd nexus
cargo install --path .

# Generate a default configuration file
nexus config init
```

### Option B: Docker

```bash
# Run with default settings
docker run -d -p 8000:8000 leocamello/nexus

# Run with a custom config file
docker run -d -p 8000:8000 \
  -v $(pwd)/nexus.toml:/home/nexus/nexus.toml \
  leocamello/nexus serve --config nexus.toml

# Run with host network (required for mDNS auto-discovery)
docker run -d --network host leocamello/nexus
```

### Option C: Pre-built Binary

Download the latest binary for your platform from [GitHub Releases](https://github.com/leocamello/nexus/releases), extract it, and place it in your `PATH`.

---

## 2. Start the Server

```bash
nexus serve
```

Nexus starts on `http://localhost:8000` by default. If mDNS discovery is enabled (the default), it will automatically find backends like Ollama on your local network.

You can customize the host and port:

```bash
nexus serve --port 9000 --host 127.0.0.1
```

Or use a specific config file:

```bash
nexus serve --config nexus.toml
```

---

## 3. Verify It Works

### Health check

```bash
curl http://localhost:8000/health
```

You should see a JSON response with system status, backend count, and model count.

### List available models

```bash
curl http://localhost:8000/v1/models
```

This returns all models discovered across your backends.

### Send your first chat request

```bash
curl http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:70b",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

For streaming responses, add `"stream": true`:

```bash
curl http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3:70b",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": true
  }'
```

> **Tip:** Replace `llama3:70b` with any model name returned by `/v1/models`.

---

## 4. Explore the CLI

Nexus ships with a full CLI for managing backends and models at runtime:

```bash
# List discovered backends
nexus backends list

# Add a backend manually (type is auto-detected)
nexus backends add http://localhost:11434 --name my-ollama --type ollama

# Remove a backend
nexus backends remove <ID>

# List available models (optionally filter by backend)
nexus models
nexus models --backend <ID>

# Show system health
nexus health

# Generate a config file
nexus config init --output nexus.toml

# JSON output for scripting
nexus backends list --json
nexus health --json
nexus models --json
```

### Shell Completions

Enable tab-completion for your shell:

```bash
# Bash
nexus completions bash > ~/.bash_completion.d/nexus

# Zsh
nexus completions zsh > ~/.zsh/completions/_nexus

# Fish
nexus completions fish > ~/.config/fish/completions/nexus.fish
```

---

## 5. Configuration

Nexus works out of the box with zero configuration. For more control, create a `nexus.toml` file:

```bash
nexus config init
```

Here's a fully annotated example:

```toml
# nexus.toml

[server]
host = "0.0.0.0"
port = 8000

[discovery]
# Auto-discover backends on your network via mDNS
enabled = true

# --- Local Backends ---

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

# --- Cloud Backend (requires API key via env var) ---

# [[backends]]
# name = "openai-cloud"
# url = "https://api.openai.com"
# type = "openai"
# priority = 100
# api_key_env = "OPENAI_API_KEY"        # Reads key from this env var
# zone = "open"                          # Privacy zone: open | internal | confidential | restricted
# tier = 3                               # Capability tier: 1 (fast) | 2 (standard) | 3 (premium)

# --- Routing ---

[routing]
strategy = "smart"    # smart | round_robin | priority_only | random

# Model aliases — map common names to your local models
[routing.aliases]
"gpt-4" = "llama3:70b"
"gpt-3.5-turbo" = "mistral:7b"

# Fallback chains — try alternatives if the primary model is unavailable
[routing.fallbacks]
"llama3:70b" = ["qwen2:72b", "mixtral:8x7b"]

# Budget — monthly spending limits for cloud backends (optional)
# [routing.budget]
# monthly_limit_usd = 50.0
# soft_limit_percent = 75              # At 75%: prefer local backends
# hard_limit_action = "block_cloud"    # At 100%: warn | block_cloud | block_all
```

### Environment Variables

You can also configure Nexus via environment variables:

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

---

## 6. Web Dashboard

Nexus includes a built-in web dashboard for real-time monitoring. Open your browser to:

```
http://localhost:8000/
```

**Features:**

- Real-time backend health monitoring with status indicators
- Model availability matrix across backends
- Request history with durations and error details
- WebSocket-based live updates (with HTTP polling fallback)
- Dark mode support (follows system preference)
- Fully responsive — works on desktop, tablet, and mobile
- Works without JavaScript (graceful degradation with auto-refresh)

---

## 7. Using with AI Coding Tools

Nexus is OpenAI-compatible, so it works with any tool that speaks the OpenAI API.

### Claude Code / Continue.dev

Point your AI coding assistant's API endpoint to:

```
http://localhost:8000
```

### OpenAI Python SDK

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

---

## 8. Generate Embeddings

Nexus supports the OpenAI-compatible embeddings endpoint for turning text into vector representations. This works with Ollama and OpenAI backends that support embedding models.

**Single input:**

```bash
curl http://localhost:8000/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{
    "model": "nomic-embed-text",
    "input": "Nexus is a distributed LLM orchestrator"
  }'
```

**Batch input:**

```bash
curl http://localhost:8000/v1/embeddings \
  -H "Content-Type: application/json" \
  -d '{
    "model": "nomic-embed-text",
    "input": [
      "First document to embed",
      "Second document to embed",
      "Third document to embed"
    ]
  }'
```

The response follows the OpenAI format — an array of embedding vectors with token usage:

```json
{
  "object": "list",
  "data": [
    { "object": "embedding", "embedding": [0.1, 0.2, ...], "index": 0 }
  ],
  "model": "nomic-embed-text",
  "usage": { "prompt_tokens": 8, "total_tokens": 8 }
}
```

> **Tip:** Use `nomic-embed-text` with Ollama or `text-embedding-3-small` with OpenAI backends. Any model listed by `/v1/models` on an embeddings-capable backend will work.

---

## 9. Observability

Nexus exposes metrics for monitoring and debugging:

```bash
# Prometheus metrics (for Grafana, Prometheus, etc.)
curl http://localhost:8000/metrics

# JSON stats (uptime, per-backend request counts, latency)
curl http://localhost:8000/v1/stats | jq
```

Configure your Prometheus scraper to target `http://<nexus-host>:8000/metrics` for request counters, duration histograms, error rates, backend latency, and token usage gauges.

---

## What's Next?

- **[API Reference](api/)** — Full endpoint documentation
- **[Architecture](architecture.md)** — System internals, module structure, and data flows
- **[Roadmap](roadmap.md)** — Feature index (F01–F23) and version history
- **[Example Config](../nexus.example.toml)** — Full annotated configuration file
