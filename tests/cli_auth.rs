//! Unit tests for auth CLI parsing
//!
//! Tests for auth subcommand parsing, flag conflicts, and requirements.

use assert_cmd::cargo::cargo_bin_cmd;
use clap::Parser;
use mockito::{Matcher, ServerGuard};
use predicates::prelude::*;
use serde_json::{json, Value};
use slack_cli::cli::auth::AuthCommands;
use slack_cli::cli::{Cli, Commands};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const USER_TOKEN: &str = "xoxp-auth-cli-user-token-123456789";
const BOT_TOKEN: &str = "xoxb-auth-cli-bot-token-123456789";
const XOXC_TOKEN: &str = "xoxc-auth-cli-browser-token-123456789";
const XOXD_TOKEN: &str = "xoxd-auth-cli-cookie-123456789";

fn command(server: &ServerGuard, store_path: &Path) -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("slack");
    cmd.env("SLACK_API_BASE_URL", server.url())
        .env("SLACK_TOKEN_STORE_PATH", store_path)
        .env_remove("SLACK_TOKEN")
        .env_remove("SLACK_WORKSPACE")
        .env_remove("SLACK_PLAIN")
        .env_remove("SLACK_CLIENT_ID")
        .env_remove("SLACK_CLIENT_SECRET")
        .env_remove("SLACK_EXTRACT_FIXTURE");
    cmd
}

fn auth_response(team_id: &str, team: &str, user_id: &str, user: &str, domain: &str) -> String {
    json!({
        "ok": true,
        "team_id": team_id,
        "team": team,
        "user_id": user_id,
        "user": user,
        "url": format!("https://{domain}.slack.com/")
    })
    .to_string()
}

async fn mock_auth(server: &mut ServerGuard, token: &str, response: Value) -> mockito::Mock {
    server
        .mock("POST", "/auth.test")
        .match_header("authorization", format!("Bearer {token}").as_str())
        .match_header(
            "content-type",
            Matcher::Regex("^application/x-www-form-urlencoded".into()),
        )
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(response.to_string())
        .create_async()
        .await
}

fn stored_token(store_path: &Path, team_id: &str) -> Value {
    let store: Value = serde_json::from_slice(&fs::read(store_path).unwrap()).unwrap();
    store["tokens"][team_id].clone()
}

fn seed_store(store_path: &Path) {
    fs::write(
        store_path,
        json!({
            "tokens": {
                "TALPHA": {
                    "token_type": "user_o_auth",
                    "access_token": USER_TOKEN,
                    "team_id": "TALPHA",
                    "team_name": "Alpha Team",
                    "team_domain": "alpha",
                    "user_id": "UALPHA",
                    "created_at": "2024-01-01T00:00:00Z",
                    "scopes": ["channels:read"]
                },
                "TBETA": {
                    "token_type": "bot_o_auth",
                    "access_token": BOT_TOKEN,
                    "team_id": "TBETA",
                    "team_name": "Beta Team",
                    "team_domain": "beta",
                    "user_id": "UBETA",
                    "created_at": "2024-01-02T00:00:00Z",
                    "scopes": []
                }
            },
            "default": "TALPHA",
            "workspaces": ["TALPHA", "TBETA"]
        })
        .to_string(),
    )
    .unwrap();
}

fn write_extract_fixture(temp: &TempDir) -> PathBuf {
    let path = temp.path().join("extract.json");
    fs::write(
        &path,
        json!({
            "workspaces": [
                {
                    "xoxc": XOXC_TOKEN,
                    "xoxd": XOXD_TOKEN,
                    "team_id": "TLOCAL",
                    "team_domain": "local",
                    "team_name": "Local Name",
                    "source": "chrome/Default"
                },
                {
                    "xoxc": "xoxc-expired-browser-token-123456789",
                    "xoxd": "xoxd-expired-cookie-123456789",
                    "team_id": "TEXPIRED",
                    "team_domain": "expired",
                    "team_name": "Expired Local",
                    "source": "chrome/Profile 1"
                },
                {
                    "xoxc": "xoxc-other-browser-token-123456789",
                    "xoxd": "xoxd-other-cookie-123456789",
                    "team_id": "TOTHER",
                    "team_domain": "other",
                    "team_name": "Other Browser",
                    "source": "slack/Default"
                }
            ]
        })
        .to_string(),
    )
    .unwrap();
    path
}

// ============================================================================
// Auth Add Command Tests
// ============================================================================

#[test]
fn test_parse_auth_add_token() {
    let cli = Cli::try_parse_from(["slack", "auth", "add", "--token", "xoxp-123456789"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        if let AuthCommands::Add {
            token,
            xoxc,
            xoxd,
            oauth,
            manual,
            scopes,
            ..
        } = auth_cmd.command
        {
            assert_eq!(token, Some("xoxp-123456789".to_string()));
            assert!(xoxc.is_none());
            assert!(xoxd.is_none());
            assert!(!oauth);
            assert!(!manual);
            // Check default scopes
            assert!(scopes.contains(&"channels:read".to_string()));
        } else {
            panic!("Expected Add command");
        }
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_parse_auth_add_browser_tokens() {
    let cli = Cli::try_parse_from([
        "slack", "auth", "add", "--xoxc", "xoxc-123", "--xoxd", "xoxd-456",
    ])
    .unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        if let AuthCommands::Add {
            token,
            xoxc,
            xoxd,
            oauth,
            ..
        } = auth_cmd.command
        {
            assert!(token.is_none());
            assert_eq!(xoxc, Some("xoxc-123".to_string()));
            assert_eq!(xoxd, Some("xoxd-456".to_string()));
            assert!(!oauth);
        } else {
            panic!("Expected Add command");
        }
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_parse_auth_add_oauth() {
    let cli = Cli::try_parse_from(["slack", "auth", "add", "--oauth"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        if let AuthCommands::Add {
            token,
            xoxc,
            xoxd,
            oauth,
            ..
        } = auth_cmd.command
        {
            assert!(token.is_none());
            assert!(xoxc.is_none());
            assert!(xoxd.is_none());
            assert!(oauth);
        } else {
            panic!("Expected Add command");
        }
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_parse_auth_add_manual() {
    let cli = Cli::try_parse_from(["slack", "auth", "add", "--manual"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        if let AuthCommands::Add { manual, .. } = auth_cmd.command {
            assert!(manual);
        } else {
            panic!("Expected Add command");
        }
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_parse_auth_add_custom_scopes() {
    let cli = Cli::try_parse_from([
        "slack",
        "auth",
        "add",
        "--oauth",
        "--scopes",
        "channels:read,chat:write,users:read",
    ])
    .unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        if let AuthCommands::Add { scopes, .. } = auth_cmd.command {
            assert_eq!(scopes, vec!["channels:read", "chat:write", "users:read"]);
        } else {
            panic!("Expected Add command");
        }
    } else {
        panic!("Expected Auth command");
    }
}

// ============================================================================
// Auth Add Flag Conflict Tests
// ============================================================================

#[test]
fn test_auth_add_conflicts_token_and_oauth() {
    let result = Cli::try_parse_from(["slack", "auth", "add", "--token", "xoxp-123", "--oauth"]);
    assert!(result.is_err());
}

#[test]
fn test_auth_add_conflicts_token_and_xoxc() {
    let result = Cli::try_parse_from([
        "slack", "auth", "add", "--token", "xoxp-123", "--xoxc", "xoxc-456", "--xoxd", "xoxd-789",
    ]);
    assert!(result.is_err());
}

#[test]
fn test_auth_add_conflicts_oauth_and_xoxc() {
    let result = Cli::try_parse_from([
        "slack", "auth", "add", "--oauth", "--xoxc", "xoxc-123", "--xoxd", "xoxd-456",
    ]);
    assert!(result.is_err());
}

// ============================================================================
// Auth Add Flag Requirement Tests
// ============================================================================

#[test]
fn test_auth_add_xoxc_requires_xoxd() {
    let result = Cli::try_parse_from(["slack", "auth", "add", "--xoxc", "xoxc-123"]);
    assert!(result.is_err());
}

#[test]
fn test_auth_add_xoxd_requires_xoxc() {
    let result = Cli::try_parse_from(["slack", "auth", "add", "--xoxd", "xoxd-123"]);
    assert!(result.is_err());
}

// ============================================================================
// Auth List Command Tests
// ============================================================================

#[test]
fn test_parse_auth_list() {
    let cli = Cli::try_parse_from(["slack", "auth", "list"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        assert!(matches!(auth_cmd.command, AuthCommands::List { .. }));
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_parse_auth_list_with_plain() {
    let cli = Cli::try_parse_from(["slack", "auth", "list", "--plain"]).unwrap();
    assert!(cli.plain);
    if let Commands::Auth(auth_cmd) = cli.command {
        assert!(matches!(auth_cmd.command, AuthCommands::List { .. }));
    } else {
        panic!("Expected Auth command");
    }
}

// ============================================================================
// Auth Remove Command Tests
// ============================================================================

#[test]
fn test_parse_auth_remove() {
    let cli = Cli::try_parse_from(["slack", "auth", "remove", "T12345"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        if let AuthCommands::Remove { workspace, yes } = auth_cmd.command {
            assert_eq!(workspace, "T12345");
            assert!(!yes);
        } else {
            panic!("Expected Remove command");
        }
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_parse_auth_remove_with_yes() {
    let cli = Cli::try_parse_from(["slack", "auth", "remove", "T12345", "--yes"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        if let AuthCommands::Remove { workspace, yes } = auth_cmd.command {
            assert_eq!(workspace, "T12345");
            assert!(yes);
        } else {
            panic!("Expected Remove command");
        }
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_parse_auth_remove_with_y_short() {
    let cli = Cli::try_parse_from(["slack", "auth", "remove", "T12345", "-y"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        if let AuthCommands::Remove { yes, .. } = auth_cmd.command {
            assert!(yes);
        } else {
            panic!("Expected Remove command");
        }
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_parse_auth_remove_by_name() {
    let cli = Cli::try_parse_from(["slack", "auth", "remove", "My Workspace"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        if let AuthCommands::Remove { workspace, .. } = auth_cmd.command {
            assert_eq!(workspace, "My Workspace");
        } else {
            panic!("Expected Remove command");
        }
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_auth_remove_requires_workspace() {
    let result = Cli::try_parse_from(["slack", "auth", "remove"]);
    assert!(result.is_err());
}

// ============================================================================
// Auth Status Command Tests
// ============================================================================

#[test]
fn test_parse_auth_status() {
    let cli = Cli::try_parse_from(["slack", "auth", "status"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        assert!(matches!(auth_cmd.command, AuthCommands::Status));
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_parse_auth_status_with_workspace() {
    let cli = Cli::try_parse_from(["slack", "-w", "T12345", "auth", "status"]).unwrap();
    assert_eq!(cli.workspace, Some("T12345".to_string()));
    if let Commands::Auth(auth_cmd) = cli.command {
        assert!(matches!(auth_cmd.command, AuthCommands::Status));
    } else {
        panic!("Expected Auth command");
    }
}

// ============================================================================
// Auth Switch Command Tests
// ============================================================================

#[test]
fn test_parse_auth_switch() {
    let cli = Cli::try_parse_from(["slack", "auth", "switch", "T12345"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        if let AuthCommands::Switch { workspace } = auth_cmd.command {
            assert_eq!(workspace, "T12345");
        } else {
            panic!("Expected Switch command");
        }
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_parse_auth_switch_by_name() {
    let cli = Cli::try_parse_from(["slack", "auth", "switch", "My Workspace"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        if let AuthCommands::Switch { workspace } = auth_cmd.command {
            assert_eq!(workspace, "My Workspace");
        } else {
            panic!("Expected Switch command");
        }
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_auth_switch_requires_workspace() {
    let result = Cli::try_parse_from(["slack", "auth", "switch"]);
    assert!(result.is_err());
}

// ============================================================================
// Auth Help Command Tests
// ============================================================================

#[test]
fn test_parse_auth_browser_help() {
    let cli = Cli::try_parse_from(["slack", "auth", "browser-help"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        assert!(matches!(auth_cmd.command, AuthCommands::BrowserHelp));
    } else {
        panic!("Expected Auth command");
    }
}

// ============================================================================
// Auth Alias Tests
// ============================================================================

#[test]
fn test_auth_alias_list() {
    let cli = Cli::try_parse_from(["slack", "a", "list"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        assert!(matches!(auth_cmd.command, AuthCommands::List { .. }));
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_auth_alias_status() {
    let cli = Cli::try_parse_from(["slack", "a", "status"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        assert!(matches!(auth_cmd.command, AuthCommands::Status));
    } else {
        panic!("Expected Auth command");
    }
}
#[tokio::test]
async fn auth_add_direct_user_and_bot_tokens_persists_identity_and_type() {
    for (token, team_id, token_type) in [
        (USER_TOKEN, "TUSER", "user_o_auth"),
        (BOT_TOKEN, "TBOT", "bot_o_auth"),
    ] {
        let mut server = mockito::Server::new_async().await;
        let temp = TempDir::new().unwrap();
        let store_path = temp.path().join("tokens.json");
        let auth = mock_auth(
            &mut server,
            token,
            json!({
                "ok": true,
                "team_id": team_id,
                "team": format!("{team_id} Team"),
                "user_id": "U12345678",
                "user": "alice",
                "url": format!("https://{}.slack.com/", team_id.to_ascii_lowercase())
            }),
        )
        .await;

        let output = command(&server, &store_path)
            .args(["auth", "add", "--token", token])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["added"], true);
        assert_eq!(result["team_id"], team_id);

        let stored = stored_token(&store_path, team_id);
        assert_eq!(stored["access_token"], token);
        assert_eq!(stored["token_type"], token_type);
        assert_eq!(stored["team_domain"], team_id.to_ascii_lowercase());
        let store: Value = serde_json::from_slice(&fs::read(&store_path).unwrap()).unwrap();
        assert_eq!(store["default"], team_id);
        auth.assert_async().await;
    }
}

#[tokio::test]
async fn auth_add_direct_plain_and_invalid_tokens_cover_error_paths() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("tokens.json");
    let auth = mock_auth(
        &mut server,
        USER_TOKEN,
        serde_json::from_str(&auth_response(
            "TPLAIN",
            "Plain Team",
            "UPLAIN",
            "plain-user",
            "plain",
        ))
        .unwrap(),
    )
    .await;
    command(&server, &store_path)
        .args(["--plain", "auth", "add", "--token", USER_TOKEN])
        .assert()
        .success()
        .stdout("Added\tTPLAIN\tPlain Team\tUPLAIN\tplain-user\n");
    auth.assert_async().await;

    command(&server, &store_path)
        .args(["auth", "add", "--token", "not-a-slack-token"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("invalid_token"));

    command(&server, &store_path)
        .args(["auth", "add", "--token", XOXC_TOKEN])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("require --xoxc and --xoxd"));
}

#[tokio::test]
async fn auth_add_api_failure_is_not_persisted() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("tokens.json");
    let auth = mock_auth(
        &mut server,
        USER_TOKEN,
        json!({"ok": false, "error": "invalid_auth"}),
    )
    .await;

    command(&server, &store_path)
        .args(["auth", "add", "--token", USER_TOKEN])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("invalid_auth"));
    assert!(!store_path.exists());
    auth.assert_async().await;
}

#[tokio::test]
async fn auth_add_browser_tokens_sends_cookie_and_persists_pair() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("tokens.json");
    let auth = server
        .mock("POST", "/auth.test")
        .match_header("authorization", format!("Bearer {XOXC_TOKEN}").as_str())
        .match_header("cookie", format!("d={XOXD_TOKEN}").as_str())
        .with_header("content-type", "application/json")
        .with_body(auth_response(
            "TBROWSER",
            "Browser Team",
            "UBROWSER",
            "browser-user",
            "browser",
        ))
        .create_async()
        .await;

    let output = command(&server, &store_path)
        .args(["auth", "add", "--xoxc", XOXC_TOKEN, "--xoxd", XOXD_TOKEN])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["token_type"], "Browser");
    let stored = stored_token(&store_path, "TBROWSER");
    assert_eq!(stored["token_type"], "browser");
    assert_eq!(stored["access_token"], XOXC_TOKEN);
    assert_eq!(stored["xoxd_cookie"], XOXD_TOKEN);
    assert_eq!(stored["team_domain"], "browser");
    auth.assert_async().await;
}

#[tokio::test]
async fn auth_list_outputs_stored_workspaces_and_checks_live_state() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("tokens.json");
    seed_store(&store_path);

    let output = command(&server, &store_path)
        .args(["auth", "list"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    assert_eq!(rows[0]["team_domain"], "alpha");

    command(&server, &store_path)
        .args(["--plain", "auth", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "TALPHA\talpha\tAlpha Team\tUserOAuth\t*",
        ))
        .stdout(predicate::str::contains(
            "TBETA\tbeta\tBeta Team\tBotOAuth\t",
        ));

    let alpha = mock_auth(
        &mut server,
        USER_TOKEN,
        json!({"ok": true, "team_id": "TALPHA", "team": "Alpha Team", "user_id": "UALPHA", "user": "alice", "url": "https://alpha.slack.com/"}),
    )
    .await;
    let beta = mock_auth(
        &mut server,
        BOT_TOKEN,
        json!({"ok": false, "error": "invalid_auth"}),
    )
    .await;
    let output = command(&server, &store_path)
        .args(["auth", "list", "--check"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    let alpha_row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["team_id"] == "TALPHA")
        .unwrap();
    let beta_row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["team_id"] == "TBETA")
        .unwrap();
    assert_eq!(alpha_row["live"], true);
    assert_eq!(beta_row["live"], false);
    alpha.assert_async().await;
    beta.assert_async().await;
}

#[tokio::test]
async fn auth_list_check_plain_prints_live_and_expired() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("tokens.json");
    seed_store(&store_path);
    let alpha = mock_auth(
        &mut server,
        USER_TOKEN,
        json!({"ok": true, "team_id": "TALPHA", "team": "Alpha", "user_id": "U", "user": "u", "url": "https://alpha.slack.com/"}),
    )
    .await;
    let beta = mock_auth(
        &mut server,
        BOT_TOKEN,
        json!({"ok": false, "error": "token_revoked"}),
    )
    .await;
    command(&server, &store_path)
        .args(["--plain", "auth", "list", "--check"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "TALPHA\talpha\tAlpha Team\tUserOAuth\t*\tlive",
        ))
        .stdout(predicate::str::contains(
            "TBETA\tbeta\tBeta Team\tBotOAuth\t\texpired",
        ));
    alpha.assert_async().await;
    beta.assert_async().await;
}

#[tokio::test]
async fn auth_status_uses_default_domain_selector_and_reports_failures() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("tokens.json");
    seed_store(&store_path);

    let alpha = mock_auth(
        &mut server,
        USER_TOKEN,
        json!({"ok": true, "team_id": "TALPHA", "team": "Alpha Team", "user_id": "UALPHA", "user": "alice", "url": "https://alpha.slack.com/"}),
    )
    .await;
    let output = command(&server, &store_path)
        .args(["auth", "status"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let status: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["ok"], true);
    assert_eq!(status["token_type"], "UserOAuth");
    alpha.assert_async().await;

    let beta = mock_auth(
        &mut server,
        BOT_TOKEN,
        json!({"ok": true, "team_id": "TBETA", "team": "Beta Team", "user_id": "UBETA", "user": "bob", "url": "https://beta.slack.com/"}),
    )
    .await;
    command(&server, &store_path)
        .args(["--plain", "--workspace", "beta", "auth", "status"])
        .assert()
        .success()
        .stdout("ok\tTBETA\tBeta Team\tUBETA\tbob\n");
    beta.assert_async().await;

    let failure = mock_auth(
        &mut server,
        USER_TOKEN,
        json!({"ok": false, "error": "invalid_auth"}),
    )
    .await;
    command(&server, &store_path)
        .args(["--plain", "auth", "status"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "error\tSlack API error: invalid_auth",
        ));
    failure.assert_async().await;
}

#[tokio::test]
async fn auth_status_rejects_bad_overrides_and_missing_auth() {
    let server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("tokens.json");

    command(&server, &store_path)
        .args(["--token", "invalid", "auth", "status"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("invalid_token"));
    command(&server, &store_path)
        .args(["--token", XOXC_TOKEN, "auth", "status"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("Browser tokens require"));
    command(&server, &store_path)
        .args(["auth", "status"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("auth_required"));
}

#[tokio::test]
async fn auth_switch_by_team_and_domain_then_remove_persists_changes() {
    let server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("tokens.json");
    seed_store(&store_path);

    command(&server, &store_path)
        .args(["auth", "switch", "TBETA"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"team_id\": \"TBETA\""));
    let store: Value = serde_json::from_slice(&fs::read(&store_path).unwrap()).unwrap();
    assert_eq!(store["default"], "TBETA");

    command(&server, &store_path)
        .args(["--plain", "auth", "switch", "alpha"])
        .assert()
        .success()
        .stdout("Switched\tTALPHA\tAlpha Team\n");

    command(&server, &store_path)
        .args(["--plain", "auth", "remove", "beta"])
        .assert()
        .success()
        .stdout("Removed\tTBETA\tBeta Team\n")
        .stderr("Removing workspace: Beta Team (TBETA)\n");
    let store: Value = serde_json::from_slice(&fs::read(&store_path).unwrap()).unwrap();
    assert!(store["tokens"].get("TBETA").is_none());
    assert_eq!(store["workspaces"], json!(["TALPHA"]));
}

#[tokio::test]
async fn auth_remove_json_yes_and_unknown_workspace_paths() {
    let server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("tokens.json");
    seed_store(&store_path);
    let output = command(&server, &store_path)
        .args(["auth", "remove", "TALPHA", "--yes"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let removed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(removed["removed"], true);
    assert_eq!(removed["team_name"], "Alpha Team");

    command(&server, &store_path)
        .args(["auth", "switch", "missing"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("workspace_not_found"));
    command(&server, &store_path)
        .args(["auth", "remove", "missing", "--yes"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("workspace_not_found"));
}

#[tokio::test]
async fn auth_browser_help_and_oauth_configuration_hint() {
    let server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("tokens.json");
    command(&server, &store_path)
        .args(["auth", "browser-help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("BROWSER TOKEN EXTRACTION GUIDE"))
        .stdout(predicate::str::contains("slack auth add --xoxc"));
    command(&server, &store_path)
        .args(["auth", "add"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("SLACK_CLIENT_ID"));
}

#[tokio::test]
async fn auth_discover_uses_fixture_and_browser_filter() {
    let server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("tokens.json");
    let fixture = write_extract_fixture(&temp);
    let output = command(&server, &store_path)
        .env("SLACK_EXTRACT_FIXTURE", &fixture)
        .args(["auth", "discover", "--browser", "slack"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["count"], 1);
    assert_eq!(result["workspaces"][0]["team_id"], "TOTHER");
    assert!(result["workspaces"][0].get("live").is_none());

    command(&server, &store_path)
        .env("SLACK_EXTRACT_FIXTURE", &fixture)
        .args(["--plain", "auth", "discover", "--browser", "slack"])
        .assert()
        .success()
        .stdout("other\tTOTHER\tOther Browser\tslack/Default\n");
}

#[tokio::test]
async fn auth_discover_check_marks_fixture_sessions_live_or_expired() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("tokens.json");
    let fixture = write_extract_fixture(&temp);
    let live = mock_auth(
        &mut server,
        XOXC_TOKEN,
        json!({"ok": true, "team_id": "TLOCAL", "team": "Authoritative Name", "user_id": "ULOCAL", "user": "local-user", "url": "https://local.slack.com/"}),
    )
    .await;
    let expired = mock_auth(
        &mut server,
        "xoxc-expired-browser-token-123456789",
        json!({"ok": false, "error": "invalid_auth"}),
    )
    .await;
    let output = command(&server, &store_path)
        .env("SLACK_EXTRACT_FIXTURE", &fixture)
        .args(["auth", "discover", "--browser", "chrome", "--check"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["count"], 2);
    assert_eq!(result["workspaces"][0]["team_name"], "Authoritative Name");
    assert_eq!(result["workspaces"][0]["live"], true);
    assert_eq!(result["workspaces"][1]["live"], false);
    assert_eq!(stored_token(&store_path, "TLOCAL")["token_type"], "browser");
    live.assert_async().await;
    expired.assert_async().await;
}

#[tokio::test]
async fn auth_add_subdomain_imports_only_matching_fixture_workspace() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("tokens.json");
    let fixture = write_extract_fixture(&temp);
    let auth = mock_auth(
        &mut server,
        XOXC_TOKEN,
        json!({"ok": true, "team_id": "TLOCAL", "team": "Imported Team", "user_id": "ULOCAL", "user": "imported-user", "url": "https://local.slack.com/"}),
    )
    .await;
    let output = command(&server, &store_path)
        .env("SLACK_EXTRACT_FIXTURE", &fixture)
        .args(["auth", "add", "local.slack.com"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["added_count"], 1);
    assert_eq!(result["errored_count"], 0);
    assert_eq!(result["added"][0]["source"], "chrome/Default");
    assert_eq!(
        stored_token(&store_path, "TLOCAL")["team_name"],
        "Imported Team"
    );
    auth.assert_async().await;
}

#[tokio::test]
async fn auth_add_from_browser_reports_partial_and_total_failures() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("tokens.json");
    let fixture_path = temp.path().join("one-fixture.json");
    fs::write(
        &fixture_path,
        json!({"workspaces": [{
            "xoxc": XOXC_TOKEN,
            "xoxd": XOXD_TOKEN,
            "team_id": "TLOCAL",
            "team_domain": "local",
            "team_name": "Local Name",
            "source": "chrome/Default"
        }]})
        .to_string(),
    )
    .unwrap();
    let failed = mock_auth(
        &mut server,
        XOXC_TOKEN,
        json!({"ok": false, "error": "invalid_auth"}),
    )
    .await;
    command(&server, &store_path)
        .env("SLACK_EXTRACT_FIXTURE", &fixture_path)
        .args(["--plain", "auth", "add", "--from-browser"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains(
            "Error\tLocal Name\tchrome/Default",
        ))
        .stderr(predicate::str::contains("none could be validated"));
    failed.assert_async().await;

    command(&server, &store_path)
        .env("SLACK_EXTRACT_FIXTURE", &fixture_path)
        .args(["auth", "add", "missing"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("matching 'missing' was found"));
}

#[tokio::test]
async fn auth_fixture_parse_error_and_storage_failure_are_reported() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let bad_fixture = temp.path().join("bad.json");
    fs::write(&bad_fixture, "not json").unwrap();
    let store_path = temp.path().join("tokens.json");
    command(&server, &store_path)
        .env("SLACK_EXTRACT_FIXTURE", &bad_fixture)
        .args(["auth", "add", "--from-browser"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("serialization_error"));

    let blocked_parent = temp.path().join("blocked");
    fs::write(&blocked_parent, "file, not directory").unwrap();
    let blocked_store = blocked_parent.join("tokens.json");
    let auth = mock_auth(
        &mut server,
        USER_TOKEN,
        json!({"ok": true, "team_id": "TBLOCKED", "team": "Blocked", "user_id": "U", "user": "u", "url": "https://blocked.slack.com/"}),
    )
    .await;
    command(&server, &blocked_store)
        .args(["auth", "add", "--token", USER_TOKEN])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("Troubleshooting:"))
        .stdout(predicate::str::contains("io_error"));
    auth.assert_async().await;
}

#[tokio::test]
async fn auth_status_token_override_and_json_api_failure() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("tokens.json");
    let ok = mock_auth(
        &mut server,
        USER_TOKEN,
        json!({"ok": true, "team_id": "TOVERRIDE", "team": "Override", "user_id": "UOVERRIDE", "user": "override", "url": "https://override.slack.com/"}),
    )
    .await;
    command(&server, &store_path)
        .args(["--token", USER_TOKEN, "auth", "status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("TOVERRIDE"));
    ok.assert_async().await;

    seed_store(&store_path);
    let failed = mock_auth(
        &mut server,
        USER_TOKEN,
        json!({"ok": false, "error": "account_inactive"}),
    )
    .await;
    command(&server, &store_path)
        .args(["auth", "status"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("\"ok\": false"))
        .stdout(predicate::str::contains("account_inactive"));
    failed.assert_async().await;

    command(&server, &store_path)
        .args(["--workspace", "missing", "auth", "status"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("workspace_not_found"));
}

#[tokio::test]
async fn auth_browser_plain_and_import_plain_success() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let first_store = temp.path().join("manual-tokens.json");
    let manual = mock_auth(
        &mut server,
        XOXC_TOKEN,
        json!({"ok": true, "team_id": "TMANUAL", "team": "Manual Browser", "user_id": "UMANUAL", "user": "manual", "url": "https://manual.slack.com/"}),
    )
    .await;
    command(&server, &first_store)
        .args([
            "--plain", "auth", "add", "--xoxc", XOXC_TOKEN, "--xoxd", XOXD_TOKEN,
        ])
        .assert()
        .success()
        .stdout("Added\tTMANUAL\tManual Browser\tUMANUAL\tmanual\n");
    manual.assert_async().await;

    let fixture = write_extract_fixture(&temp);
    let import_store = temp.path().join("import-tokens.json");
    let imported = mock_auth(
        &mut server,
        XOXC_TOKEN,
        json!({"ok": true, "team_id": "TLOCAL", "team": "Imported", "user_id": "ULOCAL", "user": "local", "url": "https://local.slack.com/"}),
    )
    .await;
    command(&server, &import_store)
        .env("SLACK_EXTRACT_FIXTURE", fixture)
        .args(["--plain", "auth", "add", "local"])
        .assert()
        .success()
        .stdout("Added\tTLOCAL\tImported\tULOCAL\tlocal\n");
    imported.assert_async().await;
}

#[tokio::test]
async fn auth_discover_check_plain_and_empty_import_without_url() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("tokens.json");
    let fixture = write_extract_fixture(&temp);
    let live = mock_auth(
        &mut server,
        XOXC_TOKEN,
        json!({"ok": true, "team_id": "TLOCAL", "team": "Checked Name", "user_id": "U", "user": "u", "url": "https://local.slack.com/"}),
    )
    .await;
    let expired = mock_auth(
        &mut server,
        "xoxc-expired-browser-token-123456789",
        json!({"ok": false, "error": "invalid_auth"}),
    )
    .await;
    command(&server, &store_path)
        .env("SLACK_EXTRACT_FIXTURE", fixture)
        .args([
            "--plain",
            "auth",
            "discover",
            "--browser",
            "chrome",
            "--check",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "local\tTLOCAL\tChecked Name\tchrome/Default\tlive",
        ))
        .stdout(predicate::str::contains(
            "expired\tTEXPIRED\tExpired Local\tchrome/Profile 1\texpired",
        ));
    live.assert_async().await;
    expired.assert_async().await;

    let empty_fixture = temp.path().join("empty.json");
    fs::write(&empty_fixture, r#"{"workspaces":[]}"#).unwrap();
    command(&server, &store_path)
        .env("SLACK_EXTRACT_FIXTURE", empty_fixture)
        .args(["auth", "add", "--from-browser"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "No locally logged-in Slack workspaces were found",
        ));
}

#[tokio::test]
async fn auth_add_rejects_malformed_prefixed_tokens_before_network_io() {
    let server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = temp.path().join("tokens.json");
    for args in [
        vec!["auth", "add", "--token", "xoxp-a"],
        vec!["auth", "add", "--xoxc", "xoxc-a", "--xoxd", XOXD_TOKEN],
    ] {
        command(&server, &store_path)
            .args(args)
            .assert()
            .code(1)
            .stdout(predicate::str::contains("invalid_token"));
    }
}
