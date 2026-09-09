use assert_cmd::cargo::cargo_bin_cmd;
use chrono::{Duration, Local, NaiveTime};
use mockito::{Matcher, ServerGuard};
use predicates::prelude::*;
use serde_json::{json, Value};
use tempfile::TempDir;

const TOKEN: &str = "xoxp-test-token-123456789";

fn command(server: &ServerGuard, temp: &TempDir) -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("slack");
    cmd.env("SLACK_API_BASE_URL", server.url())
        .env("SLACK_TOKEN_STORE_PATH", temp.path().join("tokens.json"))
        .env("SLACK_TOKEN", TOKEN)
        .env_remove("SLACK_WORKSPACE")
        .env_remove("SLACK_PLAIN");
    cmd
}

fn profile(text: &str, emoji: Option<&str>, expiration: i64) -> String {
    let mut value = json!({
        "status_text": text,
        "status_expiration": expiration,
    });
    if let Some(emoji) = emoji {
        value["status_emoji"] = json!(emoji);
    }
    value.to_string()
}

fn profile_expiration_matcher(text: &str, emoji: Option<&str>, expirations: &[i64]) -> Matcher {
    Matcher::AnyOf(
        expirations
            .iter()
            .map(|expiration| {
                Matcher::UrlEncoded("profile".into(), profile(text, emoji, *expiration))
            })
            .collect(),
    )
}

fn end_of_day(days_from_today: i64) -> i64 {
    let date = Local::now().date_naive() + Duration::days(days_from_today);
    date.and_time(NaiveTime::from_hms_opt(23, 59, 59).unwrap())
        .and_local_timezone(Local)
        .single()
        .unwrap()
        .timestamp()
}

fn write_workspace_store(temp: &TempDir) {
    let data = json!({
        "tokens": {
            "T12345678": {
                "token_type": "user_o_auth",
                "access_token": "xoxp-workspace-token-1234567890",
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
    std::fs::write(temp.path().join("tokens.json"), data.to_string()).unwrap();
}

#[tokio::test]
async fn status_get_outputs_json_profile_and_presence() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let profile = server
        .mock("POST", "/users.profile.get")
        .with_body(
            r#"{"ok":true,"profile":{"status_text":"Heads down","status_emoji":":hammer:","status_expiration":1700000000}}"#,
        )
        .create_async()
        .await;
    let presence = server
        .mock("POST", "/users.getPresence")
        .with_body(r#"{"ok":true,"presence":"away","auto_away":true,"manual_away":false}"#)
        .create_async()
        .await;

    let output = command(&server, &temp)
        .args(["status", "get"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        json!({
            "status_text": "Heads down",
            "status_emoji": ":hammer:",
            "status_expiration": 1700000000_i64,
            "presence": "away",
            "auto_away": true,
            "manual_away": false,
        })
    );
    profile.assert_async().await;
    presence.assert_async().await;
}

#[tokio::test]
async fn status_get_plain_prints_all_nonempty_fields_and_away_flags() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let profile = server
        .mock("POST", "/users.profile.get")
        .with_body(
            r#"{"ok":true,"profile":{"status_text":"In a call","status_emoji":":telephone_receiver:","status_expiration":1700000001}}"#,
        )
        .create_async()
        .await;
    let presence = server
        .mock("POST", "/users.getPresence")
        .with_body(r#"{"ok":true,"presence":"away","auto_away":true,"manual_away":true}"#)
        .create_async()
        .await;

    command(&server, &temp)
        .args(["--plain", "status", "get"])
        .assert()
        .success()
        .stdout(
            "status_text\tIn a call\nstatus_emoji\t:telephone_receiver:\nstatus_expiration\t1700000001\npresence\taway\nauto_away\ttrue\nmanual_away\ttrue\n",
        );
    profile.assert_async().await;
    presence.assert_async().await;
}

#[tokio::test]
async fn status_get_plain_omits_empty_fields_and_uses_named_workspace_auth() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    write_workspace_store(&temp);
    let profile = server
        .mock("POST", "/users.profile.get")
        .with_body(
            r#"{"ok":true,"profile":{"status_text":"","status_emoji":"","status_expiration":0}}"#,
        )
        .create_async()
        .await;
    let presence = server
        .mock("POST", "/users.getPresence")
        .with_body(r#"{"ok":true,"presence":"active"}"#)
        .create_async()
        .await;

    let mut cmd = command(&server, &temp);
    cmd.env_remove("SLACK_TOKEN");
    cmd.args(["--workspace", "workspace-one", "--plain", "status", "get"])
        .assert()
        .success()
        .stdout("presence\tactive\n");
    profile.assert_async().await;
    presence.assert_async().await;
}

#[tokio::test]
async fn status_set_normalizes_emoji_and_supports_documented_expirations() {
    for (expires, expirations) in [
        ("1h", {
            let now = Local::now().timestamp();
            vec![now + 3599, now + 3600, now + 3601]
        }),
        ("today", vec![end_of_day(0)]),
        ("tomorrow", vec![end_of_day(1)]),
    ] {
        let mut server = mockito::Server::new_async().await;
        let temp = TempDir::new().unwrap();
        let set = server
            .mock("POST", "/users.profile.set")
            .match_body(profile_expiration_matcher(
                "Deep work",
                Some(":coffee:"),
                &expirations,
            ))
            .with_body(r#"{"ok":true,"profile":{}}"#)
            .create_async()
            .await;

        command(&server, &temp)
            .args([
                "status",
                "set",
                "Deep work",
                "--emoji",
                ":coffee:",
                "--expires",
                expires,
            ])
            .assert()
            .success()
            .stdout("")
            .stderr("Status updated\n");
        set.assert_async().await;
    }
}

#[tokio::test]
async fn status_set_supports_custom_minute_and_hour_expirations() {
    for (expires, seconds) in [("15m", 15 * 60), ("2h", 2 * 60 * 60)] {
        let now = Local::now().timestamp();
        let expirations = [now + seconds - 1, now + seconds, now + seconds + 1];
        let mut server = mockito::Server::new_async().await;
        let temp = TempDir::new().unwrap();
        let set = server
            .mock("POST", "/users.profile.set")
            .match_body(profile_expiration_matcher("Custom", None, &expirations))
            .with_body(r#"{"ok":true,"profile":{}}"#)
            .create_async()
            .await;

        command(&server, &temp)
            .args(["status", "set", "Custom", "--expires", expires])
            .assert()
            .success();
        set.assert_async().await;
    }
}

#[tokio::test]
async fn status_clear_posts_empty_profile_with_zero_expiration() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let clear = server
        .mock("POST", "/users.profile.set")
        .match_body(Matcher::UrlEncoded(
            "profile".into(),
            json!({
                "status_emoji": "",
                "status_expiration": 0,
                "status_text": "",
            })
            .to_string(),
        ))
        .with_body(r#"{"ok":true,"profile":{}}"#)
        .create_async()
        .await;

    command(&server, &temp)
        .args(["status", "clear"])
        .assert()
        .success()
        .stdout("")
        .stderr("Status cleared\n");
    clear.assert_async().await;
}

#[tokio::test]
async fn status_presence_supports_away_and_auto() {
    for expected in ["away", "auto"] {
        let mut server = mockito::Server::new_async().await;
        let temp = TempDir::new().unwrap();
        let presence = server
            .mock("POST", "/users.setPresence")
            .match_body(Matcher::UrlEncoded("presence".into(), expected.into()))
            .with_body(r#"{"ok":true}"#)
            .create_async()
            .await;

        command(&server, &temp)
            .args(["status", "presence", expected])
            .assert()
            .success()
            .stdout("")
            .stderr(format!("Presence set to {expected}\n"));
        presence.assert_async().await;
    }
}

#[tokio::test]
async fn bad_status_expirations_are_usage_errors_without_api_io() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let no_set = server
        .mock("POST", "/users.profile.set")
        .expect(0)
        .create_async()
        .await;

    for expires in ["someday", "xm", "xh"] {
        command(&server, &temp)
            .args(["status", "set", "Busy", "--expires", expires])
            .assert()
            .code(2)
            .stdout(predicate::str::contains("usage_error"));
    }
    no_set.assert_async().await;
}

#[tokio::test]
async fn status_rejects_invalid_and_unpaired_browser_token_overrides() {
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
        cmd.args(["--token", token, "status", "get"])
            .assert()
            .code(1)
            .stdout(predicate::str::contains("invalid_token"));
    }
    no_request.assert_async().await;
}

#[tokio::test]
async fn status_missing_token_is_auth_required_without_api_io() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let no_request = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;

    let mut cmd = command(&server, &temp);
    cmd.env_remove("SLACK_TOKEN");
    cmd.args(["status", "get"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("auth_required"));
    no_request.assert_async().await;
}
