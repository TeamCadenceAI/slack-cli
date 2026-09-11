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
fn root_routes_pins_and_emoji_and_renders_help() {
    Cli::command().debug_assert();

    let pins = Cli::try_parse_from(["slack", "pins", "list", "C12345678"]).unwrap();
    assert!(matches!(pins.command, Commands::Pins(_)));
    let emoji = Cli::try_parse_from(["slack", "emoji", "list"]).unwrap();
    assert!(matches!(emoji.command, Commands::Emoji(_)));

    let help = Cli::command().render_help().to_string();
    assert!(help.contains("pins"));
    assert!(help.contains("emoji"));

    let pins_help = Cli::try_parse_from(["slack", "pins", "--help"]).unwrap_err();
    assert_eq!(pins_help.kind(), clap::error::ErrorKind::DisplayHelp);
    let pins_help = pins_help.to_string();
    assert!(pins_help.contains("add"));
    assert!(pins_help.contains("remove"));
    assert!(pins_help.contains("list"));

    let emoji_help = Cli::try_parse_from(["slack", "emoji", "--help"]).unwrap_err();
    assert_eq!(emoji_help.kind(), clap::error::ErrorKind::DisplayHelp);
    assert!(emoji_help.to_string().contains("list"));
}

#[tokio::test]
async fn pins_add_posts_form_and_outputs_json_with_global_token() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let add = server
        .mock("POST", "/pins.add")
        .match_header("authorization", format!("Bearer {}", TOKEN).as_str())
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("channel".into(), "C12345678".into()),
            Matcher::UrlEncoded("timestamp".into(), "111.222".into()),
        ]))
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;

    let output = isolated_command(&server, &temp.path().join("no-tokens.json"))
        .args(["pins", "add", "C12345678", "111.222", "--token", TOKEN])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        serde_json::json!({"ok":true,"channel":"C12345678","ts":"111.222"})
    );
    add.assert_async().await;
}

#[tokio::test]
async fn pins_remove_resolves_channel_name_and_outputs_json() {
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
    let remove = server
        .mock("POST", "/pins.remove")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("channel".into(), "C87654321".into()),
            Matcher::UrlEncoded("timestamp".into(), "333.444".into()),
        ]))
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;

    let output = command(&server, &temp)
        .args(["pins", "remove", "#general", "333.444"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        serde_json::json!({"ok":true,"channel":"C87654321","ts":"333.444"})
    );
    channels.assert_async().await;
    remove.assert_async().await;
}

#[tokio::test]
async fn pins_remove_plain_outputs_only_escaped_timestamp() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let remove = server
        .mock("POST", "/pins.remove")
        .match_body(Matcher::UrlEncoded(
            "timestamp".into(),
            "333\t444\r\n".into(),
        ))
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;

    command(&server, &temp)
        .args(["pins", "remove", "G12345678", "333\t444\r\n", "--plain"])
        .assert()
        .success()
        .stdout("333\\t444\\r\\n\n");
    remove.assert_async().await;
}

#[tokio::test]
async fn pins_list_retains_all_item_shapes_metadata_and_order() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let list = server
        .mock("POST", "/pins.list")
        .match_body(Matcher::UrlEncoded("channel".into(), "C12345678".into()))
        .with_body(
            r#"{"ok":true,"items":[
                {"type":"message","created":10,"created_by":"UCREATOR1","channel":"C12345678","message":{"ts":"1.1","user":"U11111111","text":"first","blocks":[{"type":"section"}]}},
                {"type":"file","created":20,"file":{"id":"F12345678","user":"U22222222","title":"report","mimetype":"text/plain"}},
                {"type":"file_comment","file":{"id":"F87654321","title":"notes"},"comment":{"id":"Fc123","user":"U33333333","comment":"keep me"},"optional_metadata":{"future":true}}
            ]}"#,
        )
        .create_async()
        .await;

    let output = command(&server, &temp)
        .args(["pins", "list", "C12345678"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["channel"], "C12345678");
    assert_eq!(json["items"].as_array().unwrap().len(), 3);
    assert_eq!(json["items"][0]["message"]["blocks"][0]["type"], "section");
    assert_eq!(json["items"][1]["file"]["id"], "F12345678");
    assert_eq!(json["items"][2]["comment"]["comment"], "keep me");
    assert_eq!(json["items"][2]["optional_metadata"]["future"], true);
    list.assert_async().await;
}

#[tokio::test]
async fn pins_list_plain_formats_records_escapes_fields_and_handles_missing_fields() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let list = server
        .mock("POST", "/pins.list")
        .match_body(Matcher::UrlEncoded("channel".into(), "D12345678".into()))
        .with_body(
            r#"{"ok":true,"items":[
                {"type":"message","message":{"ts":"1\t2","user":"U1\nX","text":"line\r\ntext"}},
                {"type":"file","file":{"id":"F1","user":"U2","title":"tab\ttitle"}},
                {"type":"future","metadata":{"kept":true}}
            ]}"#,
        )
        .create_async()
        .await;

    command(&server, &temp)
        .args(["--plain", "pins", "list", "D12345678"])
        .assert()
        .success()
        .stdout(
            "message\t1\\t2\tU1\\nX\tline\\r\\ntext\nfile\tF1\tU2\ttab\\ttitle\nfuture\t\t\t\n",
        );
    list.assert_async().await;
}

#[tokio::test]
async fn pins_list_empty_prints_no_plain_rows() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let list = server
        .mock("POST", "/pins.list")
        .match_body(Matcher::UrlEncoded("channel".into(), "C12345678".into()))
        .with_body(r#"{"ok":true,"items":[]}"#)
        .create_async()
        .await;

    command(&server, &temp)
        .args(["pins", "list", "C12345678", "--plain"])
        .assert()
        .success()
        .stdout("");
    list.assert_async().await;
}

#[tokio::test]
async fn pin_api_errors_are_propagated() {
    for (subcommand, endpoint, error) in [
        ("add", "/pins.add", "already_pinned"),
        ("remove", "/pins.remove", "not_pinned"),
    ] {
        let mut server = mock_server().await;
        let temp = TempDir::new().unwrap();
        let api = server
            .mock("POST", endpoint)
            .with_body(format!(r#"{{"ok":false,"error":"{}"}}"#, error))
            .create_async()
            .await;
        command(&server, &temp)
            .args(["pins", subcommand, "C12345678", "111.222"])
            .assert()
            .failure()
            .stdout(predicate::str::contains(error));
        api.assert_async().await;
    }

    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let list = server
        .mock("POST", "/pins.list")
        .with_body(r#"{"ok":false,"error":"missing_scope"}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["pins", "list", "C12345678"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("missing_scope"));
    list.assert_async().await;
}

#[tokio::test]
async fn emoji_list_posts_empty_form_and_preserves_urls_and_aliases() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let emoji = server
        .mock("POST", "/emoji.list")
        .match_body(Matcher::Exact(String::new()))
        .with_body(r#"{"ok":true,"emoji":{"party":"https://emoji.slack-edge.com/T/party/abc.png","party_alias":"alias:party"}}"#)
        .create_async()
        .await;

    let output = command(&server, &temp)
        .args(["emoji", "list"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        serde_json::json!({"emoji": {
            "party": "https://emoji.slack-edge.com/T/party/abc.png",
            "party_alias": "alias:party"
        }})
    );
    emoji.assert_async().await;
}

#[tokio::test]
async fn emoji_plain_is_sorted_escaped_and_empty_output_is_empty() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let populated = server
        .mock("POST", "/emoji.list")
        .with_body(r#"{"ok":true,"emoji":{"zeta":"alias:a\tb","alpha\nname":"https://example.com/a\r.png"}}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["--plain", "emoji", "list"])
        .assert()
        .success()
        .stdout("alpha\\nname\thttps://example.com/a\\r.png\nzeta\talias:a\\tb\n");
    populated.assert_async().await;

    let empty = server
        .mock("POST", "/emoji.list")
        .with_body(r#"{"ok":true,"emoji":{}}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["emoji", "list", "--plain"])
        .assert()
        .success()
        .stdout("");
    empty.assert_async().await;
}

#[tokio::test]
async fn emoji_uses_global_workspace_selection() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let store = write_workspace_store(&temp);
    let emoji = server
        .mock("POST", "/emoji.list")
        .match_header("authorization", format!("Bearer {}", STORED_TOKEN).as_str())
        .with_body(r#"{"ok":true,"emoji":{}}"#)
        .create_async()
        .await;

    isolated_command(&server, &store)
        .args(["emoji", "list", "--workspace", "workspace-one"])
        .assert()
        .success();
    emoji.assert_async().await;
}

#[tokio::test]
async fn emoji_missing_scope_and_authentication_errors_are_propagated_without_extra_io() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let missing_scope = server
        .mock("POST", "/emoji.list")
        .with_body(r#"{"ok":false,"error":"missing_scope"}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["emoji", "list"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("missing_scope"));
    missing_scope.assert_async().await;

    let no_io = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;
    isolated_command(&server, &temp.path().join("missing-store.json"))
        .args(["pins", "list", "C12345678"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
    isolated_command(&server, &temp.path().join("missing-store.json"))
        .args(["emoji", "list"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("auth_required"));
    no_io.assert_async().await;
}
