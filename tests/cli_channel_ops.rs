use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use mockito::{Matcher, ServerGuard};
use serde_json::{json, Value};
use tempfile::TempDir;

const TOKEN: &str = "xoxp-test-token-123456789";
const CHANNEL: &str = "C123456789";

fn command(server: &ServerGuard, temp: &TempDir) -> Command {
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

fn run_json(server: &ServerGuard, temp: &TempDir, args: &[&str]) -> Value {
    let output = command(server, temp).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

async fn channel_response_mock(
    server: &mut ServerGuard,
    method: &str,
    fields: &[(&str, &str)],
    id: &str,
) -> mockito::Mock {
    server
        .mock("POST", format!("/{method}").as_str())
        .match_body(body(fields))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(json!({"ok": true, "channel": {"id": id}}).to_string())
        .create_async()
        .await
}

#[tokio::test]
async fn lifecycle_endpoints_send_exact_fields_and_return_channels() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();

    let create = channel_response_mock(
        &mut server,
        "conversations.create",
        &[("name", "secret-room"), ("is_private", "true")],
        "G111111111",
    )
    .await;
    let join = channel_response_mock(
        &mut server,
        "conversations.join",
        &[("channel", CHANNEL)],
        CHANNEL,
    )
    .await;
    let leave = server
        .mock("POST", "/conversations.leave")
        .match_body(body(&[("channel", CHANNEL)]))
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;
    let archive = server
        .mock("POST", "/conversations.archive")
        .match_body(body(&[("channel", CHANNEL)]))
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;
    let unarchive = server
        .mock("POST", "/conversations.unarchive")
        .match_body(body(&[("channel", CHANNEL)]))
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;
    let topic = channel_response_mock(
        &mut server,
        "conversations.setTopic",
        &[("channel", CHANNEL), ("topic", "")],
        CHANNEL,
    )
    .await;
    let purpose = channel_response_mock(
        &mut server,
        "conversations.setPurpose",
        &[("channel", CHANNEL), ("purpose", "Team purpose")],
        CHANNEL,
    )
    .await;
    let rename = channel_response_mock(
        &mut server,
        "conversations.rename",
        &[("channel", CHANNEL), ("name", "new-name")],
        CHANNEL,
    )
    .await;

    let created = run_json(
        &server,
        &temp,
        &["channels", "create", "secret-room", "--private"],
    );
    assert_eq!(created["ok"], true);
    assert_eq!(created["channel"]["id"], "G111111111");
    let joined = run_json(&server, &temp, &["channels", "join", CHANNEL]);
    assert_eq!(joined["ok"], true);
    assert_eq!(joined["channel"]["id"], CHANNEL);
    for subcommand in ["leave", "archive", "unarchive"] {
        assert_eq!(
            run_json(&server, &temp, &["channels", subcommand, CHANNEL]),
            json!({"ok": true, "channel": CHANNEL})
        );
    }
    for args in [
        vec!["channels", "set-topic", CHANNEL, ""],
        vec!["channels", "set-purpose", CHANNEL, "Team purpose"],
        vec!["channels", "rename", CHANNEL, "new-name"],
    ] {
        let output = run_json(&server, &temp, &args);
        assert_eq!(output["ok"], true);
        assert_eq!(output["channel"]["id"], CHANNEL);
    }

    for mock in [
        create, join, leave, archive, unarchive, topic, purpose, rename,
    ] {
        mock.assert_async().await;
    }
}

#[tokio::test]
async fn public_create_sends_false_and_plain_prints_returned_id() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let create = channel_response_mock(
        &mut server,
        "conversations.create",
        &[("name", "public-room"), ("is_private", "false")],
        CHANNEL,
    )
    .await;
    command(&server, &temp)
        .args(["--plain", "channels", "create", "public-room"])
        .assert()
        .success()
        .stdout(format!("{CHANNEL}\n"));
    create.assert_async().await;
}

#[tokio::test]
async fn archived_name_is_resolved_before_rename_and_plain_prints_id() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let list = server
        .mock("POST", "/conversations.list")
        .match_body(body(&[
            ("limit", "200"),
            ("exclude_archived", "false"),
            ("types", "public_channel,private_channel,mpim,im"),
        ]))
        .with_body(format!(
            r#"{{"ok":true,"channels":[{{"id":"{CHANNEL}","name":"old-room","is_archived":true}}],"response_metadata":{{"next_cursor":""}}}}"#
        ))
        .create_async()
        .await;
    let rename = channel_response_mock(
        &mut server,
        "conversations.rename",
        &[("channel", CHANNEL), ("name", "restored-room")],
        CHANNEL,
    )
    .await;

    command(&server, &temp)
        .args(["--plain", "channels", "rename", "old-room", "restored-room"])
        .assert()
        .success()
        .stdout(format!("{CHANNEL}\n"));
    list.assert_async().await;
    rename.assert_async().await;
}

#[tokio::test]
async fn invite_resolves_and_deduplicates_every_user() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let users = server
        .mock("POST", "/users.list")
        .match_body(body(&[("limit", "200")]))
        .with_body(
            r#"{"ok":true,"members":[{"id":"U111111111","name":"alice"},{"id":"U222222222","name":"bob"}],"response_metadata":{"next_cursor":""}}"#,
        )
        .expect(3)
        .create_async()
        .await;
    let invite = channel_response_mock(
        &mut server,
        "conversations.invite",
        &[("channel", CHANNEL), ("users", "U111111111,U222222222")],
        CHANNEL,
    )
    .await;

    let output = run_json(
        &server,
        &temp,
        &["channels", "invite", CHANNEL, "alice", "bob", "alice"],
    );
    assert_eq!(output["ok"], true);
    assert_eq!(output["channel"]["id"], CHANNEL);
    users.assert_async().await;
    invite.assert_async().await;
}

#[tokio::test]
async fn invite_resolution_failure_does_not_mutate() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let users = server
        .mock("POST", "/users.list")
        .match_body(body(&[("limit", "200")]))
        .with_body(r#"{"ok":true,"members":[],"response_metadata":{"next_cursor":""}}"#)
        .create_async()
        .await;
    let invite = server
        .mock("POST", "/conversations.invite")
        .expect(0)
        .create_async()
        .await;

    command(&server, &temp)
        .args(["channels", "invite", CHANNEL, "U111111111", "missing-user"])
        .assert()
        .failure()
        .stdout(predicates::str::contains("user_not_found"));
    users.assert_async().await;
    invite.assert_async().await;
}

#[tokio::test]
async fn empty_create_and_rename_names_fail_without_http_requests() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let any = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;
    for args in [
        vec!["channels", "create", ""],
        vec!["channels", "rename", CHANNEL, ""],
    ] {
        command(&server, &temp)
            .args(args)
            .assert()
            .code(2)
            .stdout(predicates::str::contains("usage_error"));
    }
    any.assert_async().await;
}

#[tokio::test]
async fn members_paginates_deduplicates_and_loads_one_user_directory() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let members_one = server
        .mock("POST", "/conversations.members")
        .match_body(body(&[("channel", CHANNEL), ("limit", "200")]))
        .with_body(r#"{"ok":true,"members":["U111111111","U222222222"],"response_metadata":{"next_cursor":"members-next"}}"#)
        .create_async()
        .await;
    let members_two = server
        .mock("POST", "/conversations.members")
        .match_body(body(&[
            ("channel", CHANNEL),
            ("limit", "200"),
            ("cursor", "members-next"),
        ]))
        .with_body(r#"{"ok":true,"members":["U222222222","U333333333"],"response_metadata":{"next_cursor":""}}"#)
        .create_async()
        .await;
    let users_one = server
        .mock("POST", "/users.list")
        .match_body(body(&[("limit", "200")]))
        .with_body(r#"{"ok":true,"members":[{"id":"U111111111","name":"alice","profile":{"display_name":"Alias"}}],"response_metadata":{"next_cursor":"users-next"}}"#)
        .create_async()
        .await;
    let users_two = server
        .mock("POST", "/users.list")
        .match_body(body(&[("limit", "200"), ("cursor", "users-next")]))
        .with_body(r#"{"ok":true,"members":[{"id":"U222222222","profile":{"display_name":"Bob"}}],"response_metadata":{"next_cursor":""}}"#)
        .create_async()
        .await;

    let output = run_json(
        &server,
        &temp,
        &["channels", "members", CHANNEL, "--resolve"],
    );
    assert_eq!(output["channel"], CHANNEL);
    assert_eq!(
        output["members"],
        json!([
            {"id":"U111111111","user_name":"alice"},
            {"id":"U222222222","user_name":"Bob"},
            {"id":"U333333333","user_name":"U333333333"}
        ])
    );
    for mock in [members_one, members_two, users_one, users_two] {
        mock.assert_async().await;
    }
}

#[tokio::test]
async fn members_plain_and_empty_json_outputs_are_machine_readable() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let members = server
        .mock("POST", "/conversations.members")
        .match_body(body(&[("channel", CHANNEL), ("limit", "200")]))
        .with_body(r#"{"ok":true,"members":["U111111111"],"response_metadata":{}}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["--plain", "channels", "members", CHANNEL])
        .assert()
        .success()
        .stdout("U111111111\n");
    members.assert_async().await;

    let empty = server
        .mock("POST", "/conversations.members")
        .match_body(body(&[("channel", CHANNEL), ("limit", "200")]))
        .with_body(r#"{"ok":true,"members":[],"response_metadata":{"next_cursor":""}}"#)
        .create_async()
        .await;
    assert_eq!(
        run_json(&server, &temp, &["channels", "members", CHANNEL]),
        json!({"channel": CHANNEL, "members": []})
    );
    empty.assert_async().await;
}

#[tokio::test]
async fn members_rejects_repeated_cursor() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let first = server
        .mock("POST", "/conversations.members")
        .match_body(body(&[("channel", CHANNEL), ("limit", "200")]))
        .with_body(r#"{"ok":true,"members":[],"response_metadata":{"next_cursor":"again"}}"#)
        .create_async()
        .await;
    let second = server
        .mock("POST", "/conversations.members")
        .match_body(body(&[
            ("channel", CHANNEL),
            ("limit", "200"),
            ("cursor", "again"),
        ]))
        .with_body(r#"{"ok":true,"members":[],"response_metadata":{"next_cursor":"again"}}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["channels", "members", CHANNEL])
        .assert()
        .failure()
        .stdout(predicates::str::contains("pagination_cursor_loop"));
    first.assert_async().await;
    second.assert_async().await;
}

#[tokio::test]
async fn unread_paginates_filters_and_sorts_capability_dependent_counts() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let list_one = server
        .mock("POST", "/conversations.list")
        .match_body(body(&[
            ("limit", "200"),
            ("exclude_archived", "true"),
            ("types", "public_channel,private_channel,mpim,im"),
        ]))
        .with_body(r#"{"ok":true,"channels":[{"id":"C111111111","name":"alpha","is_channel":true,"is_member":true},{"id":"C999999999","name":"outside","is_channel":true,"is_member":false}],"response_metadata":{"next_cursor":"next-page"}}"#)
        .create_async()
        .await;
    let list_two = server
        .mock("POST", "/conversations.list")
        .match_body(body(&[
            ("limit", "200"),
            ("exclude_archived", "true"),
            ("types", "public_channel,private_channel,mpim,im"),
            ("cursor", "next-page"),
        ]))
        .with_body(r#"{"ok":true,"channels":[{"id":"D222222222","is_im":true,"user":"U222222222"},{"id":"G333333333","name":"group","is_mpim":true}],"response_metadata":{"next_cursor":""}}"#)
        .create_async()
        .await;
    let alpha = server
        .mock("POST", "/conversations.info")
        .match_body(body(&[("channel", "C111111111")]))
        .with_body(r#"{"ok":true,"channel":{"unread_count_display":2,"unread_count":9}}"#)
        .create_async()
        .await;
    let dm = server
        .mock("POST", "/conversations.info")
        .match_body(body(&[("channel", "D222222222")]))
        .with_body(r#"{"ok":true,"channel":{"unread_count":4}}"#)
        .create_async()
        .await;
    let mpim = server
        .mock("POST", "/conversations.info")
        .match_body(body(&[("channel", "G333333333")]))
        .with_body(r#"{"ok":true,"channel":{}}"#)
        .create_async()
        .await;

    let output = run_json(&server, &temp, &["channels", "unread"]);
    assert_eq!(output["unavailable_channels"], json!(["G333333333"]));
    assert_eq!(output["channels"][0]["id"], "D222222222");
    assert_eq!(output["channels"][0]["unread_count"], 4);
    assert_eq!(output["channels"][1]["id"], "C111111111");
    assert_eq!(output["channels"][1]["unread_count"], 2);
    assert!(output["channels"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["id"] != "C999999999"));
    for mock in [list_one, list_two, alpha, dm, mpim] {
        mock.assert_async().await;
    }
}

#[tokio::test]
async fn unread_zero_is_known_and_empty_eligible_set_succeeds() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let list = server
        .mock("POST", "/conversations.list")
        .match_body(body(&[("exclude_archived", "true")]))
        .with_body(r#"{"ok":true,"channels":[{"id":"C111111111","is_member":true}],"response_metadata":{}}"#)
        .create_async()
        .await;
    let info = server
        .mock("POST", "/conversations.info")
        .match_body(body(&[("channel", "C111111111")]))
        .with_body(r#"{"ok":true,"channel":{"unread_count_display":0}}"#)
        .create_async()
        .await;
    assert_eq!(
        run_json(&server, &temp, &["channels", "unread"]),
        json!({"channels": [], "unavailable_channels": []})
    );
    list.assert_async().await;
    info.assert_async().await;

    let empty = server
        .mock("POST", "/conversations.list")
        .match_body(body(&[("exclude_archived", "true")]))
        .with_body(r#"{"ok":true,"channels":[{"id":"C999999999","is_member":false}],"response_metadata":{}}"#)
        .create_async()
        .await;
    assert_eq!(
        run_json(&server, &temp, &["channels", "unread"]),
        json!({"channels": [], "unavailable_channels": []})
    );
    empty.assert_async().await;
}

#[tokio::test]
async fn unread_plain_warns_once_for_missing_counts() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let list = server
        .mock("POST", "/conversations.list")
        .with_body(r#"{"ok":true,"channels":[{"id":"D111111111","is_im":true,"user":"user\tname"},{"id":"G222222222","is_mpim":true}],"response_metadata":{}}"#)
        .create_async()
        .await;
    let known = server
        .mock("POST", "/conversations.info")
        .match_body(body(&[("channel", "D111111111")]))
        .with_body(r#"{"ok":true,"channel":{"unread_count":3}}"#)
        .create_async()
        .await;
    let missing = server
        .mock("POST", "/conversations.info")
        .match_body(body(&[("channel", "G222222222")]))
        .with_body(r#"{"ok":true,"channel":{}}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["--plain", "channels", "unread"])
        .assert()
        .success()
        .stdout("D111111111\tuser\\tname\t3\n")
        .stderr("warning: unread counts unavailable for 1 channel(s)\n");
    for mock in [list, known, missing] {
        mock.assert_async().await;
    }
}

#[tokio::test]
async fn unread_all_unavailable_and_api_failures_propagate() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let list = server
        .mock("POST", "/conversations.list")
        .with_body(
            r#"{"ok":true,"channels":[{"id":"D111111111","is_im":true}],"response_metadata":{}}"#,
        )
        .create_async()
        .await;
    let missing = server
        .mock("POST", "/conversations.info")
        .with_body(r#"{"ok":true,"channel":{}}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["channels", "unread"])
        .assert()
        .failure()
        .stdout(predicates::str::contains("unread_unavailable"))
        .stdout(predicates::str::contains("does not expose unread counts"));
    list.assert_async().await;
    missing.assert_async().await;

    let list_error = server
        .mock("POST", "/conversations.list")
        .with_body(r#"{"ok":false,"error":"missing_scope"}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["channels", "unread"])
        .assert()
        .failure()
        .stdout(predicates::str::contains("missing_scope"));
    list_error.assert_async().await;
}
