# Data Model: CLI & Configuration (F03)

**Date**: 2025-01-10  
**Phase**: Phase 1 - Foundation

This document defines the data entities and their relationships for the CLI & Configuration feature.

## Core Entities

### 1. NexusConfig

**Purpose**: Top-level aggregated configuration struct that unifies all Nexus settings from TOML files, environment variables, and defaults.

**Attributes**:

| Attribute | Type | Default | Constraints |
|-----------|------|---------|-------------|
| `server` | `ServerConfig` | See below | HTTP server settings |
| `discovery` | `DiscoveryConfig` | See below | mDNS discovery settings |
| `health_check` | `HealthCheckConfig` | See below | Health checker parameters |
| `routing` | `RoutingConfig` | See below | Routing strategy and weights |
| `backends` | `Vec<BackendConfig>` | `[]` | Static backend definitions |
| `logging` | `LoggingConfig` | See below | Log level and format |

**Responsibilities**:
- Load from TOML file via `load(path)`
- Apply environment variable overrides via `with_env_overrides()`
- Validate all sections via `validate()`
- Provide defaults for all fields via `#[serde(default)]`

**Lifecycle**: Created at startup. Shared via `Arc<NexusConfig>` in `AppState`. Immutable after creation (no hot-reload).

**Thread Safety**: Shared via `Arc`. No interior mutability.

---

### 2. ServerConfig

**Purpose**: HTTP server binding and request handling configuration.

**Attributes**:

| Attribute | Type | Default | Constraints |
|-----------|------|---------|-------------|
| `host` | `String` | `"0.0.0.0"` | Valid IP address or hostname |
| `port` | `u16` | `8000` | Must be non-zero (validated) |
| `request_timeout_seconds` | `u64` | `300` | HTTP client timeout for backend requests |
| `max_concurrent_requests` | `u32` | `1000` | Concurrency limit (not enforced at runtime currently) |

**TOML Section**: `[server]`

---

### 3. DiscoveryConfig

**Purpose**: mDNS auto-discovery settings.

**Attributes**:

| Attribute | Type | Default | Constraints |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `true` | Overridden by `NEXUS_DISCOVERY` env or `--no-discovery` CLI |
| `service_types` | `Vec<String>` | `["_ollama._tcp.local", "_llm._tcp.local"]` | mDNS service types to browse |
| `grace_period_seconds` | `u64` | `60` | Time before removing a disappeared mDNS backend |

**TOML Section**: `[discovery]`

---

### 4. HealthCheckConfig

**Purpose**: Backend health check timing and threshold configuration.

**Attributes**:

| Attribute | Type | Default | Constraints |
|-----------|------|---------|-------------|
| `enabled` | `bool` | `true` | Overridden by `NEXUS_HEALTH_CHECK` env or `--no-health-check` CLI |
| `interval_seconds` | `u64` | `30` | Seconds between check cycles |
| `timeout_seconds` | `u64` | `5` | Per-request timeout |
| `failure_threshold` | `u32` | `3` | Consecutive failures → Unhealthy |
| `recovery_threshold` | `u32` | `2` | Consecutive successes → Healthy |

**TOML Section**: `[health_check]`

**Note**: Defined in `src/health/config.rs` but re-exported from `src/config/mod.rs`.

---

### 5. RoutingConfig

**Purpose**: Backend selection strategy, retry behavior, and model aliasing.

**Attributes**:

| Attribute | Type | Default | Constraints |
|-----------|------|---------|-------------|
| `strategy` | `RoutingStrategy` | `Smart` | Enum: Smart, RoundRobin, PriorityOnly, Random |
| `max_retries` | `u32` | `2` | Retry count on backend failure |
| `weights` | `RoutingWeights` | See below | Factor weights for Smart strategy |
| `aliases` | `HashMap<String, String>` | `{}` | Model name aliases; validated for circular refs |
| `fallbacks` | `HashMap<String, Vec<String>>` | `{}` | Ordered fallback chains per model |

**TOML Section**: `[routing]`

---

### 6. RoutingWeights

**Purpose**: Weight factors for the Smart routing strategy's scoring function.

**Attributes**:

| Attribute | Type | Default | Constraints |
|-----------|------|---------|-------------|
| `priority` | `u32` | `50` | Weight for backend priority score |
| `load` | `u32` | `30` | Weight for current load score |
| `latency` | `u32` | `20` | Weight for latency EMA score |

**TOML Section**: `[routing.weights]`

**Conversion**: Implements `From<RoutingWeights>` for `routing::ScoringWeights`.

---

### 7. RoutingStrategy (Config Enum)

**Purpose**: Selects the backend routing algorithm.

**Variants**:

| Variant | Serde Name | Description |
|---------|------------|-------------|
| `Smart` | `smart` | Multi-factor scoring (default) |
| `RoundRobin` | `round_robin` | Cycle through backends |
| `PriorityOnly` | `priority_only` | Lowest priority value wins |
| `Random` | `random` | Random selection |

**Conversion**: Implements `From<config::RoutingStrategy>` for `routing::RoutingStrategy`.

---

### 8. BackendConfig

**Purpose**: Static backend definition from TOML configuration file.

**Attributes**:

| Attribute | Type | Default | Constraints |
|-----------|------|---------|-------------|
| `name` | `String` | — | Required, non-empty (validated) |
| `url` | `String` | — | Required, non-empty (validated) |
| `backend_type` | `BackendType` | — | Required; serialized as `"type"` in TOML |
| `priority` | `i32` | `50` | Lower = higher priority |
| `api_key_env` | `Option<String>` | `None` | Environment variable name for API key |

**TOML Section**: `[[backends]]` (array of tables)

---

### 9. LoggingConfig

**Purpose**: Logging level and format configuration.

**Attributes**:

| Attribute | Type | Default | Constraints |
|-----------|------|---------|-------------|
| `level` | `String` | `"info"` | Standard tracing levels: trace, debug, info, warn, error |
| `format` | `LogFormat` | `Pretty` | Enum: Pretty, Json |
| `component_levels` | `Option<HashMap<String, String>>` | `None` | Per-component overrides (e.g., `{"routing": "debug"}`) |
| `enable_content_logging` | `bool` | `false` | Opt-in sensitive content logging |

**TOML Section**: `[logging]`

---

### 10. LogFormat (Enum)

**Purpose**: Output format for structured logging.

**Variants**: `Pretty` (human-readable), `Json` (machine-parseable).

**Implements**: `FromStr` for environment variable parsing. Case-insensitive (`"json"`, `"JSON"` both valid).

---

### 11. ConfigError (Enum)

**Purpose**: Error types for configuration loading and validation.

**Variants**:

| Variant | Fields | Trigger |
|---------|--------|---------|
| `Io` | `std::io::Error` | File read failure |
| `NotFound` | `PathBuf` | Config file path doesn't exist |
| `Parse` | `String` | Invalid TOML syntax |
| `Validation` | `field: String, message: String` | Semantic validation failure |
| `MissingField` | `String` | Required field absent |
| `CircularAlias` | `start: String, cycle: String` | Alias chain forms a loop |

---

### 12. Cli (Clap)

**Purpose**: Top-level CLI argument parser using clap derive macros.

**Commands**:

| Command | Args Struct | Description |
|---------|-------------|-------------|
| `serve` | `ServeArgs` | Start the Nexus server |
| `backends list` | `BackendsListArgs` | List backends with optional filters |
| `backends add` | `BackendsAddArgs` | Add a backend by URL |
| `backends remove` | `BackendsRemoveArgs` | Remove a backend by name |
| `models` | `ModelsArgs` | List available models |
| `health` | `HealthArgs` | Show system health |
| `config init` | `ConfigInitArgs` | Generate config template |
| `completions` | `CompletionsArgs` | Generate shell completions |

---

### 13. ServeArgs

**Purpose**: CLI arguments for the `serve` command.

**Attributes**:

| Attribute | Flag | Default | Env Var |
|-----------|------|---------|---------|
| `config` | `-c, --config` | `nexus.toml` | — |
| `port` | `-p, --port` | `None` | `NEXUS_PORT` |
| `host` | `-H, --host` | `None` | `NEXUS_HOST` |
| `log_level` | `-l, --log-level` | `None` | `NEXUS_LOG_LEVEL` |
| `no_discovery` | `--no-discovery` | `false` | — |
| `no_health_check` | `--no-health-check` | `false` | — |

---

## Entity Relationships

```
┌──────────────────────┐
│       Cli (clap)     │
│                      │
│  command: Commands   │
│    ├── Serve ────────┼──► ServeArgs
│    ├── Backends      │       │
│    │   ├── List      │       │ loads
│    │   ├── Add       │       ▼
│    │   └── Remove    │  ┌──────────────┐
│    ├── Models        │  │  NexusConfig │
│    ├── Health        │  │              │
│    ├── Config Init   │  │  server ─────┼──► ServerConfig
│    └── Completions   │  │  discovery ──┼──► DiscoveryConfig
└──────────────────────┘  │  health_check┼──► HealthCheckConfig
                          │  routing ────┼──► RoutingConfig
                          │    ├ strategy│      ├── RoutingStrategy
                          │    ├ weights │      ├── RoutingWeights
                          │    ├ aliases │      ├── HashMap<String,String>
                          │    └ fallbacks      └── HashMap<String,Vec<String>>
                          │  backends ───┼──► Vec<BackendConfig>
                          │  logging ────┼──► LoggingConfig
                          └──────────────┘        └── LogFormat
```

---

## State Transitions

### Configuration Loading Pipeline

```
1. CLI parsed (clap)
       ↓
2. NexusConfig::load(path)
   ├── path is None → NexusConfig::default()
   ├── path not found → ConfigError::NotFound
   └── path exists → read file → toml::from_str()
       └── parse error → ConfigError::Parse
       ↓
3. config.with_env_overrides()
   ├── NEXUS_PORT → server.port
   ├── NEXUS_HOST → server.host
   ├── NEXUS_LOG_LEVEL → logging.level
   ├── NEXUS_LOG_FORMAT → logging.format
   ├── NEXUS_DISCOVERY → discovery.enabled
   └── NEXUS_HEALTH_CHECK → health_check.enabled
   (Invalid values silently ignored — defaults kept)
       ↓
4. CLI args override (in serve handler)
   ├── --port → server.port
   ├── --host → server.host
   ├── --log-level → logging.level
   ├── --no-discovery → discovery.enabled = false
   └── --no-health-check → health_check.enabled = false
       ↓
5. config.validate()
   ├── server.port ≠ 0
   ├── backends[*].url non-empty
   ├── backends[*].name non-empty
   └── aliases: no circular references
       ↓
6. Arc::new(config) → shared in AppState
```

### Configuration Precedence (highest → lowest)

```
CLI args  >  ENV vars  >  Config file  >  Defaults
```

---

## Validation & Constraints

### Port Validation

**Rule**: `server.port` must be non-zero.

**Error**: `ConfigError::Validation { field: "server.port", message: "port must be non-zero" }`

---

### Backend Name/URL Validation

**Rule**: Each backend must have non-empty `name` and `url`.

**Error**: `ConfigError::Validation { field: "backends[i].url|name", message: "... cannot be empty" }`

---

### Circular Alias Detection

**Rule**: Model aliases must not form cycles.

**Algorithm**: For each alias key, follow the chain tracking visited nodes. If a visited node is encountered again, report `CircularAlias` error.

**Implementation** (in `routing::validate_aliases`):
```rust
for start in aliases.keys() {
    let mut current = start;
    let mut visited = HashSet::new();
    visited.insert(start);
    while let Some(target) = aliases.get(current) {
        if visited.contains(target) {
            return Err(CircularAlias { start, cycle: target });
        }
        visited.insert(target);
        current = target;
    }
}
```

**Covers**: Self-referential (`a→a`), two-way (`a→b→a`), and multi-hop cycles (`a→b→c→a`).

---

### Environment Variable Handling

**Rule**: Invalid environment variable values are silently ignored (default is preserved).

**Examples**:
- `NEXUS_PORT=not-a-number` → keeps default 8000
- `NEXUS_LOG_FORMAT=xml` → keeps default Pretty

---

## Performance Characteristics

| Operation | Target Latency | Implementation |
|-----------|----------------|----------------|
| Config file read + parse | < 10ms | `fs::read_to_string` + `toml::from_str` |
| Env override application | < 1µs | 6 `std::env::var` lookups |
| Validation | < 100µs | Linear scan of backends + alias cycle detection |
| Alias cycle detection | O(A²) worst case | A = number of aliases; typically < 50 |
| CLI argument parsing | < 1ms | clap derive macros |

**Memory**: `NexusConfig` ~1 KB base + ~200 bytes per backend definition. Negligible for typical configurations.

---

## Future Extensions

### Not in Current Scope

1. **Hot-reload**: Config changes require server restart
2. **Config validation CLI**: `nexus config validate` command
3. **Remote config**: Loading from HTTP endpoints or key-value stores
4. **Per-backend timeout**: Currently global `request_timeout_seconds`
5. **Config migration**: Version-aware config schema upgrades
6. **Secret management**: API keys from vault/secret manager (currently env vars only)

These are mentioned for awareness but are NOT part of F03 implementation.
