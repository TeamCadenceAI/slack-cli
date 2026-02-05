//! Common utilities for integration tests
//!
//! Provides helpers for setting up test environments, mock servers,
//! and temporary token storage.
//!
//! **Note:** Integration tests require `SLACK_INTEGRATION_TESTS=1` to run because
//! mockito requires socket binding which may fail in restricted environments.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use mockito::ServerGuard;
use std::path::PathBuf;
use tempfile::TempDir;

/// Check if integration tests should run
///
/// Returns true if `SLACK_INTEGRATION_TESTS=1` is set.
pub fn should_run_integration_tests() -> bool {
    std::env::var("SLACK_INTEGRATION_TESTS").is_ok_and(|v| v == "1")
}

/// Macro to skip integration tests when SLACK_INTEGRATION_TESTS is not set
/// or when the mock server cannot be created.
#[macro_export]
macro_rules! skip_if_no_integration {
    () => {
        if !$crate::integration::common::should_run_integration_tests() {
            eprintln!("Skipping integration test (set SLACK_INTEGRATION_TESTS=1 to run)");
            return;
        }
    };
}

/// Macro to create a test environment or skip the test if creation fails.
///
/// Usage:
/// ```ignore
/// let mut env = test_env_or_skip!();
/// ```
#[macro_export]
macro_rules! test_env_or_skip {
    () => {{
        if !$crate::integration::common::should_run_integration_tests() {
            eprintln!("Skipping integration test (set SLACK_INTEGRATION_TESTS=1 to run)");
            return;
        }
        match $crate::integration::common::TestEnv::try_new().await {
            Some(env) => env,
            None => {
                // Message already printed by try_new()
                return;
            }
        }
    }};
}

/// Test environment setup with mock server and file-based token storage
pub struct TestEnv {
    /// Mock server for Slack API
    pub server: ServerGuard,
    /// Temporary directory for token storage (kept alive to preserve temp files)
    #[allow(dead_code)]
    tmp_dir: TempDir,
    /// Path to the token store file
    token_store_path: PathBuf,
}

impl TestEnv {
    /// Create a new test environment with mock server
    ///
    /// Returns `None` if mockito server cannot be created (e.g., in restricted environments
    /// where socket binding is not permitted). Uses `tokio::task::spawn` to catch panics
    /// from the mockito server creation.
    pub async fn try_new() -> Option<Self> {
        // Use tokio::task::spawn to catch panics - if the task panics, we get a JoinError
        let server = match tokio::task::spawn(async { mockito::Server::new_async().await }).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "Skipping integration test: mock server creation failed ({})",
                    e
                );
                return None;
            }
        };

        let tmp_dir = match TempDir::new() {
            Ok(d) => d,
            Err(e) => {
                eprintln!(
                    "Skipping integration test: temp dir creation failed ({})",
                    e
                );
                return None;
            }
        };
        let token_store_path = tmp_dir.path().join("tokens.json");

        Some(Self {
            server,
            tmp_dir,
            token_store_path,
        })
    }

    /// Create a new test environment with mock server
    ///
    /// **Panics** if mockito server cannot be created (e.g., in restricted environments).
    /// Prefer using `try_new()` with the `test_env_or_skip!` macro for graceful skipping.
    #[allow(dead_code)]
    pub async fn new() -> Self {
        Self::try_new()
            .await
            .expect("Failed to create test environment - mock server binding not permitted")
    }

    /// Get the mock server URL
    pub fn mock_url(&self) -> String {
        self.server.url()
    }

    /// Get the token store path
    #[allow(dead_code)]
    pub fn token_store_path(&self) -> &PathBuf {
        &self.token_store_path
    }

    /// Create a Command with test environment variables set
    pub fn slack_cmd(&self) -> Command {
        let mut cmd = cargo_bin_cmd!("slack");
        cmd.env("SLACK_API_BASE_URL", self.server.url());
        cmd.env("SLACK_TOKEN_STORE_PATH", &self.token_store_path);
        cmd
    }

    /// Create a Command with test environment and a token set via SLACK_TOKEN env var
    pub fn slack_cmd_with_token(&self, token_type: &str) -> Command {
        let mut cmd = cargo_bin_cmd!("slack");
        cmd.env("SLACK_API_BASE_URL", self.server.url());
        cmd.env("SLACK_TOKEN_STORE_PATH", &self.token_store_path);

        let token = match token_type {
            "UserOAuth" | "user" => "xoxp-test-token-12345678901234",
            "BotOAuth" | "bot" => "xoxb-test-token-12345678901234",
            "Browser" | "browser" => "xoxc-test-token-12345678901234",
            _ => "xoxp-test-token-12345678901234",
        };
        cmd.env("SLACK_TOKEN", token);
        cmd
    }

    /// Pre-populate the file-based token store with a test token
    ///
    /// This enables testing auth commands end-to-end.
    pub fn setup_token_store(&self, team_id: &str, team_name: &str) {
        use std::fs;
        use std::io::Write;

        // Note: token_type uses snake_case serialization (e.g., "user_o_auth" not "UserOAuth")
        let tokens = serde_json::json!({
            "tokens": {
                team_id: {
                    "token_type": "user_o_auth",
                    "access_token": "xoxp-test-token-12345678901234",
                    "xoxd_cookie": null,
                    "team_id": team_id,
                    "team_name": team_name,
                    "user_id": "U12345TEST",
                    "created_at": "2024-01-01T00:00:00Z",
                    "scopes": []
                }
            },
            "default_team": team_id,
            "workspaces": [team_id]
        });

        let mut file =
            fs::File::create(&self.token_store_path).expect("Failed to create token store file");
        file.write_all(tokens.to_string().as_bytes())
            .expect("Failed to write token store");
    }

    /// Pre-populate the file-based token store with two workspaces
    ///
    /// WS1 is set as default, WS2 is also available
    pub fn setup_multi_token_store(&self) {
        use std::fs;
        use std::io::Write;

        // Note: token_type uses snake_case serialization (e.g., "user_o_auth" not "UserOAuth")
        let tokens = serde_json::json!({
            "tokens": {
                "T_WS1": {
                    "token_type": "user_o_auth",
                    "access_token": "xoxp-ws1-token-12345678901234",
                    "xoxd_cookie": null,
                    "team_id": "T_WS1",
                    "team_name": "Workspace One",
                    "user_id": "U_WS1_USER",
                    "created_at": "2024-01-01T00:00:00Z",
                    "scopes": []
                },
                "T_WS2": {
                    "token_type": "user_o_auth",
                    "access_token": "xoxp-ws2-token-12345678901234",
                    "xoxd_cookie": null,
                    "team_id": "T_WS2",
                    "team_name": "Workspace Two",
                    "user_id": "U_WS2_USER",
                    "created_at": "2024-01-02T00:00:00Z",
                    "scopes": []
                }
            },
            "default_team": "T_WS1",
            "workspaces": ["T_WS1", "T_WS2"]
        });

        let mut file =
            fs::File::create(&self.token_store_path).expect("Failed to create token store file");
        file.write_all(tokens.to_string().as_bytes())
            .expect("Failed to write token store");
    }
}

// =============================================================================
// Mock Response Helpers
// =============================================================================

/// Mock response for auth.test
pub fn mock_auth_test_response(
    team_id: &str,
    team_name: &str,
    user_id: &str,
    user_name: &str,
) -> String {
    format!(
        r#"{{
            "ok": true,
            "url": "https://{}.slack.com/",
            "team": "{}",
            "user": "{}",
            "team_id": "{}",
            "user_id": "{}"
        }}"#,
        team_name.to_lowercase().replace(' ', "-"),
        team_name,
        user_name,
        team_id,
        user_id
    )
}

/// Mock response for conversations.list
pub fn mock_conversations_list_response(channels: &[(&str, &str, bool)]) -> String {
    let channel_json: Vec<String> = channels
        .iter()
        .map(|(id, name, is_private)| {
            format!(
                r#"{{
                    "id": "{}",
                    "name": "{}",
                    "is_channel": true,
                    "is_private": {},
                    "is_member": true,
                    "num_members": 10
                }}"#,
                id, name, is_private
            )
        })
        .collect();

    format!(
        r#"{{
            "ok": true,
            "channels": [{}],
            "response_metadata": {{
                "next_cursor": ""
            }}
        }}"#,
        channel_json.join(",")
    )
}

/// Mock response for conversations.info
pub fn mock_conversations_info_response(
    id: &str,
    name: &str,
    is_private: bool,
    topic: &str,
    purpose: &str,
) -> String {
    format!(
        r#"{{
            "ok": true,
            "channel": {{
                "id": "{}",
                "name": "{}",
                "is_channel": true,
                "is_private": {},
                "is_member": true,
                "num_members": 42,
                "topic": {{
                    "value": "{}"
                }},
                "purpose": {{
                    "value": "{}"
                }}
            }}
        }}"#,
        id, name, is_private, topic, purpose
    )
}

/// Mock response for conversations.history
pub fn mock_conversations_history_response(messages: &[(&str, &str, &str)]) -> String {
    let messages_json: Vec<String> = messages
        .iter()
        .map(|(ts, user, text)| {
            format!(
                r#"{{
                    "type": "message",
                    "ts": "{}",
                    "user": "{}",
                    "text": "{}"
                }}"#,
                ts, user, text
            )
        })
        .collect();

    format!(
        r#"{{
            "ok": true,
            "messages": [{}],
            "has_more": false,
            "response_metadata": {{
                "next_cursor": ""
            }}
        }}"#,
        messages_json.join(",")
    )
}

/// Mock response for chat.postMessage
pub fn mock_chat_post_message_response(channel: &str, ts: &str) -> String {
    format!(
        r#"{{
            "ok": true,
            "channel": "{}",
            "ts": "{}",
            "message": {{
                "type": "message",
                "ts": "{}",
                "user": "U12345TEST",
                "text": "Hello!"
            }}
        }}"#,
        channel, ts, ts
    )
}

/// Mock response for search.messages
pub fn mock_search_messages_response(query: &str, matches: &[(&str, &str, &str, &str)]) -> String {
    let matches_json: Vec<String> = matches
        .iter()
        .map(|(channel, ts, user, text)| {
            format!(
                r#"{{
                    "type": "message",
                    "channel": {{
                        "id": "{}",
                        "name": "general"
                    }},
                    "ts": "{}",
                    "user": "{}",
                    "text": "{}"
                }}"#,
                channel, ts, user, text
            )
        })
        .collect();

    format!(
        r#"{{
            "ok": true,
            "query": "{}",
            "messages": {{
                "total": {},
                "matches": [{}]
            }}
        }}"#,
        query,
        matches.len(),
        matches_json.join(",")
    )
}

/// Mock response for users.list
#[allow(dead_code)]
pub fn mock_users_list_response(users: &[(&str, &str, &str, bool)]) -> String {
    let users_json: Vec<String> = users
        .iter()
        .map(|(id, name, real_name, is_bot)| {
            format!(
                r#"{{
                    "id": "{}",
                    "name": "{}",
                    "real_name": "{}",
                    "is_bot": {}
                }}"#,
                id, name, real_name, is_bot
            )
        })
        .collect();

    format!(
        r#"{{
            "ok": true,
            "members": [{}],
            "response_metadata": {{
                "next_cursor": ""
            }}
        }}"#,
        users_json.join(",")
    )
}

/// Mock response for files.list
pub fn mock_files_list_response(files: &[(&str, &str, &str, u64)]) -> String {
    let files_json: Vec<String> = files
        .iter()
        .map(|(id, name, mimetype, size)| {
            format!(
                r#"{{
                    "id": "{}",
                    "name": "{}",
                    "mimetype": "{}",
                    "size": {},
                    "url_private": "https://files.slack.com/files-pri/T12345-{}/{}",
                    "timestamp": 1234567890
                }}"#,
                id, name, mimetype, size, id, name
            )
        })
        .collect();

    format!(
        r#"{{
            "ok": true,
            "files": [{}]
        }}"#,
        files_json.join(",")
    )
}

/// Mock response for files.info
pub fn mock_files_info_response(id: &str, name: &str, mimetype: &str, size: u64) -> String {
    format!(
        r#"{{
            "ok": true,
            "file": {{
                "id": "{}",
                "name": "{}",
                "mimetype": "{}",
                "size": {},
                "url_private": "https://files.slack.com/files-pri/T12345-{}/{}",
                "timestamp": 1234567890
            }}
        }}"#,
        id, name, mimetype, size, id, name
    )
}

/// Mock error response
pub fn mock_error_response(error: &str) -> String {
    format!(r#"{{"ok": false, "error": "{}"}}"#, error)
}
