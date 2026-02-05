//! Integration tests for auth storage persistence
//!
//! Tests that authentication tokens persist correctly across CLI invocations.
//! These tests use file-based storage by default for reliable CI testing.
//!
//! **Note:** These tests require `SLACK_INTEGRATION_TESTS=1` because mockito
//! requires socket binding which may fail in restricted environments.

use predicates::prelude::*;
use std::fs;

use super::common::*;
use crate::test_env_or_skip;

// ============================================================================
// File-Based Storage Persistence Tests
// ============================================================================

/// Test that auth add with file storage creates a valid token file
#[tokio::test]
async fn test_file_storage_auth_add_creates_file() {
    let mut env = test_env_or_skip!();

    // Mock auth.test to validate the token
    let _m = env
        .server
        .mock("POST", "/auth.test")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_auth_test_response(
            "T_FILE_TEST",
            "File Storage Test",
            "U_TEST",
            "testuser",
        ))
        .create_async()
        .await;

    // Add auth
    env.slack_cmd()
        .args(["auth", "add", "--token", "xoxp-test-file-storage-token"])
        .assert()
        .success()
        .stdout(predicate::str::contains("added"));

    // Verify the token file was created
    let token_path = env.token_store_path();
    assert!(token_path.exists(), "Token file should be created");

    // Read and verify file contents
    let contents = fs::read_to_string(token_path).expect("Should be able to read token file");
    let json: serde_json::Value =
        serde_json::from_str(&contents).expect("Token file should be valid JSON");

    // Verify structure
    assert!(
        json.get("tokens").is_some(),
        "Token file should have 'tokens' field"
    );
    assert!(
        json["tokens"].get("T_FILE_TEST").is_some(),
        "Token file should contain the team"
    );
}

/// Test that auth list retrieves tokens from file storage
#[tokio::test]
async fn test_file_storage_auth_list_reads_persisted_tokens() {
    let env = test_env_or_skip!();

    // Pre-populate the token store
    env.setup_token_store("T_LIST_TEST", "List Test Workspace");

    // List should show the workspace
    env.slack_cmd()
        .args(["auth", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("T_LIST_TEST"))
        .stdout(predicate::str::contains("List Test Workspace"));
}

/// Test end-to-end flow: add token, then list in new process
#[tokio::test]
async fn test_file_storage_add_then_list_persistence() {
    let mut env = test_env_or_skip!();

    // Mock auth.test
    let _m = env
        .server
        .mock("POST", "/auth.test")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_auth_test_response(
            "T_PERSIST",
            "Persistence Test",
            "U_TEST",
            "testuser",
        ))
        .create_async()
        .await;

    // Add auth
    env.slack_cmd()
        .args(["auth", "add", "--token", "xoxp-persistence-test-token"])
        .assert()
        .success();

    // List in a "new process" (same env, simulating restart)
    env.slack_cmd()
        .args(["auth", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("T_PERSIST"))
        .stdout(predicate::str::contains("Persistence Test"));
}

/// Test that channels list works after auth is persisted
#[tokio::test]
async fn test_file_storage_channels_list_after_auth() {
    let mut env = test_env_or_skip!();

    // Mock auth.test for the add command
    let _m_auth = env
        .server
        .mock("POST", "/auth.test")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_auth_test_response(
            "T_CHANNELS",
            "Channels Test",
            "U_TEST",
            "testuser",
        ))
        .create_async()
        .await;

    // Add auth first
    env.slack_cmd()
        .args(["auth", "add", "--token", "xoxp-channels-test-token"])
        .assert()
        .success();

    // Mock conversations.list
    let _m_channels = env
        .server
        .mock("POST", "/conversations.list")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_conversations_list_response(&[
            ("C_GENERAL", "general", false),
            ("C_RANDOM", "random", false),
        ]))
        .create_async()
        .await;

    // Channels list should work using the persisted token
    env.slack_cmd()
        .args(["channels", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("general"))
        .stdout(predicate::str::contains("random"));
}

/// Test auth remove with file storage
#[tokio::test]
async fn test_file_storage_auth_remove() {
    let env = test_env_or_skip!();

    // Pre-populate with two workspaces
    env.setup_multi_token_store();

    // Remove one workspace
    env.slack_cmd()
        .args(["auth", "remove", "T_WS2", "--yes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("removed"));

    // List should only show the remaining workspace
    env.slack_cmd()
        .args(["auth", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("T_WS1"))
        .stdout(predicate::str::contains("Workspace One"));
}

/// Test auth switch with file storage
#[tokio::test]
async fn test_file_storage_auth_switch() {
    let env = test_env_or_skip!();

    // Pre-populate with two workspaces (WS1 is default)
    env.setup_multi_token_store();

    // Switch to WS2
    env.slack_cmd()
        .args(["auth", "switch", "T_WS2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("switched"))
        .stdout(predicate::str::contains("T_WS2"));

    // Verify the switch persisted (check list output for default marker)
    // The JSON output will show is_default: true for the switched workspace
    let output = env
        .slack_cmd()
        .args(["auth", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Vec<serde_json::Value> =
        serde_json::from_slice(&output).expect("Should be valid JSON");
    let ws2 = json
        .iter()
        .find(|w| w["team_id"] == "T_WS2")
        .expect("WS2 should be in list");
    assert_eq!(
        ws2["is_default"], true,
        "WS2 should now be the default workspace"
    );
}

// ============================================================================
// Keyring Storage Tests (Ignored by default)
// ============================================================================

/// Test keyring storage persistence
///
/// This test is ignored by default because:
/// - It modifies the system keychain
/// - May require user interaction (password prompts)
/// - Will fail in CI without keyring access
///
/// Run with: SLACK_INTEGRATION_TESTS=1 SLACK_KEYRING_TESTS=1 cargo test -- --ignored
#[tokio::test]
#[ignore]
async fn test_keyring_storage_persistence() {
    if std::env::var("SLACK_KEYRING_TESTS").is_err() {
        eprintln!("Skipping keyring test (set SLACK_KEYRING_TESTS=1 to run)");
        return;
    }

    // This test uses actual keyring storage (no SLACK_TOKEN_STORE_PATH)
    // We need to clean up any existing test entries first

    let mut server = match tokio::task::spawn(async { mockito::Server::new_async().await }).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Skipping test: mock server creation failed ({})", e);
            return;
        }
    };

    let team_id = "T_KEYRING_TEST_01";

    // Clean up any existing test entry (ignore errors)
    let _ = keyring::Entry::new("slack-cli", &format!("token:{}", team_id))
        .and_then(|e| e.delete_credential());
    let _ = keyring::Entry::new("slack-cli", "workspaces").and_then(|e| e.delete_credential());
    let _ = keyring::Entry::new("slack-cli", "default").and_then(|e| e.delete_credential());

    // Mock auth.test
    let _m = server
        .mock("POST", "/auth.test")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(mock_auth_test_response(
            team_id,
            "Keyring Test",
            "U_TEST",
            "testuser",
        ))
        .create_async()
        .await;

    // Add auth using keyring (no SLACK_TOKEN_STORE_PATH)
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("slack");
    cmd.env("SLACK_API_BASE_URL", server.url());
    // Note: NOT setting SLACK_TOKEN_STORE_PATH to use keyring

    cmd.args(["auth", "add", "--token", "xoxp-keyring-test-token"])
        .assert()
        .success()
        .stdout(predicate::str::contains("added"));

    // List in a new process (uses keyring)
    let mut cmd2 = assert_cmd::cargo::cargo_bin_cmd!("slack");
    cmd2.env("SLACK_API_BASE_URL", server.url());

    cmd2.args(["auth", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(team_id))
        .stdout(predicate::str::contains("Keyring Test"));

    // Clean up
    let _ = keyring::Entry::new("slack-cli", &format!("token:{}", team_id))
        .and_then(|e| e.delete_credential());
    let _ = keyring::Entry::new("slack-cli", "workspaces").and_then(|e| e.delete_credential());
    let _ = keyring::Entry::new("slack-cli", "default").and_then(|e| e.delete_credential());
}
