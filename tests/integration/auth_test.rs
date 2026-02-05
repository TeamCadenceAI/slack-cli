//! Integration tests for auth commands
//!
//! Tests for auth add, list, remove, status, and switch commands.
//!
//! **Note:** These tests require `SLACK_INTEGRATION_TESTS=1` because mockito
//! requires socket binding which may fail in restricted environments.

use predicates::prelude::*;

use super::common::*;
use crate::test_env_or_skip;

// ============================================================================
// Auth List Tests (No Auth Setup)
// ============================================================================

#[tokio::test]
async fn test_auth_list_empty_json() {
    let env = test_env_or_skip!();

    // Without stored tokens, auth list should return an empty array
    env.slack_cmd()
        .args(["auth", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[]"));
}

#[tokio::test]
async fn test_auth_list_empty_plain() {
    let env = test_env_or_skip!();

    env.slack_cmd()
        .args(["auth", "list", "--plain"])
        .assert()
        .success();
}

// ============================================================================
// Auth Status Tests
// ============================================================================

#[tokio::test]
async fn test_auth_status_no_auth() {
    let env = test_env_or_skip!();

    env.slack_cmd()
        .args(["auth", "status"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
}

#[tokio::test]
async fn test_auth_status_with_token_override() {
    let mut env = test_env_or_skip!();

    // Mock the auth.test endpoint
    let _m = env
        .server
        .mock("POST", "/auth.test")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_auth_test_response(
            "T12345",
            "Test Workspace",
            "U12345",
            "testuser",
        ))
        .create_async()
        .await;

    env.slack_cmd_with_token("UserOAuth")
        .args(["auth", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("T12345"));
}

// ============================================================================
// Auth Add Tests (Limited - token validation only)
// ============================================================================

#[tokio::test]
async fn test_auth_add_invalid_token_prefix() {
    let env = test_env_or_skip!();

    env.slack_cmd()
        .args(["auth", "add", "--token", "invalid-token-format"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("error"));
}

#[tokio::test]
async fn test_auth_add_api_error() {
    let mut env = test_env_or_skip!();

    // Mock auth.test to return an error
    let _m = env
        .server
        .mock("POST", "/auth.test")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_error_response("invalid_auth"))
        .create_async()
        .await;

    env.slack_cmd()
        .env("SLACK_API_BASE_URL", env.mock_url())
        .args(["auth", "add", "--token", "xoxp-test-token-12345678901234"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("error"));
}

// ============================================================================
// Auth Remove Tests (Can only test error cases without keyring)
// ============================================================================

#[tokio::test]
async fn test_auth_remove_nonexistent() {
    let env = test_env_or_skip!();

    // Try to remove a workspace that doesn't exist
    env.slack_cmd()
        .args(["auth", "remove", "T_NONEXISTENT"])
        .assert()
        .failure();
}

// ============================================================================
// Auth Switch Tests (Can only test error cases without keyring)
// ============================================================================

#[tokio::test]
async fn test_auth_switch_nonexistent() {
    let env = test_env_or_skip!();

    // Try to switch to a workspace that doesn't exist
    env.slack_cmd()
        .args(["auth", "switch", "T_NONEXISTENT"])
        .assert()
        .failure();
}

// ============================================================================
// Browser Help Test
// ============================================================================

#[tokio::test]
async fn test_auth_browser_help() {
    let env = test_env_or_skip!();

    env.slack_cmd()
        .args(["auth", "browser-help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("xoxc"))
        .stdout(predicate::str::contains("xoxd"))
        .stdout(predicate::str::contains("Developer Tools"));
}

// ============================================================================
// Token Override Priority Tests
// ============================================================================

#[tokio::test]
async fn test_token_override_takes_priority() {
    let mut env = test_env_or_skip!();

    // Mock auth.test for the override token
    let _m = env
        .server
        .mock("POST", "/auth.test")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_auth_test_response(
            "T_OVERRIDE",
            "Override Workspace",
            "U_OVERRIDE",
            "overrideuser",
        ))
        .create_async()
        .await;

    // Use --token flag to override
    env.slack_cmd()
        .env("SLACK_API_BASE_URL", env.mock_url())
        .args(["auth", "status", "--token", "xoxp-override-token-123456789"])
        .assert()
        .success()
        .stdout(predicate::str::contains("T_OVERRIDE"));
}

// ============================================================================
// Error Output Format Tests
// ============================================================================

#[tokio::test]
async fn test_auth_error_json_format() {
    let env = test_env_or_skip!();

    // Auth status without any configured workspace should return JSON error
    let output = env
        .slack_cmd()
        .args(["auth", "status"])
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

    // Should have error field
    let json = parsed.unwrap();
    assert!(json.get("error").is_some() || json.get("code").is_some());
}

// ============================================================================
// End-to-End Auth Flow Tests (using file-based token store)
// ============================================================================

#[tokio::test]
async fn test_auth_add_and_list_e2e() {
    let mut env = test_env_or_skip!();

    // Mock auth.test to validate the token
    let _m = env
        .server
        .mock("POST", "/auth.test")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_auth_test_response(
            "T_ADDED",
            "Added Workspace",
            "U_ADDED",
            "addeduser",
        ))
        .create_async()
        .await;

    // Add a token via the CLI
    env.slack_cmd()
        .args(["auth", "add", "--token", "xoxp-test-token-12345678901234"])
        .assert()
        .success();

    // Verify auth list shows the added workspace (JSON)
    env.slack_cmd()
        .args(["auth", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("T_ADDED"))
        .stdout(predicate::str::contains("Added Workspace"));

    // Verify auth list shows the added workspace (plain)
    env.slack_cmd()
        .args(["auth", "list", "--plain"])
        .assert()
        .success()
        .stdout(predicate::str::contains("T_ADDED"))
        .stdout(predicate::str::contains("Added Workspace"));
}

#[tokio::test]
async fn test_auth_add_status_switch_remove_e2e() {
    let mut env = test_env_or_skip!();

    // Mock auth.test for first workspace
    let _m1 = env
        .server
        .mock("POST", "/auth.test")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_auth_test_response(
            "T_FIRST",
            "First Workspace",
            "U_FIRST",
            "firstuser",
        ))
        .expect(2) // Called once for add, once for status
        .create_async()
        .await;

    // Add first workspace
    env.slack_cmd()
        .args(["auth", "add", "--token", "xoxp-first-token-12345678901234"])
        .assert()
        .success();

    // Check status shows first workspace
    env.slack_cmd()
        .args(["auth", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("T_FIRST"))
        .stdout(predicate::str::contains("First Workspace"));

    // Remove the workspace (with --yes to skip prompt)
    env.slack_cmd()
        .args(["auth", "remove", "T_FIRST", "--yes"])
        .assert()
        .success();

    // Verify list is empty again
    env.slack_cmd()
        .args(["auth", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[]"));
}

#[tokio::test]
async fn test_auth_switch_between_workspaces_e2e() {
    let env = test_env_or_skip!();

    // Pre-populate token store with two workspaces
    env.setup_multi_token_store();

    // Verify initial default is WS1
    env.slack_cmd()
        .args(["auth", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("T_WS1"))
        .stdout(predicate::str::contains("T_WS2"));

    // Switch to WS2
    env.slack_cmd()
        .args(["auth", "switch", "T_WS2"])
        .assert()
        .success();

    // Verify WS2 is now marked as default in the list
    let output = env
        .slack_cmd()
        .args(["auth", "list"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("Failed to parse JSON");

    // Find the entry with T_WS2 and verify it's default
    if let Some(workspaces) = json.as_array() {
        let ws2 = workspaces.iter().find(|w| {
            w.get("team_id")
                .and_then(|t| t.as_str())
                .map(|s| s == "T_WS2")
                .unwrap_or(false)
        });
        assert!(ws2.is_some(), "T_WS2 should be in the list");
        let ws2 = ws2.unwrap();
        assert_eq!(
            ws2.get("is_default").and_then(|d| d.as_bool()),
            Some(true),
            "T_WS2 should be default after switch"
        );
    } else {
        panic!("Expected workspaces array");
    }
}

// ============================================================================
// Token Resolution Priority Tests
// ============================================================================

#[tokio::test]
async fn test_token_flag_overrides_env() {
    let mut env = test_env_or_skip!();

    // Mock auth.test - expect the flag token (identified by checking team response)
    let _m = env
        .server
        .mock("POST", "/auth.test")
        .match_header("authorization", "Bearer xoxp-flag-token-12345678901234")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_auth_test_response(
            "T_FLAG",
            "Flag Workspace",
            "U_FLAG",
            "flaguser",
        ))
        .create_async()
        .await;

    // Set SLACK_TOKEN env var to different token, use --token flag
    env.slack_cmd()
        .env("SLACK_TOKEN", "xoxp-env-token-12345678901234")
        .args([
            "auth",
            "status",
            "--token",
            "xoxp-flag-token-12345678901234",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("T_FLAG"));
}

#[tokio::test]
async fn test_env_token_overrides_stored() {
    let mut env = test_env_or_skip!();

    // Pre-populate token store with a different workspace
    env.setup_token_store("T_STORED", "Stored Workspace");

    // Mock auth.test - expect the env token (identified by team response)
    let _m = env
        .server
        .mock("POST", "/auth.test")
        .match_header("authorization", "Bearer xoxp-env-token-12345678901234")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_auth_test_response(
            "T_ENV",
            "Env Workspace",
            "U_ENV",
            "envuser",
        ))
        .create_async()
        .await;

    // Set SLACK_TOKEN env var - should override stored token
    env.slack_cmd()
        .env("SLACK_TOKEN", "xoxp-env-token-12345678901234")
        .args(["auth", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("T_ENV"));
}

#[tokio::test]
async fn test_stored_token_used_when_no_override() {
    let mut env = test_env_or_skip!();

    // Pre-populate token store
    env.setup_token_store("T_STORED", "Stored Workspace");

    // Mock auth.test - expect the stored token
    let _m = env
        .server
        .mock("POST", "/auth.test")
        .match_header("authorization", "Bearer xoxp-test-token-12345678901234")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_auth_test_response(
            "T_STORED",
            "Stored Workspace",
            "U_STORED",
            "storeduser",
        ))
        .create_async()
        .await;

    // No env var, no flag - should use stored token
    env.slack_cmd()
        .args(["auth", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("T_STORED"));
}
