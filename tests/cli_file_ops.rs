//! End-to-end tests for external uploads and file search.

use std::path::{Path, PathBuf};

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use mockito::{Matcher, ServerGuard};
use predicates::prelude::*;
use tempfile::TempDir;

const USER_TOKEN: &str = "xoxp-file-test-token-1234567890";
const BOT_TOKEN: &str = "xoxb-file-test-token-1234567890";
const BROWSER_TOKEN: &str = "xoxc-file-test-token-1234567890";
const BROWSER_COOKIE: &str = "xoxd-file-test-cookie-1234567890";
const FILE_ID: &str = "F123456789";

async fn server() -> ServerGuard {
    mockito::Server::new_async().await
}

fn slack_cmd(api_url: &str, store_path: &Path) -> Command {
    let mut cmd = cargo_bin_cmd!("slack");
    cmd.env("SLACK_API_BASE_URL", api_url)
        .env("SLACK_TOKEN_STORE_PATH", store_path)
        .env_remove("SLACK_TOKEN")
        .env_remove("SLACK_WORKSPACE")
        .env_remove("SLACK_PLAIN");
    cmd
}

fn user_cmd(api_url: &str, tmp: &TempDir) -> Command {
    let mut cmd = slack_cmd(api_url, &tmp.path().join("no-tokens.json"));
    cmd.env("SLACK_TOKEN", USER_TOKEN);
    cmd
}

fn write_browser_store(tmp: &TempDir) -> PathBuf {
    let path = tmp.path().join("tokens.json");
    let data = serde_json::json!({
        "tokens": {
            "T_BROWSER": {
                "token_type": "browser",
                "access_token": BROWSER_TOKEN,
                "xoxd_cookie": BROWSER_COOKIE,
                "team_id": "T_BROWSER",
                "team_name": "Browser Workspace",
                "team_domain": "browser-workspace",
                "user_id": "U123456789",
                "created_at": "2024-01-01T00:00:00Z",
                "scopes": []
            }
        },
        "default": "T_BROWSER",
        "workspaces": ["T_BROWSER"]
    });
    std::fs::write(&path, data.to_string()).unwrap();
    path
}

fn upload_file(tmp: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = tmp.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn completed_file_json() -> &'static str {
    r#"{"ok":true,"files":[{"id":"F123456789","name":"report.bin","title":"Quarterly report","permalink":"https://workspace.slack.com/files/F123456789"}]}"#
}

#[tokio::test]
async fn upload_runs_all_stages_resolves_channel_and_keeps_raw_request_unauthenticated() {
    let mut api = server().await;
    let mut raw = server().await;
    let tmp = TempDir::new().unwrap();
    let path = upload_file(&tmp, "source.bin", b"\0\x01binary\xff");
    let store = write_browser_store(&tmp);

    let resolve = api
        .mock("POST", "/conversations.list")
        .match_header("authorization", format!("Bearer {}", BROWSER_TOKEN).as_str())
        .match_header("cookie", format!("d={}", BROWSER_COOKIE).as_str())
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"ok":true,"channels":[{"id":"C123456789","name":"general"}],"response_metadata":{"next_cursor":""}}"#)
        .create_async()
        .await;
    let get_url = api
        .mock("POST", "/files.getUploadURLExternal")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("filename".into(), "report.bin".into()),
            Matcher::UrlEncoded("length".into(), "9".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(format!(
            r#"{{"ok":true,"upload_url":"{}/raw-upload?signature=secret","file_id":"{}"}}"#,
            raw.url(),
            FILE_ID
        ))
        .create_async()
        .await;
    let raw_upload = raw
        .mock("POST", "/raw-upload")
        .match_query(Matcher::UrlEncoded("signature".into(), "secret".into()))
        .match_header("content-type", "application/octet-stream")
        .match_header("authorization", Matcher::Missing)
        .match_header("cookie", Matcher::Missing)
        .match_body(Matcher::from(b"\0\x01binary\xff".to_vec()))
        .with_status(200)
        .with_body("signed response must not matter")
        .create_async()
        .await;
    let complete = api
        .mock("POST", "/files.completeUploadExternal")
        .match_header(
            "authorization",
            format!("Bearer {}", BROWSER_TOKEN).as_str(),
        )
        .match_header("cookie", format!("d={}", BROWSER_COOKIE).as_str())
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded(
                "files".into(),
                format!(r#"[{{"id":"{}","title":"Quarterly report"}}]"#, FILE_ID),
            ),
            Matcher::UrlEncoded("channel_id".into(), "C123456789".into()),
            Matcher::UrlEncoded("initial_comment".into(), "please review".into()),
            Matcher::UrlEncoded("thread_ts".into(), "123.456".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(completed_file_json())
        .create_async()
        .await;

    let output = slack_cmd(&api.url(), &store)
        .args([
            "files",
            "upload",
            path.to_str().unwrap(),
            "--channel",
            "general",
            "--title",
            "Quarterly report",
            "--comment",
            "please review",
            "--thread-ts",
            "123.456",
            "--filename",
            "report.bin",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["files"][0]["id"], FILE_ID);

    resolve.assert_async().await;
    get_url.assert_async().await;
    raw_upload.assert_async().await;
    complete.assert_async().await;
}

#[tokio::test]
async fn upload_uses_basename_actual_length_and_plain_ids() {
    let mut api = server().await;
    let mut raw = server().await;
    let tmp = TempDir::new().unwrap();
    let path = upload_file(&tmp, "notes.txt", b"changed bytes");

    let get_url = api
        .mock("POST", "/files.getUploadURLExternal")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("filename".into(), "notes.txt".into()),
            Matcher::UrlEncoded("length".into(), "13".into()),
        ]))
        .with_status(200)
        .with_body(format!(
            r#"{{"ok":true,"upload_url":"{}/upload","file_id":"{}"}}"#,
            raw.url(),
            FILE_ID
        ))
        .create_async()
        .await;
    let raw_upload = raw
        .mock("POST", "/upload")
        .match_body(Matcher::from(b"changed bytes".to_vec()))
        .with_status(204)
        .create_async()
        .await;
    let complete = api
        .mock("POST", "/files.completeUploadExternal")
        .match_body(Matcher::UrlEncoded(
            "files".into(),
            format!(r#"[{{"id":"{}"}}]"#, FILE_ID),
        ))
        .with_status(200)
        .with_body(r#"{"ok":true,"files":[{"id":"F123456789"},{"id":"F987654321"}]}"#)
        .create_async()
        .await;

    user_cmd(&api.url(), &tmp)
        .args(["--plain", "files", "upload", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout("F123456789\nF987654321\n");

    get_url.assert_async().await;
    raw_upload.assert_async().await;
    complete.assert_async().await;
}

#[tokio::test]
async fn upload_sends_zero_length_for_an_empty_regular_file() {
    let mut api = server().await;
    let mut raw = server().await;
    let tmp = TempDir::new().unwrap();
    let path = upload_file(&tmp, "empty.txt", b"");

    let get_url = api
        .mock("POST", "/files.getUploadURLExternal")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("filename".into(), "empty.txt".into()),
            Matcher::UrlEncoded("length".into(), "0".into()),
        ]))
        .with_status(200)
        .with_body(format!(
            r#"{{"ok":true,"upload_url":"{}/empty","file_id":"{}"}}"#,
            raw.url(),
            FILE_ID
        ))
        .create_async()
        .await;
    let raw_upload = raw
        .mock("POST", "/empty")
        .match_body(Matcher::Exact(String::new()))
        .with_status(200)
        .create_async()
        .await;
    let complete = api
        .mock("POST", "/files.completeUploadExternal")
        .with_status(200)
        .with_body(r#"{"ok":true,"files":[{"id":"F123456789"}]}"#)
        .create_async()
        .await;

    user_cmd(&api.url(), &tmp)
        .args(["files", "upload", path.to_str().unwrap()])
        .assert()
        .success();

    get_url.assert_async().await;
    raw_upload.assert_async().await;
    complete.assert_async().await;
}

#[tokio::test]
async fn upload_allows_files_larger_than_download_limit() {
    let mut api = server().await;
    let mut raw = server().await;
    let tmp = TempDir::new().unwrap();
    let bytes = vec![b'x'; 5 * 1024 * 1024 + 1];
    let path = upload_file(&tmp, "large.bin", &bytes);

    let get_url = api
        .mock("POST", "/files.getUploadURLExternal")
        .match_body(Matcher::UrlEncoded(
            "length".into(),
            (5 * 1024 * 1024 + 1).to_string(),
        ))
        .with_status(200)
        .with_body(format!(
            r#"{{"ok":true,"upload_url":"{}/large","file_id":"{}"}}"#,
            raw.url(),
            FILE_ID
        ))
        .create_async()
        .await;
    let raw_upload = raw
        .mock("POST", "/large")
        .match_body(Matcher::from(bytes))
        .with_status(200)
        .create_async()
        .await;
    let complete = api
        .mock("POST", "/files.completeUploadExternal")
        .with_status(200)
        .with_body(r#"{"ok":true,"files":[{"id":"F123456789"}]}"#)
        .create_async()
        .await;

    user_cmd(&api.url(), &tmp)
        .args(["files", "upload", path.to_str().unwrap()])
        .assert()
        .success();

    get_url.assert_async().await;
    raw_upload.assert_async().await;
    complete.assert_async().await;
}

#[test]
fn upload_rejects_missing_directory_and_invalid_filename_without_io() {
    let tmp = TempDir::new().unwrap();

    user_cmd("http://127.0.0.1:1", &tmp)
        .args([
            "files",
            "upload",
            tmp.path().join("missing").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("io_error"));
    user_cmd("http://127.0.0.1:1", &tmp)
        .args(["files", "upload", tmp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("not a regular file"));
    let nonempty = upload_file(&tmp, "ok.txt", b"x");
    user_cmd("http://127.0.0.1:1", &tmp)
        .args([
            "files",
            "upload",
            nonempty.to_str().unwrap(),
            "--filename",
            "../secret",
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("filename"));
    user_cmd("http://127.0.0.1:1", &tmp)
        .args(["files", "upload", "-"])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("stdin"));
}

#[test]
fn upload_comment_and_thread_require_channel() {
    let tmp = TempDir::new().unwrap();
    let path = upload_file(&tmp, "ok.txt", b"x");
    user_cmd("http://127.0.0.1:1", &tmp)
        .args([
            "files",
            "upload",
            path.to_str().unwrap(),
            "--comment",
            "hello",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "required arguments were not provided",
        ));
    user_cmd("http://127.0.0.1:1", &tmp)
        .args([
            "files",
            "upload",
            path.to_str().unwrap(),
            "--thread-ts",
            "123.456",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "required arguments were not provided",
        ));
}

async fn assert_raw_failure_does_not_complete(status: usize, retry_after: Option<&str>) {
    let mut api = server().await;
    let mut raw = server().await;
    let tmp = TempDir::new().unwrap();
    let path = upload_file(&tmp, "failure.bin", b"payload");

    let get_url = api
        .mock("POST", "/files.getUploadURLExternal")
        .with_status(200)
        .with_body(format!(
            r#"{{"ok":true,"upload_url":"{}/failure","file_id":"{}"}}"#,
            raw.url(),
            FILE_ID
        ))
        .create_async()
        .await;
    let mut raw_builder = raw
        .mock("POST", "/failure")
        .match_body(Matcher::from(b"payload".to_vec()))
        .with_status(status);
    if let Some(value) = retry_after {
        raw_builder = raw_builder.with_header("retry-after", value);
    }
    let raw_upload = raw_builder.create_async().await;
    let complete = api
        .mock("POST", "/files.completeUploadExternal")
        .expect(0)
        .create_async()
        .await;

    let assertion = user_cmd(&api.url(), &tmp)
        .args(["files", "upload", path.to_str().unwrap()])
        .assert()
        .failure();
    if status == 429 {
        assertion.stdout(predicate::str::contains("rate_limited"));
    } else {
        assertion
            .stdout(predicate::str::contains("upload_failed"))
            .stdout(predicate::str::contains(status.to_string()))
            .stdout(predicate::str::contains("payload").not());
    }

    get_url.assert_async().await;
    raw_upload.assert_async().await;
    complete.assert_async().await;
}

#[tokio::test]
async fn raw_upload_http_and_rate_limit_failures_do_not_complete() {
    assert_raw_failure_does_not_complete(500, None).await;
    assert_raw_failure_does_not_complete(429, Some("7")).await;
}

#[tokio::test]
async fn raw_upload_does_not_follow_redirects_or_complete() {
    let mut api = server().await;
    let mut raw = server().await;
    let tmp = TempDir::new().unwrap();
    let path = upload_file(&tmp, "redirect.bin", b"payload");
    let get_url = api
        .mock("POST", "/files.getUploadURLExternal")
        .with_status(200)
        .with_body(format!(
            r#"{{"ok":true,"upload_url":"{}/redirect","file_id":"{}"}}"#,
            raw.url(),
            FILE_ID
        ))
        .create_async()
        .await;
    let redirect = raw
        .mock("POST", "/redirect")
        .with_status(302)
        .with_header("location", "/sink")
        .create_async()
        .await;
    let sink = raw.mock("POST", "/sink").expect(0).create_async().await;
    let complete = api
        .mock("POST", "/files.completeUploadExternal")
        .expect(0)
        .create_async()
        .await;

    user_cmd(&api.url(), &tmp)
        .args(["files", "upload", path.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("upload_failed"));

    get_url.assert_async().await;
    redirect.assert_async().await;
    sink.assert_async().await;
    complete.assert_async().await;
}

#[tokio::test]
async fn upload_rejects_insecure_non_loopback_credentials_and_fragments_before_raw_io() {
    for upload_url in [
        "http://example.com/upload",
        "http://user:password@127.0.0.1:9/upload",
        "http://127.0.0.1:9/upload#fragment",
    ] {
        let mut api = server().await;
        let tmp = TempDir::new().unwrap();
        let path = upload_file(&tmp, "secure.bin", b"payload");
        let get_url = api
            .mock("POST", "/files.getUploadURLExternal")
            .with_status(200)
            .with_body(format!(
                r#"{{"ok":true,"upload_url":"{}","file_id":"{}"}}"#,
                upload_url, FILE_ID
            ))
            .create_async()
            .await;
        let complete = api
            .mock("POST", "/files.completeUploadExternal")
            .expect(0)
            .create_async()
            .await;

        user_cmd(&api.url(), &tmp)
            .args(["files", "upload", path.to_str().unwrap()])
            .assert()
            .failure()
            .stdout(predicate::str::contains("usage_error"));

        get_url.assert_async().await;
        complete.assert_async().await;
    }
}

#[tokio::test]
async fn upload_network_failure_does_not_complete() {
    let mut api = server().await;
    let dead_url = "http://127.0.0.1:1";
    let tmp = TempDir::new().unwrap();
    let path = upload_file(&tmp, "network.bin", b"payload");
    let get_url = api
        .mock("POST", "/files.getUploadURLExternal")
        .with_status(200)
        .with_body(format!(
            r#"{{"ok":true,"upload_url":"{}/gone?signature=do-not-print","file_id":"{}"}}"#,
            dead_url, FILE_ID
        ))
        .create_async()
        .await;
    let complete = api
        .mock("POST", "/files.completeUploadExternal")
        .expect(0)
        .create_async()
        .await;

    user_cmd(&api.url(), &tmp)
        .args(["files", "upload", path.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("network_error"))
        .stdout(predicate::str::contains("do-not-print").not());

    get_url.assert_async().await;
    complete.assert_async().await;
}

#[tokio::test]
async fn upload_propagates_completion_error_and_validates_required_response_fields() {
    let mut api = server().await;
    let mut raw = server().await;
    let tmp = TempDir::new().unwrap();
    let path = upload_file(&tmp, "complete.bin", b"payload");
    let get_url = api
        .mock("POST", "/files.getUploadURLExternal")
        .with_status(200)
        .with_body(format!(
            r#"{{"ok":true,"upload_url":"{}/upload","file_id":"{}"}}"#,
            raw.url(),
            FILE_ID
        ))
        .create_async()
        .await;
    let raw_upload = raw
        .mock("POST", "/upload")
        .with_status(200)
        .create_async()
        .await;
    let complete = api
        .mock("POST", "/files.completeUploadExternal")
        .with_status(200)
        .with_body(r#"{"ok":false,"error":"not_in_channel"}"#)
        .create_async()
        .await;

    user_cmd(&api.url(), &tmp)
        .args(["files", "upload", path.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("not_in_channel"))
        .stdout(predicate::str::contains("shared").not());

    get_url.assert_async().await;
    raw_upload.assert_async().await;
    complete.assert_async().await;

    let mut api = server().await;
    let missing = api
        .mock("POST", "/files.getUploadURLExternal")
        .with_status(200)
        .with_body(r#"{"ok":true,"file_id":"F123456789"}"#)
        .create_async()
        .await;
    user_cmd(&api.url(), &tmp)
        .args(["files", "upload", path.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("invalid_response"));
    missing.assert_async().await;
}

#[tokio::test]
async fn search_files_defaults_preserves_metadata_and_optional_fields() {
    let mut api = server().await;
    let tmp = TempDir::new().unwrap();
    let search = api
        .mock("POST", "/search.files")
        .match_header("authorization", format!("Bearer {}", USER_TOKEN).as_str())
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("query".into(), "quarterly report".into()),
            Matcher::UrlEncoded("count".into(), "20".into()),
            Matcher::UrlEncoded("page".into(), "1".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"ok":true,"files":{"total":2,"pagination":{"total_count":2,"page":1,"per_page":20,"page_count":1,"first":1,"last":2},"matches":[{"id":"F123456789","title":"Report","permalink":"https://example.test/file"},{"id":"F987654321"}]}}"#)
        .create_async()
        .await;

    let output = user_cmd(&api.url(), &tmp)
        .args(["files", "search", "quarterly report"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["total"], 2);
    assert_eq!(json["pagination"]["per_page"], 20);
    assert_eq!(json["files"][1]["id"], "F987654321");

    search.assert_async().await;
}

#[tokio::test]
async fn search_files_custom_pagination_empty_results_and_browser_auth() {
    let mut api = server().await;
    let tmp = TempDir::new().unwrap();
    let store = write_browser_store(&tmp);
    let search = api
        .mock("POST", "/search.files")
        .match_header(
            "authorization",
            format!("Bearer {}", BROWSER_TOKEN).as_str(),
        )
        .match_header("cookie", format!("d={}", BROWSER_COOKIE).as_str())
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("query".into(), "nothing".into()),
            Matcher::UrlEncoded("count".into(), "100".into()),
            Matcher::UrlEncoded("page".into(), "4".into()),
        ]))
        .with_status(200)
        .with_body(r#"{"ok":true,"files":{"total":0,"matches":[]}}"#)
        .create_async()
        .await;

    let output = slack_cmd(&api.url(), &store)
        .args([
            "files", "search", "nothing", "--count", "100", "--page", "4",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["total"], 0);
    assert_eq!(json["pagination"], serde_json::Value::Null);
    assert_eq!(json["files"], serde_json::json!([]));
    search.assert_async().await;
}

#[tokio::test]
async fn search_files_plain_escapes_all_tsv_control_characters() {
    let mut api = server().await;
    let tmp = TempDir::new().unwrap();
    let search = api
        .mock("POST", "/search.files")
        .with_status(200)
        .with_body(r#"{"ok":true,"files":{"total":2,"matches":[{"id":"F123456789","title":"title\tline\nnext\rend","name":"ignored","permalink":"https://example.test/a\tb"},{"id":"F987654321","name":"fallback"}]}}"#)
        .create_async()
        .await;

    user_cmd(&api.url(), &tmp)
        .args(["--plain", "files", "search", "report"])
        .assert()
        .success()
        .stdout(concat!(
            "F123456789\ttitle\\tline\\nnext\\rend\thttps://example.test/a\\tb\n",
            "F987654321\tfallback\t\n"
        ));
    search.assert_async().await;
}

#[tokio::test]
async fn search_files_bot_gate_makes_no_request_and_errors_propagate() {
    let mut api = server().await;
    let tmp = TempDir::new().unwrap();
    let untouched = api
        .mock("POST", "/search.files")
        .expect(0)
        .create_async()
        .await;
    let mut bot = user_cmd(&api.url(), &tmp);
    bot.env("SLACK_TOKEN", BOT_TOKEN)
        .args(["files", "search", "report"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("search_not_available"));
    untouched.assert_async().await;

    let mut api = server().await;
    let error = api
        .mock("POST", "/search.files")
        .with_status(200)
        .with_body(r#"{"ok":false,"error":"missing_scope"}"#)
        .create_async()
        .await;
    user_cmd(&api.url(), &tmp)
        .args(["files", "search", "report"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("missing_scope"));
    error.assert_async().await;
}

#[tokio::test]
async fn search_files_rejects_empty_ids() {
    let mut api = server().await;
    let tmp = TempDir::new().unwrap();
    let search = api
        .mock("POST", "/search.files")
        .with_status(200)
        .with_body(r#"{"ok":true,"files":{"total":1,"matches":[{"id":""}]}}"#)
        .create_async()
        .await;
    user_cmd(&api.url(), &tmp)
        .args(["files", "search", "report"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("invalid_response"));
    search.assert_async().await;
}
