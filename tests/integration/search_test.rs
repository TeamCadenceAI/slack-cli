//! Integration tests for search functionality
//!
//! Tests for messages search command.
//!
//! **Note:** These tests require `SLACK_INTEGRATION_TESTS=1` because mockito
//! requires socket binding which may fail in restricted environments.

use predicates::prelude::*;

use super::common::*;
use crate::test_env_or_skip;

// ============================================================================
// Search Messages Tests
// ============================================================================

#[tokio::test]
async fn test_search_messages_json() {
    let mut env = test_env_or_skip!();

    // Mock search.messages
    let _m = env
        .server
        .mock("POST", "/search.messages")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_search_messages_response(
            "hello",
            &[
                ("C001", "1234567890.123456", "U001", "Hello world!"),
                ("C002", "1234567890.123457", "U002", "Hello there!"),
            ],
        ))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["messages", "search", "hello"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello world!"))
        .stdout(predicate::str::contains("Hello there!"));
}

#[tokio::test]
async fn test_search_messages_plain() {
    let mut env = test_env_or_skip!();

    // Mock search.messages
    let _m = env
        .server
        .mock("POST", "/search.messages")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_search_messages_response(
            "hello",
            &[("C001", "1234567890.123456", "U001", "Hello world!")],
        ))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["messages", "search", "hello", "--plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello world!"));
}

#[tokio::test]
async fn test_search_messages_no_results() {
    let mut env = test_env_or_skip!();

    // Mock search.messages with no results
    let _m = env
        .server
        .mock("POST", "/search.messages")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
            "ok": true,
            "query": "nonexistent",
            "messages": {
                "total": 0,
                "matches": []
            }
        }"#,
        )
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["messages", "search", "nonexistent"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("[]")
                .or(predicate::str::contains("total").and(predicate::str::contains("0"))),
        );
}

#[tokio::test]
async fn test_search_messages_auth_required() {
    let env = test_env_or_skip!();

    env.slack_cmd()
        .args(["messages", "search", "hello"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[tokio::test]
async fn test_search_messages_api_error() {
    let mut env = test_env_or_skip!();

    // Mock search.messages error
    let _m = env
        .server
        .mock("POST", "/search.messages")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_error_response("not_allowed_token_type"))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["messages", "search", "hello"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("error"));
}

#[tokio::test]
async fn test_search_bot_token_not_supported() {
    let mut env = test_env_or_skip!();

    // Mock search.messages to return the expected error for bot tokens
    let _m = env
        .server
        .mock("POST", "/search.messages")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_error_response("not_allowed_token_type"))
        .create_async()
        .await;

    // Use a bot token - search is not supported for bot tokens
    env.slack_cmd_with_token("BotOAuth")
        .args(["messages", "search", "hello"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("error"));
}

// ============================================================================
// Search with Filters Tests
// ============================================================================

#[tokio::test]
async fn test_search_messages_in_channel() {
    let mut env = test_env_or_skip!();

    // Mock search.messages
    let _m = env
        .server
        .mock("POST", "/search.messages")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_search_messages_response(
            "hello in:general",
            &[("C001", "1234567890.123456", "U001", "Hello in general")],
        ))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["messages", "search", "hello in:general"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello in general"));
}

// ============================================================================
// Error Output Format Tests
// ============================================================================

#[tokio::test]
async fn test_search_error_json_format() {
    let env = test_env_or_skip!();

    let output = env
        .slack_cmd()
        .args(["messages", "search", "hello"])
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
