# Configuration Schema Contract

This document defines the full TOML configuration schema, environment variable overrides, validation rules, and defaults for Nexus.

**Source**: `src/config/mod.rs`, `src/config/server.rs`, `src/config/discovery.rs`, `src/config/routing.rs`, `src/config/backend.rs`, `src/config/logging.rs`, `nexus.example.toml`

---

## Configuration Precedence

Configuration values are resolved in this order (highest wins):

1. **CLI arguments** (highest priority)
2. **Environment variables** (`NEXUS_*`)
3. **Configuration file** (TOML)
4. **Default values** (lowest priority)

---

## Loading

```rust
NexusConfig::load(path: Option<&Path>) -> Result<Self, ConfigError>
```

- `Some(path)`: Load and parse TOML file. Returns `ConfigError::NotFound` if file doesn't exist.
- `None`: Returns `NexusConfig::default()`.

After loading, call `.with_env_overrides()` to apply environment variable overrides. Invalid env var values are silently ignored (defaults are kept).

After overrides, call `.validate()` to check for invalid values and circular aliases.

---

## Top-Level Structure

```rust
pub struct NexusConfig {
    pub server: ServerConfig,
    pub discovery: DiscoveryConfig,
    pub health_check: HealthCheckConfig,
    pub routing: RoutingConfig,
    pub backends: Vec<BackendConfig>,
    pub logging: LoggingConfig,
}
```

All sections use `#[serde(default)]`, so every section is optional in the TOML file.

---

## `[server]` Section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `host` | string | `"0.0.0.0"` | Bind address |
| `port` | u16 | `8000` | Listen port |
| `request_timeout_seconds` | u64 | `300` | HTTP client timeout for proxied requests |
| `max_concurrent_requests` | u32 | `1000` | Maximum concurrent requests |

**Validation**: `port` must be non-zero.

**TOML Example**:
```toml
[server]
host = "0.0.0.0"
port = 8000
request_timeout_seconds = 300
```

---

## `[discovery]` Section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable mDNS auto-discovery |
| `service_types` | string[] | `["_ollama._tcp.local", "_llm._tcp.local"]` | mDNS service types to browse |
| `grace_period_seconds` | u64 | `60` | Seconds before removing disappeared backends |

**TOML Example**:
```toml
[discovery]
enabled = true
service_types = ["_ollama._tcp.local", "_llm._tcp.local"]
grace_period_seconds = 60
```

---

## `[health_check]` Section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable background health checking |
| `interval_seconds` | u64 | `30` | Seconds between health check cycles |
| `timeout_seconds` | u64 | `5` | Timeout per health check request |
| `failure_threshold` | u32 | `3` | Consecutive failures to mark unhealthy |
| `recovery_threshold` | u32 | `2` | Consecutive successes to mark healthy |

**TOML Example**:
```toml
[health_check]
enabled = true
interval_seconds = 30
timeout_seconds = 5
```

---

## `[routing]` Section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `strategy` | string | `"smart"` | Routing strategy |
| `max_retries` | u32 | `2` | Maximum retry attempts per request |

**Strategy Values**: `"smart"` | `"round_robin"` | `"priority_only"` | `"random"`

### `[routing.weights]` Sub-Section

Weights for the Smart routing strategy's scoring algorithm. Values are relative (not percentages).

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `priority` | u32 | `50` | Weight for backend priority score |
| `load` | u32 | `30` | Weight for current load (pending requests) |
| `latency` | u32 | `20` | Weight for average latency |

### `[routing.aliases]` Sub-Section

Model name aliases mapping requested names to actual model names. Supports up to 3-level chaining.

**Type**: `HashMap<String, String>` — key is alias, value is target model.

**Validation**: Circular references are detected and rejected with `ConfigError::CircularAlias`.

**TOML Example**:
```toml
[routing.aliases]
"gpt-4" = "llama3:70b"
"gpt-4-turbo" = "llama3:70b"
"gpt-3.5-turbo" = "mistral:7b"
```

### `[routing.fallbacks]` Sub-Section

Fallback chains for model unavailability. When a primary model has no healthy backends, alternatives are tried in order.

**Type**: `HashMap<String, Vec<String>>` — key is primary model, value is ordered list of fallbacks.

**TOML Example**:
```toml
[routing.fallbacks]
"llama3:70b" = ["qwen2:72b", "mixtral:8x7b"]
```

**Full Routing TOML Example**:
```toml
[routing]
strategy = "smart"
max_retries = 2

[routing.weights]
priority = 50
load = 30
latency = 20

[routing.aliases]
"gpt-4" = "llama3:70b"
"gpt-4-turbo" = "llama3:70b"
"gpt-3.5-turbo" = "mistral:7b"

[routing.fallbacks]
"llama3:70b" = ["qwen2:72b", "mixtral:8x7b"]
```

---

## `[[backends]]` Section

Backend entries are defined as a TOML array of tables.

| Field | Type | Default | Required | Description |
|-------|------|---------|----------|-------------|
| `name` | string | — | Yes | Human-readable backend name |
| `url` | string | — | Yes | Base URL for API requests |
| `type` | string | — | Yes | Backend type |
| `priority` | i32 | `50` | No | Routing priority (lower = preferred) |
| `api_key_env` | string | `null` | No | Environment variable name for API key |

**Type Values**: `"ollama"` | `"vllm"` | `"llamacpp"` | `"exo"` | `"openai"` | `"lmstudio"` | `"generic"`

**Validation**:
- `name` must be non-empty
- `url` must be non-empty

**TOML Example**:
```toml
[[backends]]
name = "local-ollama"
url = "http://localhost:11434"
type = "ollama"
priority = 1

[[backends]]
name = "gpu-server"
url = "http://192.168.1.100:8000"
type = "vllm"
priority = 3

[[backends]]
name = "cloud-fallback"
url = "https://api.openai.com"
type = "openai"
priority = 100
api_key_env = "OPENAI_API_KEY"
```

---

## `[logging]` Section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `level` | string | `"info"` | Global log level |
| `format` | string | `"pretty"` | Log output format |
| `component_levels` | map | `null` | Component-specific log levels |
| `enable_content_logging` | bool | `false` | Log request/response content (privacy risk) |

**Level Values**: `"trace"` | `"debug"` | `"info"` | `"warn"` | `"error"`

**Format Values**: `"pretty"` (human-readable) | `"json"` (machine-parseable)

**TOML Example**:
```toml
[logging]
level = "info"
format = "pretty"

[logging.component_levels]
routing = "debug"
api = "info"
health = "warn"

# enable_content_logging = false
```

**Warning**: `enable_content_logging = true` will log request message content, which may include sensitive user data. Only use for local debugging.

---

## Environment Variable Overrides

| Environment Variable | Config Field | Type | Notes |
|---------------------|-------------|------|-------|
| `NEXUS_PORT` | `server.port` | u16 | Invalid values silently ignored |
| `NEXUS_HOST` | `server.host` | string | Any string accepted |
| `NEXUS_LOG_LEVEL` | `logging.level` | string | Any string accepted |
| `NEXUS_LOG_FORMAT` | `logging.format` | string | Must be `"pretty"` or `"json"`; invalid keeps default |
| `NEXUS_DISCOVERY` | `discovery.enabled` | bool | `"true"` (case-insensitive) enables; anything else disables |
| `NEXUS_HEALTH_CHECK` | `health_check.enabled` | bool | `"true"` (case-insensitive) enables; anything else disables |

---

## Validation Rules

Validation is performed by `NexusConfig::validate()`:

| Rule | Error Type | Message |
|------|-----------|---------|
| `server.port` must be non-zero | `ConfigError::Validation` | `"port must be non-zero"` |
| Each `backends[i].url` must be non-empty | `ConfigError::Validation` | `"URL cannot be empty"` |
| Each `backends[i].name` must be non-empty | `ConfigError::Validation` | `"name cannot be empty"` |
| No circular alias references | `ConfigError::CircularAlias` | `"'{start}' eventually points back to '{cycle}'"` |

### Error Types

```rust
pub enum ConfigError {
    Io(std::io::Error),
    NotFound(PathBuf),
    Parse(String),
    Validation { field: String, message: String },
    MissingField(String),
    CircularAlias { start: String, cycle: String },
}
```

---

## Complete Example Configuration

The complete example configuration is maintained in `nexus.example.toml` at the repository root. It includes all sections with documented defaults and commented-out optional backends.

---

## Implementation Notes

### Serde Defaults

All config sections use `#[serde(default)]`, meaning:
- Missing sections get struct defaults
- Missing fields within sections get field defaults
- An empty TOML file produces a fully valid `NexusConfig::default()`

### Type Conversions

- `config::RoutingStrategy` converts to `routing::RoutingStrategy` via `From` impl
- `config::RoutingWeights` converts to `routing::ScoringWeights` via `From` impl
- `config::BackendType` is a re-export of `registry::BackendType`

### Backend Priority

The `priority` field defaults to `50` via a custom `default_priority()` function (not the `i32` default of `0`). Lower values are preferred by the routing engine.

---

## Testing Strategy

### Unit Tests
1. Default configuration values for all sections
2. TOML parsing (minimal, full, backends array)
3. Load from file (valid, missing file)
4. Environment variable overrides (valid values, invalid values silently ignored)
5. Validation (zero port, empty URL, empty name, circular aliases, valid aliases)
6. Routing strategy serialization/deserialization
7. Log format `FromStr` parsing

### Integration Tests
1. Parse `nexus.example.toml` successfully
2. Round-trip serialization: config → TOML → config
3. Precedence: env vars override file values
