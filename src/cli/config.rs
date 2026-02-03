//! Config command handlers

use crate::cli::ConfigInitArgs;
use std::fs;

const EXAMPLE_CONFIG: &str = include_str!("../../nexus.example.toml");

/// Handle `nexus config init` command
pub fn handle_config_init(args: &ConfigInitArgs) -> Result<(), Box<dyn std::error::Error>> {
    // Check if file exists
    if args.output.exists() && !args.force {
        return Err(format!(
            "File already exists: {}. Use --force to overwrite.",
            args.output.display()
        )
        .into());
    }

    // Write config file
    fs::write(&args.output, EXAMPLE_CONFIG)?;

    println!("✓ Configuration file created: {}", args.output.display());
    println!("  Edit this file to customize your Nexus instance.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile;

    #[test]
    fn test_config_init_creates_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let output_path = temp_dir.path().join("nexus.toml");

        let args = ConfigInitArgs {
            output: output_path.clone(),
            force: false,
        };

        handle_config_init(&args).unwrap();

        assert!(output_path.exists());
        let content = std::fs::read_to_string(&output_path).unwrap();
        assert!(content.contains("[server]"));
    }

    #[test]
    fn test_config_init_no_overwrite() {
        let temp_dir = tempfile::tempdir().unwrap();
        let output_path = temp_dir.path().join("nexus.toml");

        // Create existing file
        std::fs::write(&output_path, "existing").unwrap();

        let args = ConfigInitArgs {
            output: output_path.clone(),
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
            force: true,
        };

        handle_config_init(&args).unwrap();

        let content = std::fs::read_to_string(&output_path).unwrap();
        assert!(content.contains("[server]"));
    }
}
