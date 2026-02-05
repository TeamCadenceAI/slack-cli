//! Tests for messages CLI commands
//!
//! Tests CLI parsing and command structure for message operations.

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
fn test_messages_help() {
    slack_cmd()
        .args(["messages", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("thread"))
        .stdout(predicate::str::contains("send"))
        .stdout(predicate::str::contains("search"))
        .stdout(predicate::str::contains("get"));
}

#[test]
fn test_messages_list_help() {
    slack_cmd()
        .args(["messages", "list", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("CHANNEL"))
        .stdout(predicate::str::contains("--limit"))
        .stdout(predicate::str::contains("--include-activity"))
        .stdout(predicate::str::contains("--cursor"));
}

#[test]
fn test_messages_thread_help() {
    slack_cmd()
        .args(["messages", "thread", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("CHANNEL"))
        .stdout(predicate::str::contains("THREAD_TS"))
        .stdout(predicate::str::contains("--limit"))
        .stdout(predicate::str::contains("--include-activity"));
}

#[test]
fn test_messages_send_help() {
    slack_cmd()
        .args(["messages", "send", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("CHANNEL"))
        .stdout(predicate::str::contains("--stdin"))
        .stdout(predicate::str::contains("--thread-ts"))
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--mark-read"));
}

#[test]
fn test_messages_search_help() {
    slack_cmd()
        .args(["messages", "search", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("QUERY"))
        .stdout(predicate::str::contains("--in-channel"))
        .stdout(predicate::str::contains("--in-dm"))
        .stdout(predicate::str::contains("--from"))
        .stdout(predicate::str::contains("--with-user"))
        .stdout(predicate::str::contains("--before"))
        .stdout(predicate::str::contains("--after"))
        .stdout(predicate::str::contains("--threads-only"))
        .stdout(predicate::str::contains("--count"))
        .stdout(predicate::str::contains("--page"));
}

#[test]
fn test_messages_get_help() {
    slack_cmd()
        .args(["messages", "get", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("MESSAGE"));
}

// ============================================================================
// Alias Tests
// ============================================================================

#[test]
fn test_messages_alias_m() {
    // Without auth, returns auth_required
    slack_cmd()
        .args(["m", "list", "C123456789"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_alias_msg() {
    // Without auth, returns auth_required
    slack_cmd()
        .args(["msg", "list", "C123456789"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

// ============================================================================
// Auth Required Tests (without authentication)
// ============================================================================

#[test]
fn test_messages_list_auth_required() {
    slack_cmd()
        .args(["messages", "list", "C123456789"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_list_by_name_auth_required() {
    slack_cmd()
        .args(["messages", "list", "general"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_list_with_limit_auth_required() {
    slack_cmd()
        .args(["messages", "list", "general", "--limit", "7d"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_list_with_limit_count_auth_required() {
    slack_cmd()
        .args(["messages", "list", "general", "--limit", "50"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_list_with_include_activity_auth_required() {
    slack_cmd()
        .args(["messages", "list", "general", "--include-activity"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_list_with_cursor_auth_required() {
    slack_cmd()
        .args(["messages", "list", "general", "--cursor", "cursor123"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_thread_auth_required() {
    slack_cmd()
        .args(["messages", "thread", "C123456789", "1234567890.123456"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_thread_by_name_auth_required() {
    slack_cmd()
        .args(["messages", "thread", "general", "1234567890.123456"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_send_with_text_auth_required() {
    slack_cmd()
        .args(["messages", "send", "general", "Hello world!"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_send_with_thread_auth_required() {
    slack_cmd()
        .args([
            "messages",
            "send",
            "general",
            "Reply",
            "--thread-ts",
            "1234567890.123456",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_send_with_format_plain_auth_required() {
    slack_cmd()
        .args([
            "messages",
            "send",
            "general",
            "Plain text",
            "--format",
            "plain",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_search_auth_required() {
    slack_cmd()
        .args(["messages", "search", "hello world"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_search_with_filters_auth_required() {
    slack_cmd()
        .args([
            "messages",
            "search",
            "hello",
            "--in-channel",
            "general",
            "--from",
            "@john",
            "--after",
            "2024-01-01",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_search_with_threads_only_auth_required() {
    slack_cmd()
        .args(["messages", "search", "hello", "--threads-only"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_get_by_channel_ts_auth_required() {
    slack_cmd()
        .args(["messages", "get", "C123456789:1234567890.123456"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_get_by_permalink_auth_required() {
    slack_cmd()
        .args([
            "messages",
            "get",
            "https://myworkspace.slack.com/archives/C123456789/p1234567890123456",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

// ============================================================================
// Missing Required Arguments Tests
// ============================================================================

#[test]
fn test_messages_list_missing_channel() {
    slack_cmd()
        .args(["messages", "list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_messages_thread_missing_channel() {
    slack_cmd()
        .args(["messages", "thread"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_messages_thread_missing_thread_ts() {
    slack_cmd()
        .args(["messages", "thread", "general"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_messages_send_missing_channel() {
    slack_cmd()
        .args(["messages", "send"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_messages_search_missing_query() {
    slack_cmd()
        .args(["messages", "search"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_messages_get_missing_message() {
    slack_cmd()
        .args(["messages", "get"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// ============================================================================
// Flag Parsing Tests
// ============================================================================

#[test]
fn test_messages_list_multiple_flags() {
    slack_cmd()
        .args([
            "messages",
            "list",
            "general",
            "--limit",
            "7d",
            "--include-activity",
            "--cursor",
            "abc123",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_search_all_filters() {
    slack_cmd()
        .args([
            "messages",
            "search",
            "test query",
            "--in-channel",
            "general",
            "--from",
            "@john",
            "--with-user",
            "@jane",
            "--before",
            "2024-12-31",
            "--after",
            "2024-01-01",
            "--threads-only",
            "--count",
            "50",
            "--page",
            "2",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_send_all_flags() {
    slack_cmd()
        .args([
            "messages",
            "send",
            "general",
            "Hello",
            "--thread-ts",
            "1234567890.123456",
            "--format",
            "markdown",
            "--mark-read",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

// ============================================================================
// Plain Output Mode Tests
// ============================================================================

#[test]
fn test_messages_list_plain_mode() {
    slack_cmd()
        .args(["--plain", "messages", "list", "C123456789"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Authentication required"));
}

#[test]
fn test_messages_thread_plain_mode() {
    slack_cmd()
        .args([
            "--plain",
            "messages",
            "thread",
            "C123456789",
            "1234567890.123456",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Authentication required"));
}

#[test]
fn test_messages_send_plain_mode() {
    slack_cmd()
        .args(["--plain", "messages", "send", "C123456789", "Hello"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Authentication required"));
}

#[test]
fn test_messages_search_plain_mode() {
    slack_cmd()
        .args(["--plain", "messages", "search", "hello"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Authentication required"));
}

#[test]
fn test_messages_get_plain_mode() {
    slack_cmd()
        .args(["--plain", "messages", "get", "C123456789:1234567890.123456"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Authentication required"));
}

// ============================================================================
// Time Limit Format Tests
// ============================================================================

#[test]
fn test_messages_list_limit_days() {
    slack_cmd()
        .args(["messages", "list", "general", "-l", "1d"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_list_limit_weeks() {
    slack_cmd()
        .args(["messages", "list", "general", "-l", "2w"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_list_limit_months() {
    slack_cmd()
        .args(["messages", "list", "general", "-l", "1m"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_list_limit_90_days() {
    slack_cmd()
        .args(["messages", "list", "general", "-l", "90d"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_list_limit_hours() {
    slack_cmd()
        .args(["messages", "list", "general", "-l", "12h"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_list_limit_count() {
    slack_cmd()
        .args(["messages", "list", "general", "-l", "100"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

// ============================================================================
// Message Format Tests
// ============================================================================

#[test]
fn test_messages_send_format_markdown() {
    slack_cmd()
        .args([
            "messages",
            "send",
            "general",
            "*bold* _italic_",
            "--format",
            "markdown",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_send_format_plain() {
    slack_cmd()
        .args([
            "messages",
            "send",
            "general",
            "plain text",
            "--format",
            "plain",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

// ============================================================================
// Invalid Format Tests
// ============================================================================

#[test]
fn test_messages_send_invalid_format() {
    slack_cmd()
        .args(["messages", "send", "general", "text", "--format", "invalid"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid"));
}

// ============================================================================
// Mark Read Tests
// ============================================================================

#[test]
fn test_messages_send_mark_read_flag() {
    slack_cmd()
        .args(["messages", "send", "general", "Hello", "--mark-read"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_send_mark_read_with_thread() {
    slack_cmd()
        .args([
            "messages",
            "send",
            "general",
            "Reply",
            "--thread-ts",
            "1234567890.123456",
            "--mark-read",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

// ============================================================================
// Search Date Filter Tests
// ============================================================================

#[test]
fn test_messages_search_before_date() {
    slack_cmd()
        .args(["messages", "search", "hello", "--before", "2024-12-31"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_search_after_date() {
    slack_cmd()
        .args(["messages", "search", "hello", "--after", "2024-01-01"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_search_date_range() {
    slack_cmd()
        .args([
            "messages",
            "search",
            "hello",
            "--after",
            "2024-01-01",
            "--before",
            "2024-06-30",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

// ============================================================================
// Search In DM Tests
// ============================================================================

#[test]
fn test_messages_search_in_dm() {
    slack_cmd()
        .args(["messages", "search", "hello", "--in-dm", "@john"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

// ============================================================================
// Get Message Format Tests
// ============================================================================

#[test]
fn test_messages_get_channel_ts_format() {
    slack_cmd()
        .args(["messages", "get", "general:1234567890.123456"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_get_channel_id_ts_format() {
    slack_cmd()
        .args(["messages", "get", "C123ABC456:1234567890.123456"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[test]
fn test_messages_get_permalink_format() {
    slack_cmd()
        .args([
            "messages",
            "get",
            "https://workspace.slack.com/archives/C123ABC/p1234567890123456",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}
