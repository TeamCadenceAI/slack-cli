//! Shell completion generation for Slack CLI
//!
//! Generates completion scripts for bash, zsh, fish, and other shells.

use clap::{Args, CommandFactory};
use clap_complete::{generate, Shell};
use std::io;

use super::root::Cli;

/// Arguments for completion generation
#[derive(Args, Debug)]
pub struct CompletionsArgs {
    /// Shell to generate completions for
    #[arg(value_enum)]
    pub shell: Shell,
}

/// Generate shell completions for the CLI
pub fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut io::stdout());
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap_complete::Shell;

    #[test]
    fn test_generate_bash_completions() {
        // Just ensure it doesn't panic
        let mut cmd = Cli::command();
        let name = cmd.get_name().to_string();
        let mut buf = Vec::new();
        generate(Shell::Bash, &mut cmd, name, &mut buf);
        assert!(!buf.is_empty());
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("slack"));
    }

    #[test]
    fn test_generate_zsh_completions() {
        let mut cmd = Cli::command();
        let name = cmd.get_name().to_string();
        let mut buf = Vec::new();
        generate(Shell::Zsh, &mut cmd, name, &mut buf);
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_generate_fish_completions() {
        let mut cmd = Cli::command();
        let name = cmd.get_name().to_string();
        let mut buf = Vec::new();
        generate(Shell::Fish, &mut cmd, name, &mut buf);
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_generate_powershell_completions() {
        let mut cmd = Cli::command();
        let name = cmd.get_name().to_string();
        let mut buf = Vec::new();
        generate(Shell::PowerShell, &mut cmd, name, &mut buf);
        assert!(!buf.is_empty());
    }
}
