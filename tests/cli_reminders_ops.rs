use std::path::{Path, PathBuf};

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use mockito::{Matcher, ServerGuard};
use predicates::prelude::*;
use serde_json::{json, Value};
use tempfile::TempDir;

const TOKEN: &str = "xoxp-test-token-123456789";
const STORED_TOKEN: &str = "xoxp-workspace-token-123456789";

fn isolated_command(server: &ServerGuard, store_path: &Path) -> Command {
    let mut cmd = cargo_bin_cmd!("slack");
    cmd.env("SLACK_API_BASE_URL", server.url())
        .env("SLACK_TOKEN_STORE_PATH", store_path)
        .env_remove("SLACK_TOKEN")
        .env_remove("SLACK_WORKSPACE")
        .env_remove("SLACK_PLAIN");
    cmd
}

fn command(server: &ServerGuard, temp: &TempDir) -> Command {
    let mut cmd = isolated_command(server, &temp.path().join("tokens.json"));
    cmd.env("SLACK_TOKEN", TOKEN);
    cmd
}

fn write_workspace_store(temp: &TempDir) -> PathBuf {
    let path = temp.path().join("tokens.json");
    let data = json!({
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

fn numeric_time_body(text: &str) -> Matcher {
    Matcher::AllOf(vec![
        Matcher::UrlEncoded("text".into(), text.into()),
        Matcher::Regex(r"(?:^|&)time=-?[0-9]+(?:&|$)".into()),
    ])
}

#[tokio::test]
async fn reminders_list_outputs_json_and_plain_rows() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let response = r#"{"ok":true,"reminders":[{"id":"Rm1","creator":"U1","user":"U2","text":"Review PR","time":1893456000},{"id":"Rm2","text":"Finished task","time":1893457000,"complete_ts":1893458000}]}"#;

    let json_list = server
        .mock("POST", "/reminders.list")
        .match_body(Matcher::Exact(String::new()))
        .with_body(response)
        .create_async()
        .await;
    let output = command(&server, &temp)
        .args(["reminders", "list"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reminders: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reminders.as_array().unwrap().len(), 2);
    assert_eq!(reminders[0]["id"], "Rm1");
    assert_eq!(reminders[0]["text"], "Review PR");
    assert_eq!(reminders[1]["complete_ts"], 1_893_458_000_i64);
    json_list.assert_async().await;

    let plain_list = server
        .mock("POST", "/reminders.list")
        .match_body(Matcher::Exact(String::new()))
        .with_body(response)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["--plain", "reminders", "list"])
        .assert()
        .success()
        .stdout("Rm1\t1893456000\tpending\tReview PR\nRm2\t1893457000\tcomplete\tFinished task\n");
    plain_list.assert_async().await;
}

#[tokio::test]
async fn reminders_list_handles_empty_json_and_plain_lists() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();

    let empty_json = server
        .mock("POST", "/reminders.list")
        .match_body(Matcher::Exact(String::new()))
        .with_body(r#"{"ok":true,"reminders":[]}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["reminders", "list"])
        .assert()
        .success()
        .stdout("[]\n");
    empty_json.assert_async().await;

    let empty_plain = server
        .mock("POST", "/reminders.list")
        .match_body(Matcher::Exact(String::new()))
        .with_body(r#"{"ok":true,"reminders":[]}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["reminders", "list", "--plain"])
        .assert()
        .success()
        .stdout("No reminders\n");
    empty_plain.assert_async().await;
}

#[tokio::test]
async fn reminders_plain_list_defaults_missing_optional_fields() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let list = server
        .mock("POST", "/reminders.list")
        .with_body(r#"{"ok":true,"reminders":[{"id":"Rm1"}]}"#)
        .create_async()
        .await;

    command(&server, &temp)
        .args(["--plain", "reminders", "list"])
        .assert()
        .success()
        .stdout("Rm1\t0\tpending\t\n");
    list.assert_async().await;
}

#[tokio::test]
async fn reminders_add_relative_time_posts_text_and_numeric_epoch() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let add = server
        .mock("POST", "/reminders.add")
        .match_body(numeric_time_body("Review PR"))
        .with_body(r#"{"ok":true,"reminder":{"id":"Rm1"}}"#)
        .create_async()
        .await;

    command(&server, &temp)
        .args(["reminders", "add", "Review PR", "--when", "in 2 hours"])
        .assert()
        .success()
        .stdout("")
        .stderr("Reminder created: Rm1\n");
    add.assert_async().await;
}

#[tokio::test]
async fn reminders_add_date_posts_text_and_numeric_epoch() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let add = server
        .mock("POST", "/reminders.add")
        .match_body(numeric_time_body("Christmas"))
        .with_body(r#"{"ok":true,"reminder":{"id":"Rm-date"}}"#)
        .create_async()
        .await;

    command(&server, &temp)
        .args(["reminders", "add", "Christmas", "--when", "2024-12-25"])
        .assert()
        .success()
        .stdout("")
        .stderr("Reminder created: Rm-date\n");
    add.assert_async().await;
}

#[tokio::test]
async fn reminders_complete_and_delete_post_reminder_id() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();

    let complete = server
        .mock("POST", "/reminders.complete")
        .match_body(Matcher::UrlEncoded("reminder".into(), "Rm1".into()))
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["reminders", "complete", "Rm1"])
        .assert()
        .success()
        .stdout("")
        .stderr("Reminder Rm1 marked as complete\n");
    complete.assert_async().await;

    let delete = server
        .mock("POST", "/reminders.delete")
        .match_body(Matcher::UrlEncoded("reminder".into(), "Rm1".into()))
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["reminders", "delete", "Rm1"])
        .assert()
        .success()
        .stdout("")
        .stderr("Reminder Rm1 deleted\n");
    delete.assert_async().await;
}

#[tokio::test]
async fn reminder_not_found_is_reported_as_an_api_error() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let complete = server
        .mock("POST", "/reminders.complete")
        .match_body(Matcher::UrlEncoded("reminder".into(), "Rm1".into()))
        .with_body(r#"{"ok":false,"error":"reminder_not_found"}"#)
        .create_async()
        .await;

    let output = command(&server, &temp)
        .args(["reminders", "complete", "Rm1"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        json!({
            "error": true,
            "code": "api_error",
            "message": "Slack API error: reminder_not_found",
            "detail": null
        })
    );
    complete.assert_async().await;
}

#[tokio::test]
async fn invalid_when_fails_before_sending_an_add_request() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let no_add = server
        .mock("POST", "/reminders.add")
        .expect(0)
        .create_async()
        .await;

    command(&server, &temp)
        .args(["reminders", "add", "Impossible", "--when", "next whenever"])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("usage_error"))
        .stdout(predicate::str::contains("Could not parse time"));
    no_add.assert_async().await;
}

#[tokio::test]
async fn reminders_select_stored_workspace_authentication() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = write_workspace_store(&temp);
    let list = server
        .mock("POST", "/reminders.list")
        .match_header("authorization", format!("Bearer {STORED_TOKEN}").as_str())
        .with_body(r#"{"ok":true,"reminders":[]}"#)
        .create_async()
        .await;

    isolated_command(&server, &store_path)
        .args(["reminders", "list", "--workspace", "workspace-one"])
        .assert()
        .success()
        .stdout("[]\n");
    list.assert_async().await;
}

#[tokio::test]
async fn invalid_tokens_and_unknown_workspaces_fail_before_api_io() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store_path = write_workspace_store(&temp);
    let no_requests = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;

    for token in ["not-a-slack-token", "xoxc-browser-token"] {
        let mut cmd = isolated_command(&server, &store_path);
        cmd.env("SLACK_TOKEN", token)
            .args(["reminders", "list"])
            .assert()
            .code(1)
            .stdout(predicate::str::contains("invalid_token"));
    }
    isolated_command(&server, &store_path)
        .args(["reminders", "list", "--workspace", "missing"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("workspace_not_found"));
    no_requests.assert_async().await;
}

#[tokio::test]
async fn reminders_require_authentication_before_api_io() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let no_requests = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;

    let mut cmd = command(&server, &temp);
    cmd.env_remove("SLACK_TOKEN");
    let output = cmd.args(["reminders", "list"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["code"],
        "auth_required"
    );
    no_requests.assert_async().await;
}
