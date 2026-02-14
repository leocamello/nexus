# Research: CLI & Configuration (F03)

**Date**: 2026-02-03
**Phase**: Implemented (v0.1)

This document captures the technical decisions made during implementation of the CLI and configuration system — the user-facing interface and layered config loading for Nexus.

## Research Questions & Findings

### 1. Configuration Format: TOML

**Question**: Which configuration file format should Nexus use?

**Decision**: TOML, with the `toml` crate (v0.8) for parsing and serde for deserialization.

**Rationale**:
- TOML is the standard for Rust projects (Cargo uses it) — users expect it
- Human-readable and hand-editable, which aligns with the zero-config philosophy (when config is needed, it should be obvious)
- Native table syntax maps directly to Rust structs via serde `#[derive(Deserialize)]`
- Strong typing: numbers stay numbers, booleans stay booleans — no YAML "Norway problem"
- Array-of-tables (`[[backends]]`) maps cleanly to `Vec<BackendConfig>`

**Alternatives Considered**:
- YAML: Rejected because of implicit type coercion (bare `yes`/`no` become booleans, `3.10` becomes float). YAML's complexity is unnecessary for Nexus's flat config structure.
- JSON: Rejected because it doesn't support comments. Configuration files are documentation — comments are essential for explaining default values and options.
- RON (Rust Object Notation): Rejected because it's Rust-ecosystem only. Non-Rust developers interacting with Nexus config would find it unfamiliar.

**Implementation**:
```rust
// Direct deserialization with serde
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NexusConfig {
    pub server: ServerConfig,
    pub discovery: DiscoveryConfig,
    pub health_check: HealthCheckConfig,
    pub routing: RoutingConfig,
    pub backends: Vec<BackendConfig>,
    pub logging: LoggingConfig,
}

impl NexusConfig {
    pub fn load(path: Option<&Path>) -> Result<Self, ConfigError> {
        match path {
            Some(p) => {
                let content = std::fs::read_to_string(p)?;
                toml::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))
            }
            None => Ok(Self::default()),
        }
    }
}
```

---

### 2. Configuration Precedence Chain

**Question**: How should configuration from multiple sources be merged?

**Decision**: Four-layer precedence: CLI arguments > environment variables > config file > compiled defaults.

**Rationale**:
- CLI args (highest priority) enable quick overrides without editing files: `nexus serve -p 9000`
- Env vars (`NEXUS_*`) support container deployments where file mounting is inconvenient
- Config file provides the persistent, shareable configuration
- Compiled defaults ensure Nexus starts with zero configuration — the zero-config promise
- Each layer is applied sequentially in `load_config_with_overrides()`, with later layers overwriting earlier values

**Alternatives Considered**:
- Config crate's layered builder (used `config` v0.14 in deps): Rejected for the main config flow because it adds complexity for a simple override chain. The `config` crate is included in dependencies but the actual config loading uses direct TOML parsing for clarity and control.
- Single-source config (file only): Rejected because it breaks container deployment workflows where environment variables are the standard configuration mechanism.
- Merge at the struct level (deep merge): Rejected because TOML tables don't have a natural "unset" sentinel. Shallow overrides per field are simpler and sufficient.

**Implementation**:
```rust
pub fn load_config_with_overrides(args: &ServeArgs) -> Result<NexusConfig, Box<dyn Error>> {
    // 1. Load from file (or defaults if file doesn't exist)
    let mut config = if args.config.exists() {
        NexusConfig::load(Some(&args.config))?
    } else {
        NexusConfig::default()
    };

    // 2. Apply env overrides
    config = config.with_env_overrides();

    // 3. Apply CLI overrides (highest priority)
    if let Some(port) = args.port { config.server.port = port; }
    if let Some(ref host) = args.host { config.server.host = host.clone(); }
    if args.no_discovery { config.discovery.enabled = false; }

    Ok(config)
}
```

**References**:
- LEARNINGS.md: "Configuration Precedence Pattern — Layered configuration works well: CLI > Env > File > Defaults"

---

### 3. Clap Derive vs Builder API

**Question**: Should we use clap's derive macros or the builder pattern for CLI parsing?

**Decision**: Derive macros (`#[derive(Parser)]`, `#[derive(Subcommand)]`, `#[derive(Args)]`).

**Rationale**:
- Derive macros co-locate argument definitions with their types — the struct IS the documentation
- Compile-time validation catches missing fields and type mismatches
- Less boilerplate than builder pattern: 3 lines of attributes vs 10+ lines of builder calls
- `#[arg(env = "NEXUS_PORT")]` integrates env var fallback directly into the argument definition
- Subcommand hierarchy is expressed naturally via enum variants

**Alternatives Considered**:
- Clap builder API (`Command::new().arg()...`): Rejected because it separates argument definitions from their types. The builder pattern is more flexible but the derive approach covers 100% of Nexus's needs.
- `structopt` (predecessor to clap derive): Rejected because it's deprecated in favor of clap's built-in derive feature since clap v3.
- `argh` (Google's CLI parser): Rejected because it has fewer features (no env var integration, limited shell completions) and a smaller community.

**Implementation**:
```rust
#[derive(Parser, Debug)]
#[command(name = "nexus", version, about = "Distributed LLM model serving orchestrator")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Serve(ServeArgs),
    #[command(subcommand)]
    Backends(BackendsCommands),
    Models(ModelsArgs),
    Health(HealthArgs),
    #[command(subcommand)]
    Config(ConfigCommands),
    Completions(CompletionsArgs),
}

#[derive(Args, Debug)]
pub struct ServeArgs {
    #[arg(short, long, default_value = "nexus.toml")]
    pub config: PathBuf,
    #[arg(short, long, env = "NEXUS_PORT")]
    pub port: Option<u16>,
    // ...
}
```

---

### 4. Environment Variable Prefix and Override Strategy

**Question**: How should environment variables map to configuration fields?

**Decision**: `NEXUS_*` prefix with flat naming. Invalid values are silently ignored (defaults kept).

**Rationale**:
- `NEXUS_` prefix prevents collisions with other tools' env vars
- Flat naming (`NEXUS_PORT`, `NEXUS_HOST`, `NEXUS_LOG_LEVEL`) is simpler than nested (`NEXUS_SERVER_PORT`)
- Silent ignore on invalid values (e.g., `NEXUS_PORT=abc`) prevents crashes in container environments where env vars might be misconfigured — the default value is always safe
- Only commonly overridden fields are exposed: port, host, log level, log format, discovery, health check

**Alternatives Considered**:
- Nested env vars (`NEXUS_SERVER_PORT`): Rejected because container orchestrators often have length limits on env var names, and nested naming adds cognitive overhead for the most common use case (port override).
- Strict parsing (error on invalid env var values): Rejected because it would cause startup failures in environments where another process sets an invalid `NEXUS_PORT`. Silent fallback to defaults is safer.
- `envy` crate for automatic env deserialization: Rejected because it would require either flat config (losing TOML's nested structure) or a separate env-specific struct. Manual `std::env::var()` is simple enough for 6 fields.

**Implementation**:
```rust
pub fn with_env_overrides(mut self) -> Self {
    if let Ok(port) = std::env::var("NEXUS_PORT") {
        if let Ok(p) = port.parse() {
            self.server.port = p;
        }
        // Invalid parse silently keeps default
    }
    if let Ok(host) = std::env::var("NEXUS_HOST") {
        self.server.host = host;
    }
    // ...
    self
}
```

---

### 5. Example Config via include_str!

**Question**: How should `nexus config init` generate a configuration file?

**Decision**: Embed `nexus.example.toml` at compile time via `include_str!()` and write it verbatim.

**Rationale**:
- The example config is version-controlled and tested (`test_config_parse_full_toml` parses it in CI)
- `include_str!()` embeds the file in the binary — no runtime file dependency
- Comments in the TOML file serve as inline documentation for every config option
- Single source of truth: the same file is the example config, the test fixture, and the `config init` template

**Alternatives Considered**:
- Generate TOML programmatically from `NexusConfig::default()` + serde: Rejected because serde serialization strips comments. Comments are the most valuable part of an example config file.
- Ship example config as a separate file alongside the binary: Rejected because it violates the single-binary deployment model. Users would need to know where to find the companion file.
- Hardcoded string in code: Rejected because it would duplicate the example config and drift out of sync. `include_str!()` keeps the source file as the single authority.

**Implementation**:
```rust
const EXAMPLE_CONFIG: &str = include_str!("../../nexus.example.toml");

pub fn handle_config_init(args: &ConfigInitArgs) -> Result<(), Box<dyn Error>> {
    if args.output.exists() && !args.force {
        return Err(format!("File already exists: {}. Use --force to overwrite.",
            args.output.display()).into());
    }
    fs::write(&args.output, EXAMPLE_CONFIG)?;
    println!("✓ Configuration file created: {}", args.output.display());
    Ok(())
}
```

---

### 6. Configuration Validation Strategy

**Question**: When and how should we validate the configuration?

**Decision**: Explicit `validate()` method called after loading and applying overrides, before starting the server.

**Rationale**:
- Fail-fast: invalid configuration should prevent server startup, not cause runtime errors
- Validation is separate from parsing — TOML can be syntactically valid but semantically wrong (port=0, empty URL, circular aliases)
- Circular alias detection uses a visited-set traversal — O(n) per chain, runs once at startup
- `ConfigError` variants provide field-specific error messages for actionable debugging

**Alternatives Considered**:
- Validate during deserialization (custom serde deserializer): Rejected because it mixes parsing and validation concerns. A port of 0 is a valid u16 but an invalid configuration — this distinction belongs in validation, not parsing.
- Validate on first use (lazy): Rejected because errors would surface at unpredictable times. A circular alias would only be detected when a request first uses that alias path — possibly hours after startup.
- `validator` crate with derive macros: Rejected because the validations are heterogeneous (range checks, cross-field references for aliases, non-empty strings). Custom validation is clearer than attribute-driven rules.

**Implementation**:
```rust
pub fn validate(&self) -> Result<(), ConfigError> {
    if self.server.port == 0 {
        return Err(ConfigError::Validation {
            field: "server.port".to_string(),
            message: "port must be non-zero".to_string(),
        });
    }
    for (i, backend) in self.backends.iter().enumerate() {
        if backend.url.is_empty() {
            return Err(ConfigError::Validation {
                field: format!("backends[{}].url", i),
                message: "URL cannot be empty".to_string(),
            });
        }
    }
    routing::validate_aliases(&self.routing.aliases)?;
    Ok(())
}
```

---

### 7. Shell Completions

**Question**: How should we support shell tab-completion?

**Decision**: Use `clap_complete` (v4) to generate completions at runtime via `nexus completions <shell>`.

**Rationale**:
- `clap_complete` generates completions from the same `Cli` struct used for parsing — always in sync
- Supports bash, zsh, fish, elvish, and PowerShell
- Runtime generation (not build-time) means users get completions matching their exact binary version
- No build-time dependency on specific shells

**Implementation**:
```rust
#[derive(Args, Debug)]
pub struct CompletionsArgs {
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,
}
```

---

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Config file format breaking changes between versions | High | TOML's `#[serde(default)]` on every config struct ensures new fields get defaults. Old config files remain valid. |
| Env var name collisions with other tools | Low | `NEXUS_` prefix is unlikely to collide. Docker/K8s namespacing further reduces risk. |
| `include_str!` stale if example config is edited but not recompiled | Low | CI runs `cargo test` which includes `test_config_parse_full_toml`, ensuring the example file is always parseable. |
| Silent env var ignore hides operator mistakes | Medium | Logging at `debug` level when env vars are applied. Operators can run with `NEXUS_LOG_LEVEL=debug` to see which overrides took effect. |

---

## References

- [TOML specification](https://toml.io/en/)
- [clap derive documentation](https://docs.rs/clap/4/clap/_derive/index.html)
- [clap_complete documentation](https://docs.rs/clap_complete/4/clap_complete/)
- [toml crate documentation](https://docs.rs/toml/0.8/toml/)
- LEARNINGS.md: "Configuration Precedence Pattern"
