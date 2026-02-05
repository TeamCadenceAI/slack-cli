//! Root CLI definitions for Slack CLI
//!
//! Contains the main CLI struct and global flags.

use clap::{Parser, Subcommand};

use super::auth::AuthCmd;
use super::channels::ChannelsCmd;
use super::completions::CompletionsArgs;
use super::files::FilesCmd;
use super::messages::MessagesCmd;
use super::reactions::ReactionsCmd;
use super::reminders::RemindersCmd;
use super::status::StatusCmd;
use super::users::UsersCmd;

/// Slack CLI for agents - command-line access to Slack workspaces
#[derive(Parser, Debug)]
#[command(
    name = "slack",
    version,
    about = "Slack CLI for agents",
    long_about = "A command-line interface for Slack workspaces, optimized for AI agents and scripts.\n\n\
                  Output is JSON by default. Use --plain for TSV output.",
    after_help = "Use 'slack <command> --help' for more information about a command."
)]
pub struct Cli {
    /// Plain TSV output instead of JSON
    #[arg(long, global = true)]
    pub plain: bool,

    /// Workspace to use (defaults to first authorized)
    #[arg(short = 'w', long, global = true, env = "SLACK_WORKSPACE")]
    pub workspace: Option<String>,

    /// Override token (skip keyring)
    #[arg(long, global = true, env = "SLACK_TOKEN", hide = true)]
    pub token: Option<String>,

    /// Verbose logging (to stderr)
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

/// Available commands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Authentication management
    #[command(alias = "a")]
    Auth(AuthCmd),

    /// Channel operations
    #[command(alias = "c")]
    Channels(ChannelsCmd),

    /// Message operations (list, send, search, thread)
    #[command(alias = "m", alias = "msg")]
    Messages(MessagesCmd),

    /// User operations
    #[command(alias = "u")]
    Users(UsersCmd),

    /// File operations
    #[command(alias = "f")]
    Files(FilesCmd),

    /// Reaction operations
    #[command(alias = "r")]
    Reactions(ReactionsCmd),

    /// User status/presence
    #[command(alias = "s")]
    Status(StatusCmd),

    /// Reminder operations
    Reminders(RemindersCmd),

    /// Generate shell completions
    Completions(CompletionsArgs),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::channels::ChannelsCommands;
    use clap::CommandFactory;

    #[test]
    fn test_cli_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn test_parse_auth_list() {
        let cli = Cli::try_parse_from(["slack", "auth", "list"]).unwrap();
        assert!(!cli.plain);
        assert!(cli.workspace.is_none());
        assert!(cli.token.is_none());
        assert!(!cli.verbose);
    }

    #[test]
    fn test_parse_global_flags() {
        let cli = Cli::try_parse_from([
            "slack",
            "--plain",
            "-w",
            "myworkspace",
            "-v",
            "auth",
            "list",
        ])
        .unwrap();
        assert!(cli.plain);
        assert_eq!(cli.workspace, Some("myworkspace".to_string()));
        assert!(cli.verbose);
    }

    #[test]
    fn test_parse_global_flags_after_command() {
        let cli = Cli::try_parse_from([
            "slack",
            "auth",
            "list",
            "--plain",
            "-w",
            "myworkspace",
            "-v",
        ])
        .unwrap();
        assert!(cli.plain);
        assert_eq!(cli.workspace, Some("myworkspace".to_string()));
        assert!(cli.verbose);
    }

    #[test]
    fn test_parse_channels_list() {
        let cli = Cli::try_parse_from(["slack", "channels", "list"]).unwrap();
        matches!(cli.command, Commands::Channels(_));
    }

    #[test]
    fn test_parse_channels_info() {
        let cli = Cli::try_parse_from(["slack", "channels", "info", "C123456"]).unwrap();
        if let Commands::Channels(cmd) = cli.command {
            if let ChannelsCommands::Info { channel } = cmd.command {
                assert_eq!(channel, "C123456");
            } else {
                panic!("Expected Info command");
            }
        } else {
            panic!("Expected Channels command");
        }
    }

    #[test]
    fn test_parse_messages_send() {
        let cli = Cli::try_parse_from(["slack", "messages", "send", "C123456", "Hello!"]).unwrap();
        if let Commands::Messages(cmd) = cli.command {
            if let crate::cli::messages::MessagesCommands::Send { channel, text, .. } = cmd.command
            {
                assert_eq!(channel, "C123456");
                assert_eq!(text, Some("Hello!".to_string()));
            } else {
                panic!("Expected Send command");
            }
        } else {
            panic!("Expected Messages command");
        }
    }

    #[test]
    fn test_parse_alias_auth() {
        let cli = Cli::try_parse_from(["slack", "a", "list"]).unwrap();
        matches!(cli.command, Commands::Auth(_));
    }

    #[test]
    fn test_parse_alias_channels() {
        let cli = Cli::try_parse_from(["slack", "c", "list"]).unwrap();
        matches!(cli.command, Commands::Channels(_));
    }

    #[test]
    fn test_parse_alias_messages() {
        let cli = Cli::try_parse_from(["slack", "m", "list", "C123"]).unwrap();
        matches!(cli.command, Commands::Messages(_));

        let cli = Cli::try_parse_from(["slack", "msg", "list", "C123"]).unwrap();
        matches!(cli.command, Commands::Messages(_));
    }
}
