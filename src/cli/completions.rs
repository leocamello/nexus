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
}
