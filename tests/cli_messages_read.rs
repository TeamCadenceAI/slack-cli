//! End-to-end coverage for bounded and user-resolved message reads.

use std::path::Path;

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use mockito::{Matcher, ServerGuard};
use tempfile::TempDir;

const TOKEN: &str = "xoxp-read-test-token-1234567890";
const CHANNEL: &str = "C123456789";

fn slack_cmd(server: &ServerGuard, store: &Path) -> Command {
    let mut command = cargo_bin_cmd!("slack");
    command
        .env("SLACK_API_BASE_URL", server.url())
        .env("SLACK_TOKEN_STORE_PATH", store)
        .env("SLACK_TOKEN", TOKEN)
        .env_remove("SLACK_WORKSPACE")
        .env_remove("SLACK_PLAIN");
    command
}

fn parse_json(output: &std::process::Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("valid JSON output")
}

fn form(fields: &[(&str, &str)]) -> Matcher {
    Matcher::AllOf(
        fields
            .iter()
            .map(|(key, value)| Matcher::UrlEncoded((*key).into(), (*value).into()))
            .collect(),
    )
}

#[tokio::test]
async fn list_passes_exclusive_normalized_bounds_and_cursor() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let history = server
        .mock("POST", "/conversations.history")
        .match_body(form(&[
            ("channel", CHANNEL),
            ("cursor", "next-page"),
            ("limit", "50"),
            ("oldest", "1735689600.000000"),
            ("latest", "1735776123.123456"),
        ]))
        .expect(1)
        .with_status(200)
        .with_body(
            r#"{"ok":true,"messages":[{"type":"message","user":"U111111111","text":"hello","ts":"1735700000.000001"}],"has_more":true,"response_metadata":{"next_cursor":"after"}}"#,
        )
        .create_async()
        .await;

    let output = slack_cmd(&server, &temp.path().join("tokens.json"))
        .args([
            "messages",
            "list",
            CHANNEL,
            "--since",
            "2025-01-01",
            "--until",
            "2025-01-02T00:02:03.123456Z",
            "--cursor",
            "next-page",
        ])
        .output()
        .unwrap();
    let json = parse_json(&output);
    assert_eq!(json["has_more"], true);
    assert_eq!(json["response_metadata"]["next_cursor"], "after");
    assert_eq!(json["messages"][0]["user"], "U111111111");
    assert!(json["messages"][0].get("user_name").is_none());
    history.assert_async().await;
}

#[tokio::test]
async fn list_duration_and_since_use_the_later_oldest_bound() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let history = server
        .mock("POST", "/conversations.history")
        .match_body(form(&[
            ("channel", CHANNEL),
            ("limit", "100"),
            ("oldest", "9999999999.000001"),
            ("latest", "9999999999.000002"),
        ]))
        .expect(1)
        .with_status(200)
        .with_body(r#"{"ok":true,"messages":[],"has_more":false}"#)
        .create_async()
        .await;

    let output = slack_cmd(&server, &temp.path().join("tokens.json"))
        .args([
            "messages",
            "list",
            CHANNEL,
            "--limit",
            "7d",
            "--since",
            "9999999999.000001",
            "--until",
            "9999999999.000002",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    history.assert_async().await;
}

#[tokio::test]
async fn invalid_or_reversed_bounds_fail_before_api_io() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let no_history = server
        .mock("POST", "/conversations.history")
        .expect(0)
        .create_async()
        .await;

    for args in [
        vec!["--since", "1.1234567"],
        vec!["--since", "2", "--until", "2"],
        vec!["--since", "3", "--until", "2"],
    ] {
        let mut command = slack_cmd(&server, &temp.path().join("tokens.json"));
        command.args(["messages", "list", CHANNEL]);
        let output = command.args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(json["code"], "usage_error");
    }
    no_history.assert_async().await;
}

#[tokio::test]
async fn list_all_collects_pages_ignores_numeric_limit_and_filters_activity() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let first = server
        .mock("POST", "/conversations.history")
        .match_body(form(&[
            ("channel", CHANNEL),
            ("limit", "200"),
            ("oldest", "10.000000"),
            ("latest", "20.000000"),
        ]))
        .expect(1)
        .with_status(200)
        .with_body(
            r#"{"ok":true,"messages":[{"user":"U111111111","text":"new","ts":"19.0"}],"has_more":true,"response_metadata":{"next_cursor":"page-2"}}"#,
        )
        .create_async()
        .await;
    let second = server
        .mock("POST", "/conversations.history")
        .match_body(form(&[
            ("channel", CHANNEL),
            ("cursor", "page-2"),
            ("limit", "200"),
            ("oldest", "10.000000"),
            ("latest", "20.000000"),
        ]))
        .expect(1)
        .with_status(200)
        .with_body(
            r#"{"ok":true,"messages":[{"subtype":"channel_join","text":"joined","ts":"18.0"},{"user":"U222222222","text":"old","ts":"17.0"}],"has_more":true,"response_metadata":{"next_cursor":""}}"#,
        )
        .create_async()
        .await;

    let output = slack_cmd(&server, &temp.path().join("tokens.json"))
        .args([
            "messages", "list", CHANNEL, "--limit", "1", "--since", "10", "--until", "20", "--all",
        ])
        .output()
        .unwrap();
    let json = parse_json(&output);
    assert_eq!(json["messages"].as_array().unwrap().len(), 2);
    assert_eq!(json["messages"][0]["text"], "new");
    assert_eq!(json["messages"][1]["text"], "old");
    assert_eq!(json["has_more"], false);
    assert!(json["response_metadata"].is_null());
    first.assert_async().await;
    second.assert_async().await;
}

#[tokio::test]
async fn list_resolves_users_with_one_complete_directory_traversal() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let history = server
        .mock("POST", "/conversations.history")
        .match_body(form(&[("channel", CHANNEL), ("limit", "50")]))
        .expect(1)
        .with_status(200)
        .with_body(
            r##"{"ok":true,"messages":[{"user":"U111111111","text":"Hi <@U222222222|legacy> <@U999999999> <#C222222222|elsewhere>","ts":"2.0"},{"user":"U999999999","text":"unknown author","ts":"1.0"},{"text":"system","ts":"0.5"}],"has_more":false}"##,
        )
        .create_async()
        .await;
    let users_one = server
        .mock("POST", "/users.list")
        .match_body(form(&[("limit", "200")]))
        .expect(1)
        .with_status(200)
        .with_body(
            r#"{"ok":true,"members":[{"id":"U111111111","name":"alice"}],"response_metadata":{"next_cursor":"users-2"}}"#,
        )
        .create_async()
        .await;
    let users_two = server
        .mock("POST", "/users.list")
        .match_body(form(&[("cursor", "users-2"), ("limit", "200")]))
        .expect(1)
        .with_status(200)
        .with_body(
            r#"{"ok":true,"members":[{"id":"U222222222","name":"","profile":{"display_name":"Bob"}}],"response_metadata":{"next_cursor":""}}"#,
        )
        .create_async()
        .await;

    let output = slack_cmd(&server, &temp.path().join("tokens.json"))
        .args(["messages", "list", CHANNEL, "--resolve-users"])
        .output()
        .unwrap();
    let json = parse_json(&output);
    assert_eq!(json["messages"][0]["user"], "U111111111");
    assert_eq!(json["messages"][0]["user_name"], "alice");
    assert_eq!(
        json["messages"][0]["text"],
        "Hi @Bob <@U999999999> <#C222222222|elsewhere>"
    );
    assert_eq!(json["messages"][1]["user_name"], "U999999999");
    assert!(json["messages"][2]["user_name"].is_null());
    history.assert_async().await;
    users_one.assert_async().await;
    users_two.assert_async().await;
}

#[tokio::test]
async fn directory_is_not_loaded_without_flag_or_for_empty_results() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let history = server
        .mock("POST", "/conversations.history")
        .expect(2)
        .with_status(200)
        .with_body(r#"{"ok":true,"messages":[],"has_more":false}"#)
        .create_async()
        .await;
    let no_users = server
        .mock("POST", "/users.list")
        .expect(0)
        .create_async()
        .await;

    let normal = slack_cmd(&server, &temp.path().join("tokens.json"))
        .args(["messages", "list", CHANNEL])
        .output()
        .unwrap();
    assert!(normal.status.success());
    let resolved_empty = slack_cmd(&server, &temp.path().join("tokens.json"))
        .args(["messages", "list", CHANNEL, "--resolve-users"])
        .output()
        .unwrap();
    assert!(resolved_empty.status.success());
    history.assert_async().await;
    no_users.assert_async().await;
}

#[tokio::test]
async fn thread_resolved_plain_is_four_column_escaped_tsv() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let replies = server
        .mock("POST", "/conversations.replies")
        .match_body(form(&[
            ("channel", CHANNEL),
            ("ts", "1.000001"),
            ("limit", "100"),
        ]))
        .expect(1)
        .with_status(200)
        .with_body(
            r#"{"ok":true,"messages":[{"user":"U111111111","text":"hi\t<@U111111111>\nnext\rrow","ts":"1.000002"}],"has_more":false}"#,
        )
        .create_async()
        .await;
    let users = server
        .mock("POST", "/users.list")
        .expect(1)
        .with_status(200)
        .with_body(
            r#"{"ok":true,"members":[{"id":"U111111111","name":"alice"}],"response_metadata":{"next_cursor":""}}"#,
        )
        .create_async()
        .await;

    let output = slack_cmd(&server, &temp.path().join("tokens.json"))
        .args([
            "--plain",
            "messages",
            "thread",
            CHANNEL,
            "1.000001",
            "--resolve-users",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "1.000002\talice\tC123456789\thi\\t@alice\\nnext\\rrow\n"
    );
    replies.assert_async().await;
    users.assert_async().await;
}

#[tokio::test]
async fn search_forwards_sort_and_resolves_json_once() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let search = server
        .mock("POST", "/search.messages")
        .match_body(form(&[
            ("query", "deploy in:ops"),
            ("sort", "score"),
            ("sort_dir", "asc"),
            ("count", "20"),
            ("page", "1"),
        ]))
        .expect(1)
        .with_status(200)
        .with_body(
            r#"{"ok":true,"messages":{"total":1,"pagination":{"total_count":1,"page":1,"per_page":20,"page_count":1,"first":1,"last":1},"matches":[{"user":"U111111111","text":"ask <@U111111111>","ts":"3.0","channel":{"id":"C999999999","name":"ops"}}]}}"#,
        )
        .create_async()
        .await;
    let users = server
        .mock("POST", "/users.list")
        .expect(1)
        .with_status(200)
        .with_body(
            r#"{"ok":true,"members":[{"id":"U111111111","name":"alice"}],"response_metadata":{"next_cursor":null}}"#,
        )
        .create_async()
        .await;

    let output = slack_cmd(&server, &temp.path().join("tokens.json"))
        .args([
            "messages",
            "search",
            "deploy",
            "--in-channel",
            "#ops",
            "--sort",
            "score",
            "--sort-dir",
            "asc",
            "--resolve-users",
        ])
        .output()
        .unwrap();
    let json = parse_json(&output);
    assert_eq!(json["total"], 1);
    assert_eq!(json["pagination"]["page"], 1);
    assert_eq!(json["messages"][0]["user_name"], "alice");
    assert_eq!(json["messages"][0]["text"], "ask @alice");
    search.assert_async().await;
    users.assert_async().await;
}

#[tokio::test]
async fn search_defaults_and_plain_resolution_keep_channel_position() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let search = server
        .mock("POST", "/search.messages")
        .match_body(form(&[
            ("query", "hello"),
            ("sort", "timestamp"),
            ("sort_dir", "desc"),
            ("count", "20"),
            ("page", "1"),
        ]))
        .expect(1)
        .with_status(200)
        .with_body(
            r#"{"ok":true,"messages":{"total":1,"pagination":null,"matches":[{"user":"U111111111","text":"hello","ts":"3.0","channel":"C999999999"}]}}"#,
        )
        .create_async()
        .await;
    let users = server
        .mock("POST", "/users.list")
        .expect(1)
        .with_status(200)
        .with_body(r#"{"ok":true,"members":[{"id":"U111111111","name":"alice"}]}"#)
        .create_async()
        .await;

    let output = slack_cmd(&server, &temp.path().join("tokens.json"))
        .args(["--plain", "messages", "search", "hello", "--resolve-users"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "3.0\talice\tC999999999\thello\n"
    );
    search.assert_async().await;
    users.assert_async().await;
}

#[tokio::test]
async fn user_directory_api_failure_fails_the_read() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let history = server
        .mock("POST", "/conversations.history")
        .expect(1)
        .with_status(200)
        .with_body(r#"{"ok":true,"messages":[{"user":"U111111111","ts":"1.0"}]}"#)
        .create_async()
        .await;
    let users = server
        .mock("POST", "/users.list")
        .expect(1)
        .with_status(200)
        .with_body(r#"{"ok":false,"error":"missing_scope"}"#)
        .create_async()
        .await;

    let output = slack_cmd(&server, &temp.path().join("tokens.json"))
        .args(["messages", "list", CHANNEL, "--resolve-users"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["code"], "api_error");
    history.assert_async().await;
    users.assert_async().await;
}

#[tokio::test]
async fn all_and_cursor_conflict_fails_without_api_io() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let no_history = server
        .mock("POST", "/conversations.history")
        .expect(0)
        .create_async()
        .await;
    let output = slack_cmd(&server, &temp.path().join("tokens.json"))
        .args(["messages", "list", CHANNEL, "--all", "--cursor", "next"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    no_history.assert_async().await;
}
