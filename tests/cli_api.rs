//! End-to-end tests for the `slack api` escape hatch command.
//!
//! Runs the real binary via assert_cmd against a mockito mock Slack server
//! (via `SLACK_API_BASE_URL`) and a file-based token store (via
//! `SLACK_TOKEN_STORE_PATH`). Unlike the tests in tests/integration/, these
//! run under an ordinary `cargo test` with no environment gating.

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use mockito::{Matcher, ServerGuard};
use predicates::prelude::*;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const ENV_TOKEN: &str = "xoxp-env-token-1234567890";
const WS1_TOKEN: &str = "xoxp-ws1-token-1234567890";
const WS2_TOKEN: &str = "xoxp-ws2-token-1234567890";
const XOXC_TOKEN: &str = "xoxc-browser-token-1234567890";
const XOXD_COOKIE: &str = "xoxd-test-cookie-abc123";

async fn mock_server() -> ServerGuard {
    mockito::Server::new_async().await
}

/// Build a `slack` command isolated from the developer's real environment,
/// pointed at the given mock server and token store path.
fn slack_cmd(server_url: &str, store_path: &Path) -> Command {
    let mut cmd = cargo_bin_cmd!("slack");
    cmd.env("SLACK_API_BASE_URL", server_url);
    cmd.env("SLACK_TOKEN_STORE_PATH", store_path);
    cmd.env_remove("SLACK_TOKEN");
    cmd.env_remove("SLACK_WORKSPACE");
    cmd.env_remove("SLACK_PLAIN");
    cmd
}

/// A token store path that does not exist -> commands see "no auth".
fn empty_store(tmp: &TempDir) -> PathBuf {
    tmp.path().join("no-tokens.json")
}

fn oauth_token_json(
    team_id: &str,
    team_name: &str,
    domain: &str,
    token: &str,
) -> serde_json::Value {
    serde_json::json!({
        "token_type": "user_o_auth",
        "access_token": token,
        "team_id": team_id,
        "team_name": team_name,
        "team_domain": domain,
        "user_id": "U12345TEST",
        "created_at": "2024-01-01T00:00:00Z",
        "scopes": []
    })
}

/// Write a token store file with two OAuth workspaces (T_WS1 default) plus a
/// browser-token workspace (T_WS3, with xoxd cookie).
fn write_multi_store(tmp: &TempDir) -> PathBuf {
    let path = tmp.path().join("tokens.json");
    let data = serde_json::json!({
        "tokens": {
            "T_WS1": oauth_token_json("T_WS1", "Workspace One", "wsone", WS1_TOKEN),
            "T_WS2": oauth_token_json("T_WS2", "Workspace Two", "wstwo", WS2_TOKEN),
            "T_WS3": {
                "token_type": "browser",
                "access_token": XOXC_TOKEN,
                "xoxd_cookie": XOXD_COOKIE,
                "team_id": "T_WS3",
                "team_name": "Workspace Three",
                "team_domain": "wsthree",
                "user_id": "U12345TEST",
                "created_at": "2024-01-01T00:00:00Z",
                "scopes": []
            }
        },
        "default": "T_WS1",
        "workspaces": ["T_WS1", "T_WS2", "T_WS3"]
    });
    std::fs::write(&path, data.to_string()).expect("write token store");
    path
}

// ============================================================================
// Help / CLI surface
// ============================================================================

#[test]
fn test_api_help() {
    let tmp = TempDir::new().unwrap();
    slack_cmd("http://127.0.0.1:1", &empty_store(&tmp))
        .args(["api", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--raw-field"))
        .stdout(predicate::str::contains("--field"))
        .stdout(predicate::str::contains("--input"))
        .stdout(predicate::str::contains("--method"));
}

// ============================================================================
// Successful requests: method name, full URL, default POST
// ============================================================================

#[tokio::test]
async fn test_api_post_method_name_success_full_response() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    // Default method is POST; -f values are raw strings, form-encoded.
    let mock = server
        .mock("POST", "/chat.postMessage")
        .match_header("authorization", format!("Bearer {}", ENV_TOKEN).as_str())
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("channel".into(), "C123456".into()),
            Matcher::UrlEncoded("text".into(), "hello world".into()),
        ]))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"ok":true,"channel":"C123456","ts":"111.222","extra":{"deep":[1,2,3]}}"#)
        .create_async()
        .await;

    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args([
            "api",
            "chat.postMessage",
            "-f",
            "channel=C123456",
            "-f",
            "text=hello world",
        ])
        .assert()
        .success()
        // Full raw JSON response, including `ok` and unknown fields.
        .stdout(predicate::str::contains("\"ok\": true"))
        .stdout(predicate::str::contains("111.222"))
        .stdout(predicate::str::contains("\"deep\""));

    mock.assert_async().await;
}

#[tokio::test]
async fn test_api_full_url_normalized_to_method_name() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    // A canonical https://slack.com/api/<method> URL is normalized to the
    // bare method and issued against the configured (mock) base URL.
    let mock = server
        .mock("POST", "/auth.test")
        .with_status(200)
        .with_body(r#"{"ok":true,"team_id":"T_WS1"}"#)
        .create_async()
        .await;

    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args(["api", "https://slack.com/api/auth.test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ok\": true"));

    mock.assert_async().await;
}

// ============================================================================
// Field typing and form encoding
// ============================================================================

#[tokio::test]
async fn test_api_typed_fields_form_encoding() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    // -F parses JSON booleans/numbers/arrays/objects; nested values are sent
    // as JSON strings (matching to_form_params); -f is always a raw string.
    let mock = server
        .mock("POST", "/chat.postMessage")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("channel".into(), "C123456".into()),
            Matcher::UrlEncoded("count".into(), "42".into()),
            Matcher::UrlEncoded("flag".into(), "true".into()),
            Matcher::UrlEncoded("blocks".into(), r#"[{"type":"divider"}]"#.into()),
            Matcher::UrlEncoded("meta".into(), r#"{"a":1}"#.into()),
            Matcher::UrlEncoded("note".into(), "plain text".into()),
        ]))
        .with_status(200)
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;

    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args([
            "api",
            "chat.postMessage",
            "-f",
            "channel=C123456",
            "-F",
            "count=42",
            "-F",
            "flag=true",
            "-F",
            r#"blocks=[{"type":"divider"}]"#,
            "-F",
            r#"meta={"a":1}"#,
            "-F",
            "note=plain text",
        ])
        .assert()
        .success();

    mock.assert_async().await;
}

#[tokio::test]
async fn test_api_raw_field_never_parses_json() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    // -f "count=10" must be sent as the exact string; body is form-encoded
    // with keys in sorted order (serde_json object ordering).
    let mock = server
        .mock("POST", "/some.method")
        .match_body(Matcher::Exact("count=10&flag=true".to_string()))
        .with_status(200)
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;

    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args(["api", "some.method", "-f", "count=10", "-f", "flag=true"])
        .assert()
        .success();

    mock.assert_async().await;
}

#[tokio::test]
async fn test_api_typed_null_field_rejected_before_request() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    let catch_all = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;

    // -F key=null has explicit, predictable handling: rejected as a usage
    // error (form encoding omits nulls) before any request is made.
    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args(["api", "chat.postMessage", "-F", "text=null"])
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::contains("usage_error"))
        .stdout(predicate::str::contains("null"));

    catch_all.assert_async().await;
}

// ============================================================================
// GET requests: query parameters
// ============================================================================

#[tokio::test]
async fn test_api_get_sends_query_params() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    let mock = server
        .mock("GET", "/conversations.list")
        .match_query(Matcher::AllOf(vec![
            Matcher::UrlEncoded("limit".into(), "2".into()),
            Matcher::UrlEncoded("cursor".into(), "abc123".into()),
        ]))
        .match_header("authorization", format!("Bearer {}", ENV_TOKEN).as_str())
        .with_status(200)
        .with_body(r#"{"ok":true,"channels":[]}"#)
        .create_async()
        .await;

    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args([
            "api",
            "conversations.list",
            "-X",
            "GET",
            "-F",
            "limit=2",
            "-f",
            "cursor=abc123",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"channels\""));

    mock.assert_async().await;
}

// ============================================================================
// --input: file, stdin, validation
// ============================================================================

#[tokio::test]
async fn test_api_input_file() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    let input_path = tmp.path().join("params.json");
    // A null-valued field in the input object is omitted from the body
    // (form encoding skips nulls) -- exact-match body asserts its absence.
    std::fs::write(
        &input_path,
        r#"{"channel":"C9CHANNEL","note":null,"text":"from file"}"#,
    )
    .unwrap();

    let mock = server
        .mock("POST", "/chat.postMessage")
        .match_body(Matcher::Exact(
            "channel=C9CHANNEL&text=from+file".to_string(),
        ))
        .with_status(200)
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;

    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args([
            "api",
            "chat.postMessage",
            "--input",
            input_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    mock.assert_async().await;
}

#[tokio::test]
async fn test_api_input_stdin() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    let mock = server
        .mock("POST", "/chat.postMessage")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("channel".into(), "C9CHANNEL".into()),
            Matcher::UrlEncoded("text".into(), "from stdin".into()),
            // Nested JSON in the input object is sent as a JSON string.
            Matcher::UrlEncoded("blocks".into(), r#"[{"type":"divider"}]"#.into()),
        ]))
        .with_status(200)
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;

    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args(["api", "chat.postMessage", "--input", "-"])
        .write_stdin(r#"{"channel":"C9CHANNEL","text":"from stdin","blocks":[{"type":"divider"}]}"#)
        .assert()
        .success();

    mock.assert_async().await;
}

#[tokio::test]
async fn test_api_input_rejects_non_object_before_request() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    let catch_all = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;

    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args(["api", "chat.postMessage", "--input", "-"])
        .write_stdin("[1, 2, 3]")
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::contains("usage_error"));

    catch_all.assert_async().await;
}

#[tokio::test]
async fn test_api_rejects_mixing_input_and_fields() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    let catch_all = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;

    // --input with -f
    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args([
            "api",
            "chat.postMessage",
            "--input",
            "params.json",
            "-f",
            "text=hi",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));

    // --input with -F
    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args(["api", "chat.postMessage", "--input", "-", "-F", "limit=10"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));

    catch_all.assert_async().await;
}

// ============================================================================
// Invalid arguments rejected before any request
// ============================================================================

#[tokio::test]
async fn test_api_rejects_plain() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    let catch_all = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;

    // In --plain mode errors go to stderr; usage errors exit 2.
    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args(["api", "auth.test", "--plain"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--plain"));

    catch_all.assert_async().await;
}

#[tokio::test]
async fn test_api_rejects_unsupported_http_methods() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    let catch_all = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;

    for method in ["DELETE", "PUT", "PATCH", "HEAD"] {
        slack_cmd(&server.url(), &empty_store(&tmp))
            .env("SLACK_TOKEN", ENV_TOKEN)
            .args(["api", "users.list", "-X", method])
            .assert()
            .failure()
            .stderr(predicate::str::contains("only GET and POST"));
    }

    catch_all.assert_async().await;
}

#[tokio::test]
async fn test_api_rejects_invalid_field_syntax() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    let catch_all = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;

    // Missing '=' separator
    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args(["api", "auth.test", "-f", "noequals"])
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::contains("usage_error"));

    // Duplicate keys
    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args(["api", "auth.test", "-f", "a=1", "-F", "a=2"])
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::contains("usage_error"));

    catch_all.assert_async().await;
}

#[tokio::test]
async fn test_api_rejects_unsafe_endpoints_before_request() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    let catch_all_post = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;
    let catch_all_get = server
        .mock("GET", Matcher::Any)
        .expect(0)
        .create_async()
        .await;

    let unsafe_endpoints = [
        // Wrong host
        "https://evil.com/api/auth.test",
        // Subdomain is not exactly slack.com
        "https://api.slack.com/api/auth.test",
        // Not https
        "http://slack.com/api/auth.test",
        // Credentials in URL
        "https://user:pass@slack.com/api/auth.test",
        // Query string
        "https://slack.com/api/auth.test?x=1",
        // Fragment
        "https://slack.com/api/auth.test#frag",
        // Custom port
        "https://slack.com:8443/api/auth.test",
        // Path not under /api/
        "https://slack.com/auth.test",
        // More than one method segment
        "https://slack.com/api/auth.test/extra",
        // Path traversal
        "https://slack.com/api/../auth.test",
        // Bare name with a path separator
        "auth/test",
        "../auth.test",
    ];

    for endpoint in unsafe_endpoints {
        slack_cmd(&server.url(), &empty_store(&tmp))
            .env("SLACK_TOKEN", ENV_TOKEN)
            .args(["api", endpoint])
            .assert()
            .failure()
            .code(2)
            .stdout(predicate::str::contains("usage_error"));
    }

    catch_all_post.assert_async().await;
    catch_all_get.assert_async().await;
}

// ============================================================================
// Error conventions: ok:false, HTTP failure, rate limits
// ============================================================================

#[tokio::test]
async fn test_api_slack_ok_false_is_api_error() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    let mock = server
        .mock("POST", "/chat.postMessage")
        .with_status(200)
        .with_body(r#"{"ok":false,"error":"channel_not_found"}"#)
        .create_async()
        .await;

    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args(["api", "chat.postMessage", "-f", "channel=C123456"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("api_error"))
        .stdout(predicate::str::contains("channel_not_found"));

    mock.assert_async().await;
}

#[tokio::test]
async fn test_api_http_failure_is_api_error() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    let mock = server
        .mock("POST", "/auth.test")
        .with_status(500)
        .with_body("gateway on fire")
        .create_async()
        .await;

    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args(["api", "auth.test"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("api_error"))
        .stdout(predicate::str::contains("HTTP 500"));

    mock.assert_async().await;
}

#[tokio::test]
async fn test_api_rate_limited() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    // Retry-After: 0 keeps retries instant; after bounded retries the CLI
    // reports the standard rate_limited error.
    let mock = server
        .mock("POST", "/auth.test")
        .with_status(429)
        .with_header("retry-after", "0")
        .with_body(r#"{"ok":false,"error":"ratelimited"}"#)
        .expect_at_least(1)
        .create_async()
        .await;

    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args(["api", "auth.test"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("rate_limited"));

    mock.assert_async().await;
}

#[tokio::test]
async fn test_api_excessive_retry_delay_returns_without_retrying() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();
    let mock = server
        .mock("POST", "/auth.test")
        .with_status(429)
        .with_header("retry-after", "18446744073709551615")
        .expect(1)
        .create_async()
        .await;

    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args(["api", "auth.test"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("rate_limited"));

    mock.assert_async().await;
}

#[tokio::test]
async fn test_api_does_not_follow_redirects() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    let redirect = server
        .mock("POST", "/auth.test")
        .with_status(302)
        .with_header("location", &format!("{}/leak", server.url()))
        .create_async()
        .await;
    let leak_get = server.mock("GET", "/leak").expect(0).create_async().await;
    let leak_post = server.mock("POST", "/leak").expect(0).create_async().await;

    slack_cmd(&server.url(), &empty_store(&tmp))
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args(["api", "auth.test"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("api_error"));

    redirect.assert_async().await;
    leak_get.assert_async().await;
    leak_post.assert_async().await;
}

// ============================================================================
// Auth: required, SLACK_TOKEN override, workspace selection, browser cookies
// ============================================================================

#[tokio::test]
async fn test_api_auth_required() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();

    let catch_all = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;

    slack_cmd(&server.url(), &empty_store(&tmp))
        .args(["api", "auth.test"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("auth_required"));

    catch_all.assert_async().await;
}

#[tokio::test]
async fn test_api_token_env_overrides_store() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();
    let store = write_multi_store(&tmp);

    // SLACK_TOKEN takes precedence over the stored default workspace token.
    let mock = server
        .mock("POST", "/auth.test")
        .match_header("authorization", format!("Bearer {}", ENV_TOKEN).as_str())
        .with_status(200)
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;

    slack_cmd(&server.url(), &store)
        .env("SLACK_TOKEN", ENV_TOKEN)
        .args(["api", "auth.test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"ok\": true"));

    mock.assert_async().await;
}

#[tokio::test]
async fn test_api_uses_default_workspace_from_file_store() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();
    let store = write_multi_store(&tmp);

    let mock = server
        .mock("POST", "/auth.test")
        .match_header("authorization", format!("Bearer {}", WS1_TOKEN).as_str())
        .with_status(200)
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;

    slack_cmd(&server.url(), &store)
        .args(["api", "auth.test"])
        .assert()
        .success();

    mock.assert_async().await;
}

#[tokio::test]
async fn test_api_workspace_flag_selects_stored_token() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();
    let store = write_multi_store(&tmp);

    let mock = server
        .mock("POST", "/auth.test")
        .match_header("authorization", format!("Bearer {}", WS2_TOKEN).as_str())
        .with_status(200)
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;

    slack_cmd(&server.url(), &store)
        .args(["-w", "T_WS2", "api", "auth.test"])
        .assert()
        .success();

    mock.assert_async().await;
}

#[tokio::test]
async fn test_api_workspace_not_found() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();
    let store = write_multi_store(&tmp);

    let catch_all = server
        .mock("POST", Matcher::Any)
        .expect(0)
        .create_async()
        .await;

    slack_cmd(&server.url(), &store)
        .args(["-w", "nosuchworkspace", "api", "auth.test"])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::contains("workspace_not_found"));

    catch_all.assert_async().await;
}

#[tokio::test]
async fn test_api_browser_token_sends_cookie() {
    let mut server = mock_server().await;
    let tmp = TempDir::new().unwrap();
    let store = write_multi_store(&tmp);

    // Browser (xoxc) workspace: bearer token plus the stored xoxd cookie.
    let mock = server
        .mock("POST", "/auth.test")
        .match_header("authorization", format!("Bearer {}", XOXC_TOKEN).as_str())
        .match_header("cookie", format!("d={}", XOXD_COOKIE).as_str())
        .with_status(200)
        .with_body(r#"{"ok":true}"#)
        .create_async()
        .await;

    slack_cmd(&server.url(), &store)
        .args(["-w", "T_WS3", "api", "auth.test"])
        .assert()
        .success();

    mock.assert_async().await;
}
