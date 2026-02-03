use clap::Parser;
use nexus::cli::{
    backends, health, models, handle_completions, handle_config_init, Cli, Commands,
    BackendsCommands, ConfigCommands,
};
use nexus::config::NexusConfig;
use nexus::registry::Registry;
use std::sync::Arc;
use std::time::Instant;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let start_time = Instant::now();

    let result = match cli.command {
        Commands::Serve(args) => nexus::cli::serve::run_serve(args).await,
        Commands::Backends(cmd) => match cmd {
            BackendsCommands::List(args) => {
                // Load config to get backends
                let config = NexusConfig::load(Some(&args.config))
                    .unwrap_or_else(|_| NexusConfig::default());
                let registry = Arc::new(Registry::new());
                
                // Load static backends from config
                if let Err(e) = nexus::cli::serve::load_backends_from_config(&config, &registry) {
                    eprintln!("Warning: Failed to load backends: {}", e);
                }

                match backends::handle_backends_list(&args, &registry) {
                    Ok(output) => {
                        println!("{}", output);
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
            BackendsCommands::Add(args) => {
                let registry = Arc::new(Registry::new());
                match backends::handle_backends_add(&args, &registry).await {
                    Ok(msg) => {
                        println!("{}", msg);
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
            BackendsCommands::Remove(args) => {
                let registry = Arc::new(Registry::new());
                // TODO: Load registry from config first
                match backends::handle_backends_remove(&args, &registry) {
                    Ok(msg) => {
                        println!("{}", msg);
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
        },
        Commands::Models(args) => {
            let config = NexusConfig::load(Some(&args.config))
                .unwrap_or_else(|_| NexusConfig::default());
            let registry = Arc::new(Registry::new());
            
            if let Err(e) = nexus::cli::serve::load_backends_from_config(&config, &registry) {
                eprintln!("Warning: Failed to load backends: {}", e);
            }

            match models::handle_models(&args, &registry) {
                Ok(output) => {
                    println!("{}", output);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        Commands::Health(args) => {
            let config = NexusConfig::load(Some(&args.config))
                .unwrap_or_else(|_| NexusConfig::default());
            let registry = Arc::new(Registry::new());
            
            if let Err(e) = nexus::cli::serve::load_backends_from_config(&config, &registry) {
                eprintln!("Warning: Failed to load backends: {}", e);
            }

            let uptime = start_time.elapsed();
            match health::handle_health(&args, &registry, uptime) {
                Ok(output) => {
                    println!("{}", output);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        Commands::Config(config_cmd) => match config_cmd {
            ConfigCommands::Init(args) => handle_config_init(&args),
        },
        Commands::Completions(args) => {
            handle_completions(&args);
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
