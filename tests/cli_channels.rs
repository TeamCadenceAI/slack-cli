//! Tests for channels CLI commands
//!
//! Tests CLI parsing and command structure for channel operations.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use predicates::prelude::*;

fn slack_cmd() -> Command {
    cargo_bin_cmd!("slack")
}

// ============================================================================
// Help Output Tests
// ============================================================================

#[test]
fn test_channels_help() {
    slack_cmd()
        .args(["channels", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("info"))
        .stdout(predicate::str::contains("dms"))
        .stdout(predicate::str::contains("export"));
}

#[test]
fn test_channels_list_help() {
    slack_cmd()
        .args(["channels", "list", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--types"))
        .stdout(predicate::str::contains("--limit"))
        .stdout(predicate::str::contains("--cursor"))
        .stdout(predicate::str::contains("--exclude-archived"))
        .stdout(predicate::str::contains("--sort-popularity"));
}

#[test]
fn test_channels_info_help() {
    slack_cmd()
        .args(["channels", "info", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("CHANNEL"));
}

#[test]
fn test_channels_dms_help() {
    slack_cmd()
        .args(["channels", "dms", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--include-mpim"));
}

#[test]
fn test_channels_export_help() {
    slack_cmd()
        .args(["channels", "export", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--output"))
        .stdout(predicate::str::contains("--types"));
}

// ============================================================================
// Alias Tests
// ============================================================================

#[test]
fn test_channels_alias_c() {
    // Without auth, returns auth_required
    slack_cmd()
        .args(["c", "list"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

// ============================================================================
// Auth Required Tests (without authentication)
// ============================================================================

#[test]
fn test_channels_list_auth_required() {
    slack_cmd()
        .args(["channels", "list"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_channels_list_with_types_auth_required() {
    slack_cmd()
        .args(["channels", "list", "--types", "public_channel"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_channels_list_with_limit_auth_required() {
    slack_cmd()
        .args(["channels", "list", "--limit", "10"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_channels_list_with_sort_popularity_auth_required() {
    slack_cmd()
        .args(["channels", "list", "--sort-popularity"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_channels_list_with_exclude_archived_auth_required() {
    slack_cmd()
        .args(["channels", "list", "--exclude-archived"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_channels_info_auth_required() {
    slack_cmd()
        .args(["channels", "info", "C123456789"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_channels_info_by_name_auth_required() {
    slack_cmd()
        .args(["channels", "info", "#general"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_channels_info_by_name_without_hash_auth_required() {
    slack_cmd()
        .args(["channels", "info", "general"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_channels_dms_auth_required() {
    slack_cmd()
        .args(["channels", "dms"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_channels_dms_with_mpim_auth_required() {
    slack_cmd()
        .args(["channels", "dms", "--include-mpim"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_channels_export_auth_required() {
    slack_cmd()
        .args(["channels", "export"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_channels_export_with_output_auth_required() {
    slack_cmd()
        .args(["channels", "export", "-o", "channels.csv"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

// ============================================================================
// Missing Required Arguments Tests
// ============================================================================

#[test]
fn test_channels_info_missing_channel() {
    slack_cmd()
        .args(["channels", "info"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// ============================================================================
// Flag Parsing Tests
// ============================================================================

#[test]
fn test_channels_list_multiple_flags() {
    // Ensure all flags can be combined
    slack_cmd()
        .args([
            "channels",
            "list",
            "--types",
            "public_channel,private_channel",
            "--limit",
            "50",
            "--exclude-archived",
            "--sort-popularity",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_channels_export_all_flags() {
    slack_cmd()
        .args([
            "channels",
            "export",
            "--types",
            "public_channel",
            "--output",
            "out.csv",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

// ============================================================================
// Plain Output Mode Tests
// ============================================================================

#[test]
fn test_channels_list_plain_mode() {
    slack_cmd()
        .args(["--plain", "channels", "list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Authentication required"));
}

#[test]
fn test_channels_info_plain_mode() {
    slack_cmd()
        .args(["--plain", "channels", "info", "C123456789"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Authentication required"));
}

#[test]
fn test_channels_dms_plain_mode() {
    slack_cmd()
        .args(["--plain", "channels", "dms"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Authentication required"));
}
