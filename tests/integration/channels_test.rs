//! Integration tests for channels commands
//!
//! Tests for channels list, info, and dms commands.
//!
//! **Note:** These tests require `SLACK_INTEGRATION_TESTS=1` because mockito
//! requires socket binding which may fail in restricted environments.

use predicates::prelude::*;

use super::common::*;
use crate::test_env_or_skip;

// ============================================================================
// Channels List Tests
// ============================================================================

#[tokio::test]
async fn test_channels_list_json() {
    let mut env = test_env_or_skip!();

    // Mock conversations.list
    let _m = env
        .server
        .mock("POST", "/conversations.list")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_conversations_list_response(&[
            ("C00112345", "general", false),
            ("C00223456", "random", false),
            ("C00334567", "private-channel", true),
        ]))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["channels", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("C00112345"))
        .stdout(predicate::str::contains("general"))
        .stdout(predicate::str::contains("C00223456"))
        .stdout(predicate::str::contains("random"));
}

#[tokio::test]
async fn test_channels_list_plain() {
    let mut env = test_env_or_skip!();

    // Mock conversations.list
    let _m = env
        .server
        .mock("POST", "/conversations.list")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_conversations_list_response(&[
            ("C00112345", "general", false),
            ("C00223456", "random", false),
        ]))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["channels", "list", "--plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains("C00112345"))
        .stdout(predicate::str::contains("general"));
}

#[tokio::test]
async fn test_channels_list_auth_required() {
    let env = test_env_or_skip!();

    env.slack_cmd()
        .args(["channels", "list"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[tokio::test]
async fn test_channels_list_api_error() {
    let mut env = test_env_or_skip!();

    // Mock API error
    let _m = env
        .server
        .mock("POST", "/conversations.list")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_error_response("not_authed"))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["channels", "list"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("error"))
        .stdout(predicate::str::contains("not_authed"));
}

// ============================================================================
// Channels Info Tests
// ============================================================================

#[tokio::test]
async fn test_channels_info_json() {
    let mut env = test_env_or_skip!();

    // Mock conversations.info
    let _m = env
        .server
        .mock("POST", "/conversations.info")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_conversations_info_response(
            "C00112345",
            "general",
            false,
            "Company announcements",
            "General discussion",
        ))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["channels", "info", "C00112345"])
        .assert()
        .success()
        .stdout(predicate::str::contains("C00112345"))
        .stdout(predicate::str::contains("general"))
        .stdout(predicate::str::contains("Company announcements"));
}

#[tokio::test]
async fn test_channels_info_plain() {
    let mut env = test_env_or_skip!();

    // Mock conversations.info
    let _m = env
        .server
        .mock("POST", "/conversations.info")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_conversations_info_response(
            "C00112345",
            "general",
            false,
            "Company announcements",
            "General discussion",
        ))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["channels", "info", "C00112345", "--plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains("general"));
}

#[tokio::test]
async fn test_channels_info_not_found() {
    let mut env = test_env_or_skip!();

    // Mock channel not found error
    let _m = env
        .server
        .mock("POST", "/conversations.info")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_error_response("channel_not_found"))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["channels", "info", "C_NONEXISTENT"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("error"))
        .stdout(predicate::str::contains("channel_not_found"));
}

#[tokio::test]
async fn test_channels_info_auth_required() {
    let env = test_env_or_skip!();

    env.slack_cmd()
        .args(["channels", "info", "C00112345"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

// ============================================================================
// Channels DMs Tests
// ============================================================================

#[tokio::test]
async fn test_channels_dms() {
    let mut env = test_env_or_skip!();

    // Mock conversations.list for IMs
    let _m = env
        .server
        .mock("POST", "/conversations.list")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
            "ok": true,
            "channels": [
                {
                    "id": "D001",
                    "user": "U001",
                    "is_im": true
                },
                {
                    "id": "G001",
                    "name": "mpdm-user1--user2",
                    "is_mpim": true
                }
            ],
            "response_metadata": {
                "next_cursor": ""
            }
        }"#,
        )
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["channels", "dms"])
        .assert()
        .success()
        .stdout(predicate::str::contains("D001").or(predicate::str::contains("G001")));
}

// ============================================================================
// Error Output Format Tests
// ============================================================================

#[tokio::test]
async fn test_channels_error_json_format() {
    let env = test_env_or_skip!();

    let output = env
        .slack_cmd()
        .args(["channels", "list"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should be valid JSON
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(
        parsed.is_ok(),
        "Error output should be valid JSON: {}",
        stdout
    );
}
