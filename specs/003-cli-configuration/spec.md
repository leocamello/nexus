# F04: CLI and Configuration

**Feature ID**: F04  
**Spec ID**: 003-cli-configuration  
**Priority**: P0 (Core MVP)  
**Status**: Draft  
**Created**: 2026-02-02

---

## Overview

Command-line interface and TOML configuration file support for Nexus. Provides both interactive commands for management and a `serve` command for running the daemon.

### Goals

1. **Zero-friction startup**: `nexus serve` works out-of-the-box with sensible defaults
2. **Layered configuration**: CLI args > Environment variables > Config file > Defaults
3. **Operator-friendly output**: Pretty tables for humans, JSON for scripts
4. **Single binary**: All commands are subcommands of the `nexus` binary

### Non-Goals

- GUI configuration (see F10: Web Dashboard)
- Remote management API (future feature)
- Config hot-reloading (restart required for config changes)

---

## User Stories

| ID | Story | Priority |
|----|-------|----------|
| US1 | As an operator, I want to start Nexus with `nexus serve` so I can run the server | P0 |
| US2 | As an operator, I want to specify a config file path so I can use different configurations | P0 |
| US3 | As an operator, I want to override config with CLI flags so I can quickly test settings | P0 |
| US4 | As an operator, I want to list backends so I can see what's connected | P0 |
| US5 | As an operator, I want to list models so I can see what's available | P0 |
| US6 | As an operator, I want to check health status so I can verify the system is working | P0 |
| US7 | As a developer, I want JSON output so I can script against Nexus | P1 |
| US8 | As an operator, I want to add/remove backends at runtime so I can manage the cluster | P1 |
| US9 | As a new user, I want to generate a config template so I can get started quickly | P1 |
| US10 | As an operator, I want environment variable overrides for containerized deployments | P0 |

---

## CLI Commands

### nexus serve [OPTIONS]

Start the Nexus server (main daemon mode).

```
OPTIONS:
  -c, --config <FILE>     Config file path [default: nexus.toml]
  -p, --port <PORT>       Listen port [default: 8000]
  -H, --host <HOST>       Listen address [default: 0.0.0.0]
  -l, --log-level <LEVEL> Log level: trace, debug, info, warn, error [default: info]
      --no-discovery      Disable mDNS discovery
      --no-health-check   Disable background health checking
  -h, --help              Print help
```

**Behavior:**
- Loads config file if present (not required)
- Starts HTTP server on `host:port`
- Starts health checker (unless `--no-health-check`)
- Starts mDNS discovery (unless `--no-discovery`)
- Runs until SIGINT/SIGTERM (graceful shutdown)

**Exit Codes:**
- 0: Graceful shutdown
- 1: Startup error (port in use, config parse error, etc.)

---

### nexus backends [OPTIONS]

List all registered backends.

```
OPTIONS:
      --json              Output as JSON
      --status <STATUS>   Filter by status: healthy, unhealthy, unknown
  -h, --help              Print help
```

**Table Output:**
```
┌──────────────┬────────────────────────────┬─────────┬──────────┬────────┬──────────┐
│ Name         │ URL                        │ Type    │ Status   │ Models │ Latency  │
├──────────────┼────────────────────────────┼─────────┼──────────┼────────┼──────────┤
│ local-ollama │ http://localhost:11434     │ Ollama  │ Healthy  │ 3      │ 45ms     │
│ gpu-server   │ http://192.168.1.100:8000  │ vLLM    │ Healthy  │ 1      │ 12ms     │
│ pi-cluster   │ http://192.168.1.50:52415  │ Exo     │ Unhealthy│ 0      │ -        │
└──────────────┴────────────────────────────┴─────────┴──────────┴────────┴──────────┘
```

**JSON Output:**
```json
{
  "backends": [
    {
      "id": "backend-abc123",
      "name": "local-ollama",
      "url": "http://localhost:11434",
      "type": "ollama",
      "status": "healthy",
      "models": 3,
      "avg_latency_ms": 45,
      "pending_requests": 2,
      "discovery_source": "static"
    }
  ]
}
```

---

### nexus backends add <URL> [OPTIONS]

Add a backend manually at runtime.

```
ARGS:
  <URL>                   Backend base URL (e.g., http://192.168.1.100:11434)

OPTIONS:
      --name <NAME>       Display name [default: derived from URL]
      --type <TYPE>       Backend type: ollama, vllm, llamacpp, exo, openai, generic
                          [default: auto-detect]
      --priority <N>      Routing priority (lower = prefer) [default: 50]
  -h, --help              Print help
```

**Behavior:**
- Adds backend to registry with `DiscoverySource::Manual`
- Triggers immediate health check
- Prints result (success with backend ID, or error)

**Auto-detection:**
If `--type` not specified, attempt to detect by:
1. Try GET `/api/tags` → Ollama
2. Try GET `/v1/models` → OpenAI-compatible
3. Try GET `/health` → LlamaCpp
4. Fall back to `generic`

---

### nexus backends remove <ID>

Remove a backend by ID or name.

```
ARGS:
  <ID>                    Backend ID or name

OPTIONS:
  -h, --help              Print help
```

**Behavior:**
- Removes backend from registry
- Active requests continue (graceful)
- Returns error if ID not found

---

### nexus models [OPTIONS]

List all available models across backends.

```
OPTIONS:
      --json              Output as JSON
      --backend <ID>      Filter by backend ID or name
  -h, --help              Print help
```

**Table Output:**
```
┌──────────────┬─────────────────┬─────────┬────────┬───────┬──────────┐
│ Model        │ Backend         │ Context │ Vision │ Tools │ JSON     │
├──────────────┼─────────────────┼─────────┼────────┼───────┼──────────┤
│ llama3:70b   │ local-ollama    │ 8192    │ No     │ Yes   │ No       │
│ llama3:70b   │ gpu-server      │ 8192    │ No     │ Yes   │ No       │
│ mistral:7b   │ local-ollama    │ 32768   │ No     │ No    │ No       │
│ llava:13b    │ local-ollama    │ 4096    │ Yes    │ No    │ No       │
└──────────────┴─────────────────┴─────────┴────────┴───────┴──────────┘
```

**JSON Output:**
```json
{
  "models": [
    {
      "id": "llama3:70b",
      "backends": ["local-ollama", "gpu-server"],
      "context_length": 8192,
      "supports_vision": false,
      "supports_tools": true,
      "supports_json_mode": false
    }
  ]
}
```

---

### nexus health [OPTIONS]

Show system health status.

```
OPTIONS:
      --json              Output as JSON
  -h, --help              Print help
```

**Pretty Output:**
```
Status: Healthy
Version: 0.1.0
Uptime: 2h 34m 12s

Backends: 2/3 healthy
Models: 4 available

Backend Details:
  ✓ local-ollama     3 models  45ms avg  2 pending
  ✓ gpu-server       1 model   12ms avg  0 pending
  ✗ pi-cluster       connection refused (3m ago)
```

**JSON Output:**
```json
{
  "status": "healthy",
  "version": "0.1.0",
  "uptime_seconds": 9252,
  "backends": {
    "total": 3,
    "healthy": 2,
    "unhealthy": 1
  },
  "models": {
    "total": 4
  },
  "backend_details": [
    {
      "name": "local-ollama",
      "status": "healthy",
      "models": 3,
      "avg_latency_ms": 45,
      "pending_requests": 2
    }
  ]
}
```

---

### nexus config init [OPTIONS]

Generate an example configuration file.

```
OPTIONS:
  -o, --output <FILE>     Output file path [default: nexus.toml]
      --minimal           Generate minimal config (only essential settings)
      --force             Overwrite existing file
  -h, --help              Print help
```

**Behavior:**
- Writes a fully-commented example config
- Fails if file exists (unless `--force`)
- `--minimal` omits optional sections

---

### nexus completions <SHELL>

Generate shell completion scripts.

```
ARGS:
  <SHELL>                 Target shell: bash, zsh, fish, powershell, elvish

OPTIONS:
  -h, --help              Print help
```

**Usage Examples:**
```bash
# Bash (add to ~/.bashrc or ~/.bash_completion.d/)
nexus completions bash > ~/.bash_completion.d/nexus
source ~/.bash_completion.d/nexus

# Zsh (add to fpath)
nexus completions zsh > ~/.zfunc/_nexus

# Fish
nexus completions fish > ~/.config/fish/completions/nexus.fish

# PowerShell
nexus completions powershell >> $PROFILE
```

---

### nexus --version

Print version information.

```
nexus 0.1.0
```

### nexus --help

Print help for all commands.

---

## Configuration File

### Full Example: nexus.toml

```toml
# Nexus Configuration
# See: https://github.com/user/nexus

[server]
host = "0.0.0.0"              # Listen address
port = 8000                    # Listen port
request_timeout_seconds = 300  # Max request duration
max_concurrent_requests = 1000 # Limit in-flight requests

[discovery]
enabled = true
service_types = ["_ollama._tcp.local", "_llm._tcp.local"]
grace_period_seconds = 60      # Wait before removing disappeared backends

[health_check]
enabled = true
interval_seconds = 30
timeout_seconds = 5
failure_threshold = 3          # Failures before marking unhealthy
recovery_threshold = 2         # Successes before marking healthy

[routing]
strategy = "smart"             # smart | round_robin | priority_only | random
max_retries = 2                # Retry on backend failure

[routing.weights]
priority = 50                  # Weight for backend priority
load = 30                      # Weight for current load
latency = 20                   # Weight for average latency

[routing.aliases]
"gpt-4" = "llama3:70b"
"gpt-4-turbo" = "llama3:70b"
"gpt-3.5-turbo" = "mistral:7b"

[routing.fallbacks]
"llama3:70b" = ["qwen2:72b", "mixtral:8x7b"]

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

[logging]
level = "info"                 # trace, debug, info, warn, error
format = "pretty"              # pretty | json
```

### Configuration Sections

| Section | Required | Description |
|---------|----------|-------------|
| `[server]` | No | HTTP server settings |
| `[discovery]` | No | mDNS discovery settings |
| `[health_check]` | No | Health checker settings |
| `[routing]` | No | Routing strategy and weights |
| `[routing.aliases]` | No | Model name mappings |
| `[routing.fallbacks]` | No | Fallback model chains |
| `[[backends]]` | No | Static backend definitions |
| `[logging]` | No | Logging configuration |

### Default Values

| Setting | Default | Notes |
|---------|---------|-------|
| `server.host` | `"0.0.0.0"` | Listen on all interfaces |
| `server.port` | `8000` | Standard port |
| `server.request_timeout_seconds` | `300` | 5 minutes for long completions |
| `server.max_concurrent_requests` | `1000` | Per-server limit |
| `discovery.enabled` | `true` | Zero-config by default |
| `discovery.grace_period_seconds` | `60` | Avoid thrashing |
| `health_check.enabled` | `true` | Always check health |
| `health_check.interval_seconds` | `30` | Balance freshness vs load |
| `health_check.timeout_seconds` | `5` | Fail fast |
| `health_check.failure_threshold` | `3` | Avoid flapping |
| `health_check.recovery_threshold` | `2` | Confirm recovery |
| `routing.strategy` | `"smart"` | Best of all factors |
| `routing.max_retries` | `2` | Try 3 backends total |
| `routing.weights.priority` | `50` | |
| `routing.weights.load` | `30` | |
| `routing.weights.latency` | `20` | |
| `logging.level` | `"info"` | Reasonable default |
| `logging.format` | `"pretty"` | Human-readable |

---

## Environment Variables

All settings can be overridden via environment variables.

| Variable | Config Equivalent | Example |
|----------|-------------------|---------|
| `NEXUS_CONFIG` | (config file path) | `/etc/nexus/nexus.toml` |
| `NEXUS_HOST` | `server.host` | `127.0.0.1` |
| `NEXUS_PORT` | `server.port` | `9000` |
| `NEXUS_LOG_LEVEL` | `logging.level` | `debug` |
| `NEXUS_LOG_FORMAT` | `logging.format` | `json` |
| `NEXUS_DISCOVERY` | `discovery.enabled` | `false` |
| `NEXUS_HEALTH_CHECK` | `health_check.enabled` | `false` |
| `NEXUS_ROUTING_STRATEGY` | `routing.strategy` | `round_robin` |

---

## Configuration Precedence

Settings are resolved in this order (later wins):

1. **Compiled defaults** - Hardcoded in source
2. **Config file** - `nexus.toml` or `--config`
3. **Environment variables** - `NEXUS_*`
4. **CLI arguments** - `--port`, `--host`, etc.

### Example

```bash
# Config file has: port = 8000
# Environment has: NEXUS_PORT=9000
# CLI has: --port 9001

# Result: port = 9001 (CLI wins)
```

---

## Technical Stack

| Crate | Purpose | Notes |
|-------|---------|-------|
| `clap` | CLI argument parsing | Use derive feature for declarative API |
| `config` | Layered configuration | Merge TOML + env + defaults |
| `toml` | TOML serialization | For `config init` output |
| `comfy-table` | Pretty table output | Terminal-aware formatting |
| `serde` | Serialization | Already a dependency |

---

## Data Structures

### Unified Config Struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NexusConfig {
    pub server: ServerConfig,
    pub discovery: DiscoveryConfig,
    pub health_check: HealthCheckConfig,
    pub routing: RoutingConfig,
    pub backends: Vec<BackendConfig>,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub request_timeout_seconds: u64,
    pub max_concurrent_requests: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RoutingConfig {
    pub strategy: RoutingStrategy,
    pub max_retries: u32,
    pub weights: RoutingWeights,
    pub aliases: HashMap<String, String>,
    pub fallbacks: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,
    pub format: LogFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendConfig {
    pub name: String,
    pub url: String,
    #[serde(rename = "type")]
    pub backend_type: BackendType,
    #[serde(default = "default_priority")]
    pub priority: i32,
}
```

### CLI Argument Structs (clap)

```rust
#[derive(Parser)]
#[command(name = "nexus", version, about = "Distributed LLM Orchestrator")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the Nexus server
    Serve(ServeArgs),
    /// Manage backends
    Backends(BackendsArgs),
    /// List available models
    Models(ModelsArgs),
    /// Show system health
    Health(HealthArgs),
    /// Configuration utilities
    Config(ConfigArgs),
}

#[derive(Args)]
pub struct ServeArgs {
    #[arg(short, long, default_value = "nexus.toml")]
    pub config: PathBuf,
    
    #[arg(short, long)]
    pub port: Option<u16>,
    
    #[arg(short = 'H', long)]
    pub host: Option<String>,
    
    #[arg(short, long)]
    pub log_level: Option<String>,
    
    #[arg(long)]
    pub no_discovery: bool,
    
    #[arg(long)]
    pub no_health_check: bool,
}
```

---

## Functional Requirements

| ID | Requirement | Priority |
|----|-------------|----------|
| FR-001 | Parse TOML config file | P0 |
| FR-002 | Apply defaults for missing config values | P0 |
| FR-003 | Override config with environment variables | P0 |
| FR-004 | Override config/env with CLI arguments | P0 |
| FR-005 | `serve` command starts HTTP server | P0 |
| FR-006 | `backends` command lists backends | P0 |
| FR-007 | `models` command lists models | P0 |
| FR-008 | `health` command shows status | P0 |
| FR-009 | JSON output flag for scripting | P1 |
| FR-010 | `backends add` adds runtime backend with auto-type detection | P1 |
| FR-011 | `backends remove` removes backend | P1 |
| FR-012 | `config init` generates template | P1 |
| FR-013 | Graceful shutdown on SIGINT/SIGTERM | P0 |
| FR-014 | Exit code 1 on startup errors | P0 |
| FR-015 | Pretty table output with colors | P1 |
| FR-016 | `completions` command generates shell scripts | P1 |
| FR-017 | Warn on unknown config keys (don't fail) | P1 |

---

## Non-Functional Requirements

| ID | Requirement | Metric |
|----|-------------|--------|
| NFR-001 | Config parsing < 10ms | Measured |
| NFR-002 | CLI startup to output < 100ms | For non-serve commands |
| NFR-003 | Works without config file | Zero-config mode |
| NFR-004 | Helpful error messages | Include fix suggestions |
| NFR-005 | Shell completion support | Fish, Bash, Zsh |

---

## Error Handling

### Config Errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config file not found: {0}")]
    NotFound(PathBuf),
    
    #[error("config parse error at line {line}: {message}")]
    ParseError { line: usize, message: String },
    
    #[error("invalid value for {field}: {message}")]
    ValidationError { field: String, message: String },
    
    #[error("environment variable {0} has invalid value: {1}")]
    EnvError(String, String),
}
```

### Example Error Output

```
Error: config parse error at line 15: invalid backend type 'local'

  14 | [[backends]]
  15 | type = "local"
     |        ^^^^^^^ expected one of: ollama, vllm, llamacpp, exo, openai, generic

Tip: Use 'nexus config init' to generate a valid config template.
```

---

## Edge Cases

| Scenario | Behavior |
|----------|----------|
| Config file not found | Use defaults (warn if `--config` explicit) |
| Config file parse error | Exit with error, show line number |
| Invalid backend URL in config | Warn and skip that backend |
| Port already in use | Exit with error, suggest alternative |
| No backends available | Start anyway (health shows "degraded") |
| SIGINT during startup | Clean exit |
| Invalid UTF-8 in config | Exit with error |

---

## Testing Strategy

### Unit Tests

| Test | Description |
|------|-------------|
| `test_config_defaults` | Verify all defaults are sensible |
| `test_config_parse_full` | Parse complete config file |
| `test_config_parse_minimal` | Parse minimal config file |
| `test_config_merge_env` | Environment overrides config |
| `test_config_merge_cli` | CLI overrides environment |
| `test_config_validation_*` | Validate each field |
| `test_backend_type_serde` | BackendType serialization |

### Integration Tests

| Test | Description |
|------|-------------|
| `test_serve_starts_server` | Server binds to port |
| `test_serve_with_config` | Loads specified config |
| `test_backends_list_empty` | No backends shows empty table |
| `test_backends_list_json` | JSON output is valid |
| `test_backends_add_remove` | Add and remove lifecycle |
| `test_config_init_creates_file` | Creates valid template |
| `test_graceful_shutdown` | SIGINT triggers clean exit |

### CLI Output Tests

| Test | Description |
|------|-------------|
| `test_version_output` | `--version` shows version |
| `test_help_output` | `--help` shows all commands |
| `test_table_formatting` | Tables render correctly |
| `test_json_valid` | All JSON output is valid |

---

## Acceptance Criteria

- [ ] AC-01: `nexus serve` starts server with all options working
- [ ] AC-02: `nexus serve` works without any config file (zero-config mode)
- [ ] AC-03: `nexus backends` lists backends with pretty table
- [ ] AC-04: `nexus backends --json` outputs valid JSON
- [ ] AC-05: `nexus backends --status healthy` filters correctly
- [ ] AC-06: `nexus backends add <URL>` adds backend and triggers health check
- [ ] AC-07: `nexus backends remove <ID>` removes backend
- [ ] AC-08: `nexus models` lists all models with capabilities
- [ ] AC-09: `nexus models --json` outputs valid JSON
- [ ] AC-10: `nexus health` shows system status
- [ ] AC-11: `nexus health --json` outputs valid JSON
- [ ] AC-12: `nexus config init` generates valid template
- [ ] AC-13: Config file parses correctly
- [ ] AC-14: Environment variables override config
- [ ] AC-15: CLI arguments override environment and config
- [ ] AC-16: Graceful shutdown on SIGINT/SIGTERM
- [ ] AC-17: Exit code 1 on startup errors with helpful message
- [ ] AC-18: `nexus --version` and `nexus --help` work

---

## Dependencies

| Dependency | Reason |
|------------|--------|
| F02: Backend Registry | Required for `backends` and `models` commands |
| F03: Health Checker | Required for `health` command and `serve` |

---

## Design Decisions

### Decision 1: Shell Completion Generation

**Question**: Where should shell completion generation live?

**Decision**: Separate top-level command `nexus completions <shell>`

**Rationale**:
- Follows established patterns from `rustup`, `gh`, `kubectl`
- More discoverable than nested subcommand
- Cleaner separation of concerns

**Usage**:
```bash
# Bash
nexus completions bash > ~/.bash_completion.d/nexus

# Zsh
nexus completions zsh > ~/.zfunc/_nexus

# Fish
nexus completions fish > ~/.config/fish/completions/nexus.fish
```

---

### Decision 2: Config Validation Strictness

**Question**: How should unknown config keys be handled?

**Decision**: Warn on unknown keys, continue loading

**Rationale**:
- Catches typos without being overly strict
- Allows forward compatibility (old Nexus version with newer config)
- User sees the issue in logs but service still starts

**Behavior**:
```
$ nexus serve -c nexus.toml
WARN nexus::config: Unknown config key 'server.unknown_setting' - ignoring
INFO nexus: Nexus server starting on 0.0.0.0:8000
```

---

### Decision 3: Backend Auto-Type Detection

**Question**: Should `nexus backends add` auto-detect backend type?

**Decision**: Auto-detect with fallback to `generic`

**Rationale**:
- Zero-friction for common use cases (Ollama, vLLM)
- Fallback to `generic` prevents blocking on network issues
- User can always override with explicit `--type` flag

**Detection Order**:
1. Try `GET /api/tags` → If 200 with valid JSON: **Ollama**
2. Try `GET /health` → If 200 with `{"status": "ok"}`: **LlamaCpp**
3. Try `GET /v1/models` → If 200 with valid JSON: **OpenAI-compatible** (vLLM/Exo/Generic)
4. If all fail or timeout (2s): Default to **Generic**

**Override**: `nexus backends add http://... --type vllm`

---

## References

- [F02: Backend Registry Spec](../001-backend-registry/spec.md)
- [F03: Health Checker Spec](../002-health-checker/spec.md)
- [clap documentation](https://docs.rs/clap/latest/clap/)
- [config crate documentation](https://docs.rs/config/latest/config/)
