use mockito::{Matcher, Server, ServerGuard};
use serde::Deserialize;
use serde_json::{json, Value};
use slack_cli::api::EdgeClient;
use slack_cli::auth::TokenSet;
use slack_cli::error::SlackError;

const XOXC_TOKEN: &str = "xoxc-test-token-1234567890";
const XOXD_COOKIE: &str = "xoxd-test-cookie-value";
const TEAM_ID: &str = "T123EDGE";

fn browser_token() -> TokenSet {
    TokenSet::new_browser(
        XOXC_TOKEN.to_string(),
        XOXD_COOKIE.to_string(),
        TEAM_ID.to_string(),
        "Edge Workspace".to_string(),
        "U123EDGE".to_string(),
    )
    .unwrap()
}

fn edge_client(server: &ServerGuard) -> EdgeClient {
    EdgeClient::with_base_url(browser_token(), server.url()).unwrap()
}

fn authenticated_mock(server: &mut ServerGuard, endpoint: &str, body: Value) -> mockito::Mock {
    server
        .mock("POST", format!("/{TEAM_ID}/{endpoint}").as_str())
        .match_header("authorization", format!("Bearer {XOXC_TOKEN}").as_str())
        .match_header("cookie", format!("d={XOXD_COOKIE}").as_str())
        .match_header("content-type", Matcher::Regex("application/json.*".into()))
        .match_body(Matcher::Json(body))
}

#[tokio::test]
async fn client_boot_posts_authenticated_empty_json_and_deserializes_response() {
    let mut server = Server::new_async().await;
    let request = authenticated_mock(&mut server, "client.boot", json!({}))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{
                "ok": true,
                "self": {"id":"U123EDGE","name":"alice","real_name":"Alice Example"},
                "team": {"id":"T123EDGE","name":"Edge Workspace","domain":"edge-test"}
            }"#,
        )
        .create_async()
        .await;

    let response = edge_client(&server).client_boot().await.unwrap();
    let user = response.self_user.unwrap();
    assert_eq!(user.id, "U123EDGE");
    assert_eq!(user.name.as_deref(), Some("alice"));
    assert_eq!(user.real_name.as_deref(), Some("Alice Example"));
    let team = response.team.unwrap();
    assert_eq!(team.id, TEAM_ID);
    assert_eq!(team.name.as_deref(), Some("Edge Workspace"));
    assert_eq!(team.domain.as_deref(), Some("edge-test"));
    request.assert_async().await;
}

#[tokio::test]
async fn conversations_view_posts_channel_and_deserializes_all_channel_fields() {
    let mut server = Server::new_async().await;
    let request = authenticated_mock(
        &mut server,
        "conversations.view",
        json!({"channel": "C123CHANNEL"}),
    )
    .with_status(200)
    .with_header("content-type", "application/json")
    .with_body(
        r#"{
            "ok": true,
            "channel": {
                "id":"C123CHANNEL",
                "name":"private-dev",
                "is_channel":true,
                "is_group":false,
                "is_im":false,
                "is_mpim":false,
                "is_private":true,
                "is_member":true
            }
        }"#,
    )
    .create_async()
    .await;

    let response = edge_client(&server)
        .conversations_view("C123CHANNEL")
        .await
        .unwrap();
    let channel = response.channel.unwrap();
    assert_eq!(channel.id, "C123CHANNEL");
    assert_eq!(channel.name.as_deref(), Some("private-dev"));
    assert_eq!(channel.is_channel, Some(true));
    assert_eq!(channel.is_group, Some(false));
    assert_eq!(channel.is_im, Some(false));
    assert_eq!(channel.is_mpim, Some(false));
    assert_eq!(channel.is_private, Some(true));
    assert_eq!(channel.is_member, Some(true));
    request.assert_async().await;
}

#[tokio::test]
async fn search_channels_posts_query_and_limit_and_deserializes_results() {
    let mut server = Server::new_async().await;
    let request = authenticated_mock(
        &mut server,
        "channels.search",
        json!({"query": "rust", "count": 25}),
    )
    .with_status(200)
    .with_header("content-type", "application/json")
    .with_body(
        r#"{
            "ok": true,
            "channels": [
                {"id":"C123RUST","name":"rust","is_private":false,"num_members":42}
            ]
        }"#,
    )
    .create_async()
    .await;

    let response = edge_client(&server)
        .search_channels("rust", 25)
        .await
        .unwrap();
    let channels = response.channels.unwrap();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0].id, "C123RUST");
    assert_eq!(channels[0].name.as_deref(), Some("rust"));
    assert_eq!(channels[0].is_private, Some(false));
    assert_eq!(channels[0].num_members, Some(42));
    request.assert_async().await;
}

#[derive(Debug, Deserialize)]
struct GenericResponse {
    result: String,
    total: u32,
}

#[tokio::test]
async fn generic_request_posts_params_and_deserializes_typed_response() {
    let mut server = Server::new_async().await;
    let request = authenticated_mock(
        &mut server,
        "custom.method",
        json!({"query":"hello", "options":{"include_archived":true}}),
    )
    .with_status(200)
    .with_header("content-type", "application/json")
    .with_body(r#"{"ok":true,"result":"matched","total":3}"#)
    .create_async()
    .await;

    let response: GenericResponse = edge_client(&server)
        .request(
            "custom.method",
            &json!({"query":"hello", "options":{"include_archived":true}}),
        )
        .await
        .unwrap();
    assert_eq!(response.result, "matched");
    assert_eq!(response.total, 3);
    request.assert_async().await;
}

#[tokio::test]
async fn ok_false_maps_to_api_error() {
    let mut server = Server::new_async().await;
    let request = authenticated_mock(&mut server, "client.boot", json!({}))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"ok":false,"error":"invalid_auth"}"#)
        .create_async()
        .await;

    let error = edge_client(&server).client_boot().await.unwrap_err();
    match error {
        SlackError::Api { error, detail } => {
            assert_eq!(error, "invalid_auth");
            assert_eq!(detail, None);
        }
        other => panic!("expected SlackError::Api, got {other:?}"),
    }
    request.assert_async().await;
}

#[tokio::test]
async fn non_json_response_maps_to_network_error() {
    let mut server = Server::new_async().await;
    let request = authenticated_mock(&mut server, "broken.method", json!({"key":"value"}))
        .with_status(200)
        .with_header("content-type", "text/html")
        .with_body("<html>not json</html>")
        .create_async()
        .await;

    let error = edge_client(&server)
        .request::<Value, _>("broken.method", &json!({"key":"value"}))
        .await
        .unwrap_err();
    assert!(matches!(error, SlackError::Network(_)), "got {error:?}");
    request.assert_async().await;
}

#[tokio::test]
async fn non_success_http_status_with_non_edge_body_maps_to_network_error() {
    let mut server = Server::new_async().await;
    let request = authenticated_mock(
        &mut server,
        "channels.search",
        json!({"query":"rust","count":10}),
    )
    .with_status(503)
    .with_header("content-type", "application/json")
    .with_body(r#"{"message":"service unavailable"}"#)
    .create_async()
    .await;

    let error = edge_client(&server)
        .search_channels("rust", 10)
        .await
        .unwrap_err();
    assert!(matches!(error, SlackError::Network(_)), "got {error:?}");
    request.assert_async().await;
}
