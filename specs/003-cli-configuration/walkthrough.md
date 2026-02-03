# CLI & Configuration - Code Walkthrough

**Feature**: F04 - CLI & Configuration  
**Audience**: Junior developers joining the project  
**Last Updated**: 2026-02-03

---

## Table of Contents

1. [The Big Picture](#the-big-picture)
2. [File Structure](#file-structure)
3. [Part 1: Configuration Module](#part-1-configuration-module)
   - [mod.rs - The Main Config](#modrs---the-main-config)
   - [server.rs - Server Settings](#serverrs---server-settings)
   - [routing.rs - Routing Settings](#routingrs---routing-settings)
   - [logging.rs - Logging Settings](#loggingrs---logging-settings)
   - [error.rs - Config Errors](#errorrs---config-errors)
4. [Part 2: CLI Module](#part-2-cli-module)
   - [mod.rs - Command Definitions](#modrs---command-definitions)
   - [serve.rs - The Main Server](#servers---the-main-server)
   - [backends.rs - Backend Management](#backendsrs---backend-management)
   - [output.rs - Pretty Printing](#outputrs---pretty-printing)
   - [completions.rs - Shell Completions](#completionsrs---shell-completions)
5. [Part 3: Main Entry Point](#part-3-main-entry-point)
6. [Understanding the Tests](#understanding-the-tests)
7. [Key Rust Concepts](#key-rust-concepts)
8. [Common Patterns in This Module](#common-patterns-in-this-module)

---

## The Big Picture

Think of this feature as the **user interface for Nexus**. Just like how your phone has a home screen and settings app, Nexus has:

1. **CLI (Command Line Interface)** - The buttons and menus you interact with
2. **Configuration** - The settings that control how everything works

### Why Do We Need This?

Without a CLI and config system:
- Users would have to modify code to change settings
- There'd be no way to start the server
- No way to inspect what's running

With CLI & Configuration:
```bash
# Start with defaults
nexus serve

# Override port via CLI
nexus serve --port 9000

# Or via environment variable
NEXUS_PORT=9000 nexus serve

# Or via config file
nexus serve --config production.toml
```

### The Configuration Precedence Pyramid

```
        ┌─────────────┐
        │   CLI args  │  ← Highest priority (--port 9000)
        ├─────────────┤
        │  Env vars   │  ← NEXUS_PORT=9000
        ├─────────────┤
        │ Config file │  ← nexus.toml
        ├─────────────┤
        │  Defaults   │  ← Lowest priority (port = 8000)
        └─────────────┘
```

This means if you set `--port 9000` on the command line, it wins over everything else.

### How It Fits in Nexus

```
┌─────────────────────────────────────────────────────────────────┐
│                         Nexus                                   │
│                                                                 │
│  ┌──────────────────┐     ┌──────────────────────────────┐     │
│  │  CLI (mod.rs)    │────▶│  Configuration (config/)     │     │
│  │  (you are here!) │     │  - Server settings           │     │
│  └──────────────────┘     │  - Routing rules             │     │
│           │               │  - Backend definitions       │     │
│           │               └──────────────────────────────┘     │
│           ▼                                                     │
│  ┌──────────────┐     ┌──────────────┐     ┌──────────────┐    │
│  │    serve     │────▶│   Registry   │◀────│Health Checker│    │
│  │   command    │     │              │     │              │    │
│  └──────────────┘     └──────────────┘     └──────────────┘    │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## File Structure

```
src/
├── main.rs                 # Entry point - routes CLI commands
├── config/                 # Configuration module
│   ├── mod.rs    (267 lines) # Main NexusConfig + loading logic
│   ├── server.rs  (38 lines) # Server settings (port, host, etc.)
│   ├── routing.rs (81 lines) # Routing strategy and weights
│   ├── logging.rs (63 lines) # Log level and format
│   ├── discovery.rs (23 lines) # mDNS discovery settings
│   ├── backend.rs (23 lines) # Backend config definition
│   └── error.rs   (23 lines) # Configuration errors
│
├── cli/                    # CLI module
│   ├── mod.rs    (282 lines) # Command definitions (clap)
│   ├── serve.rs  (314 lines) # Start server command
│   ├── backends.rs (329 lines) # backends list/add/remove
│   ├── models.rs (168 lines) # List models
│   ├── health.rs (226 lines) # Show health status
│   ├── output.rs (172 lines) # Table/JSON formatting
│   ├── config.rs  (88 lines) # config init command
│   └── completions.rs (33 lines) # Shell completions
│
tests/
└── cli_integration.rs (119 lines) # End-to-end CLI tests
```

---

## Part 1: Configuration Module

The config module holds all the settings for Nexus. Each file represents a "section" of the config file.

### mod.rs - The Main Config

This is the hub that combines all config sections into one struct:

```rust
/// Unified configuration for the Nexus server.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct NexusConfig {
    pub server: ServerConfig,           // [server] section
    pub discovery: DiscoveryConfig,     // [discovery] section
    pub health_check: HealthCheckConfig, // [health_check] section
    pub routing: RoutingConfig,         // [routing] section
    pub backends: Vec<BackendConfig>,   // [[backends]] array
    pub logging: LoggingConfig,         // [logging] section
}
```

**What's happening here:**
- `#[derive(Serialize, Deserialize)]` - Lets us read/write TOML files automatically
- `#[serde(default)]` - If a section is missing, use the default values
- Each field corresponds to a section in `nexus.toml`

**Loading Configuration:**

```rust
impl NexusConfig {
    pub fn load(path: Option<&Path>) -> Result<Self, ConfigError> {
        match path {
            Some(p) => {
                if !p.exists() {
                    return Err(ConfigError::NotFound(p.to_path_buf()));
                }
                let content = std::fs::read_to_string(p)?;
                toml::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))
            }
            None => Ok(Self::default()),
        }
    }
}
```

**What's happening:**
1. If no path given, return defaults
2. If path doesn't exist, return a clear error
3. Read the file, parse as TOML
4. serde + toml do the magic of turning text into structs

**Environment Variable Overrides:**

```rust
pub fn with_env_overrides(mut self) -> Self {
    // Server settings
    if let Ok(port) = std::env::var("NEXUS_PORT") {
        if let Ok(p) = port.parse() {
            self.server.port = p;
        }
    }
    // ... more overrides
    self
}
```

**Pattern explained:**
- `std::env::var("NEXUS_PORT")` - Read environment variable
- `port.parse()` - Try to convert string to number
- If parsing fails, silently keep the default (don't crash!)

### server.rs - Server Settings

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: String,                  // Listen address
    pub port: u16,                     // Listen port
    pub request_timeout_seconds: u64,  // How long to wait for responses
    pub max_concurrent_requests: u32,  // Max parallel requests
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),  // Listen on all interfaces
            port: 8000,                    // Default port
            request_timeout_seconds: 300,  // 5 minutes
            max_concurrent_requests: 1000,
        }
    }
}
```

**In the config file:**
```toml
[server]
host = "0.0.0.0"
port = 8000
```

### routing.rs - Routing Settings

```rust
/// Routing strategy for backend selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RoutingStrategy {
    #[default]
    Smart,        // Consider multiple factors
    RoundRobin,   // Take turns
    PriorityOnly, // Always use highest priority
    Random,       // Random selection
}
```

**What's happening:**
- `#[serde(rename_all = "snake_case")]` - In TOML, we write `round_robin` not `RoundRobin`
- `#[default]` - `Smart` is used if not specified

**In the config file:**
```toml
[routing]
strategy = "round_robin"
max_retries = 2

[routing.weights]
priority = 50
load = 30
latency = 20
```

### logging.rs - Logging Settings

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    #[default]
    Pretty,  // Human-readable with colors
    Json,    // Machine-readable for log aggregators
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub level: String,    // trace, debug, info, warn, error
    pub format: LogFormat,
}
```

**Why two formats?**
- **Pretty**: For humans running `nexus serve` in a terminal
- **JSON**: For production systems that feed logs to Datadog, Splunk, etc.

### error.rs - Config Errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config file not found: {0}")]
    NotFound(PathBuf),
    
    #[error("failed to read config: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("config parse error: {0}")]
    Parse(String),
    
    #[error("invalid value for '{field}': {message}")]
    Validation { field: String, message: String },
}
```

**What's happening:**
- `thiserror::Error` - Automatically implements `std::error::Error` and `Display`
- `#[from]` - Automatically converts `std::io::Error` to `ConfigError::Io`
- `{field}` and `{message}` - Replaced with actual values when error is displayed

---

## Part 2: CLI Module

The CLI module defines all the commands and their arguments using the `clap` library.

### mod.rs - Command Definitions

**The Main CLI Struct:**

```rust
/// Nexus - Distributed LLM Orchestrator
#[derive(Parser, Debug)]
#[command(name = "nexus", version, about = "Distributed LLM model serving orchestrator")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}
```

**What's happening:**
- `#[derive(Parser)]` - Magic! Clap generates parsing code automatically
- `#[command(...)]` - Metadata shown in `--help`
- `#[command(subcommand)]` - This field holds which command was chosen

**The Commands Enum:**

```rust
#[derive(Subcommand, Debug)]
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
    
    /// Generate shell completions
    Completions(CompletionsArgs),
}
```

**How this becomes CLI:**
```bash
nexus serve          # Commands::Serve
nexus backends list  # Commands::Backends(BackendsCommands::List)
nexus models         # Commands::Models
nexus health         # Commands::Health
nexus config init    # Commands::Config(ConfigCommands::Init)
nexus completions    # Commands::Completions
```

**Argument Definition Example:**

```rust
#[derive(Args, Debug)]
pub struct ServeArgs {
    /// Path to configuration file
    #[arg(short, long, default_value = "nexus.toml")]
    pub config: PathBuf,

    /// Override server port
    #[arg(short, long, env = "NEXUS_PORT")]
    pub port: Option<u16>,

    /// Disable mDNS backend discovery
    #[arg(long)]
    pub no_discovery: bool,
}
```

**What's happening:**
- `#[arg(short, long)]` - Accept `-c` or `--config`
- `default_value = "nexus.toml"` - Use this if not specified
- `env = "NEXUS_PORT"` - Also check this env var
- `Option<u16>` - This argument is optional
- `bool` flags default to `false`

### serve.rs - The Main Server

This is where Nexus actually starts up:

```rust
pub async fn run_serve(args: ServeArgs) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load and merge configuration
    let config = load_config_with_overrides(&args)?;
    config.validate()?;

    // 2. Initialize tracing (logging)
    init_tracing(&config.logging)?;
    tracing::info!("Starting Nexus server");

    // 3. Create registry and load static backends
    let registry = Arc::new(Registry::new());
    load_backends_from_config(&config, &registry)?;

    // 4. Start health checker (if enabled)
    let cancel_token = CancellationToken::new();
    let health_handle = if config.health_check.enabled {
        let checker = HealthChecker::new(registry.clone(), config.health_check.clone());
        Some(checker.start(cancel_token.clone()))
    } else {
        None
    };

    // 5. Build minimal HTTP server
    let app = build_basic_router(registry.clone());

    // 6. Bind and serve
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(cancel_token.clone()))
        .await?;

    // 7. Cleanup
    if let Some(handle) = health_handle {
        handle.await?;
    }

    Ok(())
}
```

**The startup sequence:**
1. **Load config** - File → env vars → CLI args
2. **Initialize logging** - Before anything else, so we can see what's happening
3. **Create registry** - The in-memory database of backends
4. **Start health checker** - Background task to monitor backends
5. **Build HTTP router** - The API endpoints
6. **Start serving** - Listen for requests
7. **Cleanup on shutdown** - Stop background tasks gracefully

**Graceful Shutdown:**

```rust
async fn shutdown_signal(cancel_token: CancellationToken) {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("Failed to install CTRL+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received SIGINT, shutting down..."),
        _ = terminate => tracing::info!("Received SIGTERM, shutting down..."),
    }

    cancel_token.cancel();  // Tell health checker to stop
}
```

**What's happening:**
- Listen for Ctrl+C (SIGINT) or kill signal (SIGTERM)
- When received, cancel the token which stops background tasks
- Server finishes current requests and shuts down cleanly

### backends.rs - Backend Management

**Listing Backends:**

```rust
pub fn handle_backends_list(
    args: &BackendsListArgs,
    registry: &Registry,
) -> Result<String, Box<dyn std::error::Error>> {
    let backends = registry.get_all_backends();

    // Filter by status if provided
    let filtered: Vec<Backend> = if let Some(ref status) = args.status {
        let target_status = parse_status(status)?;
        backends.into_iter().filter(|b| b.status == target_status).collect()
    } else {
        backends
    };

    // Convert to view models for display
    let views: Vec<BackendView> = filtered.iter().map(BackendView::from).collect();

    if args.json {
        Ok(format_backends_json(&views))
    } else {
        Ok(format_backends_table(&views))
    }
}
```

**Auto-detecting Backend Type:**

```rust
async fn detect_backend_type(base_url: &str) -> Option<BackendType> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))  // Don't wait forever
        .build().ok()?;

    // Try Ollama: GET /api/tags
    if let Ok(resp) = client.get(format!("{}/api/tags", base_url)).send().await {
        if resp.status().is_success() {
            return Some(BackendType::Ollama);
        }
    }

    // Try LlamaCpp: GET /health
    if let Ok(resp) = client.get(format!("{}/health", base_url)).send().await {
        if resp.status().is_success() {
            return Some(BackendType::LlamaCpp);
        }
    }

    // Try OpenAI-compatible: GET /v1/models
    if let Ok(resp) = client.get(format!("{}/v1/models", base_url)).send().await {
        if resp.status().is_success() {
            return Some(BackendType::Generic);
        }
    }

    None  // Unknown, will use Generic as fallback
}
```

**How it works:**
1. Create HTTP client with 2-second timeout
2. Try Ollama's unique endpoint
3. Try LlamaCpp's unique endpoint
4. Try generic OpenAI endpoint
5. Return `None` if nothing works (caller will use Generic)

### output.rs - Pretty Printing

**The BackendView Pattern:**

```rust
/// View model for backend display
#[derive(Debug, Clone, serde::Serialize)]
pub struct BackendView {
    pub name: String,
    pub url: String,
    pub backend_type: String,
    pub status: BackendStatus,
    pub models: Vec<String>,
    pub avg_latency_ms: u64,
}

impl From<&Backend> for BackendView {
    fn from(backend: &Backend) -> Self {
        Self {
            name: backend.name.clone(),
            url: backend.url.clone(),
            backend_type: format!("{:?}", backend.backend_type),
            status: backend.status,
            models: backend.models.iter().map(|m| m.id.clone()).collect(),
            avg_latency_ms: backend.avg_latency_ms.load(Ordering::Relaxed) as u64,
        }
    }
}
```

**Why use a View Model?**
- The internal `Backend` struct has complex fields (atomics, hashmaps)
- `BackendView` is simple and easy to serialize to JSON or display in a table
- Separates "what we store" from "what we show"

**Table Formatting:**

```rust
pub fn format_backends_table(backends: &[BackendView]) -> String {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);  // Pretty Unicode borders
    table.set_header(vec!["Name", "URL", "Type", "Status", "Models", "Latency"]);

    for b in backends {
        let status_str = match b.status {
            BackendStatus::Healthy => "Healthy".green().to_string(),
            BackendStatus::Unhealthy => "Unhealthy".red().to_string(),
            BackendStatus::Unknown => "Unknown".yellow().to_string(),
            BackendStatus::Draining => "Draining".cyan().to_string(),
        };

        table.add_row(vec![
            Cell::new(&b.name),
            Cell::new(&b.url),
            Cell::new(&b.backend_type),
            Cell::new(status_str),
            Cell::new(b.models.len()),
            Cell::new(format!("{}ms", b.avg_latency_ms)),
        ]);
    }

    table.to_string()
}
```

**Result:**
```
┌──────────────┬────────────────────────────┬─────────┬──────────┬────────┬─────────┐
│ Name         │ URL                        │ Type    │ Status   │ Models │ Latency │
├──────────────┼────────────────────────────┼─────────┼──────────┼────────┼─────────┤
│ local-ollama │ http://localhost:11434     │ Ollama  │ Healthy  │ 3      │ 45ms    │
│ gpu-server   │ http://192.168.1.100:8000  │ VLLM    │ Healthy  │ 1      │ 23ms    │
└──────────────┴────────────────────────────┴─────────┴──────────┴────────┴─────────┘
```

### completions.rs - Shell Completions

```rust
pub fn handle_completions(args: &CompletionsArgs) -> String {
    let mut cmd = Cli::command();
    let mut buf = Vec::new();
    
    clap_complete::generate(args.shell, &mut cmd, "nexus", &mut buf);
    
    String::from_utf8(buf).unwrap()
}
```

**What's happening:**
- `Cli::command()` - Get the command structure from clap
- `clap_complete::generate` - Generate completion script for the target shell
- Return as string so user can pipe to file

**Usage:**
```bash
nexus completions bash > ~/.bash_completion.d/nexus
nexus completions zsh > ~/.zsh/completions/_nexus
```

---

## Part 3: Main Entry Point

**src/main.rs:**

```rust
#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Serve(args) => {
            nexus::cli::serve::run_serve(args).await
        }
        Commands::Backends(cmd) => match cmd {
            BackendsCommands::List(args) => {
                // Load config and create registry
                let config = NexusConfig::load(Some(&args.config))
                    .unwrap_or_else(|_| NexusConfig::default());
                let registry = Arc::new(Registry::new());
                
                // Load backends from config
                load_backends_from_config(&config, &registry)?;

                // Run command
                let output = backends::handle_backends_list(&args, &registry)?;
                println!("{}", output);
                Ok(())
            }
            // ... other commands
        }
        Commands::Completions(args) => {
            println!("{}", handle_completions(&args));
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
```

**Pattern:**
1. Parse CLI arguments
2. Match on command
3. Execute appropriate handler
4. Print output or error
5. Exit with appropriate code

---

## Understanding the Tests

### Config Module Tests

**Testing defaults:**
```rust
#[test]
fn test_nexus_config_defaults() {
    let config = NexusConfig::default();
    assert_eq!(config.server.port, 8000);
    assert!(config.discovery.enabled);
    assert!(config.backends.is_empty());
}
```

**Testing TOML parsing:**
```rust
#[test]
fn test_config_parse_minimal_toml() {
    let toml = r#"
    [server]
    port = 9000
    "#;

    let config: NexusConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.server.port, 9000);
    assert_eq!(config.server.host, "0.0.0.0");  // Default applied!
}
```

**Testing environment overrides:**
```rust
#[test]
fn test_config_env_override_port() {
    std::env::set_var("NEXUS_PORT", "9999");
    let config = NexusConfig::default().with_env_overrides();
    std::env::remove_var("NEXUS_PORT");  // Clean up!

    assert_eq!(config.server.port, 9999);
}
```

### CLI Module Tests

**Testing argument parsing:**
```rust
#[test]
fn test_cli_parse_serve_with_port() {
    let cli = Cli::try_parse_from(["nexus", "serve", "-p", "9000"]).unwrap();
    match cli.command {
        Commands::Serve(args) => assert_eq!(args.port, Some(9000)),
        _ => panic!("Expected Serve command"),
    }
}
```

**Key technique:** `try_parse_from` lets you test parsing without running a real CLI.

### Integration Tests

**tests/cli_integration.rs:**
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
fn test_config_init_creates_file() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("nexus.toml");

    Command::cargo_bin("nexus")
        .unwrap()
        .args(["config", "init", "-o", config_path.to_str().unwrap()])
        .assert()
        .success();

    assert!(config_path.exists());
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("[server]"));
}
```

**What's happening:**
- `assert_cmd` runs the actual binary in a subprocess
- `predicates` provides nice assertions for output
- `TempDir` creates isolated test directories that clean up automatically

---

## Key Rust Concepts

### 1. The Builder Pattern

```rust
let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(2))
    .build()
    .ok()?;
```

Start with a builder, chain method calls, finish with `.build()`.

### 2. Error Propagation with `?`

```rust
let content = std::fs::read_to_string(p)?;  // If error, return early
toml::from_str(&content)?                    // If error, return early
```

The `?` operator unwraps `Ok` values or returns `Err` immediately.

### 3. Option Chaining

```rust
let name = args.name.clone().unwrap_or_else(|| {
    url.host_str().unwrap_or("backend").to_string()
});
```

- If `args.name` is `Some`, use it
- Otherwise, try to get hostname from URL
- Otherwise, use "backend"

### 4. Enums for States

```rust
pub enum LogFormat {
    Pretty,
    Json,
}

match config.format {
    LogFormat::Pretty => { /* pretty formatting */ }
    LogFormat::Json => { /* JSON formatting */ }
}
```

Enums + match = exhaustive handling of all cases.

### 5. Arc for Shared Ownership

```rust
let registry = Arc::new(Registry::new());
load_backends_from_config(&config, &registry)?;
// registry can now be cloned and shared across threads
```

`Arc` = Atomic Reference Counting. Multiple owners, thread-safe.

---

## Common Patterns in This Module

### 1. Layered Configuration

```
Defaults → File → Env → CLI
```

Each layer can override the previous. This pattern is common in cloud-native apps.

### 2. View Models for Display

```rust
Backend (internal) → BackendView (display)
```

Don't expose internal complexity to the UI layer.

### 3. Command-Handler Separation

```rust
// mod.rs defines structure
pub struct ServeArgs { ... }

// serve.rs implements logic
pub async fn run_serve(args: ServeArgs) { ... }
```

Keep command definitions separate from implementation.

### 4. Graceful Degradation

```rust
if let Ok(p) = port.parse() {
    self.server.port = p;
}
// If parse fails, just keep the default
```

Don't crash on invalid input when you can use sensible defaults.

### 5. Feature Flags via CLI

```rust
#[arg(long)]
pub no_discovery: bool,

#[arg(long)]
pub no_health_check: bool,
```

Let users disable features at runtime with `--no-X` flags.

---

## Summary

The CLI & Configuration module provides the user-facing interface for Nexus:

| Component | Purpose |
|-----------|---------|
| `config/mod.rs` | Load and merge configuration from multiple sources |
| `config/server.rs` | HTTP server settings |
| `config/routing.rs` | Routing strategy and weights |
| `cli/mod.rs` | Define all commands using clap |
| `cli/serve.rs` | Start the server with all components |
| `cli/backends.rs` | Manage backends (list, add, remove) |
| `cli/output.rs` | Format data for terminal or JSON |

**Test Count**: 173 tests total (151 unit + 10 CLI integration + 6 health + 6 doc)

**Next up**: F01 (API Gateway) will add the OpenAI-compatible HTTP endpoints that clients actually talk to!
