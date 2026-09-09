use assert_cmd::cargo::cargo_bin_cmd;
use mockito::{Matcher, ServerGuard};
use predicates::prelude::*;
use serde_json::{json, Value};
use tempfile::TempDir;

const TOKEN: &str = "xoxp-test-token-123456789";
const CHANNEL: &str = "C123456789";
const TS: &str = "1234567890.123456";

fn command(server: &ServerGuard, temp: &TempDir) -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("slack");
    cmd.env("SLACK_API_BASE_URL", server.url())
        .env("SLACK_TOKEN_STORE_PATH", temp.path().join("tokens.json"))
        .env("SLACK_TOKEN", TOKEN)
        .env_remove("SLACK_WORKSPACE")
        .env_remove("SLACK_PLAIN");
    cmd
}

fn body(fields: &[(&str, &str)]) -> Matcher {
    Matcher::AllOf(
        fields
            .iter()
            .map(|(key, value)| Matcher::UrlEncoded((*key).into(), (*value).into()))
            .collect(),
    )
}

async fn channel_resolution(server: &mut ServerGuard) -> mockito::Mock {
    server
        .mock("POST", "/conversations.list")
        .match_body(body(&[
            ("limit", "200"),
            ("exclude_archived", "false"),
            ("types", "public_channel,private_channel,mpim,im"),
        ]))
        .with_body(format!(
            r#"{{"ok":true,"channels":[{{"id":"{CHANNEL}","name":"general"}}],"response_metadata":{{"next_cursor":""}}}}"#
        ))
        .create_async()
        .await
}

#[tokio::test]
async fn reactions_add_resolves_channel_and_posts_normalized_emoji() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let resolution = channel_resolution(&mut server).await;
    let add = server
        .mock("POST", "/reactions.add")
        .match_body(body(&[
            ("channel", CHANNEL),
            ("timestamp", TS),
            ("name", "thumbsup"),
        ]))
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;

    command(&server, &temp)
        .args(["reactions", "add", "#general", TS, ":thumbsup:"])
        .assert()
        .success()
        .stdout("")
        .stderr("Added :thumbsup:\n");
    resolution.assert_async().await;
    add.assert_async().await;
}

#[tokio::test]
async fn reactions_remove_resolves_channel_and_posts_normalized_emoji() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let resolution = channel_resolution(&mut server).await;
    let remove = server
        .mock("POST", "/reactions.remove")
        .match_body(body(&[
            ("channel", CHANNEL),
            ("timestamp", TS),
            ("name", "eyes"),
        ]))
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;

    command(&server, &temp)
        .args(["reactions", "remove", "#general", TS, "eyes:"])
        .assert()
        .success()
        .stdout("")
        .stderr("Removed :eyes:\n");
    resolution.assert_async().await;
    remove.assert_async().await;
}

#[tokio::test]
async fn reactions_list_outputs_json_and_sends_full_form() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let get = server
        .mock("POST", "/reactions.get")
        .match_body(body(&[
            ("channel", CHANNEL),
            ("timestamp", TS),
            ("full", "true"),
        ]))
        .with_body(format!(
            r#"{{"ok":true,"message":{{"ts":"{TS}","reactions":[{{"name":"thumbsup","count":2,"users":["U111111111","U222222222"]}}]}}}}"#
        ))
        .create_async()
        .await;

    let output = command(&server, &temp)
        .args(["reactions", "list", CHANNEL, TS])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        json!({
            "reactions": [{
                "name": "thumbsup",
                "count": 2,
                "users": ["U111111111", "U222222222"],
            }]
        })
    );
    get.assert_async().await;
}

#[tokio::test]
async fn reactions_list_plain_formats_each_reaction() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let get = server
        .mock("POST", "/reactions.get")
        .match_body(body(&[
            ("channel", CHANNEL),
            ("timestamp", TS),
            ("full", "true"),
        ]))
        .with_body(format!(
            r#"{{"ok":true,"message":{{"ts":"{TS}","reactions":[{{"name":"eyes","count":2,"users":["U111111111","U222222222"]}},{{"name":"heart","count":1,"users":["U333333333"]}}]}}}}"#
        ))
        .create_async()
        .await;

    command(&server, &temp)
        .args(["--plain", "reactions", "list", CHANNEL, TS])
        .assert()
        .success()
        .stdout(":eyes: (2)\tU111111111,U222222222\n:heart: (1)\tU333333333\n");
    get.assert_async().await;
}

#[tokio::test]
async fn reactions_list_plain_reports_missing_reactions_field() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let get = server
        .mock("POST", "/reactions.get")
        .with_body(format!(r#"{{"ok":true,"message":{{"ts":"{TS}"}}}}"#))
        .create_async()
        .await;

    command(&server, &temp)
        .args(["--plain", "reactions", "list", CHANNEL, TS])
        .assert()
        .success()
        .stdout("No reactions\n");
    get.assert_async().await;
}

#[tokio::test]
async fn reaction_conflict_errors_exit_as_api_errors() {
    for (subcommand, endpoint, error) in [
        ("add", "/reactions.add", "already_reacted"),
        ("remove", "/reactions.remove", "no_reaction"),
    ] {
        let mut server = mockito::Server::new_async().await;
        let temp = TempDir::new().unwrap();
        let reaction = server
            .mock("POST", endpoint)
            .match_body(body(&[
                ("channel", CHANNEL),
                ("timestamp", TS),
                ("name", "thumbsup"),
            ]))
            .with_body(format!(r#"{{"ok":false,"error":"{error}"}}"#))
            .create_async()
            .await;

        command(&server, &temp)
            .args(["reactions", subcommand, CHANNEL, TS, "thumbsup"])
            .assert()
            .code(1)
            .stdout(predicate::str::contains(error));
        reaction.assert_async().await;
    }
}

#[tokio::test]
async fn reactions_reject_invalid_and_unpaired_browser_token_overrides() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let no_request = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;

    for token in ["not-a-slack-token", "xoxc-browser-token"] {
        let mut cmd = command(&server, &temp);
        cmd.env_remove("SLACK_TOKEN");
        cmd.args(["--token", token, "reactions", "list", CHANNEL, TS])
            .assert()
            .code(1)
            .stdout(predicate::str::contains("invalid_token"));
    }
    no_request.assert_async().await;
}

#[tokio::test]
async fn reactions_missing_token_is_auth_required_without_api_io() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let no_request = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;

    let mut cmd = command(&server, &temp);
    cmd.env_remove("SLACK_TOKEN");
    cmd.args(["reactions", "list", CHANNEL, TS])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("auth_required"));
    no_request.assert_async().await;
}
