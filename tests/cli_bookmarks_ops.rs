use std::path::Path;

use assert_cmd::cargo::cargo_bin_cmd;
use clap::{CommandFactory, Parser};
use mockito::{Matcher, ServerGuard};
use predicates::prelude::*;
use serde_json::Value;
use slack_cli::cli::{Cli, Commands};
use tempfile::TempDir;

const TOKEN: &str = "xoxp-test-token-12345678901234";
const STORED_TOKEN: &str = "xoxp-workspace-token-1234567890";

async fn mock_server() -> ServerGuard {
    mockito::Server::new_async().await
}

fn isolated_command(server: &ServerGuard, store_path: &Path) -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("slack");
    cmd.env("SLACK_API_BASE_URL", server.url())
        .env("SLACK_TOKEN_STORE_PATH", store_path)
        .env_remove("SLACK_TOKEN")
        .env_remove("SLACK_WORKSPACE")
        .env_remove("SLACK_PLAIN");
    cmd
}

fn command(server: &ServerGuard, temp: &TempDir) -> assert_cmd::Command {
    let mut cmd = isolated_command(server, &temp.path().join("no-tokens.json"));
    cmd.env("SLACK_TOKEN", TOKEN);
    cmd
}

fn write_workspace_store(temp: &TempDir) -> std::path::PathBuf {
    let path = temp.path().join("tokens.json");
    let data = serde_json::json!({
        "tokens": {
            "T12345678": {
                "token_type": "user_o_auth",
                "access_token": STORED_TOKEN,
                "team_id": "T12345678",
                "team_name": "Workspace One",
                "team_domain": "workspace-one",
                "user_id": "U12345678",
                "created_at": "2024-01-01T00:00:00Z",
                "scopes": []
            }
        },
        "default": "T12345678",
        "workspaces": ["T12345678"]
    });
    std::fs::write(&path, data.to_string()).unwrap();
    path
}

#[test]
fn root_routes_bookmarks_renders_help_and_accepts_global_flags() {
    Cli::command().debug_assert();

    let cli = Cli::try_parse_from([
        "slack",
        "bookmarks",
        "list",
        "C12345678",
        "--plain",
        "--workspace",
        "workspace-one",
        "--token",
        TOKEN,
    ])
    .unwrap();
    assert!(matches!(cli.command, Commands::Bookmarks(_)));
    assert!(cli.plain);
    assert_eq!(cli.workspace.as_deref(), Some("workspace-one"));
    assert_eq!(cli.token.as_deref(), Some(TOKEN));

    let help = Cli::command().render_help().to_string();
    assert!(help.contains("bookmarks"));
    let bookmarks_help = Cli::try_parse_from(["slack", "bookmarks", "--help"]).unwrap_err();
    assert_eq!(bookmarks_help.kind(), clap::error::ErrorKind::DisplayHelp);
    let bookmarks_help = bookmarks_help.to_string();
    assert!(bookmarks_help.contains("list"));
    assert!(bookmarks_help.contains("add"));
    assert!(bookmarks_help.contains("remove"));

    let add_help = Cli::try_parse_from(["slack", "bookmarks", "add", "--help"]).unwrap_err();
    assert_eq!(add_help.kind(), clap::error::ErrorKind::DisplayHelp);
    assert!(add_help.to_string().contains("--emoji"));
}

#[test]
fn bookmarks_required_operands_are_enforced() {
    assert!(Cli::try_parse_from(["slack", "bookmarks", "list"]).is_err());
    assert!(Cli::try_parse_from(["slack", "bookmarks", "add", "C12345678", "Docs"]).is_err());
    assert!(Cli::try_parse_from(["slack", "bookmarks", "remove", "C12345678"]).is_err());
}

#[tokio::test]
async fn bookmarks_list_posts_exact_form_resolves_channel_and_preserves_json_metadata_order() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let channels = server
        .mock("POST", "/conversations.list")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("limit".into(), "200".into()),
            Matcher::UrlEncoded("exclude_archived".into(), "false".into()),
            Matcher::UrlEncoded(
                "types".into(),
                "public_channel,private_channel,mpim,im".into(),
            ),
        ]))
        .with_body(r#"{"ok":true,"channels":[{"id":"C87654321","name":"general"}],"response_metadata":{"next_cursor":""}}"#)
        .create_async()
        .await;
    let list = server
        .mock("POST", "/bookmarks.list")
        .match_body(Matcher::Exact("channel_id=C87654321".to_string()))
        .with_body(
            r#"{"ok":true,"bookmarks":[
                {"id":"BkFIRST123","title":"First","link":"https://example.com/one","emoji":":one:","type":"link","date_created":10},
                {"id":"BkSECOND12","title":"Second","link":"https://example.com/two","icon_url":"https://example.com/icon.png","future":{"kept":true}}
            ]}"#,
        )
        .create_async()
        .await;

    let output = command(&server, &temp)
        .args(["bookmarks", "list", "#general"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["channel"], "C87654321");
    let bookmarks = json["bookmarks"].as_array().unwrap();
    assert_eq!(bookmarks.len(), 2);
    assert_eq!(bookmarks[0]["id"], "BkFIRST123");
    assert_eq!(bookmarks[0]["type"], "link");
    assert_eq!(bookmarks[0]["date_created"], 10);
    assert_eq!(bookmarks[1]["id"], "BkSECOND12");
    assert_eq!(bookmarks[1]["emoji"], Value::Null);
    assert_eq!(bookmarks[1]["future"]["kept"], true);
    channels.assert_async().await;
    list.assert_async().await;
}

#[tokio::test]
async fn bookmarks_list_plain_escapes_every_column_and_empty_list_has_no_rows() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let populated = server
        .mock("POST", "/bookmarks.list")
        .match_body(Matcher::Exact("channel_id=D12345678".to_string()))
        .with_body(
            "{\"ok\":true,\"bookmarks\":[{\"id\":\"Bk\\t1\",\"title\":\"Line\\nTitle\",\"link\":\"https://example.com/a\\rb\",\"emoji\":\":book\\tmark:\"},{\"id\":\"Bk2\",\"title\":\"No emoji\",\"link\":\"https://example.com\"}]}"
        )
        .create_async()
        .await;

    command(&server, &temp)
        .args(["--plain", "bookmarks", "list", "D12345678"])
        .assert()
        .success()
        .stdout("Bk\\t1\tLine\\nTitle\thttps://example.com/a\\rb\t:book\\tmark:\nBk2\tNo emoji\thttps://example.com\t\n");
    populated.assert_async().await;

    let empty = server
        .mock("POST", "/bookmarks.list")
        .match_body(Matcher::Exact("channel_id=C12345678".to_string()))
        .with_body(r#"{"ok":true,"bookmarks":[]}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["bookmarks", "list", "C12345678", "--plain"])
        .assert()
        .success()
        .stdout("");
    empty.assert_async().await;
}

#[tokio::test]
async fn bookmarks_add_omits_absent_emoji_and_outputs_returned_bookmark() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let add = server
        .mock("POST", "/bookmarks.add")
        .match_header("authorization", format!("Bearer {}", TOKEN).as_str())
        .match_body(Matcher::Exact(
            "channel_id=C12345678&link=https%3A%2F%2Fexample.com%2Fdocs%3Fa%3D1%26b%3D2&title=Team+Docs&type=link"
                .to_string(),
        ))
        .with_body(r#"{"ok":true,"bookmark":{"id":"Bk12345678","title":"Team Docs","link":"https://example.com/docs?a=1&b=2","type":"link","channel_id":"C12345678","date_updated":20}}"#)
        .create_async()
        .await;

    let output = isolated_command(&server, &temp.path().join("no-tokens.json"))
        .args([
            "bookmarks",
            "add",
            "C12345678",
            "Team Docs",
            "https://example.com/docs?a=1&b=2",
            "--token",
            TOKEN,
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["channel"], "C12345678");
    assert_eq!(json["bookmark"]["id"], "Bk12345678");
    assert_eq!(json["bookmark"]["channel_id"], "C12345678");
    assert_eq!(json["bookmark"]["date_updated"], 20);
    assert_eq!(json["bookmark"]["emoji"], Value::Null);
    add.assert_async().await;
}

#[tokio::test]
async fn bookmarks_add_normalizes_emoji_and_plain_outputs_returned_id() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let add = server
        .mock("POST", "/bookmarks.add")
        .match_body(Matcher::Exact(
            "channel_id=G12345678&emoji=%3Abooks%3A&link=http%3A%2F%2Fexample.com%2Fdocs&title=Docs&type=link"
                .to_string(),
        ))
        .with_body(
            "{\"ok\":true,\"bookmark\":{\"id\":\"Bk\\t123\\r\\n\",\"title\":\"Docs\",\"link\":\"http://example.com/docs\",\"emoji\":\":books:\"}}",
        )
        .create_async()
        .await;

    command(&server, &temp)
        .args([
            "bookmarks",
            "add",
            "G12345678",
            "Docs",
            "http://example.com/docs",
            "--emoji",
            "books",
            "--plain",
        ])
        .assert()
        .success()
        .stdout("Bk\\t123\\r\\n\n");
    add.assert_async().await;
}

#[tokio::test]
async fn bookmarks_remove_posts_exact_form_and_supports_json_and_plain() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let remove_json = server
        .mock("POST", "/bookmarks.remove")
        .match_body(Matcher::Exact(
            "bookmark_id=Bk12345678&channel_id=C12345678".to_string(),
        ))
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;

    let output = command(&server, &temp)
        .args(["bookmarks", "remove", "C12345678", "Bk12345678"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        serde_json::json!({
            "ok": true,
            "channel": "C12345678",
            "bookmark_id": "Bk12345678"
        })
    );
    remove_json.assert_async().await;

    let remove_plain = server
        .mock("POST", "/bookmarks.remove")
        .match_body(Matcher::Exact(
            "bookmark_id=Bk%09123&channel_id=G12345678".to_string(),
        ))
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["bookmarks", "remove", "G12345678", "Bk\t123", "--plain"])
        .assert()
        .success()
        .stdout("Bk\\t123\n");
    remove_plain.assert_async().await;
}

#[tokio::test]
async fn invalid_bookmark_inputs_fail_with_usage_before_network_io() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let no_io = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;

    for args in [
        vec![
            "bookmarks",
            "add",
            "not-a-channel-id",
            "   ",
            "https://example.com",
        ],
        vec![
            "bookmarks",
            "add",
            "not-a-channel-id",
            "Docs",
            "relative/path",
        ],
        vec![
            "bookmarks",
            "add",
            "not-a-channel-id",
            "Docs",
            "ftp://example.com/docs",
        ],
        vec![
            "bookmarks",
            "add",
            "not-a-channel-id",
            "Docs",
            "https://user:pass@example.com/docs",
        ],
        vec![
            "bookmarks",
            "add",
            "not-a-channel-id",
            "Docs",
            "https://example.com",
            "--emoji",
            "",
        ],
        vec![
            "bookmarks",
            "add",
            "not-a-channel-id",
            "Docs",
            "https://example.com",
            "--emoji",
            ":bad",
        ],
        vec!["bookmarks", "remove", "not-a-channel-id", "   "],
    ] {
        command(&server, &temp)
            .args(args)
            .assert()
            .code(2)
            .stdout(predicate::str::contains("usage_error"));
    }
    no_io.assert_async().await;
}

#[tokio::test]
async fn bookmark_missing_scope_not_found_and_api_errors_propagate() {
    for (args, endpoint, error) in [
        (
            vec!["bookmarks", "list", "C12345678"],
            "/bookmarks.list",
            "missing_scope",
        ),
        (
            vec![
                "bookmarks",
                "add",
                "C12345678",
                "Docs",
                "https://example.com",
            ],
            "/bookmarks.add",
            "restricted_action",
        ),
        (
            vec!["bookmarks", "remove", "C12345678", "BkMISSING1"],
            "/bookmarks.remove",
            "bookmark_not_found",
        ),
    ] {
        let mut server = mock_server().await;
        let temp = TempDir::new().unwrap();
        let api = server
            .mock("POST", endpoint)
            .with_body(format!(r#"{{"ok":false,"error":"{}"}}"#, error))
            .create_async()
            .await;
        command(&server, &temp)
            .args(args)
            .assert()
            .failure()
            .stdout(predicate::str::contains(error));
        api.assert_async().await;
    }
}

#[tokio::test]
async fn bookmarks_use_workspace_auth_and_missing_auth_performs_no_api_io() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let store = write_workspace_store(&temp);
    let list = server
        .mock("POST", "/bookmarks.list")
        .match_header("authorization", format!("Bearer {}", STORED_TOKEN).as_str())
        .match_body(Matcher::Exact("channel_id=C12345678".to_string()))
        .with_body(r#"{"ok":true,"bookmarks":[]}"#)
        .create_async()
        .await;
    isolated_command(&server, &store)
        .args([
            "bookmarks",
            "list",
            "C12345678",
            "--workspace",
            "workspace-one",
        ])
        .assert()
        .success();
    list.assert_async().await;

    let no_io = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;
    isolated_command(&server, &temp.path().join("missing-store.json"))
        .args(["bookmarks", "list", "C12345678"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
    no_io.assert_async().await;
}
