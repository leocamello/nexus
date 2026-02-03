//! CLI module for Nexus
//!
//! Command-line interface definitions and handlers for the Nexus LLM orchestrator.
//!
//! # Commands
//!
//! - `serve` - Start the Nexus server
//! - `backends` - Manage LLM backends (list, add, remove)
//! - `models` - List available models across all backends
//! - `health` - Show system health status
//! - `config` - Configuration utilities (init)
//! - `completions` - Generate shell completions
//!
//! # Example
//!
//! ```bash
//! # Start server with default config
//! nexus serve
//!
//! # List backends with status
//! nexus backends list --status healthy
//!
//! # Generate shell completions
//! nexus completions bash > ~/.bash_completion.d/nexus
//! ```

pub mod backends;
pub mod completions;
pub mod config;
pub mod health;
pub mod models;
pub mod output;
pub mod serve;

pub use completions::handle_completions;
pub use config::handle_config_init;

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// Nexus - Distributed LLM Orchestrator
#[derive(Parser, Debug)]
#[command(
    name = "nexus",
    version,
    about = "Distributed LLM model serving orchestrator"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

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

#[derive(Args, Debug)]
pub struct ServeArgs {
    /// Path to configuration file
    #[arg(short, long, default_value = "nexus.toml")]
    pub config: PathBuf,

    /// Override server port
    #[arg(short, long, env = "NEXUS_PORT")]
    pub port: Option<u16>,

    /// Override server host
    #[arg(short = 'H', long, env = "NEXUS_HOST")]
    pub host: Option<String>,

    /// Set log level (trace, debug, info, warn, error)
    #[arg(short, long, env = "NEXUS_LOG_LEVEL")]
    pub log_level: Option<String>,

    /// Disable mDNS backend discovery
    #[arg(long)]
    pub no_discovery: bool,

    /// Disable health checks
    #[arg(long)]
    pub no_health_check: bool,
}

#[derive(Subcommand, Debug)]
pub enum BackendsCommands {
    /// List configured and discovered backends
    List(BackendsListArgs),
    /// Add a new backend
    Add(BackendsAddArgs),
    /// Remove a backend
    Remove(BackendsRemoveArgs),
}

#[derive(Args, Debug)]
pub struct BackendsListArgs {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Filter by status (healthy, unhealthy, unknown, draining)
    #[arg(short, long)]
    pub status: Option<String>,

    /// Path to configuration file
    #[arg(short, long, default_value = "nexus.toml")]
    pub config: PathBuf,
}

#[derive(Args, Debug)]
pub struct BackendsAddArgs {
    /// Backend URL (e.g., http://localhost:11434)
    pub url: String,

    /// Backend name (optional, auto-detected if not provided)
    #[arg(short, long)]
    pub name: Option<String>,

    /// Backend type (ollama, vllm, openai, claude)
    #[arg(short = 't', long)]
    pub backend_type: Option<String>,

    /// Priority (lower = higher priority)
    #[arg(short, long, default_value = "50")]
    pub priority: i32,

    /// Path to configuration file
    #[arg(short, long, default_value = "nexus.toml")]
    pub config: PathBuf,
}

#[derive(Args, Debug)]
pub struct BackendsRemoveArgs {
    /// Backend name to remove
    pub name: String,

    /// Path to configuration file
    #[arg(short, long, default_value = "nexus.toml")]
    pub config: PathBuf,
}

#[derive(Args, Debug)]
pub struct ModelsArgs {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Filter by backend ID
    #[arg(short, long)]
    pub backend: Option<String>,

    /// Path to configuration file
    #[arg(short = 'c', long, default_value = "nexus.toml")]
    pub config: PathBuf,
}

#[derive(Args, Debug)]
pub struct HealthArgs {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Path to configuration file
    #[arg(short, long, default_value = "nexus.toml")]
    pub config: PathBuf,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Initialize a new configuration file
    Init(ConfigInitArgs),
}

#[derive(Args, Debug)]
pub struct ConfigInitArgs {
    /// Output file path
    #[arg(short, long, default_value = "nexus.toml")]
    pub output: PathBuf,

    /// Overwrite existing file
    #[arg(short, long)]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct CompletionsArgs {
    /// Shell to generate completions for
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

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
        assert!(matches!(
            cli.command,
            Commands::Backends(BackendsCommands::List(_))
        ));
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
        let cli =
            Cli::try_parse_from(["nexus", "backends", "add", "http://localhost:11434"]).unwrap();
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
}
