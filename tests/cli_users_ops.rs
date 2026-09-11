use assert_cmd::cargo::cargo_bin_cmd;
use mockito::{Matcher, ServerGuard};
use predicates::prelude::*;
use serde_json::{json, Value};
use std::path::Path;
use tempfile::TempDir;

const TOKEN: &str = "xoxp-test-token-123456789";
const USER_ID: &str = "U12345678";

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

#[tokio::test]
async fn users_list_defaults_to_active_users_and_preserves_metadata_as_json() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let list = server
        .mock("POST", "/users.list")
        .match_body(Matcher::Exact(String::new()))
        .with_body(
            r#"{"ok":true,"members":[{"id":"U11111111","name":"alice","real_name":"Alice","profile":{"email":"alice@example.com"}},{"id":"U22222222","name":"former","deleted":true}],"response_metadata":{"next_cursor":"next-page","messages":["notice"]}}"#,
        )
        .create_async()
        .await;

    let output = run_json(&server, &temp, &["users", "list"]);
    assert_eq!(output["members"].as_array().unwrap().len(), 1);
    assert_eq!(output["members"][0]["id"], "U11111111");
    assert_eq!(output["response_metadata"]["next_cursor"], "next-page");
    list.assert_async().await;
}

#[tokio::test]
async fn users_list_plain_forwards_limit_and_cursor_and_can_include_deactivated() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let list = server
        .mock("POST", "/users.list")
        .match_body(body(&[("limit", "25"), ("cursor", "page-two")]))
        .with_body(
            r#"{"ok":true,"members":[{"id":"U11111111","name":"alice","real_name":"Alice","profile":{"email":"alice@example.com"}},{"id":"U22222222","deleted":true}],"response_metadata":{"next_cursor":""}}"#,
        )
        .create_async()
        .await;

    command(&server, &temp)
        .args([
            "--plain",
            "users",
            "list",
            "--include-deactivated",
            "--limit",
            "25",
            "--cursor",
            "page-two",
        ])
        .assert()
        .success()
        .stdout("U11111111\talice\tAlice\talice@example.com\nU22222222\t\t\t\n");
    list.assert_async().await;
}

#[tokio::test]
async fn users_info_by_id_returns_the_full_json_user() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let info = server
        .mock("POST", "/users.info")
        .match_body(body(&[("user", USER_ID)]))
        .with_body(format!(
            r#"{{"ok":true,"user":{{"id":"{USER_ID}","name":"alice","real_name":"Alice Example","is_admin":true}}}}"#
        ))
        .create_async()
        .await;

    let output = run_json(&server, &temp, &["users", "info", USER_ID]);
    assert_eq!(output["id"], USER_ID);
    assert_eq!(output["name"], "alice");
    assert_eq!(output["is_admin"], true);
    info.assert_async().await;
}

#[tokio::test]
async fn users_info_by_name_resolves_with_users_list_and_prints_plain_fields() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let list = server
        .mock("POST", "/users.list")
        .match_body(body(&[("limit", "200")]))
        .with_body(format!(
            r#"{{"ok":true,"members":[{{"id":"{USER_ID}","name":"alice"}}],"response_metadata":{{"next_cursor":""}}}}"#
        ))
        .create_async()
        .await;
    let info = server
        .mock("POST", "/users.info")
        .match_body(body(&[("user", USER_ID)]))
        .with_body(format!(
            r#"{{"ok":true,"user":{{"id":"{USER_ID}","name":"alice","real_name":"Alice Example","profile":{{"email":"alice@example.com","title":"Engineer","phone":"555-0100","status_text":"Building","status_emoji":":hammer:"}},"is_admin":true,"is_owner":false,"is_bot":false,"deleted":false,"tz":"Europe/Berlin"}}}}"#
        ))
        .create_async()
        .await;

    command(&server, &temp)
        .args(["--plain", "users", "info", "@Alice"])
        .assert()
        .success()
        .stdout(format!(
            "id\t{USER_ID}\nname\talice\nreal_name\tAlice Example\nemail\talice@example.com\ntitle\tEngineer\nphone\t555-0100\nstatus_text\tBuilding\nstatus_emoji\t:hammer:\nis_admin\ttrue\nis_owner\tfalse\nis_bot\tfalse\ndeleted\tfalse\ntz\tEurope/Berlin\n"
        ));
    list.assert_async().await;
    info.assert_async().await;
}

#[tokio::test]
async fn users_me_combines_auth_test_and_users_info_in_json() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let auth = server
        .mock("POST", "/auth.test")
        .match_body(Matcher::Exact(String::new()))
        .with_body(format!(
            r#"{{"ok":true,"url":"https://example.slack.com/","team":"Example","user":"alice","team_id":"T12345678","user_id":"{USER_ID}"}}"#
        ))
        .create_async()
        .await;
    let info = server
        .mock("POST", "/users.info")
        .match_body(body(&[("user", USER_ID)]))
        .with_body(format!(
            r#"{{"ok":true,"user":{{"id":"{USER_ID}","name":"alice"}}}}"#
        ))
        .create_async()
        .await;

    let output = run_json(&server, &temp, &["users", "me"]);
    assert_eq!(output["user"]["id"], USER_ID);
    assert_eq!(
        output["auth"],
        json!({
            "team_id": "T12345678",
            "team": "Example",
            "url": "https://example.slack.com/"
        })
    );
    auth.assert_async().await;
    info.assert_async().await;
}

#[tokio::test]
async fn users_me_plain_prints_identity_and_email() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let auth = server
        .mock("POST", "/auth.test")
        .with_body(format!(
            r#"{{"ok":true,"url":"https://example.slack.com/","team":"Example","user":"alice","team_id":"T12345678","user_id":"{USER_ID}"}}"#
        ))
        .create_async()
        .await;
    let info = server
        .mock("POST", "/users.info")
        .match_body(body(&[("user", USER_ID)]))
        .with_body(format!(
            r#"{{"ok":true,"user":{{"id":"{USER_ID}","name":"alice","real_name":"Alice Example","profile":{{"email":"alice@example.com"}}}}}}"#
        ))
        .create_async()
        .await;

    command(&server, &temp)
        .args(["--plain", "users", "me"])
        .assert()
        .success()
        .stdout(format!(
            "id\t{USER_ID}\nname\talice\nreal_name\tAlice Example\nteam_id\tT12345678\nteam\tExample\nemail\talice@example.com\n"
        ));
    auth.assert_async().await;
    info.assert_async().await;
}

#[tokio::test]
async fn users_export_paginates_filters_and_writes_escaped_csv() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let first = server
        .mock("POST", "/users.list")
        .match_body(body(&[("limit", "200")]))
        .with_body(
            r#"{"ok":true,"members":[{"id":"U11111111","name":"alice,ops","real_name":"Alice \"A\"","profile":{"email":"alice,ops@example.com"},"is_admin":true},{"id":"U22222222","name":"former","deleted":true}],"response_metadata":{"next_cursor":"more"}}"#,
        )
        .create_async()
        .await;
    let second = server
        .mock("POST", "/users.list")
        .match_body(body(&[("limit", "200"), ("cursor", "more")]))
        .with_body(
            r#"{"ok":true,"members":[{"id":"U33333333"}],"response_metadata":{"next_cursor":""}}"#,
        )
        .create_async()
        .await;
    let output_path = temp.path().join("users.csv");

    command(&server, &temp)
        .args(["users", "export", "--output", output_path.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains(format!(
            "Exported 2 users to {}",
            output_path.display()
        )));
    assert_eq!(
        std::fs::read_to_string(&output_path).unwrap(),
        "id,name,real_name,email,is_admin\nU11111111,\"alice,ops\",\"Alice \"\"A\"\"\",\"alice,ops@example.com\",true\nU33333333,,,,false\n"
    );
    first.assert_async().await;
    second.assert_async().await;
}

#[tokio::test]
async fn users_export_can_include_deactivated_users_on_stdout() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let list = server
        .mock("POST", "/users.list")
        .match_body(body(&[("limit", "200")]))
        .with_body(
            r#"{"ok":true,"members":[{"id":"U22222222","name":"former","deleted":true}],"response_metadata":{}}"#,
        )
        .create_async()
        .await;

    command(&server, &temp)
        .args(["users", "export", "--include-deactivated"])
        .assert()
        .success()
        .stdout("id,name,real_name,email,is_admin\nU22222222,former,,,false\n");
    list.assert_async().await;
}

#[tokio::test]
async fn users_errors_cover_not_found_usage_and_missing_auth_without_extra_io() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let list = server
        .mock("POST", "/users.list")
        .match_body(body(&[("limit", "200")]))
        .with_body(r#"{"ok":true,"members":[],"response_metadata":{}}"#)
        .create_async()
        .await;
    command(&server, &temp)
        .args(["users", "info", "@missing"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("\"code\": \"user_not_found\""));
    list.assert_async().await;

    command(&server, &temp)
        .args(["users", "info"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("required arguments"));

    let no_io = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;
    let mut missing_auth = command(&server, &temp);
    missing_auth.env_remove("SLACK_TOKEN");
    missing_auth
        .env("SLACK_TOKEN_STORE_PATH", temp.path().join("empty.json"))
        .args(["users", "list"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("\"code\": \"auth_required\""));
    no_io.assert_async().await;
}

fn write_workspace_store(path: &Path) {
    let data = json!({
        "tokens": {
            "T_WORK": {
                "token_type": "user_o_auth",
                "access_token": TOKEN,
                "team_id": "T_WORK",
                "team_name": "Work",
                "team_domain": "work",
                "user_id": USER_ID,
                "created_at": "2024-01-01T00:00:00Z",
                "scopes": []
            }
        },
        "default": "T_WORK",
        "workspaces": ["T_WORK"]
    });
    std::fs::write(path, data.to_string()).unwrap();
}

#[tokio::test]
async fn users_support_workspace_tokens_and_reject_invalid_token_overrides() {
    let mut server = mockito::Server::new_async().await;
    let temp = TempDir::new().unwrap();
    let store = temp.path().join("stored.json");
    write_workspace_store(&store);
    let list = server
        .mock("POST", "/users.list")
        .match_body(Matcher::Exact(String::new()))
        .with_body(r#"{"ok":true,"members":[],"response_metadata":{}}"#)
        .create_async()
        .await;
    let mut stored = command(&server, &temp);
    stored
        .env_remove("SLACK_TOKEN")
        .env("SLACK_TOKEN_STORE_PATH", &store)
        .args(["--workspace", "work", "users", "list"])
        .assert()
        .success();
    list.assert_async().await;

    for token in ["invalid-token", "xoxc-browser-token-123456789"] {
        command(&server, &temp)
            .args(["--token", token, "users", "list"])
            .assert()
            .code(1)
            .stdout(predicate::str::contains("\"code\": \"invalid_token\""));
    }

    let mut missing_workspace = command(&server, &temp);
    missing_workspace
        .env_remove("SLACK_TOKEN")
        .env("SLACK_TOKEN_STORE_PATH", &store)
        .args(["--workspace", "unknown", "users", "list"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "\"code\": \"workspace_not_found\"",
        ));
}
