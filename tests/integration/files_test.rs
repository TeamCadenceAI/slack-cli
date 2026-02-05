//! Integration tests for files commands
//!
//! Tests for files list, info, and get commands.
//!
//! **Note:** These tests require `SLACK_INTEGRATION_TESTS=1` because mockito
//! requires socket binding which may fail in restricted environments.

use predicates::prelude::*;

use super::common::*;
use crate::test_env_or_skip;

// ============================================================================
// Files List Tests
// ============================================================================

#[tokio::test]
async fn test_files_list_json() {
    let mut env = test_env_or_skip!();

    // Mock files.list
    let _m = env
        .server
        .mock("POST", "/files.list")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_files_list_response(&[
            ("F001", "document.pdf", "application/pdf", 1024),
            ("F002", "image.png", "image/png", 2048),
        ]))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["files", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("F001"))
        .stdout(predicate::str::contains("document.pdf"))
        .stdout(predicate::str::contains("F002"))
        .stdout(predicate::str::contains("image.png"));
}

#[tokio::test]
async fn test_files_list_plain() {
    let mut env = test_env_or_skip!();

    // Mock files.list
    let _m = env
        .server
        .mock("POST", "/files.list")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_files_list_response(&[(
            "F001",
            "document.pdf",
            "application/pdf",
            1024,
        )]))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["files", "list", "--plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains("document.pdf"));
}

#[tokio::test]
async fn test_files_list_empty() {
    let mut env = test_env_or_skip!();

    // Mock files.list with no files
    let _m = env
        .server
        .mock("POST", "/files.list")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
            "ok": true,
            "files": []
        }"#,
        )
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["files", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[]"));
}

#[tokio::test]
async fn test_files_list_auth_required() {
    let env = test_env_or_skip!();

    env.slack_cmd()
        .args(["files", "list"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

// ============================================================================
// Files Info Tests
// ============================================================================

#[tokio::test]
async fn test_files_info_json() {
    let mut env = test_env_or_skip!();

    // Mock files.info
    let _m = env
        .server
        .mock("POST", "/files.info")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_files_info_response(
            "F001",
            "document.pdf",
            "application/pdf",
            1024,
        ))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["files", "info", "F001"])
        .assert()
        .success()
        .stdout(predicate::str::contains("F001"))
        .stdout(predicate::str::contains("document.pdf"))
        .stdout(predicate::str::contains("application/pdf"));
}

#[tokio::test]
async fn test_files_info_plain() {
    let mut env = test_env_or_skip!();

    // Mock files.info
    let _m = env
        .server
        .mock("POST", "/files.info")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_files_info_response(
            "F001",
            "document.pdf",
            "application/pdf",
            1024,
        ))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["files", "info", "F001", "--plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains("document.pdf"));
}

#[tokio::test]
async fn test_files_info_not_found() {
    let mut env = test_env_or_skip!();

    // Mock files.info with not found error
    let _m = env
        .server
        .mock("POST", "/files.info")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_error_response("file_not_found"))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["files", "info", "F_NONEXISTENT"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("file_not_found"));
}

#[tokio::test]
async fn test_files_info_auth_required() {
    let env = test_env_or_skip!();

    env.slack_cmd()
        .args(["files", "info", "F001"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

// ============================================================================
// Files Get Tests (Download)
// ============================================================================

#[tokio::test]
async fn test_files_get_to_stdout() {
    let mut env = test_env_or_skip!();

    // First mock files.info to get the URL
    let _m1 = env
        .server
        .mock("POST", "/files.info")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(format!(
            r#"{{
                "ok": true,
                "file": {{
                    "id": "F001",
                    "name": "test.txt",
                    "mimetype": "text/plain",
                    "size": 13,
                    "url_private": "{}/files/test.txt",
                    "timestamp": 1234567890
                }}
            }}"#,
            env.mock_url()
        ))
        .create_async()
        .await;

    // Then mock the file download
    let _m2 = env
        .server
        .mock("GET", "/files/test.txt")
        .with_status(200)
        .with_header("content-type", "text/plain")
        .with_body("Hello, World!")
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["files", "get", "F001"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello, World!"));
}

#[tokio::test]
async fn test_files_get_auth_required() {
    let env = test_env_or_skip!();

    env.slack_cmd()
        .args(["files", "get", "F001"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

// ============================================================================
// Files Filter Tests
// ============================================================================

#[tokio::test]
async fn test_files_list_in_channel() {
    let mut env = test_env_or_skip!();

    // Mock files.list with channel filter
    let _m = env
        .server
        .mock("POST", "/files.list")
        .match_body(mockito::Matcher::PartialJson(serde_json::json!({
            "channel": "C001"
        })))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_files_list_response(&[(
            "F001",
            "channel-file.pdf",
            "application/pdf",
            1024,
        )]))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["files", "list", "--channel", "C001"])
        .assert()
        .success()
        .stdout(predicate::str::contains("channel-file.pdf"));
}

// ============================================================================
// Error Output Format Tests
// ============================================================================

#[tokio::test]
async fn test_files_error_json_format() {
    let env = test_env_or_skip!();

    let output = env
        .slack_cmd()
        .args(["files", "list"])
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
