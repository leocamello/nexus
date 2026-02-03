# Implementation Tasks: CLI and Configuration

**Spec**: [spec.md](./spec.md)  
**Plan**: [plan.md](./plan.md)  
**Status**: Ready for Implementation

## Task Overview

| Task | Description | Est. Time | Dependencies |
|------|-------------|-----------|--------------|
| T01 | Add dependencies & module scaffolding | 1h | None |
| T02 | NexusConfig struct & defaults | 2h | T01 |
| T03 | Config file loading & parsing | 2h | T02 |
| T04 | Environment variable overrides | 1.5h | T03 |
| T05 | ConfigError enum & validation (with unknown key warnings) | 1.5h | T02 |
| T06 | CLI command definitions (clap) | 2h | T01 |
| T07 | Output formatting (tables/JSON) | 2h | T06 |
| T08 | Serve command implementation | 3h | T04, T07 |
| T09 | Backends list command | 1.5h | T07 |
| T10 | Backends add/remove commands (with auto-detection) | 2.5h | T09 |
| T11 | Models command | 1.5h | T07 |
| T12 | Health command | 1.5h | T07 |
| T13 | Config init command | 1.5h | T02 |
| T14 | Completions command | 1h | T06 |
| T15 | Graceful shutdown handling | 1.5h | T08 |
| T16 | Integration tests | 2.5h | All |
| T17 | Documentation & cleanup | 1.5h | All |

**Total Estimated Time**: ~29 hours
**Total Tests**: 70 (unit + integration)

---

## T01: Add Dependencies & Module Scaffolding

**Goal**: Add required dependencies and create module structure.

**Files to create/modify**:
- `Cargo.toml` (add comfy-table, config, colored)
- `src/lib.rs` (add config and cli modules)
- `src/config/mod.rs` (create)
- `src/config/server.rs` (create, placeholder)
- `src/config/routing.rs` (create, placeholder)
- `src/config/logging.rs` (create, placeholder)
- `src/config/error.rs` (create, placeholder)
- `src/cli/mod.rs` (create)
- `src/cli/serve.rs` (create, placeholder)
- `src/cli/backends.rs` (create, placeholder)
- `src/cli/models.rs` (create, placeholder)
- `src/cli/health.rs` (create, placeholder)
- `src/cli/output.rs` (create, placeholder)

**Implementation Steps**:
1. Add to `Cargo.toml`:
   ```toml
   # Pretty table output
   comfy-table = "7"
   
   # Layered configuration
   config = { version = "0.14", default-features = false, features = ["toml"] }
   
   # Terminal colors
   colored = "2"
   
   # Shell completion generation
   clap_complete = "4"
   ```
2. Add dev-dependencies:
   ```toml
   tempfile = "3"
   wiremock = "0.6"  # Already present, verify version
   ```
3. Update `src/lib.rs`:
   ```rust
   pub mod registry;
   pub mod health;
   pub mod config;
   pub mod cli;
   ```
4. Create module structure with placeholder files
5. Run `cargo check` to verify structure compiles

**Acceptance Criteria**:
- [ ] `cargo check` passes with no errors
- [ ] All dependencies resolve correctly
- [ ] Module structure matches plan's file layout

**Test Command**: `cargo check`

---

## T02: NexusConfig Struct & Defaults

**Goal**: Implement the unified configuration struct with all sub-configs.

**Files to modify**:
- `src/config/mod.rs`
- `src/config/server.rs`
- `src/config/routing.rs`
- `src/config/logging.rs`

**Tests to Write First** (6 tests):
```rust
#[test]
fn test_server_config_defaults() {
    let config = ServerConfig::default();
    assert_eq!(config.host, "0.0.0.0");
    assert_eq!(config.port, 8000);
    assert_eq!(config.request_timeout_seconds, 300);
    assert_eq!(config.max_concurrent_requests, 1000);
}

#[test]
fn test_routing_config_defaults() {
    let config = RoutingConfig::default();
    assert_eq!(config.strategy, RoutingStrategy::Smart);
    assert_eq!(config.max_retries, 2);
}

#[test]
fn test_routing_strategy_serde() {
    let strategy = RoutingStrategy::RoundRobin;
    let json = serde_json::to_string(&strategy).unwrap();
    assert_eq!(json, "\"round_robin\"");
}

#[test]
fn test_logging_config_defaults() {
    let config = LoggingConfig::default();
    assert_eq!(config.level, "info");
    assert_eq!(config.format, LogFormat::Pretty);
}

#[test]
fn test_log_format_serde() {
    let format = LogFormat::Json;
    let json = serde_json::to_string(&format).unwrap();
    assert_eq!(json, "\"json\"");
}

#[test]
fn test_nexus_config_defaults() {
    let config = NexusConfig::default();
    assert_eq!(config.server.port, 8000);
    assert!(config.discovery.enabled);
    assert!(config.health_check.enabled);
    assert!(config.backends.is_empty());
}
```

**Implementation**:
1. Implement `ServerConfig` with Default:
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(default)]
   pub struct ServerConfig {
       pub host: String,
       pub port: u16,
       pub request_timeout_seconds: u64,
       pub max_concurrent_requests: u32,
   }
   ```

2. Implement `RoutingConfig` with enums:
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
   #[serde(rename_all = "snake_case")]
   pub enum RoutingStrategy {
       #[default]
       Smart,
       RoundRobin,
       PriorityOnly,
       Random,
   }
   ```

3. Implement `LoggingConfig`:
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
   #[serde(rename_all = "snake_case")]
   pub enum LogFormat {
       #[default]
       Pretty,
       Json,
   }
   ```

4. Implement top-level `NexusConfig`:
   ```rust
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
   ```

**Acceptance Criteria**:
- [ ] All 6 tests pass
- [ ] All config structs have Default implementation
- [ ] Serialization uses snake_case for enums
- [ ] `#[serde(default)]` applied to all config structs

**Test Command**: `cargo test config::`

---

## T03: Config File Loading & Parsing

**Goal**: Implement TOML config file loading.

**Files to modify**:
- `src/config/mod.rs`

**Tests to Write First** (5 tests):
```rust
#[test]
fn test_config_parse_minimal_toml() {
    let toml = r#"
    [server]
    port = 9000
    "#;
    
    let config: NexusConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.server.port, 9000);
    assert_eq!(config.server.host, "0.0.0.0"); // Default
}

#[test]
fn test_config_parse_full_toml() {
    let toml = include_str!("../../nexus.example.toml");
    let config: NexusConfig = toml::from_str(toml).unwrap();
    assert!(config.server.port > 0);
}

#[test]
fn test_config_parse_backends_array() {
    let toml = r#"
    [[backends]]
    name = "local"
    url = "http://localhost:11434"
    type = "ollama"
    
    [[backends]]
    name = "remote"
    url = "http://192.168.1.100:8000"
    type = "vllm"
    "#;
    
    let config: NexusConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.backends.len(), 2);
}

#[test]
fn test_config_load_from_file() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(temp.path(), "[server]\nport = 8080").unwrap();
    
    let config = NexusConfig::load(Some(temp.path())).unwrap();
    assert_eq!(config.server.port, 8080);
}

#[test]
fn test_config_missing_file_error() {
    let result = NexusConfig::load(Some(Path::new("/nonexistent/config.toml")));
    assert!(matches!(result, Err(ConfigError::NotFound(_))));
}
```

**Implementation**:
1. Add `BackendConfig` struct:
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct BackendConfig {
       pub name: String,
       pub url: String,
       #[serde(rename = "type")]
       pub backend_type: BackendType,
       #[serde(default = "default_priority")]
       pub priority: i32,
   }
   
   fn default_priority() -> i32 { 50 }
   ```

2. Implement `NexusConfig::load()`:
   ```rust
   impl NexusConfig {
       pub fn load(path: Option<&Path>) -> Result<Self, ConfigError> {
           match path {
               Some(p) => {
                   if !p.exists() {
                       return Err(ConfigError::NotFound(p.to_path_buf()));
                   }
                   let content = std::fs::read_to_string(p)
                       .map_err(|e| ConfigError::IoError(e.to_string()))?;
                   toml::from_str(&content)
                       .map_err(|e| ConfigError::ParseError(e.to_string()))
               }
               None => Ok(Self::default()),
           }
       }
   }
   ```

**Acceptance Criteria**:
- [ ] All 5 tests pass
- [ ] Parses `nexus.example.toml` without error
- [ ] Returns proper error for missing file
- [ ] Default values applied for missing fields

**Test Command**: `cargo test config::tests::test_config_parse`

---

## T04: Environment Variable Overrides

**Goal**: Implement NEXUS_* environment variable override support.

**Files to modify**:
- `src/config/mod.rs`

**Tests to Write First** (4 tests):
```rust
#[test]
fn test_config_env_override_port() {
    std::env::set_var("NEXUS_PORT", "9999");
    let config = NexusConfig::default().with_env_overrides();
    std::env::remove_var("NEXUS_PORT");
    
    assert_eq!(config.server.port, 9999);
}

#[test]
fn test_config_env_override_host() {
    std::env::set_var("NEXUS_HOST", "127.0.0.1");
    let config = NexusConfig::default().with_env_overrides();
    std::env::remove_var("NEXUS_HOST");
    
    assert_eq!(config.server.host, "127.0.0.1");
}

#[test]
fn test_config_env_override_log_level() {
    std::env::set_var("NEXUS_LOG_LEVEL", "debug");
    let config = NexusConfig::default().with_env_overrides();
    std::env::remove_var("NEXUS_LOG_LEVEL");
    
    assert_eq!(config.logging.level, "debug");
}

#[test]
fn test_config_env_invalid_value_ignored() {
    std::env::set_var("NEXUS_PORT", "not-a-number");
    let config = NexusConfig::default().with_env_overrides();
    std::env::remove_var("NEXUS_PORT");
    
    // Should keep default, not crash
    assert_eq!(config.server.port, 8000);
}
```

**Implementation**:
```rust
impl NexusConfig {
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(port) = std::env::var("NEXUS_PORT") {
            if let Ok(p) = port.parse() {
                self.server.port = p;
            }
        }
        if let Ok(host) = std::env::var("NEXUS_HOST") {
            self.server.host = host;
        }
        if let Ok(level) = std::env::var("NEXUS_LOG_LEVEL") {
            self.logging.level = level;
        }
        if let Ok(format) = std::env::var("NEXUS_LOG_FORMAT") {
            if let Ok(f) = format.parse() {
                self.logging.format = f;
            }
        }
        if let Ok(discovery) = std::env::var("NEXUS_DISCOVERY") {
            self.discovery.enabled = discovery.to_lowercase() == "true";
        }
        if let Ok(health) = std::env::var("NEXUS_HEALTH_CHECK") {
            self.health_check.enabled = health.to_lowercase() == "true";
        }
        self
    }
}
```

**Acceptance Criteria**:
- [ ] All 4 tests pass
- [ ] Invalid env values don't crash, use defaults
- [ ] All documented NEXUS_* variables work

**Test Command**: `cargo test config::tests::test_config_env`

---

## T05: ConfigError Enum & Validation (with Unknown Key Warnings)

**Goal**: Implement error types with helpful messages and unknown key detection.

**Files to modify**:
- `src/config/error.rs`
- `src/config/mod.rs`

**Tests to Write First** (5 tests):
```rust
#[test]
fn test_config_error_not_found_display() {
    let err = ConfigError::NotFound(PathBuf::from("/etc/nexus.toml"));
    assert!(err.to_string().contains("/etc/nexus.toml"));
}

#[test]
fn test_config_error_parse_display() {
    let err = ConfigError::ParseError("expected string at line 5".to_string());
    assert!(err.to_string().contains("line 5"));
}

#[test]
fn test_config_validation_invalid_port() {
    let mut config = NexusConfig::default();
    config.server.port = 0;
    
    let result = config.validate();
    assert!(matches!(result, Err(ConfigError::ValidationError { field, .. }) if field == "server.port"));
}

#[test]
fn test_config_validation_empty_backend_url() {
    let mut config = NexusConfig::default();
    config.backends.push(BackendConfig {
        name: "test".to_string(),
        url: "".to_string(),
        backend_type: BackendType::Ollama,
        priority: 1,
    });
    
    let result = config.validate();
    assert!(matches!(result, Err(ConfigError::ValidationError { field, .. }) if field.contains("url")));
}

#[test]
fn test_config_warns_on_unknown_keys() {
    // This test verifies unknown keys don't cause errors
    // The warning is logged via tracing (can verify with tracing_test crate)
    let toml = r#"
    [server]
    port = 8000
    
    [unknown_section]
    foo = "bar"
    "#;
    
    // Should parse successfully (unknown keys are warned, not rejected)
    let result = NexusConfig::load_from_str(toml);
    assert!(result.is_ok());
}
```

**Implementation**:
```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config file not found: {0}")]
    NotFound(PathBuf),
    
    #[error("failed to read config: {0}")]
    IoError(String),
    
    #[error("config parse error: {0}")]
    ParseError(String),
    
    #[error("invalid value for '{field}': {message}")]
    ValidationError { field: String, message: String },
}

impl NexusConfig {
    /// Load config from string, warning on unknown keys
    pub fn load_from_str(content: &str) -> Result<Self, ConfigError> {
        // First, detect unknown keys
        Self::warn_unknown_keys(content);
        
        // Then parse normally
        toml::from_str(content)
            .map_err(|e| ConfigError::ParseError(e.to_string()))
    }
    
    /// Warn about unknown top-level config keys
    fn warn_unknown_keys(content: &str) {
        let known_keys = ["server", "discovery", "health_check", 
                         "routing", "backends", "logging"];
        
        if let Ok(raw_value) = content.parse::<toml::Value>() {
            if let toml::Value::Table(table) = raw_value {
                for key in table.keys() {
                    if !known_keys.contains(&key.as_str()) {
                        tracing::warn!(
                            key = %key, 
                            "Unknown config key '{}' - ignoring", 
                            key
                        );
                    }
                }
            }
        }
    }
    
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.server.port == 0 {
            return Err(ConfigError::ValidationError {
                field: "server.port".to_string(),
                message: "port must be non-zero".to_string(),
            });
        }
        
        for (i, backend) in self.backends.iter().enumerate() {
            if backend.url.is_empty() {
                return Err(ConfigError::ValidationError {
                    field: format!("backends[{}].url", i),
                    message: "URL cannot be empty".to_string(),
                });
            }
            if backend.name.is_empty() {
                return Err(ConfigError::ValidationError {
                    field: format!("backends[{}].name", i),
                    message: "name cannot be empty".to_string(),
                });
            }
        }
        
        Ok(())
    }
}
```

**Acceptance Criteria**:
- [ ] All 5 tests pass
- [ ] Error messages include field names
- [ ] Validation catches common mistakes

**Test Command**: `cargo test config::error`

---

## T06: CLI Command Definitions (clap)

**Goal**: Define all CLI commands using clap derive.

**Files to modify**:
- `src/cli/mod.rs`
- `src/main.rs`

**Tests to Write First** (8 tests):
```rust
#[test]
fn test_cli_parse_serve_defaults() {
    let cli = Cli::try_parse_from(["nexus", "serve"]).unwrap();
    match cli.command {
        Commands::Serve(args) => {
            assert_eq!(args.config, PathBuf::from("nexus.toml"));
            assert!(args.port.is_none());
            assert!(!args.no_discovery);
        }
        _ => panic!("Expected Serve command"),
    }
}

#[test]
fn test_cli_parse_serve_with_port() {
    let cli = Cli::try_parse_from(["nexus", "serve", "-p", "9000"]).unwrap();
    match cli.command {
        Commands::Serve(args) => assert_eq!(args.port, Some(9000)),
        _ => panic!("Expected Serve command"),
    }
}

#[test]
fn test_cli_parse_serve_with_config() {
    let cli = Cli::try_parse_from(["nexus", "serve", "-c", "custom.toml"]).unwrap();
    match cli.command {
        Commands::Serve(args) => assert_eq!(args.config, PathBuf::from("custom.toml")),
        _ => panic!("Expected Serve command"),
    }
}

#[test]
fn test_cli_parse_backends_list() {
    let cli = Cli::try_parse_from(["nexus", "backends", "list"]).unwrap();
    assert!(matches!(cli.command, Commands::Backends(BackendsCommands::List(_))));
}

#[test]
fn test_cli_parse_backends_list_json() {
    let cli = Cli::try_parse_from(["nexus", "backends", "list", "--json"]).unwrap();
    match cli.command {
        Commands::Backends(BackendsCommands::List(args)) => assert!(args.json),
        _ => panic!("Expected Backends List command"),
    }
}

#[test]
fn test_cli_parse_backends_add() {
    let cli = Cli::try_parse_from(["nexus", "backends", "add", "http://localhost:11434"]).unwrap();
    match cli.command {
        Commands::Backends(BackendsCommands::Add(args)) => {
            assert_eq!(args.url, "http://localhost:11434");
        }
        _ => panic!("Expected Backends Add command"),
    }
}

#[test]
fn test_cli_parse_models() {
    let cli = Cli::try_parse_from(["nexus", "models"]).unwrap();
    assert!(matches!(cli.command, Commands::Models(_)));
}

#[test]
fn test_cli_parse_health() {
    let cli = Cli::try_parse_from(["nexus", "health"]).unwrap();
    assert!(matches!(cli.command, Commands::Health(_)));
}
```

**Implementation**:
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

#[derive(Args)]
pub struct ServeArgs {
    #[arg(short, long, default_value = "nexus.toml")]
    pub config: PathBuf,
    
    #[arg(short, long, env = "NEXUS_PORT")]
    pub port: Option<u16>,
    
    #[arg(short = 'H', long, env = "NEXUS_HOST")]
    pub host: Option<String>,
    
    #[arg(short, long, env = "NEXUS_LOG_LEVEL")]
    pub log_level: Option<String>,
    
    #[arg(long)]
    pub no_discovery: bool,
    
    #[arg(long)]
    pub no_health_check: bool,
}

// ... additional Args structs
```

**Acceptance Criteria**:
- [ ] All 8 tests pass
- [ ] `nexus --help` shows all commands
- [ ] `nexus serve --help` shows all options
- [ ] Environment variables work for serve args

**Test Command**: `cargo test cli::tests`

---

## T07: Output Formatting (Tables/JSON)

**Goal**: Implement table and JSON output helpers.

**Files to modify**:
- `src/cli/output.rs`

**Tests to Write First** (6 tests):
```rust
#[test]
fn test_format_backends_table_empty() {
    let output = format_backends_table(&[]);
    assert!(output.contains("Name")); // Header present
}

#[test]
fn test_format_backends_table_with_data() {
    let backends = vec![create_test_backend_view()];
    let output = format_backends_table(&backends);
    assert!(output.contains("test-backend"));
    assert!(output.contains("Healthy"));
}

#[test]
fn test_format_backends_json_valid() {
    let backends = vec![create_test_backend_view()];
    let output = format_backends_json(&backends);
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(parsed.get("backends").is_some());
}

#[test]
fn test_format_models_table() {
    let models = vec![create_test_model_view()];
    let output = format_models_table(&models);
    assert!(output.contains("Model"));
    assert!(output.contains("Context"));
}

#[test]
fn test_format_health_pretty() {
    let status = create_test_health_status();
    let output = format_health_pretty(&status);
    assert!(output.contains("Status:"));
    assert!(output.contains("Backends:"));
}

#[test]
fn test_status_icon_healthy() {
    assert_eq!(status_icon(BackendStatus::Healthy), "✓");
    assert_eq!(status_icon(BackendStatus::Unhealthy), "✗");
    assert_eq!(status_icon(BackendStatus::Unknown), "?");
}
```

**Implementation**:
```rust
use comfy_table::{Table, Cell, Color, ContentArrangement};
use colored::Colorize;

pub fn format_backends_table(backends: &[BackendView]) -> String {
    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec!["Name", "URL", "Type", "Status", "Models", "Latency"]);
    
    for b in backends {
        let status_str = match b.status {
            BackendStatus::Healthy => "Healthy".green().to_string(),
            BackendStatus::Unhealthy => "Unhealthy".red().to_string(),
            BackendStatus::Unknown => "Unknown".yellow().to_string(),
            BackendStatus::Draining => "Draining".cyan().to_string(),
        };
        
        table.add_row(vec![
            &b.name,
            &b.url,
            &format!("{:?}", b.backend_type),
            &status_str,
            &b.models.len().to_string(),
            &format!("{}ms", b.avg_latency_ms),
        ]);
    }
    
    table.to_string()
}

pub fn format_backends_json(backends: &[BackendView]) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "backends": backends
    })).unwrap()
}

pub fn status_icon(status: BackendStatus) -> &'static str {
    match status {
        BackendStatus::Healthy => "✓",
        BackendStatus::Unhealthy => "✗",
        BackendStatus::Unknown => "?",
        BackendStatus::Draining => "~",
    }
}
```

**Acceptance Criteria**:
- [ ] All 6 tests pass
- [ ] Tables render with proper alignment
- [ ] JSON output is valid and pretty-printed
- [ ] Status colors work in terminal

**Test Command**: `cargo test cli::output`

---

## T08: Serve Command Implementation

**Goal**: Implement the main serve command that starts the server.

**Files to modify**:
- `src/cli/serve.rs`
- `src/main.rs`

**Tests to Write First** (6 tests):
```rust
#[tokio::test]
async fn test_serve_config_loading() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(temp.path(), "[server]\nport = 8080").unwrap();
    
    let args = ServeArgs {
        config: temp.path().to_path_buf(),
        port: None,
        ..Default::default()
    };
    
    let config = load_config_with_overrides(&args).unwrap();
    assert_eq!(config.server.port, 8080);
}

#[tokio::test]
async fn test_serve_cli_overrides_config() {
    let temp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(temp.path(), "[server]\nport = 8080").unwrap();
    
    let args = ServeArgs {
        config: temp.path().to_path_buf(),
        port: Some(9000),  // Override
        ..Default::default()
    };
    
    let config = load_config_with_overrides(&args).unwrap();
    assert_eq!(config.server.port, 9000);  // CLI wins
}

#[tokio::test]
async fn test_serve_works_without_config_file() {
    let args = ServeArgs {
        config: PathBuf::from("nonexistent.toml"),
        ..Default::default()
    };
    
    let config = load_config_with_overrides(&args).unwrap();
    assert_eq!(config.server.port, 8000);  // Default
}

#[tokio::test]
async fn test_init_tracing_pretty() {
    let config = LoggingConfig {
        level: "debug".to_string(),
        format: LogFormat::Pretty,
    };
    
    // Should not panic
    let result = init_tracing(&config);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_init_tracing_json() {
    let config = LoggingConfig {
        level: "info".to_string(),
        format: LogFormat::Json,
    };
    
    let result = init_tracing(&config);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_backends_loaded_from_config() {
    let config = NexusConfig {
        backends: vec![
            BackendConfig {
                name: "test".to_string(),
                url: "http://localhost:11434".to_string(),
                backend_type: BackendType::Ollama,
                priority: 1,
            }
        ],
        ..Default::default()
    };
    
    let registry = Arc::new(Registry::new());
    load_backends_from_config(&config, &registry).unwrap();
    
    assert_eq!(registry.backend_count(), 1);
}
```

**Implementation**:
```rust
pub async fn run_serve(args: ServeArgs) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load and merge configuration
    let config = load_config_with_overrides(&args)?;
    config.validate()?;
    
    // 2. Initialize tracing
    init_tracing(&config.logging)?;
    
    // 3. Create registry and load static backends
    let registry = Arc::new(Registry::new());
    load_backends_from_config(&config, &registry)?;
    
    // 4. Start health checker (if enabled)
    let cancel_token = CancellationToken::new();
    let health_handle = if config.health_check.enabled && !args.no_health_check {
        let checker = HealthChecker::new(registry.clone(), config.health_check.clone());
        Some(checker.start(cancel_token.clone()))
    } else {
        None
    };
    
    // 5. Build minimal HTTP server (full API Gateway is separate feature)
    let app = build_basic_router(registry.clone());
    
    // 6. Bind and serve
    let addr = format!("{}:{}", config.server.host, config.server.port);
    tracing::info!(addr = %addr, "Nexus server starting");
    
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(cancel_token.clone()))
        .await?;
    
    // 7. Cleanup
    if let Some(handle) = health_handle {
        handle.await?;
    }
    
    tracing::info!("Nexus server stopped");
    Ok(())
}

fn build_basic_router(registry: Arc<Registry>) -> Router {
    Router::new()
        .route("/health", get(|| async { "OK" }))
        .with_state(registry)
}
```

**Acceptance Criteria**:
- [ ] All 6 tests pass
- [ ] Server starts and binds to configured port
- [ ] Health checker starts (unless disabled)
- [ ] Config precedence works: CLI > env > file > defaults

**Test Command**: `cargo test cli::serve`

---

## T09: Backends List Command

**Goal**: Implement `nexus backends list` command.

**Files to modify**:
- `src/cli/backends.rs`

**Tests to Write First** (4 tests):
```rust
#[test]
fn test_backends_list_empty_registry() {
    let registry = Arc::new(Registry::new());
    let args = BackendsListArgs { json: false, status: None };
    
    let output = handle_backends_list(&args, &registry);
    assert!(output.is_ok());
}

#[test]
fn test_backends_list_with_backends() {
    let registry = Arc::new(Registry::new());
    registry.add_backend(create_test_backend()).unwrap();
    
    let args = BackendsListArgs { json: false, status: None };
    let output = handle_backends_list(&args, &registry).unwrap();
    
    assert!(output.contains("test-backend"));
}

#[test]
fn test_backends_list_filter_healthy() {
    let registry = Arc::new(Registry::new());
    
    let mut healthy = create_test_backend();
    healthy.id = "healthy".to_string();
    registry.add_backend(healthy).unwrap();
    registry.update_status("healthy", BackendStatus::Healthy, None).unwrap();
    
    let mut unhealthy = create_test_backend();
    unhealthy.id = "unhealthy".to_string();
    registry.add_backend(unhealthy).unwrap();
    registry.update_status("unhealthy", BackendStatus::Unhealthy, Some("error")).unwrap();
    
    let args = BackendsListArgs { json: false, status: Some("healthy".to_string()) };
    let output = handle_backends_list(&args, &registry).unwrap();
    
    assert!(output.contains("healthy"));
    assert!(!output.contains("unhealthy"));
}

#[test]
fn test_backends_list_json_output() {
    let registry = Arc::new(Registry::new());
    registry.add_backend(create_test_backend()).unwrap();
    
    let args = BackendsListArgs { json: true, status: None };
    let output = handle_backends_list(&args, &registry).unwrap();
    
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(parsed.get("backends").is_some());
}
```

**Implementation**:
```rust
pub fn handle_backends_list(
    args: &BackendsListArgs,
    registry: &Registry,
) -> Result<String, Box<dyn std::error::Error>> {
    let backends = registry.get_all_backends();
    
    let filtered: Vec<_> = if let Some(ref status) = args.status {
        let target_status = parse_status(status)?;
        backends.into_iter().filter(|b| b.status == target_status).collect()
    } else {
        backends
    };
    
    if args.json {
        Ok(format_backends_json(&filtered))
    } else {
        Ok(format_backends_table(&filtered))
    }
}

fn parse_status(s: &str) -> Result<BackendStatus, Box<dyn std::error::Error>> {
    match s.to_lowercase().as_str() {
        "healthy" => Ok(BackendStatus::Healthy),
        "unhealthy" => Ok(BackendStatus::Unhealthy),
        "unknown" => Ok(BackendStatus::Unknown),
        _ => Err(format!("Invalid status: {}. Use: healthy, unhealthy, unknown", s).into()),
    }
}
```

**Acceptance Criteria**:
- [ ] All 4 tests pass
- [ ] Empty registry shows empty table (not error)
- [ ] `--status` filter works
- [ ] `--json` outputs valid JSON

**Test Command**: `cargo test cli::backends::tests::test_backends_list`

---

## T10: Backends Add/Remove Commands (with Auto-Detection)

**Goal**: Implement `nexus backends add` with auto-type detection and `nexus backends remove`.

**Files to modify**:
- `src/cli/backends.rs`

**Tests to Write First** (7 tests):
```rust
#[tokio::test]
async fn test_backends_add_success() {
    let registry = Arc::new(Registry::new());
    let args = BackendsAddArgs {
        url: "http://localhost:11434".to_string(),
        name: Some("test".to_string()),
        backend_type: Some(BackendType::Ollama),
        priority: Some(1),
    };
    
    let result = handle_backends_add(&args, &registry).await;
    assert!(result.is_ok());
    assert_eq!(registry.backend_count(), 1);
}

#[tokio::test]
async fn test_backends_add_generates_name() {
    let registry = Arc::new(Registry::new());
    let args = BackendsAddArgs {
        url: "http://192.168.1.100:8000".to_string(),
        name: None,
        backend_type: Some(BackendType::VLLM),
        priority: None,
    };
    
    handle_backends_add(&args, &registry).await.unwrap();
    
    let backends = registry.get_all_backends();
    assert!(!backends[0].name.is_empty());
}

#[tokio::test]
async fn test_backends_add_invalid_url() {
    let registry = Arc::new(Registry::new());
    let args = BackendsAddArgs {
        url: "not-a-url".to_string(),
        name: None,
        backend_type: None,
        priority: None,
    };
    
    let result = handle_backends_add(&args, &registry).await;
    assert!(result.is_err());
}

#[test]
fn test_backends_remove_success() {
    let registry = Arc::new(Registry::new());
    registry.add_backend(create_test_backend()).unwrap();
    
    let args = BackendsRemoveArgs { id: "test-backend".to_string() };
    let result = handle_backends_remove(&args, &registry);
    
    assert!(result.is_ok());
    assert_eq!(registry.backend_count(), 0);
}

#[test]
fn test_backends_remove_not_found() {
    let registry = Arc::new(Registry::new());
    
    let args = BackendsRemoveArgs { id: "nonexistent".to_string() };
    let result = handle_backends_remove(&args, &registry);
    
    assert!(result.is_err());
}

#[tokio::test]
async fn test_backends_add_auto_detect_ollama() {
    // Mock server that responds like Ollama
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"models": []})))
        .mount(&mock_server)
        .await;
    
    let registry = Arc::new(Registry::new());
    let args = BackendsAddArgs {
        url: mock_server.uri(),
        name: None,
        backend_type: None,  // Auto-detect
        priority: None,
    };
    
    handle_backends_add(&args, &registry).await.unwrap();
    
    let backends = registry.get_all_backends();
    assert_eq!(backends[0].backend_type, BackendType::Ollama);
}

#[tokio::test]
async fn test_backends_add_auto_detect_fallback_generic() {
    // Mock server that doesn't respond to any known endpoints
    let mock_server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;
    
    let registry = Arc::new(Registry::new());
    let args = BackendsAddArgs {
        url: mock_server.uri(),
        name: None,
        backend_type: None,
        priority: None,
    };
    
    handle_backends_add(&args, &registry).await.unwrap();
    
    let backends = registry.get_all_backends();
    assert_eq!(backends[0].backend_type, BackendType::Generic);  // Fallback
}
```

**Implementation**:
```rust
/// Auto-detect backend type by probing known endpoints.
/// Detection order: Ollama -> LlamaCpp -> OpenAI-compatible -> Generic
async fn detect_backend_type(base_url: &str) -> Option<BackendType> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .ok()?;
    
    // Try Ollama: GET /api/tags
    if let Ok(resp) = client.get(format!("{}/api/tags", base_url)).send().await {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if text.contains("models") {
                    tracing::debug!(url = %base_url, "Detected Ollama backend");
                    return Some(BackendType::Ollama);
                }
            }
        }
    }
    
    // Try LlamaCpp: GET /health
    if let Ok(resp) = client.get(format!("{}/health", base_url)).send().await {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if text.contains("ok") {
                    tracing::debug!(url = %base_url, "Detected LlamaCpp backend");
                    return Some(BackendType::LlamaCpp);
                }
            }
        }
    }
    
    // Try OpenAI-compatible: GET /v1/models
    if let Ok(resp) = client.get(format!("{}/v1/models", base_url)).send().await {
        if resp.status().is_success() {
            tracing::debug!(url = %base_url, "Detected OpenAI-compatible backend");
            return Some(BackendType::Generic);  // Could be vLLM, Exo, etc.
        }
    }
    
    // Fallback: unknown, will use Generic
    tracing::debug!(url = %base_url, "Could not detect backend type, using Generic");
    None
}

pub async fn handle_backends_add(
    args: &BackendsAddArgs,
    registry: &Registry,
) -> Result<String, Box<dyn std::error::Error>> {
    // Validate URL
    let url = reqwest::Url::parse(&args.url)
        .map_err(|e| format!("Invalid URL: {}", e))?;
    
    // Generate name if not provided
    let name = args.name.clone().unwrap_or_else(|| {
        url.host_str().unwrap_or("backend").to_string()
    });
    
    // Auto-detect type if not provided
    let backend_type = match args.backend_type {
        Some(t) => t,
        None => {
            tracing::info!(url = %args.url, "Auto-detecting backend type...");
            detect_backend_type(&args.url).await.unwrap_or(BackendType::Generic)
        }
    };
    
    let backend = Backend::new(
        uuid::Uuid::new_v4().to_string(),
        name.clone(),
        args.url.clone(),
        backend_type,
        vec![],
        DiscoverySource::Manual,
        HashMap::new(),
    );
    
    let id = backend.id.clone();
    registry.add_backend(backend)?;
    
    tracing::info!(name = %name, id = %id, backend_type = ?backend_type, "Backend added");
    Ok(format!("Added backend '{}' ({}) as {:?}", name, id, backend_type))
}

pub fn handle_backends_remove(
    args: &BackendsRemoveArgs,
    registry: &Registry,
) -> Result<String, Box<dyn std::error::Error>> {
    registry.remove_backend(&args.id)?;
    Ok(format!("Removed backend: {}", args.id))
}
```

**Acceptance Criteria**:
- [ ] All 7 tests pass
- [ ] Add generates name from URL if not provided
- [ ] Add validates URL format
- [ ] Auto-detection tries Ollama → LlamaCpp → OpenAI → Generic
- [ ] Auto-detection times out after 2s per endpoint
- [ ] Remove returns error for unknown ID

**Test Command**: `cargo test cli::backends::tests::test_backends_add`

---

## T11: Models Command

**Goal**: Implement `nexus models` command.

**Files to modify**:
- `src/cli/models.rs`

**Tests to Write First** (4 tests):
```rust
#[test]
fn test_models_list_empty() {
    let registry = Arc::new(Registry::new());
    let args = ModelsArgs { json: false, backend: None };
    
    let output = handle_models(&args, &registry).unwrap();
    assert!(output.contains("Model")); // Header
}

#[test]
fn test_models_list_aggregated() {
    let registry = Arc::new(Registry::new());
    
    // Add two backends with overlapping models
    let mut backend1 = create_test_backend();
    backend1.id = "backend1".to_string();
    backend1.models = vec![create_test_model("llama3:70b")];
    registry.add_backend(backend1).unwrap();
    
    let mut backend2 = create_test_backend();
    backend2.id = "backend2".to_string();
    backend2.models = vec![create_test_model("llama3:70b"), create_test_model("mistral:7b")];
    registry.add_backend(backend2).unwrap();
    
    let args = ModelsArgs { json: false, backend: None };
    let output = handle_models(&args, &registry).unwrap();
    
    assert!(output.contains("llama3:70b"));
    assert!(output.contains("mistral:7b"));
}

#[test]
fn test_models_filter_by_backend() {
    let registry = Arc::new(Registry::new());
    
    let mut backend1 = create_test_backend();
    backend1.id = "backend1".to_string();
    backend1.models = vec![create_test_model("llama3:70b")];
    registry.add_backend(backend1).unwrap();
    
    let mut backend2 = create_test_backend();
    backend2.id = "backend2".to_string();
    backend2.models = vec![create_test_model("mistral:7b")];
    registry.add_backend(backend2).unwrap();
    
    let args = ModelsArgs { json: false, backend: Some("backend1".to_string()) };
    let output = handle_models(&args, &registry).unwrap();
    
    assert!(output.contains("llama3:70b"));
    assert!(!output.contains("mistral:7b"));
}

#[test]
fn test_models_json_output() {
    let registry = Arc::new(Registry::new());
    let mut backend = create_test_backend();
    backend.models = vec![create_test_model("llama3:70b")];
    registry.add_backend(backend).unwrap();
    
    let args = ModelsArgs { json: true, backend: None };
    let output = handle_models(&args, &registry).unwrap();
    
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(parsed.get("models").is_some());
}
```

**Implementation**:
```rust
pub fn handle_models(
    args: &ModelsArgs,
    registry: &Registry,
) -> Result<String, Box<dyn std::error::Error>> {
    let backends = if let Some(ref id) = args.backend {
        match registry.get_backend(id) {
            Some(b) => vec![b],
            None => return Err(format!("Backend not found: {}", id).into()),
        }
    } else {
        registry.get_all_backends()
    };
    
    // Aggregate models with their backends
    let mut model_map: HashMap<String, ModelView> = HashMap::new();
    for backend in backends {
        for model in &backend.models {
            model_map.entry(model.id.clone())
                .or_insert_with(|| ModelView::from(model))
                .backends.push(backend.name.clone());
        }
    }
    
    let models: Vec<_> = model_map.into_values().collect();
    
    if args.json {
        Ok(format_models_json(&models))
    } else {
        Ok(format_models_table(&models))
    }
}
```

**Acceptance Criteria**:
- [ ] All 4 tests pass
- [ ] Models aggregated across backends
- [ ] `--backend` filter works
- [ ] Shows capability columns (Vision, Tools, JSON)

**Test Command**: `cargo test cli::models`

---

## T12: Health Command

**Goal**: Implement `nexus health` command.

**Files to modify**:
- `src/cli/health.rs`

**Tests to Write First** (4 tests):
```rust
#[test]
fn test_health_shows_summary() {
    let registry = Arc::new(Registry::new());
    
    let mut healthy = create_test_backend();
    healthy.id = "healthy".to_string();
    registry.add_backend(healthy).unwrap();
    registry.update_status("healthy", BackendStatus::Healthy, None).unwrap();
    
    let args = HealthArgs { json: false };
    let output = handle_health(&args, &registry, Duration::from_secs(3600)).unwrap();
    
    assert!(output.contains("Status:"));
    assert!(output.contains("1/1 healthy"));
}

#[test]
fn test_health_degraded_status() {
    let registry = Arc::new(Registry::new());
    
    // All backends unhealthy = degraded
    let mut backend = create_test_backend();
    registry.add_backend(backend).unwrap();
    registry.update_status(&backend.id, BackendStatus::Unhealthy, Some("error")).unwrap();
    
    let args = HealthArgs { json: false };
    let output = handle_health(&args, &registry, Duration::from_secs(0)).unwrap();
    
    assert!(output.contains("degraded") || output.contains("Degraded"));
}

#[test]
fn test_health_json_valid() {
    let registry = Arc::new(Registry::new());
    
    let args = HealthArgs { json: true };
    let output = handle_health(&args, &registry, Duration::from_secs(100)).unwrap();
    
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(parsed.get("status").is_some());
    assert!(parsed.get("uptime_seconds").is_some());
}

#[test]
fn test_health_shows_uptime() {
    let registry = Arc::new(Registry::new());
    
    let args = HealthArgs { json: false };
    let output = handle_health(&args, &registry, Duration::from_secs(3661)).unwrap();
    
    assert!(output.contains("1h") || output.contains("3661"));
}
```

**Implementation**:
```rust
#[derive(Serialize)]
pub struct HealthStatus {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
    pub backends: BackendCounts,
    pub models: ModelCounts,
}

pub fn handle_health(
    args: &HealthArgs,
    registry: &Registry,
    uptime: Duration,
) -> Result<String, Box<dyn std::error::Error>> {
    let backends = registry.get_all_backends();
    let healthy = backends.iter().filter(|b| b.status == BackendStatus::Healthy).count();
    let model_count = registry.model_count();
    
    let status = HealthStatus {
        status: if healthy > 0 { "healthy".to_string() } else { "degraded".to_string() },
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime.as_secs(),
        backends: BackendCounts {
            total: backends.len(),
            healthy,
            unhealthy: backends.len() - healthy,
        },
        models: ModelCounts { total: model_count },
    };
    
    if args.json {
        Ok(serde_json::to_string_pretty(&status)?)
    } else {
        Ok(format_health_pretty(&status, &backends))
    }
}

fn format_health_pretty(status: &HealthStatus, backends: &[BackendView]) -> String {
    let mut output = String::new();
    
    let status_display = if status.status == "healthy" {
        "Healthy".green()
    } else {
        "Degraded".yellow()
    };
    
    writeln!(output, "Status: {}", status_display).unwrap();
    writeln!(output, "Version: {}", status.version).unwrap();
    writeln!(output, "Uptime: {}", format_duration(status.uptime_seconds)).unwrap();
    writeln!(output).unwrap();
    writeln!(output, "Backends: {}/{} healthy", 
        status.backends.healthy, status.backends.total).unwrap();
    writeln!(output, "Models: {} available", status.models.total).unwrap();
    
    // ... backend details
    
    output
}
```

**Acceptance Criteria**:
- [ ] All 4 tests pass
- [ ] Shows "healthy" or "degraded" status
- [ ] Shows formatted uptime
- [ ] JSON includes all required fields

**Test Command**: `cargo test cli::health`

---

## T13: Config Init Command

**Goal**: Implement `nexus config init` command.

**Files to modify**:
- `src/cli/mod.rs` (add ConfigCommands)
- Create `templates/` directory with template files

**Tests to Write First** (4 tests):
```rust
#[test]
fn test_config_init_creates_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output_path = temp_dir.path().join("nexus.toml");
    
    let args = ConfigInitArgs {
        output: output_path.clone(),
        minimal: false,
        force: false,
    };
    
    handle_config_init(&args).unwrap();
    
    assert!(output_path.exists());
    let content = std::fs::read_to_string(&output_path).unwrap();
    assert!(content.contains("[server]"));
}

#[test]
fn test_config_init_minimal() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output_path = temp_dir.path().join("nexus.toml");
    
    let args = ConfigInitArgs {
        output: output_path.clone(),
        minimal: true,
        force: false,
    };
    
    handle_config_init(&args).unwrap();
    
    let content = std::fs::read_to_string(&output_path).unwrap();
    // Minimal should be shorter
    assert!(content.len() < 500);
}

#[test]
fn test_config_init_no_overwrite() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output_path = temp_dir.path().join("nexus.toml");
    
    // Create existing file
    std::fs::write(&output_path, "existing").unwrap();
    
    let args = ConfigInitArgs {
        output: output_path.clone(),
        minimal: false,
        force: false,
    };
    
    let result = handle_config_init(&args);
    assert!(result.is_err());
    
    // Original content preserved
    let content = std::fs::read_to_string(&output_path).unwrap();
    assert_eq!(content, "existing");
}

#[test]
fn test_config_init_force_overwrites() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output_path = temp_dir.path().join("nexus.toml");
    
    std::fs::write(&output_path, "old content").unwrap();
    
    let args = ConfigInitArgs {
        output: output_path.clone(),
        minimal: false,
        force: true,
    };
    
    handle_config_init(&args).unwrap();
    
    let content = std::fs::read_to_string(&output_path).unwrap();
    assert!(content.contains("[server]"));
}
```

**Implementation**:
1. Create `templates/nexus.example.toml` (copy from repo root)
2. Create `templates/nexus.minimal.toml`:
   ```toml
   # Minimal Nexus configuration
   [server]
   port = 8000
   
   [logging]
   level = "info"
   ```
3. Implement handler:
   ```rust
   pub fn handle_config_init(args: &ConfigInitArgs) -> Result<String, Box<dyn std::error::Error>> {
       if args.output.exists() && !args.force {
           return Err(format!(
               "File already exists: {}. Use --force to overwrite.",
               args.output.display()
           ).into());
       }
       
       let template = if args.minimal {
           include_str!("../../templates/nexus.minimal.toml")
       } else {
           include_str!("../../templates/nexus.example.toml")
       };
       
       std::fs::write(&args.output, template)?;
       Ok(format!("Created config file: {}", args.output.display()))
   }
   ```

**Acceptance Criteria**:
- [ ] All 4 tests pass
- [ ] Creates valid TOML file
- [ ] `--minimal` generates shorter config
- [ ] Won't overwrite without `--force`

**Test Command**: `cargo test cli::tests::test_config_init`

---

## T14: Completions Command

**Goal**: Implement `nexus completions <shell>` for shell completion generation.

**Files to modify**:
- `src/cli/mod.rs` (add Completions command)

**Tests to Write First** (4 tests):
```rust
#[test]
fn test_completions_bash() {
    let cli = Cli::try_parse_from(["nexus", "completions", "bash"]).unwrap();
    match cli.command {
        Commands::Completions(args) => assert_eq!(args.shell, Shell::Bash),
        _ => panic!("Expected Completions command"),
    }
}

#[test]
fn test_completions_zsh() {
    let cli = Cli::try_parse_from(["nexus", "completions", "zsh"]).unwrap();
    match cli.command {
        Commands::Completions(args) => assert_eq!(args.shell, Shell::Zsh),
        _ => panic!("Expected Completions command"),
    }
}

#[test]
fn test_completions_generates_output() {
    let output = generate_completions(Shell::Bash);
    assert!(!output.is_empty());
    assert!(output.contains("nexus")); // Should reference the command name
}

#[test]
fn test_completions_fish() {
    let output = generate_completions(Shell::Fish);
    assert!(!output.is_empty());
}
```

**Implementation**:
```rust
use clap::CommandFactory;
use clap_complete::{generate, Shell};

#[derive(Args)]
pub struct CompletionsArgs {
    /// Target shell
    #[arg(value_enum)]
    pub shell: Shell,
}

pub fn handle_completions(args: &CompletionsArgs) -> String {
    let mut cmd = Cli::command();
    let mut buf = Vec::new();
    generate(args.shell, &mut cmd, "nexus", &mut buf);
    String::from_utf8(buf).expect("Generated completions should be valid UTF-8")
}
```

Add to `Commands` enum:
```rust
#[derive(Subcommand)]
pub enum Commands {
    // ... existing commands ...
    
    /// Generate shell completions
    Completions(CompletionsArgs),
}
```

Add to `Cargo.toml`:
```toml
clap_complete = "4"
```

**Acceptance Criteria**:
- [ ] All 4 tests pass
- [ ] `nexus completions bash` outputs valid bash completion script
- [ ] `nexus completions zsh` outputs valid zsh completion script
- [ ] `nexus completions fish` outputs valid fish completion script

**Test Command**: `cargo test cli::tests::test_completions`

---

## T15: Graceful Shutdown Handling

**Goal**: Implement proper shutdown on SIGINT/SIGTERM.

**Files to modify**:
- `src/cli/serve.rs`

**Tests to Write First** (3 tests):
```rust
#[tokio::test]
async fn test_shutdown_signal_triggers_cancel() {
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    
    let handle = tokio::spawn(async move {
        // Simulate shutdown after 100ms
        tokio::time::sleep(Duration::from_millis(100)).await;
        cancel_clone.cancel();
    });
    
    // This should return when cancelled
    tokio::select! {
        _ = cancel.cancelled() => {}
        _ = tokio::time::sleep(Duration::from_secs(5)) => {
            panic!("Shutdown didn't trigger");
        }
    }
    
    handle.await.unwrap();
}

#[tokio::test]
async fn test_health_checker_stops_on_shutdown() {
    let registry = Arc::new(Registry::new());
    let config = HealthCheckConfig::default();
    let checker = HealthChecker::new(registry, config);
    
    let cancel = CancellationToken::new();
    let handle = checker.start(cancel.clone());
    
    // Let it run briefly
    tokio::time::sleep(Duration::from_millis(50)).await;
    
    // Trigger shutdown
    cancel.cancel();
    
    // Should complete quickly
    let result = tokio::time::timeout(Duration::from_secs(1), handle).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_exit_code_on_error() {
    // Port 0 should fail
    let args = ServeArgs {
        config: PathBuf::from("nonexistent.toml"),
        port: Some(0),  // Invalid
        ..Default::default()
    };
    
    // This would require refactoring run_serve to return Result
    // For now, verify the function signature returns Result
}
```

**Implementation**:
```rust
async fn shutdown_signal(cancel_token: CancellationToken) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received SIGINT, shutting down...");
        }
        _ = terminate => {
            tracing::info!("Received SIGTERM, shutting down...");
        }
    }
    
    cancel_token.cancel();
}
```

**Acceptance Criteria**:
- [ ] All 3 tests pass
- [ ] SIGINT triggers graceful shutdown
- [ ] SIGTERM triggers graceful shutdown (Unix)
- [ ] Health checker stops cleanly
- [ ] Exit code 0 on clean shutdown

**Test Command**: `cargo test cli::serve::tests::test_shutdown`

---

## T16: Integration Tests

**Goal**: End-to-end CLI tests using `assert_cmd`.

**Files to create**:
- `tests/cli_integration.rs`

**Tests to Write** (10 tests):
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
fn test_help_shows_all_commands() {
    Command::cargo_bin("nexus")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("serve"))
        .stdout(predicate::str::contains("backends"))
        .stdout(predicate::str::contains("models"))
        .stdout(predicate::str::contains("health"))
        .stdout(predicate::str::contains("config"));
}

#[test]
fn test_serve_help() {
    Command::cargo_bin("nexus")
        .unwrap()
        .args(["serve", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--port"))
        .stdout(predicate::str::contains("--config"));
}

#[test]
fn test_backends_help() {
    Command::cargo_bin("nexus")
        .unwrap()
        .args(["backends", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("add"))
        .stdout(predicate::str::contains("remove"));
}

#[test]
fn test_config_init_creates_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output = temp_dir.path().join("test.toml");
    
    Command::cargo_bin("nexus")
        .unwrap()
        .args(["config", "init", "-o", output.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created"));
    
    assert!(output.exists());
}

#[test]
fn test_config_init_no_overwrite() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output = temp_dir.path().join("existing.toml");
    std::fs::write(&output, "existing").unwrap();
    
    Command::cargo_bin("nexus")
        .unwrap()
        .args(["config", "init", "-o", output.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn test_invalid_command() {
    Command::cargo_bin("nexus")
        .unwrap()
        .arg("invalid")
        .assert()
        .failure();
}

#[test]
fn test_serve_invalid_config_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config = temp_dir.path().join("bad.toml");
    std::fs::write(&config, "not valid toml {{{{").unwrap();
    
    Command::cargo_bin("nexus")
        .unwrap()
        .args(["serve", "-c", config.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("parse error").or(predicate::str::contains("error")));
}

#[test]
fn test_env_var_for_port() {
    Command::cargo_bin("nexus")
        .unwrap()
        .env("NEXUS_PORT", "9999")
        .args(["serve", "--help"])  // Just verify it parses
        .assert()
        .success();
}

#[test]
fn test_backends_list_no_server() {
    // Without a running server, this should show an error or empty list
    // Depends on implementation - might need IPC or shared state
    Command::cargo_bin("nexus")
        .unwrap()
        .args(["backends", "list"])
        .assert();
    // Just verify it doesn't panic
}
```

**Acceptance Criteria**:
- [ ] All 10 tests pass
- [ ] `--version` shows version string
- [ ] `--help` shows all commands
- [ ] Invalid config causes non-zero exit
- [ ] Environment variables are recognized

**Test Command**: `cargo test --test cli_integration`

---

## T17: Documentation & Cleanup

**Goal**: Final documentation, examples, and code cleanup.

**Files to modify/create**:
- `src/cli/mod.rs` (doc comments)
- `src/config/mod.rs` (doc comments)
- `README.md` (update with CLI usage)
- Move `nexus.example.toml` handling

**Tasks**:
1. Add doc comments to all public types:
   ```rust
   /// Unified configuration for the Nexus server.
   ///
   /// # Example
   /// ```
   /// use nexus::config::NexusConfig;
   /// 
   /// let config = NexusConfig::default();
   /// assert_eq!(config.server.port, 8000);
   /// ```
   #[derive(Debug, Clone, Serialize, Deserialize, Default)]
   pub struct NexusConfig { ... }
   ```

2. Add doc examples that compile:
   ```rust
   /// Parse configuration from a TOML string.
   ///
   /// # Example
   /// ```
   /// use nexus::config::NexusConfig;
   /// 
   /// let toml = "[server]\nport = 9000";
   /// let config: NexusConfig = toml::from_str(toml).unwrap();
   /// ```
   ```

3. Update README with CLI examples:
   ```markdown
   ## Quick Start
   
   ```bash
   # Generate config file
   nexus config init
   
   # Start the server
   nexus serve
   
   # Check status
   nexus health
   ```
   ```

4. Run final checks:
   - `cargo clippy --all-features -- -D warnings`
   - `cargo fmt --all -- --check`
   - `cargo test --all`
   - `cargo doc --no-deps`

**Acceptance Criteria**:
- [ ] All public types have doc comments
- [ ] Doc examples compile (`cargo test --doc`)
- [ ] README includes CLI usage
- [ ] `cargo clippy` has no warnings
- [ ] `cargo fmt` passes

**Test Command**: `cargo test --doc && cargo clippy`

---

## Test Summary

| Task | Unit Tests | Integration Tests | Doc Tests | Total |
|------|------------|-------------------|-----------|-------|
| T01 | 0 | 0 | 0 | 0 |
| T02 | 6 | 0 | 0 | 6 |
| T03 | 5 | 0 | 0 | 5 |
| T04 | 4 | 0 | 0 | 4 |
| T05 | 5 | 0 | 0 | 5 |
| T06 | 8 | 0 | 0 | 8 |
| T07 | 6 | 0 | 0 | 6 |
| T08 | 6 | 0 | 0 | 6 |
| T09 | 4 | 0 | 0 | 4 |
| T10 | 7 | 0 | 0 | 7 |
| T11 | 4 | 0 | 0 | 4 |
| T12 | 4 | 0 | 0 | 4 |
| T13 | 4 | 0 | 0 | 4 |
| T14 | 4 | 0 | 0 | 4 |
| T15 | 3 | 0 | 0 | 3 |
| T16 | 0 | 10 | 0 | 10 |
| T17 | 0 | 0 | ~4 | 4 |
| **Total** | **57** | **10** | **~4** | **~71** |

---

## Definition of Done

- [ ] All 71 tests pass
- [ ] `cargo clippy` reports no warnings
- [ ] `cargo fmt --check` passes
- [ ] `nexus --version` shows version
- [ ] `nexus --help` shows all commands
- [ ] `nexus serve` starts server on configured port
- [ ] `nexus config init` generates valid config
- [ ] `nexus completions <shell>` generates valid completions
- [ ] Config precedence: CLI > env > file > defaults
- [ ] Unknown config keys logged as warnings (not errors)
- [ ] Graceful shutdown on SIGINT/SIGTERM
- [ ] JSON output valid for all commands
- [ ] Documentation complete with examples
