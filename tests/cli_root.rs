//! Unit tests for root CLI parsing
//!
//! Tests for global flags, subcommand parsing, and help/version output.

use clap::{CommandFactory, Parser};
use slack_cli::cli::{Cli, Commands};

#[test]
fn test_cli_valid_structure() {
    // Verify the CLI structure is valid according to clap
    Cli::command().debug_assert();
}

// ============================================================================
// Global Flags Tests
// ============================================================================

#[test]
fn test_parse_plain_flag() {
    let cli = Cli::try_parse_from(["slack", "--plain", "auth", "list"]).unwrap();
    assert!(cli.plain);
}

#[test]
fn test_parse_plain_flag_after_command() {
    let cli = Cli::try_parse_from(["slack", "auth", "list", "--plain"]).unwrap();
    assert!(cli.plain);
}

#[test]
fn test_parse_workspace_flag_short() {
    let cli = Cli::try_parse_from(["slack", "-w", "myworkspace", "auth", "list"]).unwrap();
    assert_eq!(cli.workspace, Some("myworkspace".to_string()));
}

#[test]
fn test_parse_workspace_flag_long() {
    let cli = Cli::try_parse_from(["slack", "--workspace", "myworkspace", "auth", "list"]).unwrap();
    assert_eq!(cli.workspace, Some("myworkspace".to_string()));
}

#[test]
fn test_parse_verbose_flag_short() {
    let cli = Cli::try_parse_from(["slack", "-v", "auth", "list"]).unwrap();
    assert!(cli.verbose);
}

#[test]
fn test_parse_verbose_flag_long() {
    let cli = Cli::try_parse_from(["slack", "--verbose", "auth", "list"]).unwrap();
    assert!(cli.verbose);
}

#[test]
fn test_parse_token_flag() {
    let cli = Cli::try_parse_from(["slack", "--token", "xoxp-123", "auth", "list"]).unwrap();
    assert_eq!(cli.token, Some("xoxp-123".to_string()));
}

#[test]
fn test_parse_all_global_flags() {
    let cli = Cli::try_parse_from([
        "slack",
        "--plain",
        "-w",
        "workspace",
        "--token",
        "xoxp-123",
        "-v",
        "auth",
        "list",
    ])
    .unwrap();
    assert!(cli.plain);
    assert_eq!(cli.workspace, Some("workspace".to_string()));
    assert_eq!(cli.token, Some("xoxp-123".to_string()));
    assert!(cli.verbose);
}

#[test]
fn test_default_values() {
    let cli = Cli::try_parse_from(["slack", "auth", "list"]).unwrap();
    assert!(!cli.plain);
    assert!(cli.workspace.is_none());
    assert!(cli.token.is_none());
    assert!(!cli.verbose);
}

// ============================================================================
// Help and Version Tests
// ============================================================================

#[test]
fn test_help_output_contains_commands() {
    let mut cmd = Cli::command();
    let help = cmd.render_help();
    let help_str = help.to_string();

    // Check that all main commands are documented
    assert!(help_str.contains("auth"));
    assert!(help_str.contains("channels"));
    assert!(help_str.contains("messages"));
    assert!(help_str.contains("users"));
    assert!(help_str.contains("files"));
    assert!(help_str.contains("reactions"));
    assert!(help_str.contains("status"));
    assert!(help_str.contains("reminders"));
}

#[test]
fn test_help_output_contains_global_flags() {
    let mut cmd = Cli::command();
    let help = cmd.render_help();
    let help_str = help.to_string();

    // Check that global flags are documented
    assert!(help_str.contains("--plain"));
    assert!(help_str.contains("-w"));
    assert!(help_str.contains("--workspace"));
    assert!(help_str.contains("-v"));
    assert!(help_str.contains("--verbose"));
}

#[test]
fn test_version_flag() {
    let result = Cli::try_parse_from(["slack", "--version"]);
    // --version causes clap to exit, which manifests as an error in try_parse_from
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
}

#[test]
fn test_help_flag() {
    let result = Cli::try_parse_from(["slack", "--help"]);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
}

// ============================================================================
// Subcommand Routing Tests
// ============================================================================

#[test]
fn test_route_auth_command() {
    let cli = Cli::try_parse_from(["slack", "auth", "list"]).unwrap();
    assert!(matches!(cli.command, Commands::Auth(_)));
}

#[test]
fn test_route_channels_command() {
    let cli = Cli::try_parse_from(["slack", "channels", "list"]).unwrap();
    assert!(matches!(cli.command, Commands::Channels(_)));
}

#[test]
fn test_route_messages_command() {
    let cli = Cli::try_parse_from(["slack", "messages", "list", "C123"]).unwrap();
    assert!(matches!(cli.command, Commands::Messages(_)));
}

#[test]
fn test_route_users_command() {
    let cli = Cli::try_parse_from(["slack", "users", "list"]).unwrap();
    assert!(matches!(cli.command, Commands::Users(_)));
}

#[test]
fn test_route_files_command() {
    let cli = Cli::try_parse_from(["slack", "files", "list"]).unwrap();
    assert!(matches!(cli.command, Commands::Files(_)));
}

#[test]
fn test_route_reactions_command() {
    let cli =
        Cli::try_parse_from(["slack", "reactions", "add", "C123", "1234.5", "thumbsup"]).unwrap();
    assert!(matches!(cli.command, Commands::Reactions(_)));
}

#[test]
fn test_route_status_command() {
    let cli = Cli::try_parse_from(["slack", "status", "get"]).unwrap();
    assert!(matches!(cli.command, Commands::Status(_)));
}

#[test]
fn test_route_reminders_command() {
    let cli = Cli::try_parse_from(["slack", "reminders", "list"]).unwrap();
    assert!(matches!(cli.command, Commands::Reminders(_)));
}

// ============================================================================
// Command Alias Tests
// ============================================================================

#[test]
fn test_auth_alias() {
    let cli = Cli::try_parse_from(["slack", "a", "list"]).unwrap();
    assert!(matches!(cli.command, Commands::Auth(_)));
}

#[test]
fn test_channels_alias() {
    let cli = Cli::try_parse_from(["slack", "c", "list"]).unwrap();
    assert!(matches!(cli.command, Commands::Channels(_)));
}

#[test]
fn test_messages_alias_m() {
    let cli = Cli::try_parse_from(["slack", "m", "list", "C123"]).unwrap();
    assert!(matches!(cli.command, Commands::Messages(_)));
}

#[test]
fn test_messages_alias_msg() {
    let cli = Cli::try_parse_from(["slack", "msg", "list", "C123"]).unwrap();
    assert!(matches!(cli.command, Commands::Messages(_)));
}

#[test]
fn test_users_alias() {
    let cli = Cli::try_parse_from(["slack", "u", "list"]).unwrap();
    assert!(matches!(cli.command, Commands::Users(_)));
}

#[test]
fn test_files_alias() {
    let cli = Cli::try_parse_from(["slack", "f", "list"]).unwrap();
    assert!(matches!(cli.command, Commands::Files(_)));
}

#[test]
fn test_reactions_alias() {
    let cli = Cli::try_parse_from(["slack", "r", "add", "C123", "1234.5", "thumbsup"]).unwrap();
    assert!(matches!(cli.command, Commands::Reactions(_)));
}

#[test]
fn test_status_alias() {
    let cli = Cli::try_parse_from(["slack", "s", "get"]).unwrap();
    assert!(matches!(cli.command, Commands::Status(_)));
}

// ============================================================================
// Error Cases
// ============================================================================

#[test]
fn test_error_no_command() {
    let result = Cli::try_parse_from(["slack"]);
    assert!(result.is_err());
}

#[test]
fn test_error_unknown_command() {
    let result = Cli::try_parse_from(["slack", "unknown"]);
    assert!(result.is_err());
}

#[test]
fn test_error_unknown_flag() {
    let result = Cli::try_parse_from(["slack", "--unknown-flag", "auth", "list"]);
    assert!(result.is_err());
}
