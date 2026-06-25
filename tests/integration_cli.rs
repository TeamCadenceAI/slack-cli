//! Integration tests for CLI entrypoint
//!
//! Tests using assert_cmd to run the actual binary.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use predicates::prelude::*;

fn slack_cmd() -> Command {
    cargo_bin_cmd!("slack")
}

// ============================================================================
// Help and Version Tests
// ============================================================================

#[test]
fn test_help_output() {
    slack_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Slack workspaces"))
        .stdout(predicate::str::contains("AI agents"))
        .stdout(predicate::str::contains("auth"))
        .stdout(predicate::str::contains("channels"))
        .stdout(predicate::str::contains("messages"))
        .stdout(predicate::str::contains("--plain"))
        .stdout(predicate::str::contains("--workspace"));
}

#[test]
fn test_version_output() {
    slack_cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("slack"))
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn test_auth_help() {
    slack_cmd()
        .args(["auth", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("add"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("remove"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("switch"))
        .stdout(predicate::str::contains("browser-help"));
}

#[test]
fn test_auth_add_help() {
    slack_cmd()
        .args(["auth", "add", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--token"))
        .stdout(predicate::str::contains("--xoxc"))
        .stdout(predicate::str::contains("--xoxd"))
        .stdout(predicate::str::contains("--oauth"))
        .stdout(predicate::str::contains("--scopes"));
}

// ============================================================================
// Error Exit Code Tests
// ============================================================================

#[test]
fn test_no_command_error() {
    slack_cmd()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage:"));
}

#[test]
fn test_unknown_command_error() {
    slack_cmd()
        .arg("unknown-command")
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn test_missing_required_arg_error() {
    slack_cmd()
        .args(["auth", "remove"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_invalid_flag_combination_error() {
    slack_cmd()
        .args(["auth", "add", "--token", "xoxp-123", "--oauth"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn test_xoxc_requires_xoxd_error() {
    slack_cmd()
        .args(["auth", "add", "--xoxc", "xoxc-123"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// ============================================================================
// Auth List Test (No Auth Setup)
// ============================================================================

#[test]
fn test_auth_list_empty() {
    // When no workspaces are configured, auth list should return an empty list
    // This test doesn't require keyring access and should work in CI
    slack_cmd().args(["auth", "list"]).assert().success();
}

// ============================================================================
// Auth Help Instructions Test
// ============================================================================

#[test]
fn test_auth_browser_help_instructions() {
    slack_cmd()
        .args(["auth", "browser-help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("browser"))
        .stdout(predicate::str::contains("xoxc"))
        .stdout(predicate::str::contains("xoxd"))
        .stdout(predicate::str::contains("Developer Tools"));
}

// ============================================================================
// Plain Output Mode Tests
// ============================================================================

#[test]
fn test_plain_flag_accepted() {
    slack_cmd()
        .args(["--plain", "auth", "list"])
        .assert()
        .success();
}

#[test]
fn test_plain_flag_after_command() {
    slack_cmd()
        .args(["auth", "list", "--plain"])
        .assert()
        .success();
}

// ============================================================================
// Verbose Flag Tests
// ============================================================================

#[test]
fn test_verbose_flag_accepted() {
    slack_cmd()
        .args(["--verbose", "auth", "list"])
        .assert()
        .success();
}

#[test]
fn test_verbose_short_flag_accepted() {
    slack_cmd().args(["-v", "auth", "list"]).assert().success();
}

// ============================================================================
// Workspace Flag Tests
// ============================================================================

#[test]
fn test_workspace_flag_accepted() {
    slack_cmd()
        .args(["--workspace", "myworkspace", "auth", "list"])
        .assert()
        .success();
}

#[test]
fn test_workspace_short_flag_accepted() {
    slack_cmd()
        .args(["-w", "myworkspace", "auth", "list"])
        .assert()
        .success();
}

// ============================================================================
// Command Alias Tests
// ============================================================================

#[test]
fn test_auth_alias() {
    slack_cmd().args(["a", "list"]).assert().success();
}

#[test]
fn test_channels_alias() {
    // Channels is now implemented, returns auth_required without authentication
    slack_cmd()
        .args(["c", "list"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_alias() {
    // Messages is now implemented, returns auth_required without authentication
    slack_cmd()
        .args(["m", "list", "C123"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

// ============================================================================
// Auth Required Tests (Commands are implemented, but require authentication)
// ============================================================================

#[test]
fn test_channels_auth_required() {
    // Channels is implemented, returns auth_required without authentication
    slack_cmd()
        .args(["channels", "list"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_auth_required() {
    // Messages is implemented, returns auth_required without authentication
    slack_cmd()
        .args(["messages", "list", "C123"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_users_auth_required() {
    // Users is implemented, returns auth_required without authentication
    slack_cmd()
        .args(["users", "list"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_files_auth_required() {
    // Files is implemented, returns auth_required without authentication
    slack_cmd()
        .args(["files", "list"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_reactions_auth_required() {
    // Reactions is implemented, returns auth_required without authentication
    slack_cmd()
        .args(["reactions", "add", "C123", "1234.5", "thumbsup"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_status_auth_required() {
    // Status is implemented, returns auth_required without authentication
    slack_cmd()
        .args(["status", "get"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_reminders_auth_required() {
    // Reminders is implemented, returns auth_required without authentication
    slack_cmd()
        .args(["reminders", "list"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}
