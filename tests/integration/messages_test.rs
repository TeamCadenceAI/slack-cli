//! Integration tests for messages commands
//!
//! Tests for messages list, send, thread, and related commands.
//!
//! **Note:** These tests require `SLACK_INTEGRATION_TESTS=1` because mockito
//! requires socket binding which may fail in restricted environments.

use predicates::prelude::*;

use super::common::*;
use crate::test_env_or_skip;

// ============================================================================
// Messages List Tests
// ============================================================================

#[tokio::test]
async fn test_messages_list_json() {
    let mut env = test_env_or_skip!();

    // Mock conversations.history
    let _m = env
        .server
        .mock("POST", "/conversations.history")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_conversations_history_response(&[
            ("1234567890.123456", "U001", "Hello world!"),
            ("1234567890.123457", "U002", "Hi there!"),
        ]))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["messages", "list", "C00112345"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello world!"))
        .stdout(predicate::str::contains("Hi there!"));
}

#[tokio::test]
async fn test_messages_list_plain() {
    let mut env = test_env_or_skip!();

    // Mock conversations.history
    let _m = env
        .server
        .mock("POST", "/conversations.history")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_conversations_history_response(&[(
            "1234567890.123456",
            "U001",
            "Hello world!",
        )]))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["messages", "list", "C00112345", "--plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello world!"));
}

#[tokio::test]
async fn test_messages_list_with_limit() {
    let mut env = test_env_or_skip!();

    // Mock should receive limit parameter
    let _m = env
        .server
        .mock("POST", "/conversations.history")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_conversations_history_response(&[(
            "1234567890.123456",
            "U001",
            "Message 1",
        )]))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["messages", "list", "C00112345", "--limit", "5"])
        .assert()
        .success();
}

#[tokio::test]
async fn test_messages_list_auth_required() {
    let env = test_env_or_skip!();

    env.slack_cmd()
        .args(["messages", "list", "C00112345"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[tokio::test]
async fn test_messages_list_channel_not_found() {
    let mut env = test_env_or_skip!();

    // Mock channel not found error
    let _m = env
        .server
        .mock("POST", "/conversations.history")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_error_response("channel_not_found"))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["messages", "list", "C_INVALID"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("channel_not_found"));
}

// ============================================================================
// Messages Send Tests
// ============================================================================

#[tokio::test]
async fn test_messages_send_json() {
    let mut env = test_env_or_skip!();

    // Mock chat.postMessage
    let _m = env
        .server
        .mock("POST", "/chat.postMessage")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_chat_post_message_response(
            "C00112345",
            "1234567890.123456",
        ))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["messages", "send", "C00112345", "Hello, Slack!"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1234567890.123456"));
}

#[tokio::test]
async fn test_messages_send_plain() {
    let mut env = test_env_or_skip!();

    // Mock chat.postMessage
    let _m = env
        .server
        .mock("POST", "/chat.postMessage")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_chat_post_message_response(
            "C00112345",
            "1234567890.123456",
        ))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["messages", "send", "C00112345", "Hello, Slack!", "--plain"])
        .assert()
        .success();
}

#[tokio::test]
async fn test_messages_send_to_thread() {
    let mut env = test_env_or_skip!();

    // Mock chat.postMessage with thread_ts
    let _m = env
        .server
        .mock("POST", "/chat.postMessage")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_chat_post_message_response(
            "C00112345",
            "1234567890.123456",
        ))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args([
            "messages",
            "send",
            "C00112345",
            "Reply in thread",
            "--thread-ts",
            "1234567890.000001",
        ])
        .assert()
        .success();
}

#[tokio::test]
async fn test_messages_send_auth_required() {
    let env = test_env_or_skip!();

    env.slack_cmd()
        .args(["messages", "send", "C00112345", "Hello"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

// ============================================================================
// Messages Thread Tests
// ============================================================================

#[tokio::test]
async fn test_messages_thread() {
    let mut env = test_env_or_skip!();

    // Mock conversations.replies
    let _m = env
        .server
        .mock("POST", "/conversations.replies")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
            "ok": true,
            "messages": [
                {
                    "type": "message",
                    "ts": "1234567890.000001",
                    "user": "U001",
                    "text": "Thread parent message"
                },
                {
                    "type": "message",
                    "ts": "1234567890.000002",
                    "user": "U002",
                    "text": "First reply"
                },
                {
                    "type": "message",
                    "ts": "1234567890.000003",
                    "user": "U001",
                    "text": "Second reply"
                }
            ],
            "has_more": false
        }"#,
        )
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["messages", "thread", "C00112345", "1234567890.000001"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Thread parent message"))
        .stdout(predicate::str::contains("First reply"));
}

#[tokio::test]
async fn test_messages_thread_plain() {
    let mut env = test_env_or_skip!();

    // Mock conversations.replies
    let _m = env
        .server
        .mock("POST", "/conversations.replies")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
            "ok": true,
            "messages": [
                {
                    "type": "message",
                    "ts": "1234567890.000001",
                    "user": "U001",
                    "text": "Thread message"
                }
            ],
            "has_more": false
        }"#,
        )
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args([
            "messages",
            "thread",
            "C00112345",
            "1234567890.000001",
            "--plain",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Thread message"));
}

// ============================================================================
// Error Output Format Tests
// ============================================================================

#[tokio::test]
async fn test_messages_error_json_format() {
    let env = test_env_or_skip!();

    let output = env
        .slack_cmd()
        .args(["messages", "list", "C00112345"])
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
