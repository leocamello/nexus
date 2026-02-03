use clap::Parser;
use nexus::cli::{handle_completions, handle_config_init, Cli, Commands, ConfigCommands};

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve(_) => {
            println!("Starting Nexus server...");
            println!("Serve command not yet fully implemented");
        }
        Commands::Backends(_) => {
            println!("Backends command not yet fully implemented");
        }
        Commands::Models(_) => {
            println!("Models command not yet fully implemented");
        }
        Commands::Health(_) => {
            println!("Health command not yet fully implemented");
        }
        Commands::Config(config_cmd) => match config_cmd {
            ConfigCommands::Init(args) => {
                if let Err(e) = handle_config_init(&args) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        },
        Commands::Completions(args) => {
            handle_completions(&args);
        }
    }
}
