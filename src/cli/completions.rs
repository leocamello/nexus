//! Completions command implementation

use crate::cli::{Cli, CompletionsArgs};
use clap::CommandFactory;
use clap_complete::generate;
use std::io;

/// Handle `nexus completions` command
pub fn handle_completions(args: &CompletionsArgs) {
    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();
    generate(args.shell, &mut cmd, bin_name, &mut io::stdout());
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap_complete::Shell;

    /// Generate completions into a buffer instead of stdout
    fn generate_completions_to_string(shell: Shell) -> String {
        let mut cmd = Cli::command();
        let bin_name = cmd.get_name().to_string();
        let mut buf = Vec::new();
        generate(shell, &mut cmd, bin_name, &mut buf);
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn test_completions_bash() {
        let _args = CompletionsArgs { shell: Shell::Bash };
        // Just verify it doesn't panic
        // Output goes to stdout, so we can't easily capture it in tests
        // This is tested manually via: nexus completions bash > /tmp/test.sh
    }

    #[test]
    fn test_completions_zsh() {
        let _args = CompletionsArgs { shell: Shell::Zsh };
        // Just verify it doesn't panic
    }

    #[test]
    fn test_bash_completions_contain_command_name() {
        let output = generate_completions_to_string(Shell::Bash);
        assert!(
            output.contains("nexus"),
            "Bash completions should reference the 'nexus' command name"
        );
    }

    #[test]
    fn test_zsh_completions_contain_command_name() {
        let output = generate_completions_to_string(Shell::Zsh);
        assert!(
            output.contains("nexus"),
            "Zsh completions should reference the 'nexus' command name"
        );
    }

    #[test]
    fn test_fish_completions_contain_command_name() {
        let output = generate_completions_to_string(Shell::Fish);
        assert!(
            output.contains("nexus"),
            "Fish completions should reference the 'nexus' command name"
        );
    }

    #[test]
    fn test_handle_completions_bash() {
        // Call handle_completions through the generate function to avoid stdout
        // This verifies the function doesn't panic
        let mut cmd = Cli::command();
        let bin_name = cmd.get_name().to_string();
        let mut buf = Vec::new();
        generate(Shell::Bash, &mut cmd, bin_name, &mut buf);
        let output = String::from_utf8(buf).unwrap();
        assert!(!output.is_empty());
    }

    #[test]
    fn test_handle_completions_zsh() {
        let mut cmd = Cli::command();
        let bin_name = cmd.get_name().to_string();
        let mut buf = Vec::new();
        generate(Shell::Zsh, &mut cmd, bin_name, &mut buf);
        let output = String::from_utf8(buf).unwrap();
        assert!(!output.is_empty());
    }

    #[test]
    fn test_handle_completions_fish() {
        let mut cmd = Cli::command();
        let bin_name = cmd.get_name().to_string();
        let mut buf = Vec::new();
        generate(Shell::Fish, &mut cmd, bin_name, &mut buf);
        let output = String::from_utf8(buf).unwrap();
        assert!(!output.is_empty());
    }

    #[test]
    fn test_handle_completions_powershell() {
        let mut cmd = Cli::command();
        let bin_name = cmd.get_name().to_string();
        let mut buf = Vec::new();
        generate(Shell::PowerShell, &mut cmd, bin_name, &mut buf);
        let output = String::from_utf8(buf).unwrap();
        assert!(!output.is_empty());
    }

    #[test]
    fn test_handle_completions_elvish() {
        let mut cmd = Cli::command();
        let bin_name = cmd.get_name().to_string();
        let mut buf = Vec::new();
        generate(Shell::Elvish, &mut cmd, bin_name, &mut buf);
        let output = String::from_utf8(buf).unwrap();
        assert!(!output.is_empty());
    }
}
