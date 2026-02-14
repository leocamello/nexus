# Quickstart: CLI Configuration

**Feature**: F03 CLI Configuration  
**Status**: ✅ Implemented  
**Prerequisites**: Rust 1.75+, Nexus codebase cloned

---

## Overview

Nexus uses a layered configuration system: **CLI args > environment variables > config file > defaults**. The CLI provides subcommands for server management, backend management, model listing, health inspection, config generation, and shell completions.

This guide covers the full CLI interface, configuration file format, environment variable overrides, and how all three layers interact.

---

## Development Setup

### 1. Build Nexus

```bash
cargo build
```

### 2. Generate a Config File

```bash
cargo run -- config init
```

This creates `nexus.toml` with annotated defaults. To write to a specific path:

```bash
cargo run -- config init -o /etc/nexus/nexus.toml
cargo run -- config init -o ~/.config/nexus.toml
cargo run -- config init --force  # Overwrite existing file
```

---

## Project Structure

```
nexus/
├── src/
│   ├── cli/
│   │   ├── mod.rs              # Top-level CLI definition (clap derive)
│   │   ├── serve.rs            # `nexus serve` — start the server
│   │   ├── backends.rs         # `nexus backends` — list/add/remove backends
│   │   ├── models.rs           # `nexus models` — list available models
│   │   ├── health.rs           # `nexus health` — show system health
│   │   ├── config.rs           # `nexus config` — config utilities
│   │   ├── completions.rs      # `nexus completions` — shell completions
│   │   └── output.rs           # Table & JSON output formatters
│   ├── config/
│   │   ├── mod.rs              # NexusConfig — load, validate, env overrides
│   │   ├── server.rs           # ServerConfig (host, port, timeout)
│   │   ├── backend.rs          # BackendConfig (name, url, type, priority)
│   │   ├── routing.rs          # RoutingConfig (strategy, weights, aliases, fallbacks)
│   │   ├── discovery.rs        # DiscoveryConfig (enabled, service_types)
│   │   └── logging.rs          # LoggingConfig (level, format, component_levels)
│   └── main.rs                 # Entry point — dispatches CLI subcommands
├── nexus.example.toml          # Annotated example configuration
└── rustfmt.toml                # Code formatting config
```

---

## CLI Command Reference

### Top-Level

```bash
nexus [COMMAND]

Commands:
  serve        Start the Nexus server
  backends     Manage LLM backends
  models       List available models
  health       Show system health
  config       Configuration utilities
  completions  Generate shell completions
```

### `nexus serve`

Start the Nexus API gateway:

```bash
# Basic start with default config (nexus.toml in current dir)
cargo run -- serve

# Specify config file
cargo run -- serve -c /path/to/nexus.toml

# Override port and host
cargo run -- serve --port 3001 --host 127.0.0.1

# Override log level
cargo run -- serve --log-level debug

# Disable optional features
cargo run -- serve --no-discovery --no-health-check

# Combine overrides
cargo run -- serve -c nexus.toml -p 3001 -l debug --no-discovery
```

**Flags:**

| Flag | Short | Env Var | Default | Description |
|------|-------|---------|---------|-------------|
| `--config` | `-c` | — | `nexus.toml` | Config file path |
| `--port` | `-p` | `NEXUS_PORT` | `8000` | Server port |
| `--host` | `-H` | `NEXUS_HOST` | `0.0.0.0` | Server bind address |
| `--log-level` | `-l` | `NEXUS_LOG_LEVEL` | `info` | Log level (trace/debug/info/warn/error) |
| `--no-discovery` | — | `NEXUS_DISCOVERY=false` | enabled | Disable mDNS discovery |
| `--no-health-check` | — | `NEXUS_HEALTH_CHECK=false` | enabled | Disable health checks |

### `nexus backends`

Manage LLM backends:

```bash
# List all backends
cargo run -- backends list
cargo run -- backends list --json
cargo run -- backends list --status healthy

# Add a backend
cargo run -- backends add http://localhost:11434
cargo run -- backends add http://192.168.1.50:8000 \
  --name gpu-server \
  --backend-type vllm \
  --priority 3

# Remove a backend
cargo run -- backends remove gpu-server
```

### `nexus models`

List available models across all healthy backends:

```bash
# Table format
cargo run -- models

# JSON format
cargo run -- models --json

# Filter by specific backend
cargo run -- models --backend local-ollama
```

### `nexus health`

Show system health status:

```bash
# Table format
cargo run -- health

# JSON format for scripting
cargo run -- health --json
```

### `nexus config init`

Generate a config file:

```bash
# Default output to nexus.toml
cargo run -- config init

# Custom output path
cargo run -- config init -o /etc/nexus/nexus.toml

# Force overwrite
cargo run -- config init --force
```

### `nexus completions`

Generate shell completion scripts:

```bash
# Bash
cargo run -- completions bash > ~/.bash_completion.d/nexus
source ~/.bash_completion.d/nexus

# Zsh
cargo run -- completions zsh > ~/.zfunc/_nexus

# Fish
cargo run -- completions fish > ~/.config/fish/completions/nexus.fish

# PowerShell
cargo run -- completions powershell > nexus.ps1
```

---

## Configuration File Reference

### Full `nexus.toml` Example

```toml
[server]
host = "0.0.0.0"                       # Bind address
port = 8000                             # HTTP port
request_timeout_seconds = 300           # Per-request timeout (5 min)

[discovery]
enabled = true                          # mDNS auto-discovery
service_types = [                       # mDNS service types to browse
  "_ollama._tcp.local",
  "_llm._tcp.local"
]
grace_period_seconds = 60               # Remove disappeared backends after 60s

[health_check]
enabled = true                          # Background health checking
interval_seconds = 30                   # Check every 30s
timeout_seconds = 5                     # HTTP timeout per check

[routing]
strategy = "smart"                      # smart | round_robin | priority_only | random
max_retries = 2                         # Retry on different backend

[routing.weights]                       # Smart routing scoring weights
priority = 50                           # Backend priority weight
load = 30                               # Current load weight
latency = 20                            # Latency EMA weight

[routing.aliases]                       # Map OpenAI model names to local models
"gpt-4" = "llama3:70b"
"gpt-4-turbo" = "llama3:70b"
"gpt-3.5-turbo" = "mistral:7b"

[routing.fallbacks]                     # Try alternatives if primary unavailable
"llama3:70b" = ["qwen2:72b", "mixtral:8x7b"]

[[backends]]                            # Static backend definitions
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
priority = 1

[[backends]]
name = "gpu-server"
url = "http://192.168.1.100:8000"
type = "vllm"
priority = 3

[logging]
level = "info"                          # Global log level
format = "pretty"                       # pretty | json

# [logging.component_levels]            # Per-component log levels (optional)
# routing = "debug"
# api = "info"
# health = "warn"

# enable_content_logging = false        # WARNING: logs message content
```

### Minimal Config

The smallest useful config file:

```toml
[[backends]]
name = "ollama"
url = "http://localhost:11434"
type = "ollama"
```

Everything else uses defaults: port 8000, discovery enabled, health checks every 30s, smart routing.

---

## Environment Variable Overrides

Environment variables override config file values but are overridden by CLI args:

```bash
# Server settings
export NEXUS_PORT=3001
export NEXUS_HOST=127.0.0.1

# Logging
export NEXUS_LOG_LEVEL=debug
export NEXUS_LOG_FORMAT=json

# Feature toggles
export NEXUS_DISCOVERY=false
export NEXUS_HEALTH_CHECK=false

# Standard Rust logging (also respected)
export RUST_LOG=nexus=debug,tower_http=info

# Start with env overrides
cargo run -- serve
```

### Precedence Demonstration

```bash
# Config file says port=8000, env says 3001, CLI says 3002
# Result: port 3002 (CLI wins)
NEXUS_PORT=3001 cargo run -- serve -c nexus.toml --port 3002

# Config file says port=8000, env says 3001, no CLI override
# Result: port 3001 (env wins over config)
NEXUS_PORT=3001 cargo run -- serve -c nexus.toml

# Config file says port=8000, no env, no CLI override
# Result: port 8000 (config wins over default)
cargo run -- serve -c nexus.toml

# No config file, no env, no CLI override
# Result: port 8000 (default)
cargo run -- serve
```

---

## Manual Testing

### Test 1: Config File Generation

```bash
# Generate default config
cargo run -- config init -o /tmp/nexus-test.toml

# Verify it's valid TOML and contains expected sections
grep '\[server\]' /tmp/nexus-test.toml
grep '\[discovery\]' /tmp/nexus-test.toml
grep '\[health_check\]' /tmp/nexus-test.toml
grep '\[routing\]' /tmp/nexus-test.toml
grep '\[logging\]' /tmp/nexus-test.toml

# Try to overwrite without --force (should fail)
cargo run -- config init -o /tmp/nexus-test.toml
# Expected: error about file already existing

# Overwrite with --force
cargo run -- config init -o /tmp/nexus-test.toml --force
# Expected: success
```

### Test 2: Invalid Config

```bash
# Missing required field
cat > /tmp/nexus-bad.toml << 'EOF'
[[backends]]
url = "http://localhost:11434"
type = "ollama"
EOF

cargo run -- serve -c /tmp/nexus-bad.toml
# Expected: error about missing backend name

# Invalid TOML syntax
echo "invalid = [" > /tmp/nexus-broken.toml
cargo run -- serve -c /tmp/nexus-broken.toml
# Expected: TOML parse error

# Non-existent config file
cargo run -- serve -c /tmp/nexus-nonexistent.toml
# Expected: file not found error
```

### Test 3: CLI Arg Overrides

```bash
cargo run -- serve --port 9999 &
sleep 2
curl -s http://localhost:9999/health | jq .status   # Expected: "healthy"
kill %1
```

### Test 4: Environment Variable Overrides

```bash
NEXUS_PORT=7777 cargo run -- serve &
sleep 2
curl -s http://localhost:7777/health | jq .status   # Expected: "healthy"
kill %1

NEXUS_LOG_LEVEL=debug cargo run -- serve 2>&1 | head -20
# Expected: DEBUG-level log lines appear
```

### Test 5: All Subcommands Respond

```bash
cargo run -- serve --help
cargo run -- backends list --help
cargo run -- models --help
cargo run -- health --help
cargo run -- config init --help
cargo run -- completions --help
```

### Test 6: JSON Output and Shell Completions

```bash
cargo run -- backends list --json 2>/dev/null | jq type   # "object"
cargo run -- models --json 2>/dev/null | jq type           # "object"
cargo run -- health --json 2>/dev/null | jq type           # "object"
cargo run -- completions bash | head -5                    # valid bash script
```

### Test 7: Run Unit Tests

```bash
# Config module tests
cargo test config::

# CLI module tests
cargo test cli::
```

---

## Debugging Tips

### Config Not Loading

1. Check file path — default is `nexus.toml` in the current working directory:
   ```bash
   ls -la nexus.toml
   ```

2. Validate TOML syntax:
   ```bash
   # Quick syntax check — Python has a TOML parser
   python3 -c "import tomllib; tomllib.load(open('nexus.toml', 'rb'))"
   ```

3. Run with debug logging to see config loading:
   ```bash
   RUST_LOG=debug cargo run -- serve -c nexus.toml 2>&1 | grep -i config
   ```

### Env Var Not Taking Effect

1. Verify the variable is set:
   ```bash
   env | grep NEXUS_
   ```

2. Remember CLI args override env vars — check you're not also passing a CLI flag.

3. Supported env vars are limited to: `NEXUS_PORT`, `NEXUS_HOST`, `NEXUS_LOG_LEVEL`, `NEXUS_LOG_FORMAT`, `NEXUS_DISCOVERY`, `NEXUS_HEALTH_CHECK`.

### Routing Aliases Not Working

1. Check for circular alias references (validation catches these):
   ```toml
   # BAD: circular reference
   [routing.aliases]
   "gpt-4" = "model-a"
   "model-a" = "gpt-4"    # Circular!
   ```

2. Alias resolution chains up to 3 levels deep:
   ```toml
   # OK: 2-level chain
   [routing.aliases]
   "gpt-4" = "llama3"
   "llama3" = "llama3:70b"
   ```

3. Run with routing debug logging:
   ```bash
   RUST_LOG=nexus::routing=debug cargo run -- serve
   ```

---

## Code Style

- CLI uses `clap` with derive macros — add new subcommands by adding enum variants
- Config uses `serde::Deserialize` with `#[serde(default)]` for optional fields with defaults
- Env overrides are applied via `NexusConfig::with_env_overrides()` after TOML loading
- Validation runs via `NexusConfig::validate()` — checks ports, URLs, circular aliases
- Output formatting lives in `output.rs` — table format uses UTF8 box-drawing chars

---

## References

- **Feature Spec**: `specs/003-cli-configuration/spec.md`
- **Data Model**: `specs/003-cli-configuration/data-model.md`
- **Implementation Walkthrough**: `specs/003-cli-configuration/walkthrough.md`
- **Example Config**: `nexus.example.toml`
- **Clap Docs**: https://docs.rs/clap/latest/clap/
- **TOML Spec**: https://toml.io/en/
