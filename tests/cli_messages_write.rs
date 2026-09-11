use assert_cmd::cargo::cargo_bin_cmd;
use mockito::{Matcher, ServerGuard};
use serde_json::Value;
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

fn post_response(permalink: Option<&str>) -> String {
    serde_json::json!({
        "ok": true,
        "channel": CHANNEL,
        "ts": TS,
        "message": {
            "type": "message",
            "ts": TS,
            "text": "sent",
            "permalink": permalink,
        }
    })
    .to_string()
}

#[tokio::test]
async fn edit_markdown_permalink_identifier_and_plain_edit() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let edit = server
        .mock("POST", "/chat.update")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("channel".into(), CHANNEL.into()),
            Matcher::UrlEncoded("ts".into(), TS.into()),
            Matcher::UrlEncoded("text".into(), "*updated*".into()),
            Matcher::UrlEncoded("mrkdwn".into(), "true".into()),
        ]))
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;
    let output = command(&server, &temp)
        .args([
            "messages",
            "edit",
            "https://workspace.slack.com/archives/C123456789/p1234567890123456",
            "**updated**",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["channel"], CHANNEL);
    assert_eq!(json["ts"], TS);
    assert_eq!(json["text"], "*updated*");
    edit.assert_async().await;

    let plain_edit = server
        .mock("POST", "/chat.update")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("channel".into(), CHANNEL.into()),
            Matcher::UrlEncoded("ts".into(), TS.into()),
            Matcher::UrlEncoded("text".into(), "literal *text*".into()),
            Matcher::UrlEncoded("mrkdwn".into(), "false".into()),
        ]))
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args([
            "--plain",
            "messages",
            "edit",
            &format!("{CHANNEL}:{TS}"),
            "literal *text*",
            "--format",
            "plain",
        ])
        .assert()
        .success()
        .stdout(format!("{TS}\n"));
    plain_edit.assert_async().await;
}

#[tokio::test]
async fn delete_permalink_and_mark_use_expected_forms() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let delete = server
        .mock("POST", "/chat.delete")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("channel".into(), CHANNEL.into()),
            Matcher::UrlEncoded("ts".into(), TS.into()),
        ]))
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;
    let output = command(&server, &temp)
        .args(["messages", "delete", &format!("{CHANNEL}:{TS}")])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["ts"],
        TS
    );
    delete.assert_async().await;

    let permalink = server
        .mock("POST", "/chat.getPermalink")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("channel".into(), CHANNEL.into()),
            Matcher::UrlEncoded("message_ts".into(), TS.into()),
        ]))
        .with_body(
            r#"{"ok":true,"channel":"C123456789","permalink":"https://workspace.slack.com/archives/C123456789/p1234567890123456"}"#,
        )
        .create_async()
        .await;
    let output = command(&server, &temp)
        .args(["messages", "permalink", &format!("{CHANNEL}:{TS}")])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["permalink"],
        "https://workspace.slack.com/archives/C123456789/p1234567890123456"
    );
    permalink.assert_async().await;

    let permalink_plain = server
        .mock("POST", "/chat.getPermalink")
        .with_body(r#"{"ok":true,"channel":"C123456789","permalink":"https://example.test/plain"}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args([
            "--plain",
            "messages",
            "permalink",
            &format!("{CHANNEL}:{TS}"),
        ])
        .assert()
        .success()
        .stdout("https://example.test/plain\n");
    permalink_plain.assert_async().await;

    let mark = server
        .mock("POST", "/conversations.mark")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("channel".into(), CHANNEL.into()),
            Matcher::UrlEncoded("ts".into(), TS.into()),
        ]))
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;
    let output = command(&server, &temp)
        .args(["messages", "mark", CHANNEL, TS])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["ok"],
        true
    );
    mark.assert_async().await;
}

#[tokio::test]
async fn explicit_permalink_propagates_api_error() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let mock = server
        .mock("POST", "/chat.getPermalink")
        .with_body(r#"{"ok":false,"error":"message_not_found"}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["messages", "permalink", &format!("{CHANNEL}:{TS}")])
        .assert()
        .failure();
    mock.assert_async().await;
}

#[tokio::test]
async fn send_broadcast_blocks_and_plain_fields() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let blocks_path = temp.path().join("blocks.json");
    std::fs::write(
        &blocks_path,
        r#"[{"type":"section","text":{"type":"mrkdwn","text":"**unchanged**"}}]"#,
    )
    .unwrap();
    let blocks = r#"[{"text":{"text":"**unchanged**","type":"mrkdwn"},"type":"section"}]"#;
    let no_permalink = server
        .mock("POST", "/chat.getPermalink")
        .expect(0)
        .create_async()
        .await;
    let post = server
        .mock("POST", "/chat.postMessage")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("channel".into(), CHANNEL.into()),
            Matcher::UrlEncoded("text".into(), "fallback *bold*".into()),
            Matcher::UrlEncoded("thread_ts".into(), TS.into()),
            Matcher::UrlEncoded("reply_broadcast".into(), "true".into()),
            Matcher::UrlEncoded("blocks".into(), blocks.into()),
        ]))
        .with_body(post_response(Some("https://example.test/message")))
        .create_async()
        .await;
    let output = command(&server, &temp)
        .args([
            "messages",
            "send",
            CHANNEL,
            "fallback **bold**",
            "--thread-ts",
            TS,
            "--broadcast",
            "--blocks",
            blocks_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["permalink"], "https://example.test/message");
    post.assert_async().await;
    no_permalink.assert_async().await;
}

#[tokio::test]
async fn blocks_from_stdin_allow_block_only_send_and_plain_output_is_stable() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let post = server
        .mock("POST", "/chat.postMessage")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("channel".into(), CHANNEL.into()),
            Matcher::UrlEncoded("blocks".into(), r#"[{"type":"divider"}]"#.into()),
            Matcher::UrlEncoded("mrkdwn".into(), "false".into()),
        ]))
        .with_body(post_response(None))
        .create_async()
        .await;
    command(&server, &temp)
        .args([
            "--plain", "messages", "send", CHANNEL, "--blocks", "-", "--format", "plain",
        ])
        .write_stdin(r#"[{"type":"divider"}]"#)
        .assert()
        .success()
        .stdout(format!("{TS}\n"));
    post.assert_async().await;
}

#[tokio::test]
async fn invalid_blocks_broadcast_and_stdin_conflict_do_no_requests() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let no_post = server
        .mock("POST", "/chat.postMessage")
        .expect(0)
        .create_async()
        .await;
    for (name, body) in [
        ("malformed.json", "{"),
        ("object.json", "{}"),
        ("empty.json", "[]"),
    ] {
        let bad = temp.path().join(name);
        std::fs::write(&bad, body).unwrap();
        command(&server, &temp)
            .args([
                "messages",
                "send",
                CHANNEL,
                "--blocks",
                bad.to_str().unwrap(),
            ])
            .assert()
            .code(2);
    }
    command(&server, &temp)
        .args(["messages", "send", CHANNEL, "hello", "--broadcast"])
        .assert()
        .code(2);
    command(&server, &temp)
        .args(["messages", "send", CHANNEL, "--stdin", "--blocks", "-"])
        .assert()
        .code(2);
    no_post.assert_async().await;
}

#[tokio::test]
async fn immediate_send_enrichment_failure_is_best_effort() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let post = server
        .mock("POST", "/chat.postMessage")
        .with_body(post_response(None))
        .create_async()
        .await;
    let permalink = server
        .mock("POST", "/chat.getPermalink")
        .with_body(r#"{"ok":false,"error":"missing_scope"}"#)
        .create_async()
        .await;
    let output = command(&server, &temp)
        .args(["messages", "send", CHANNEL, "hello"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("warning"));
    assert!(serde_json::from_slice::<Value>(&output.stdout).unwrap()["permalink"].is_null());
    post.assert_async().await;
    permalink.assert_async().await;
}

#[tokio::test]
async fn get_enriches_json_but_plain_does_not_request_permalink() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let history = server
        .mock("POST", "/conversations.history")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("channel".into(), CHANNEL.into()),
            Matcher::UrlEncoded("oldest".into(), TS.into()),
            Matcher::UrlEncoded("latest".into(), TS.into()),
            Matcher::UrlEncoded("inclusive".into(), "true".into()),
        ]))
        .with_body(format!(
            r#"{{"ok":true,"messages":[{{"ts":"{TS}","text":"hello"}}]}}"#
        ))
        .create_async()
        .await;
    let permalink = server
        .mock("POST", "/chat.getPermalink")
        .with_body(format!(
            r#"{{"ok":true,"channel":"{CHANNEL}","permalink":"https://example.test/get"}}"#
        ))
        .create_async()
        .await;
    let output = command(&server, &temp)
        .args(["messages", "get", &format!("{CHANNEL}:{TS}")])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["permalink"],
        "https://example.test/get"
    );
    history.assert_async().await;
    permalink.assert_async().await;

    let plain_history = server
        .mock("POST", "/conversations.history")
        .with_body(format!(
            r#"{{"ok":true,"messages":[{{"ts":"{TS}","text":"hello"}}]}}"#
        ))
        .create_async()
        .await;
    let no_plain_permalink = server
        .mock("POST", "/chat.getPermalink")
        .expect(0)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["--plain", "messages", "get", &format!("{CHANNEL}:{TS}")])
        .assert()
        .success()
        .stdout(format!("ts\t{TS}\ntext\thello\n"));
    plain_history.assert_async().await;
    no_plain_permalink.assert_async().await;
}

#[tokio::test]
async fn schedule_routes_only_to_schedule_endpoint() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let post_at = chrono::Utc::now().timestamp() + 3600;
    let post_at_string = post_at.to_string();
    let schedule = server
        .mock("POST", "/chat.scheduleMessage")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("channel".into(), CHANNEL.into()),
            Matcher::UrlEncoded("post_at".into(), post_at_string.clone()),
            Matcher::UrlEncoded("text".into(), "scheduled *text*".into()),
            Matcher::UrlEncoded("thread_ts".into(), TS.into()),
        ]))
        .with_body(format!(r#"{{"ok":true,"channel":"{CHANNEL}","scheduled_message_id":"Q123","post_at":{post_at}}}"#))
        .create_async()
        .await;
    let output = command(&server, &temp)
        .args([
            "messages",
            "send",
            CHANNEL,
            "scheduled **text**",
            "--thread-ts",
            TS,
            "--schedule",
            &post_at_string,
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["scheduled_message_id"], "Q123");
    assert!(json["permalink"].is_null());
    schedule.assert_async().await;
}

#[tokio::test]
async fn scheduling_rejects_incompatible_and_invalid_times_without_requests() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let none = server
        .mock("POST", "/chat.scheduleMessage")
        .expect(0)
        .create_async()
        .await;
    for args in [
        vec!["messages", "send", CHANNEL, "x", "--schedule", "1"],
        vec!["messages", "send", CHANNEL, "x", "--schedule", "in 121d"],
        vec![
            "messages",
            "send",
            CHANNEL,
            "x",
            "--schedule",
            "in 1h",
            "--mark-read",
        ],
        vec![
            "messages",
            "send",
            CHANNEL,
            "x",
            "--schedule",
            "in 1h",
            "--broadcast",
            "--thread-ts",
            TS,
        ],
    ] {
        command(&server, &temp).args(args).assert().code(2);
    }
    none.assert_async().await;
}

#[tokio::test]
async fn scheduled_list_paginates_and_escapes_plain_text() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let first = server
        .mock("POST", "/chat.scheduledMessages.list")
        .match_body(Matcher::Exact("limit=100".to_string()))
        .with_body(format!(r#"{{"ok":true,"scheduled_messages":[{{"id":"Q1","channel_id":"{CHANNEL}","post_at":1893456000,"text":"one"}}],"response_metadata":{{"next_cursor":"next"}}}}"#))
        .create_async()
        .await;
    let second = server
        .mock("POST", "/chat.scheduledMessages.list")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("limit".into(), "100".into()),
            Matcher::UrlEncoded("cursor".into(), "next".into()),
        ]))
        .with_body(format!(r#"{{"ok":true,"scheduled_messages":[{{"id":"Q2","channel_id":"{CHANNEL}","post_at":1893457000,"text":"two\tlines\n"}}],"response_metadata":{{"next_cursor":""}}}}"#))
        .create_async()
        .await;
    command(&server, &temp)
        .args(["--plain", "messages", "scheduled", "list"])
        .assert()
        .success()
        .stdout(format!(
            "Q1\t{CHANNEL}\t1893456000\tone\nQ2\t{CHANNEL}\t1893457000\ttwo\\tlines\\n\n"
        ));
    first.assert_async().await;
    second.assert_async().await;
}

#[tokio::test]
async fn scheduled_list_outputs_json_envelope() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let list = server
        .mock("POST", "/chat.scheduledMessages.list")
        .match_body(Matcher::Exact("limit=100".to_string()))
        .with_body(format!(
            r#"{{"ok":true,"scheduled_messages":[{{"id":"Q1","channel_id":"{CHANNEL}","post_at":1893456000,"text":"one"}}]}}"#
        ))
        .create_async()
        .await;
    let output = command(&server, &temp)
        .args(["messages", "scheduled", "list"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["scheduled_messages"][0]["id"], "Q1");
    list.assert_async().await;
}

#[tokio::test]
async fn scheduled_list_detects_repeated_cursor() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let repeated = server
        .mock("POST", "/chat.scheduledMessages.list")
        .with_body(
            r#"{"ok":true,"scheduled_messages":[],"response_metadata":{"next_cursor":"same"}}"#,
        )
        .expect(2)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["messages", "scheduled", "list"])
        .assert()
        .failure();
    repeated.assert_async().await;
}

#[tokio::test]
async fn scheduled_delete_resolves_channel_and_outputs_json() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let delete = server
        .mock("POST", "/chat.deleteScheduledMessage")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("channel".into(), CHANNEL.into()),
            Matcher::UrlEncoded("scheduled_message_id".into(), "Q123".into()),
        ]))
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;
    let output = command(&server, &temp)
        .args(["messages", "scheduled", "delete", CHANNEL, "Q123"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["scheduled_message_id"],
        "Q123"
    );
    delete.assert_async().await;

    let denied = server
        .mock("POST", "/chat.deleteScheduledMessage")
        .with_body(r#"{"ok":false,"error":"not_authed"}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["messages", "scheduled", "delete", CHANNEL, "Q999"])
        .assert()
        .failure();
    denied.assert_async().await;
}
