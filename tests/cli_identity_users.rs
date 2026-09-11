use assert_cmd::cargo::cargo_bin_cmd;
use mockito::{Matcher, ServerGuard};
use predicates::prelude::*;
use serde_json::Value;
use slack_cli::api::SlackClient;
use slack_cli::auth::TokenSet;
use tempfile::TempDir;

const TOKEN: &str = "xoxp-test-token-12345678901234";

async fn mock_server() -> ServerGuard {
    mockito::Server::new_async().await
}

fn command(server: &ServerGuard, temp: &TempDir) -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("slack");
    cmd.env("SLACK_API_BASE_URL", server.url())
        .env("SLACK_TOKEN_STORE_PATH", temp.path().join("tokens.json"))
        .env("SLACK_TOKEN", TOKEN)
        .env_remove("SLACK_WORKSPACE")
        .env_remove("SLACK_PLAIN");
    cmd
}

fn post_message_response(channel: &str, ts: &str) -> String {
    format!(
        r#"{{"ok":true,"channel":"{}","ts":"{}","message":{{"ts":"{}"}}}}"#,
        channel, ts, ts
    )
}

#[tokio::test]
async fn send_to_named_user_opens_dm_then_posts() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let users = server
        .mock("POST", "/users.list")
        .match_body(Matcher::UrlEncoded("limit".into(), "200".into()))
        .with_body(r#"{"ok":true,"members":[{"id":"U12345678","name":"alice"}],"response_metadata":{"next_cursor":""}}"#)
        .create_async()
        .await;
    let open = server
        .mock("POST", "/conversations.open")
        .match_body(Matcher::UrlEncoded("users".into(), "U12345678".into()))
        .with_body(r#"{"ok":true,"channel":{"id":"D12345678"}}"#)
        .create_async()
        .await;
    let post = server
        .mock("POST", "/chat.postMessage")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("channel".into(), "D12345678".into()),
            Matcher::UrlEncoded("text".into(), "hello".into()),
        ]))
        .with_body(post_message_response("D12345678", "111.222"))
        .create_async()
        .await;

    command(&server, &temp)
        .args(["--plain", "messages", "send", "@alice", "hello"])
        .assert()
        .success()
        .stdout("111.222\n");

    users.assert_async().await;
    open.assert_async().await;
    post.assert_async().await;
}

#[tokio::test]
async fn user_ids_with_or_without_at_bypass_users_list() {
    for target in ["U12345678", "@U12345678"] {
        let mut server = mock_server().await;
        let temp = TempDir::new().unwrap();
        let open = server
            .mock("POST", "/conversations.open")
            .match_body(Matcher::UrlEncoded("users".into(), "U12345678".into()))
            .with_body(r#"{"ok":true,"channel":{"id":"D12345678"}}"#)
            .create_async()
            .await;
        let post = server
            .mock("POST", "/chat.postMessage")
            .match_body(Matcher::UrlEncoded("channel".into(), "D12345678".into()))
            .with_body(post_message_response("D12345678", "111.223"))
            .create_async()
            .await;

        command(&server, &temp)
            .args(["--plain", "messages", "send", target, "hello"])
            .assert()
            .success()
            .stdout("111.223\n");
        open.assert_async().await;
        post.assert_async().await;
    }
}

#[tokio::test]
async fn channel_id_never_opens_a_dm() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let post = server
        .mock("POST", "/chat.postMessage")
        .match_body(Matcher::UrlEncoded("channel".into(), "C12345678".into()))
        .with_body(post_message_response("C12345678", "111.224"))
        .create_async()
        .await;

    command(&server, &temp)
        .args(["--plain", "messages", "send", "C12345678", "hello"])
        .assert()
        .success();
    post.assert_async().await;
}

#[tokio::test]
async fn unknown_user_and_open_failure_prevent_posting() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let users = server
        .mock("POST", "/users.list")
        .with_body(r#"{"ok":true,"members":[],"response_metadata":{"next_cursor":""}}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["messages", "send", "@missing", "hello"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("user_not_found"));
    users.assert_async().await;

    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let open = server
        .mock("POST", "/conversations.open")
        .with_body(r#"{"ok":false,"error":"missing_scope"}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["messages", "send", "@U12345678", "hello"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("missing_scope"));
    open.assert_async().await;
}

#[tokio::test]
async fn empty_at_target_is_usage_without_io() {
    let server = mock_server().await;
    let temp = TempDir::new().unwrap();
    command(&server, &temp)
        .args(["messages", "send", "@", "hello"])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("usage_error"));
}

#[tokio::test]
async fn users_info_by_email_uses_lookup_response_directly() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let lookup = server
        .mock("POST", "/users.lookupByEmail")
        .match_body(Matcher::UrlEncoded(
            "email".into(),
            "alice+cli@example.com".into(),
        ))
        .with_body(r#"{"ok":true,"user":{"id":"U12345678","name":"alice","profile":{"email":"alice+cli@example.com"}}}"#)
        .create_async()
        .await;

    let output = command(&server, &temp)
        .args(["users", "info", " alice+cli@example.com "])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["id"], "U12345678");
    assert_eq!(json["profile"]["email"], "alice+cli@example.com");
    lookup.assert_async().await;
}

#[tokio::test]
async fn resolve_user_email_library_call_and_api_failure_do_not_fallback() {
    let mut server = mock_server().await;
    let lookup = server
        .mock("POST", "/users.lookupByEmail")
        .match_body(Matcher::UrlEncoded(
            "email".into(),
            "alice+ops@example.com".into(),
        ))
        .with_body(r#"{"ok":true,"user":{"id":"U87654321","name":"alice"}}"#)
        .create_async()
        .await;
    let token = TokenSet::new_oauth(
        TOKEN.to_string(),
        "T12345678".to_string(),
        "test".to_string(),
        "U00000000".to_string(),
        vec![],
    )
    .unwrap();
    let client = SlackClient::with_base_url(token, server.url()).unwrap();
    assert_eq!(
        client
            .resolve_user(" alice+ops@example.com ")
            .await
            .unwrap(),
        "U87654321"
    );
    lookup.assert_async().await;

    let mut server = mock_server().await;
    let lookup = server
        .mock("POST", "/users.lookupByEmail")
        .match_body(Matcher::UrlEncoded(
            "email".into(),
            "nobody@example.com".into(),
        ))
        .with_body(r#"{"ok":false,"error":"users_not_found"}"#)
        .create_async()
        .await;
    let token = TokenSet::new_oauth(
        TOKEN.to_string(),
        "T12345678".to_string(),
        "test".to_string(),
        "U00000000".to_string(),
        vec![],
    )
    .unwrap();
    let client = SlackClient::with_base_url(token, server.url()).unwrap();
    let error = client.resolve_user("nobody@example.com").await.unwrap_err();
    assert!(matches!(
        error,
        slack_cli::error::SlackError::Api { ref error, .. } if error == "users_not_found"
    ));
    lookup.assert_async().await;
}

#[tokio::test]
async fn usergroups_list_outputs_json_and_tsv() {
    for plain in [false, true] {
        let mut server = mock_server().await;
        let temp = TempDir::new().unwrap();
        let list = server
            .mock("POST", "/usergroups.list")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("include_disabled".into(), "false".into()),
                Matcher::UrlEncoded("include_count".into(), "true".into()),
                Matcher::UrlEncoded("include_users".into(), "false".into()),
            ]))
            .with_body(r#"{"ok":true,"usergroups":[{"id":"S12345678","handle":"eng","name":"Engineering","user_count":"2"}]}"#)
            .create_async()
            .await;
        let mut cmd = command(&server, &temp);
        if plain {
            cmd.arg("--plain");
        }
        let output = cmd.args(["users", "groups", "list"]).output().unwrap();
        assert!(output.status.success());
        if plain {
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                "S12345678\teng\tEngineering\t2\n"
            );
        } else {
            let json: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(json["usergroups"][0]["handle"], "eng");
        }
        list.assert_async().await;
    }
}

#[tokio::test]
async fn usergroup_members_resolves_handle_and_supports_empty_members() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let groups = server
        .mock("POST", "/usergroups.list")
        .with_body(r#"{"ok":true,"usergroups":[{"id":"S12345678","handle":"eng","name":"Engineering","user_count":0}]}"#)
        .create_async()
        .await;
    let members = server
        .mock("POST", "/usergroups.users.list")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("usergroup".into(), "S12345678".into()),
            Matcher::UrlEncoded("include_disabled".into(), "false".into()),
        ]))
        .with_body(r#"{"ok":true,"users":[]}"#)
        .create_async()
        .await;
    let output = command(&server, &temp)
        .args(["users", "groups", "members", "@eng"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json,
        serde_json::json!({"usergroup":"S12345678","members":[]})
    );
    groups.assert_async().await;
    members.assert_async().await;
}

#[tokio::test]
async fn usergroup_id_bypasses_group_list_and_plain_outputs_ids() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let members = server
        .mock("POST", "/usergroups.users.list")
        .match_body(Matcher::UrlEncoded("usergroup".into(), "S12345678".into()))
        .with_body(r#"{"ok":true,"users":["U11111111","U22222222"]}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["--plain", "users", "groups", "members", "S12345678"])
        .assert()
        .success()
        .stdout("U11111111\nU22222222\n");
    members.assert_async().await;
}

#[tokio::test]
async fn unknown_usergroup_handle_returns_api_error() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let groups = server
        .mock("POST", "/usergroups.list")
        .with_body(r#"{"ok":true,"usergroups":[]}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["users", "groups", "members", "missing"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("usergroup_not_found"));
    groups.assert_async().await;
}

#[tokio::test]
async fn resolved_members_use_one_paginated_users_traversal_and_fallback_names() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    let members = server
        .mock("POST", "/usergroups.users.list")
        .with_body(r#"{"ok":true,"users":["U11111111","U22222222","U33333333","U44444444"]}"#)
        .create_async()
        .await;
    let page_one = server
        .mock("POST", "/users.list")
        .match_body(Matcher::UrlEncoded("limit".into(), "200".into()))
        .with_body(r#"{"ok":true,"members":[{"id":"U11111111","name":"alice"}],"response_metadata":{"next_cursor":"next"}}"#)
        .create_async()
        .await;
    let page_two = server
        .mock("POST", "/users.list")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("limit".into(), "200".into()),
            Matcher::UrlEncoded("cursor".into(), "next".into()),
        ]))
        .with_body(r#"{"ok":true,"members":[{"id":"U22222222","name":"","deleted":true,"profile":{"display_name":"Former User"}},{"id":"U44444444","name":""}],"response_metadata":{"next_cursor":""}}"#)
        .create_async()
        .await;
    let output = command(&server, &temp)
        .args(["users", "groups", "members", "S12345678", "--resolve"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "usergroup":"S12345678",
            "members":[
                {"id":"U11111111","user_name":"alice"},
                {"id":"U22222222","user_name":"Former User"},
                {"id":"U33333333","user_name":"U33333333"},
                {"id":"U44444444","user_name":"U44444444"}
            ]
        })
    );
    members.assert_async().await;
    page_one.assert_async().await;
    page_two.assert_async().await;
}

#[tokio::test]
async fn repeated_users_cursor_fails_instead_of_looping() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    server
        .mock("POST", "/usergroups.users.list")
        .with_body(r#"{"ok":true,"users":["U11111111"]}"#)
        .create_async()
        .await;
    let users = server
        .mock("POST", "/users.list")
        .with_body(r#"{"ok":true,"members":[],"response_metadata":{"next_cursor":"same"}}"#)
        .expect(2)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["users", "groups", "members", "S12345678", "--resolve"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("repeated_cursor"));
    users.assert_async().await;
}

#[tokio::test]
async fn resolved_members_plain_escapes_names() {
    let mut server = mock_server().await;
    let temp = TempDir::new().unwrap();
    server
        .mock("POST", "/usergroups.users.list")
        .with_body(r#"{"ok":true,"users":["U11111111"]}"#)
        .create_async()
        .await;
    server
        .mock("POST", "/users.list")
        .with_body(r#"{"ok":true,"members":[{"id":"U11111111","name":"line\tbreak\r\n"}],"response_metadata":{"next_cursor":""}}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args([
            "--plain",
            "users",
            "groups",
            "members",
            "S12345678",
            "--resolve",
        ])
        .assert()
        .success()
        .stdout("U11111111\tline\\tbreak\\r\\n\n");
}
