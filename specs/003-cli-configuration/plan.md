# Implementation Plan: CLI and Configuration

**Spec**: [spec.md](./spec.md)  
**Status**: Ready for Implementation  
**Estimated Complexity**: Medium-High

## Approach

Implement the CLI using clap's derive macros and layered configuration using the `config` crate. Follow TDD: write failing tests first, then implement to make them pass.

### Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| CLI framework | `clap` with derive | Declarative, type-safe, built-in env support |
| Config loading | `config` crate | Native layered merging (file + env + defaults) |
| Config format | TOML only | Consistent with Rust ecosystem, readable |
| Table output | `comfy-table` | Terminal-aware, Unicode borders, colors |
| CLI structure | Subcommands | `nexus <cmd>` pattern, extensible |
| Config struct | Single `NexusConfig` | One source of truth, shared across modules |

### File Structure

```
src/
├── main.rs                 # Entry point with clap parsing
├── lib.rs                  # Library root, re-exports modules
├── cli/
│   ├── mod.rs              # CLI command definitions (clap)
│   ├── serve.rs            # serve command implementation
│   ├── backends.rs         # backends subcommand handlers
│   ├── models.rs           # models command handler
│   ├── health.rs           # health command handler
│   └── output.rs           # Table/JSON output helpers
├── config/
│   ├── mod.rs              # NexusConfig struct, loading logic
│   ├── server.rs           # ServerConfig
│   ├── routing.rs          # RoutingConfig, RoutingWeights
│   ├── logging.rs          # LoggingConfig
│   └── error.rs            # ConfigError enum
├── health/                 # (existing)
└── registry/               # (existing)
```

### Dependencies

**Already in Cargo.toml:**
- `clap = { version = "4", features = ["derive", "env"] }` ✓
- `toml = "0.8"` ✓
- `serde = { features = ["derive"] }` ✓
- `tracing` ✓
- `tracing-subscriber = { features = ["env-filter", "json"] }` ✓

**New dependencies needed:**
```toml
# Pretty table output
comfy-table = "7"

# Layered configuration
config = { version = "0.14", default-features = false, features = ["toml"] }

# Terminal colors (optional, for status indicators)
colored = "2"
```

**New dev-dependencies:**
```toml
# CLI integration testing
assert_cmd = "2"       # Already present ✓
predicates = "3"       # Already present ✓
tempfile = "3"         # For config file tests
```

---

## Implementation Phases

### Phase 1: Configuration Module (Tests First)

**Goal**: Create the unified config system with layered loading.

**Tests to write first** (10 tests):
1. `test_config_default_values` - All defaults are sensible
2. `test_config_parse_minimal_toml` - Parse file with only `[server]`
3. `test_config_parse_full_toml` - Parse complete config file
4. `test_config_parse_backends_array` - Parse `[[backends]]` array
5. `test_config_env_override_port` - NEXUS_PORT overrides config
6. `test_config_env_override_log_level` - NEXUS_LOG_LEVEL overrides config
7. `test_config_missing_file_uses_defaults` - No file → defaults work
8. `test_config_invalid_toml_error` - Parse error with line number
9. `test_config_invalid_backend_type_error` - Validation error message
10. `test_routing_strategy_serde` - RoutingStrategy serialization

**Implementation**:

1. Create `src/config/mod.rs`:
   ```rust
   pub struct NexusConfig {
       pub server: ServerConfig,
       pub discovery: DiscoveryConfig,
       pub health_check: HealthCheckConfig,
       pub routing: RoutingConfig,
       pub backends: Vec<BackendConfig>,
       pub logging: LoggingConfig,
   }
   
   impl NexusConfig {
       pub fn load(path: Option<&Path>) -> Result<Self, ConfigError>;
       pub fn with_overrides(self, overrides: ConfigOverrides) -> Self;
   }
   ```

2. Create `src/config/server.rs`:
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(default)]
   pub struct ServerConfig {
       pub host: String,           // "0.0.0.0"
       pub port: u16,              // 8000
       pub request_timeout_seconds: u64,  // 300
       pub max_concurrent_requests: u32,  // 1000
   }
   ```

3. Create `src/config/routing.rs`:
   ```rust
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
   #[serde(rename_all = "snake_case")]
   pub enum RoutingStrategy {
       Smart,
       RoundRobin,
       PriorityOnly,
       Random,
   }
   ```

4. Create `src/config/logging.rs`:
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(default)]
   pub struct LoggingConfig {
       pub level: String,      // "info"
       pub format: LogFormat,  // Pretty
   }
   
   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(rename_all = "snake_case")]
   pub enum LogFormat {
       Pretty,
       Json,
   }
   ```

5. Create `src/config/error.rs`:
   ```rust
   #[derive(Debug, thiserror::Error)]
   pub enum ConfigError {
       #[error("config file not found: {0}")]
       NotFound(PathBuf),
       
       #[error("config parse error: {0}")]
       ParseError(String),
       
       #[error("invalid value for '{field}': {message}")]
       ValidationError { field: String, message: String },
   }
   ```

**Acceptance**: All 10 tests pass.

---

### Phase 2: CLI Command Definitions (Tests First)

**Goal**: Define the CLI structure with clap derive.

**Tests to write first** (8 tests):
1. `test_cli_parse_serve_defaults` - `nexus serve` parses with defaults
2. `test_cli_parse_serve_with_port` - `nexus serve -p 9000` parses port
3. `test_cli_parse_serve_with_config` - `nexus serve -c custom.toml` parses path
4. `test_cli_parse_backends_list` - `nexus backends` parses
5. `test_cli_parse_backends_json` - `nexus backends --json` sets flag
6. `test_cli_parse_backends_add` - `nexus backends add http://...` parses URL
7. `test_cli_parse_models` - `nexus models` parses
8. `test_cli_parse_health` - `nexus health` parses

**Implementation**:

1. Create `src/cli/mod.rs`:
   ```rust
   use clap::{Parser, Subcommand, Args};
   
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
       #[command(subcommand)]
       Backends(BackendsCommands),
       /// List available models
       Models(ModelsArgs),
       /// Show system health
       Health(HealthArgs),
       /// Configuration utilities
       #[command(subcommand)]
       Config(ConfigCommands),
   }
   ```

2. Define `ServeArgs`:
   ```rust
   #[derive(Args)]
   pub struct ServeArgs {
       /// Config file path
       #[arg(short, long, default_value = "nexus.toml")]
       pub config: PathBuf,
       
       /// Listen port
       #[arg(short, long, env = "NEXUS_PORT")]
       pub port: Option<u16>,
       
       /// Listen address
       #[arg(short = 'H', long, env = "NEXUS_HOST")]
       pub host: Option<String>,
       
       /// Log level: trace, debug, info, warn, error
       #[arg(short, long, env = "NEXUS_LOG_LEVEL")]
       pub log_level: Option<String>,
       
       /// Disable mDNS discovery
       #[arg(long)]
       pub no_discovery: bool,
       
       /// Disable health checking
       #[arg(long)]
       pub no_health_check: bool,
   }
   ```

3. Define `BackendsCommands`:
   ```rust
   #[derive(Subcommand)]
   pub enum BackendsCommands {
       /// List all backends
       List(BackendsListArgs),
       /// Add a backend
       Add(BackendsAddArgs),
       /// Remove a backend
       Remove(BackendsRemoveArgs),
   }
   
   #[derive(Args)]
   pub struct BackendsListArgs {
       /// Output as JSON
       #[arg(long)]
       pub json: bool,
       
       /// Filter by status
       #[arg(long)]
       pub status: Option<String>,
   }
   ```

4. Update `src/main.rs`:
   ```rust
   use clap::Parser;
   use nexus::cli::{Cli, Commands};
   
   fn main() {
       let cli = Cli::parse();
       
       match cli.command {
           Commands::Serve(args) => todo!(),
           Commands::Backends(cmd) => todo!(),
           Commands::Models(args) => todo!(),
           Commands::Health(args) => todo!(),
           Commands::Config(cmd) => todo!(),
       }
   }
   ```

**Acceptance**: All 8 tests pass, `nexus --help` shows all commands.

---

### Phase 3: Output Formatting (Tests First)

**Goal**: Create table and JSON output helpers.

**Tests to write first** (6 tests):
1. `test_format_backends_table` - Backends render as table
2. `test_format_backends_json` - Backends render as JSON
3. `test_format_models_table` - Models render as table
4. `test_format_models_json` - Models render as JSON
5. `test_format_health_pretty` - Health renders with status icons
6. `test_format_health_json` - Health renders as JSON

**Implementation**:

1. Create `src/cli/output.rs`:
   ```rust
   use comfy_table::{Table, Cell, Color, Attribute};
   
   pub fn print_backends_table(backends: &[BackendView]) {
       let mut table = Table::new();
       table.set_header(vec!["Name", "URL", "Type", "Status", "Models", "Latency"]);
       
       for b in backends {
           let status_cell = match b.status {
               BackendStatus::Healthy => Cell::new("Healthy").fg(Color::Green),
               BackendStatus::Unhealthy => Cell::new("Unhealthy").fg(Color::Red),
               BackendStatus::Unknown => Cell::new("Unknown").fg(Color::Yellow),
           };
           
           table.add_row(vec![
               Cell::new(&b.name),
               Cell::new(&b.url),
               Cell::new(format!("{:?}", b.backend_type)),
               status_cell,
               Cell::new(b.models.len()),
               Cell::new(format!("{}ms", b.avg_latency_ms)),
           ]);
       }
       
       println!("{table}");
   }
   
   pub fn print_json<T: Serialize>(value: &T) -> Result<(), serde_json::Error> {
       println!("{}", serde_json::to_string_pretty(value)?);
       Ok(())
   }
   ```

2. Create status icon helpers:
   ```rust
   pub fn status_icon(status: BackendStatus) -> &'static str {
       match status {
           BackendStatus::Healthy => "✓",
           BackendStatus::Unhealthy => "✗",
           BackendStatus::Unknown => "?",
       }
   }
   ```

**Acceptance**: All 6 tests pass, output is visually correct.

---

### Phase 4: Serve Command (Tests First)

**Goal**: Implement the main `serve` command that starts the server.

**Tests to write first** (8 tests):
1. `test_serve_starts_on_default_port` - Server binds to 8000
2. `test_serve_respects_port_arg` - `--port 9000` uses 9000
3. `test_serve_respects_config_file` - Loads specified config
4. `test_serve_works_without_config` - Zero-config mode works
5. `test_serve_initializes_health_checker` - Health checker starts
6. `test_serve_graceful_shutdown` - SIGINT triggers clean exit
7. `test_serve_logs_startup_info` - Logs host:port on startup
8. `test_serve_exits_on_port_conflict` - Exit code 1 if port in use

**Implementation**:

1. Create `src/cli/serve.rs`:
   ```rust
   pub async fn run_serve(args: ServeArgs) -> Result<(), Box<dyn std::error::Error>> {
       // 1. Load configuration
       let config = load_config_with_overrides(&args)?;
       
       // 2. Initialize tracing
       init_tracing(&config.logging)?;
       
       // 3. Create registry
       let registry = Arc::new(Registry::new());
       
       // 4. Load static backends from config
       for backend_config in &config.backends {
           let backend = Backend::from_config(backend_config);
           registry.add_backend(backend)?;
       }
       
       // 5. Start health checker
       let cancel_token = CancellationToken::new();
       let health_handle = if config.health_check.enabled && !args.no_health_check {
           let checker = HealthChecker::new(registry.clone(), config.health_check.clone());
           Some(checker.start(cancel_token.clone()))
       } else {
           None
       };
       
       // 6. Build HTTP server (placeholder until API Gateway is implemented)
       let app = Router::new()
           .route("/health", get(health_handler));
       
       let addr = format!("{}:{}", config.server.host, config.server.port);
       tracing::info!("Nexus listening on {}", addr);
       
       // 7. Run server with graceful shutdown
       let listener = TcpListener::bind(&addr).await?;
       axum::serve(listener, app)
           .with_graceful_shutdown(shutdown_signal(cancel_token.clone()))
           .await?;
       
       // 8. Wait for health checker to finish
       if let Some(handle) = health_handle {
           handle.await?;
       }
       
       Ok(())
   }
   
   async fn shutdown_signal(cancel_token: CancellationToken) {
       tokio::signal::ctrl_c().await.ok();
       tracing::info!("Shutdown signal received");
       cancel_token.cancel();
   }
   ```

2. Implement config loading with overrides:
   ```rust
   fn load_config_with_overrides(args: &ServeArgs) -> Result<NexusConfig, ConfigError> {
       let mut config = if args.config.exists() {
           NexusConfig::load(Some(&args.config))?
       } else {
           NexusConfig::default()
       };
       
       // Apply CLI overrides
       if let Some(port) = args.port {
           config.server.port = port;
       }
       if let Some(ref host) = args.host {
           config.server.host = host.clone();
       }
       if let Some(ref level) = args.log_level {
           config.logging.level = level.clone();
       }
       
       Ok(config)
   }
   ```

**Acceptance**: All 8 tests pass, server starts and shuts down cleanly.

---

### Phase 5: Query Commands (Tests First)

**Goal**: Implement `backends`, `models`, and `health` commands.

**Tests to write first** (10 tests):
1. `test_backends_list_empty` - Empty registry shows empty table
2. `test_backends_list_with_data` - Shows all backends
3. `test_backends_list_filter_healthy` - `--status healthy` filters
4. `test_backends_add_success` - Adds backend, shows confirmation
5. `test_backends_add_invalid_url` - Invalid URL returns error
6. `test_backends_remove_success` - Removes backend
7. `test_backends_remove_not_found` - Unknown ID returns error
8. `test_models_list_aggregated` - Models grouped across backends
9. `test_health_shows_summary` - Shows backend counts
10. `test_health_json_valid` - JSON output is valid

**Implementation**:

1. Create `src/cli/backends.rs`:
   ```rust
   pub async fn handle_backends_command(
       cmd: BackendsCommands,
       registry: Arc<Registry>,
   ) -> Result<(), Box<dyn std::error::Error>> {
       match cmd {
           BackendsCommands::List(args) => {
               let backends = registry.get_all_backends();
               let filtered = if let Some(status) = args.status {
                   filter_by_status(backends, &status)
               } else {
                   backends
               };
               
               if args.json {
                   print_json(&filtered)?;
               } else {
                   print_backends_table(&filtered);
               }
           }
           BackendsCommands::Add(args) => {
               let backend = create_backend_from_args(&args)?;
               registry.add_backend(backend.clone())?;
               println!("Added backend: {} ({})", backend.name, backend.id);
           }
           BackendsCommands::Remove(args) => {
               registry.remove_backend(&args.id)?;
               println!("Removed backend: {}", args.id);
           }
       }
       Ok(())
   }
   ```

2. Create `src/cli/models.rs`:
   ```rust
   pub async fn handle_models_command(
       args: ModelsArgs,
       registry: Arc<Registry>,
   ) -> Result<(), Box<dyn std::error::Error>> {
       let models = registry.get_all_models_aggregated();
       
       if args.json {
           print_json(&models)?;
       } else {
           print_models_table(&models);
       }
       Ok(())
   }
   ```

3. Create `src/cli/health.rs`:
   ```rust
   pub async fn handle_health_command(
       args: HealthArgs,
       registry: Arc<Registry>,
       uptime: Duration,
   ) -> Result<(), Box<dyn std::error::Error>> {
       let backends = registry.get_all_backends();
       let healthy = backends.iter().filter(|b| b.status == BackendStatus::Healthy).count();
       let models = registry.model_count();
       
       let status = HealthStatus {
           status: if healthy > 0 { "healthy" } else { "degraded" },
           version: env!("CARGO_PKG_VERSION"),
           uptime_seconds: uptime.as_secs(),
           backends: BackendCounts { total: backends.len(), healthy, unhealthy: backends.len() - healthy },
           models: ModelCounts { total: models },
           backend_details: backends,
       };
       
       if args.json {
           print_json(&status)?;
       } else {
           print_health_pretty(&status);
       }
       Ok(())
   }
   ```

**Acceptance**: All 10 tests pass.

---

### Phase 6: Config Init Command (Tests First)

**Goal**: Implement `nexus config init` to generate template.

**Tests to write first** (4 tests):
1. `test_config_init_creates_file` - Creates nexus.toml
2. `test_config_init_custom_path` - `--output custom.toml` works
3. `test_config_init_no_overwrite` - Fails if file exists
4. `test_config_init_force` - `--force` overwrites existing

**Implementation**:

1. Add to `src/cli/mod.rs`:
   ```rust
   #[derive(Subcommand)]
   pub enum ConfigCommands {
       /// Generate example config file
       Init(ConfigInitArgs),
   }
   
   #[derive(Args)]
   pub struct ConfigInitArgs {
       /// Output file path
       #[arg(short, long, default_value = "nexus.toml")]
       pub output: PathBuf,
       
       /// Generate minimal config
       #[arg(long)]
       pub minimal: bool,
       
       /// Overwrite existing file
       #[arg(long)]
       pub force: bool,
   }
   ```

2. Implement config generation:
   ```rust
   pub fn handle_config_init(args: ConfigInitArgs) -> Result<(), Box<dyn std::error::Error>> {
       if args.output.exists() && !args.force {
           return Err(format!("File already exists: {}. Use --force to overwrite.", 
               args.output.display()).into());
       }
       
       let template = if args.minimal {
           include_str!("../../templates/nexus.minimal.toml")
       } else {
           include_str!("../../templates/nexus.example.toml")
       };
       
       std::fs::write(&args.output, template)?;
       println!("Created config file: {}", args.output.display());
       Ok(())
   }
   ```

**Acceptance**: All 4 tests pass.

---

### Phase 7: Integration Tests

**Goal**: End-to-end CLI tests using `assert_cmd`.

**Tests to write** (10 tests):
1. `test_version_output` - `nexus --version` shows version
2. `test_help_output` - `nexus --help` shows all commands
3. `test_serve_help` - `nexus serve --help` shows options
4. `test_backends_empty` - `nexus backends` with no server returns error
5. `test_config_init_e2e` - Full config init workflow
6. `test_invalid_command` - Unknown command shows help
7. `test_serve_invalid_config` - Bad TOML returns error with message
8. `test_serve_port_conflict` - Port in use returns exit code 1
9. `test_env_var_override` - NEXUS_PORT overrides config
10. `test_cli_arg_override` - `--port` overrides env and config

**Implementation** (in `tests/cli_integration.rs`):
```rust
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_version_output() {
    Command::cargo_bin("nexus")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("nexus"));
}

#[test]
fn test_help_shows_commands() {
    Command::cargo_bin("nexus")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("serve"))
        .stdout(predicate::str::contains("backends"))
        .stdout(predicate::str::contains("models"))
        .stdout(predicate::str::contains("health"));
}
```

**Acceptance**: All 10 integration tests pass.

---

## Test Summary

| Phase | Unit Tests | Integration Tests | Total |
|-------|------------|-------------------|-------|
| Phase 1: Config | 10 | 0 | 10 |
| Phase 2: CLI Defs | 8 | 0 | 8 |
| Phase 3: Output | 6 | 0 | 6 |
| Phase 4: Serve | 8 | 0 | 8 |
| Phase 5: Query Cmds | 10 | 0 | 10 |
| Phase 6: Config Init | 4 | 0 | 4 |
| Phase 7: Integration | 0 | 10 | 10 |
| **Total** | **46** | **10** | **56** |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Config crate complexity | Start simple, use TOML only |
| Async CLI for non-serve commands | Run tokio runtime inline for query commands |
| Table formatting edge cases | Use comfy-table's built-in truncation |
| Shell completion generation | Defer to P1, not blocking MVP |

---

## Definition of Done

- [ ] All 56 tests pass
- [ ] `cargo clippy` reports no warnings
- [ ] `cargo fmt --check` passes
- [ ] `nexus --help` shows all commands
- [ ] `nexus serve` starts server
- [ ] `nexus config init` generates valid config
- [ ] Config precedence works: CLI > env > file > defaults
- [ ] JSON output is valid for all commands
- [ ] Exit code 1 on errors with helpful messages
- [ ] Graceful shutdown on SIGINT
