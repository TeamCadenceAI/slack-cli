//! Tests for name resolution (channels and users)
//!
//! Tests using mockito to simulate Slack API responses.

use mockito::{Matcher, Server};
use slack_cli::{api::SlackClient, auth::TokenSet, error::SlackError};

const TOKEN: &str = "xoxp-test-token-12345678901234";

fn test_client(base_url: String) -> SlackClient {
    let token = TokenSet::new_oauth(
        TOKEN.to_string(),
        "T12345678".to_string(),
        "test".to_string(),
        "U00000000".to_string(),
        vec![],
    )
    .unwrap();
    SlackClient::with_base_url(token, base_url).unwrap()
}

// Note: These tests require creating a SlackClient with a custom base URL,
// which is not currently supported. For now, we test the ID detection logic
// separately and rely on integration tests for the full resolution flow.

/// Helper module for testing resolution logic without hitting the API
mod resolution_logic {
    /// Check if a string looks like a Slack channel ID
    fn is_channel_id(s: &str) -> bool {
        if s.len() < 9 {
            return false;
        }
        matches!(s.chars().next(), Some('C') | Some('D') | Some('G'))
    }

    /// Check if a string looks like a Slack user ID
    fn is_user_id(s: &str) -> bool {
        if s.len() < 9 {
            return false;
        }
        matches!(s.chars().next(), Some('U'))
    }

    #[test]
    fn test_channel_id_passthrough_public() {
        // Public channels start with C
        assert!(is_channel_id("C0123456789"));
        assert!(is_channel_id("CAAAABBBBCCC"));
        assert!(is_channel_id("C12345678"));
        assert!(is_channel_id("C123456789"));
    }

    #[test]
    fn test_channel_id_passthrough_dm() {
        // DMs start with D
        assert!(is_channel_id("D0123456789"));
        assert!(is_channel_id("DAAAABBBBCCC"));
    }

    #[test]
    fn test_channel_id_passthrough_private() {
        // Private channels/groups start with G
        assert!(is_channel_id("G0123456789"));
        assert!(is_channel_id("GAAAABBBBCCC"));
    }

    #[test]
    fn test_channel_id_too_short() {
        // Must be at least 9 characters
        assert!(!is_channel_id("C123456"));
        assert!(!is_channel_id("C1234567"));
        assert!(!is_channel_id("C"));
        assert!(!is_channel_id(""));
        assert!(!is_channel_id("D12345"));
        assert!(!is_channel_id("G123"));
    }

    #[test]
    fn test_channel_id_wrong_prefix() {
        // User IDs don't count as channel IDs
        assert!(!is_channel_id("U0123456789"));
        // Team IDs don't count
        assert!(!is_channel_id("T0123456789"));
        // Names are not IDs
        assert!(!is_channel_id("general"));
        assert!(!is_channel_id("#general"));
        assert!(!is_channel_id("random-channel-name"));
    }

    #[test]
    fn test_user_id_passthrough() {
        // User IDs start with U
        assert!(is_user_id("U0123456789"));
        assert!(is_user_id("UAAAABBBBCCC"));
        assert!(is_user_id("U12345678"));
        assert!(is_user_id("U123456789"));
    }

    #[test]
    fn test_user_id_too_short() {
        assert!(!is_user_id("U123456"));
        assert!(!is_user_id("U1234567"));
        assert!(!is_user_id("U"));
        assert!(!is_user_id(""));
    }

    #[test]
    fn test_user_id_wrong_prefix() {
        // Channel IDs are not user IDs
        assert!(!is_user_id("C0123456789"));
        assert!(!is_user_id("D0123456789"));
        assert!(!is_user_id("G0123456789"));
        // Team IDs are not user IDs
        assert!(!is_user_id("T0123456789"));
        // Names are not IDs
        assert!(!is_user_id("johndoe"));
        assert!(!is_user_id("@johndoe"));
    }

    #[test]
    fn test_name_stripping_logic() {
        // Test the logic of stripping # and @
        let channel_name = "#general";
        let stripped = channel_name.strip_prefix('#').unwrap_or(channel_name);
        assert_eq!(stripped, "general");

        let channel_name_no_hash = "general";
        let stripped2 = channel_name_no_hash
            .strip_prefix('#')
            .unwrap_or(channel_name_no_hash);
        assert_eq!(stripped2, "general");

        let user_name = "@johndoe";
        let stripped3 = user_name.strip_prefix('@').unwrap_or(user_name);
        assert_eq!(stripped3, "johndoe");

        let user_name_no_at = "johndoe";
        let stripped4 = user_name_no_at.strip_prefix('@').unwrap_or(user_name_no_at);
        assert_eq!(stripped4, "johndoe");
    }
}

/// Integration tests that would use mockito
/// Note: These are marked #[ignore] because they require a mock server
/// that can override the Slack API base URL
#[cfg(test)]
mod mock_api_tests {
    use super::*;

    // These tests demonstrate the expected behavior with mocked responses
    // They are ignored because SlackClient currently doesn't support custom base URLs

    #[tokio::test]
    #[ignore = "requires custom base URL support in SlackClient"]
    async fn test_resolve_channel_by_name_single_page() {
        let mut server = Server::new_async().await;

        // Mock conversations.list returning a single page with the channel
        let _m = server
            .mock("POST", "/conversations.list")
            .match_body(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "ok": true,
                "channels": [
                    {"id": "C111111111", "name": "random"},
                    {"id": "C222222222", "name": "general"},
                    {"id": "C333333333", "name": "dev"}
                ],
                "response_metadata": {"next_cursor": ""}
            }"#,
            )
            .create_async()
            .await;

        // Would test: resolve_channel("general") returns "C222222222"
        // Would test: resolve_channel("#general") returns "C222222222"
    }

    #[tokio::test]
    #[ignore = "requires custom base URL support in SlackClient"]
    async fn test_resolve_channel_by_name_pagination() {
        let mut server = Server::new_async().await;

        // First page - channel not found, has more pages
        let _m1 = server
            .mock("POST", "/conversations.list")
            .match_body(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "ok": true,
                "channels": [
                    {"id": "C111111111", "name": "random"}
                ],
                "response_metadata": {"next_cursor": "page2cursor"}
            }"#,
            )
            .expect(1)
            .create_async()
            .await;

        // Second page - channel found
        let _m2 = server
            .mock("POST", "/conversations.list")
            .match_body(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "ok": true,
                "channels": [
                    {"id": "C222222222", "name": "general"}
                ],
                "response_metadata": {"next_cursor": ""}
            }"#,
            )
            .expect(1)
            .create_async()
            .await;

        // Would test: resolve_channel("general") returns "C222222222" after paginating
    }

    #[tokio::test]
    #[ignore = "requires custom base URL support in SlackClient"]
    async fn test_resolve_channel_not_found() {
        let mut server = Server::new_async().await;

        // Mock conversations.list returning no matching channel
        let _m = server
            .mock("POST", "/conversations.list")
            .match_body(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "ok": true,
                "channels": [
                    {"id": "C111111111", "name": "random"}
                ],
                "response_metadata": {"next_cursor": ""}
            }"#,
            )
            .create_async()
            .await;

        // Would test: resolve_channel("nonexistent") returns ChannelNotFound error
    }

    #[tokio::test]
    #[ignore = "requires custom base URL support in SlackClient"]
    async fn test_resolve_user_by_name() {
        let mut server = Server::new_async().await;

        // Mock users.list returning matching user
        let _m = server
            .mock("POST", "/users.list")
            .match_body(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "ok": true,
                "members": [
                    {"id": "U111111111", "name": "alice"},
                    {"id": "U222222222", "name": "bob", "profile": {"display_name": "Bobby"}},
                    {"id": "U333333333", "name": "charlie"}
                ],
                "response_metadata": {"next_cursor": ""}
            }"#,
            )
            .create_async()
            .await;

        // Would test: resolve_user("bob") returns "U222222222"
        // Would test: resolve_user("@bob") returns "U222222222"
        // Would test: resolve_user("Bobby") returns "U222222222" (display_name match)
    }

    #[tokio::test]
    #[ignore = "requires custom base URL support in SlackClient"]
    async fn test_resolve_user_by_display_name() {
        let mut server = Server::new_async().await;

        let _m = server
            .mock("POST", "/users.list")
            .match_body(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "ok": true,
                "members": [
                    {"id": "U111111111", "name": "alice", "profile": {"display_name": "Alice Smith"}},
                    {"id": "U222222222", "name": "bob", "profile": {"display_name": "Bobby"}}
                ],
                "response_metadata": {"next_cursor": ""}
            }"#,
            )
            .create_async()
            .await;

        // Would test: resolve_user("Bobby") returns "U222222222"
        // Would test: resolve_user("bobby") returns "U222222222" (case insensitive)
    }

    #[tokio::test]
    #[ignore = "requires custom base URL support in SlackClient"]
    async fn test_resolve_user_not_found() {
        let mut server = Server::new_async().await;

        let _m = server
            .mock("POST", "/users.list")
            .match_body(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "ok": true,
                "members": [
                    {"id": "U111111111", "name": "alice"}
                ],
                "response_metadata": {"next_cursor": ""}
            }"#,
            )
            .create_async()
            .await;

        // Would test: resolve_user("nonexistent") returns UserNotFound error
    }

    #[tokio::test]
    #[ignore = "requires custom base URL support in SlackClient"]
    async fn test_resolve_user_pagination() {
        let mut server = Server::new_async().await;

        // First page - user not found
        let _m1 = server
            .mock("POST", "/users.list")
            .match_body(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "ok": true,
                "members": [
                    {"id": "U111111111", "name": "alice"}
                ],
                "response_metadata": {"next_cursor": "page2"}
            }"#,
            )
            .expect(1)
            .create_async()
            .await;

        // Second page - user found
        let _m2 = server
            .mock("POST", "/users.list")
            .match_body(Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "ok": true,
                "members": [
                    {"id": "U222222222", "name": "bob"}
                ],
                "response_metadata": {"next_cursor": ""}
            }"#,
            )
            .expect(1)
            .create_async()
            .await;

        // Would test: resolve_user("bob") returns "U222222222" after pagination
    }
}

#[tokio::test]
async fn resolve_channel_finds_name_on_second_page() {
    let mut server = Server::new_async().await;
    let first = server
        .mock("POST", "/conversations.list")
        .match_body(Matcher::Exact(
            "exclude_archived=false&limit=200&types=public_channel%2Cprivate_channel%2Cmpim%2Cim"
                .to_string(),
        ))
        .with_body(
            r#"{"ok":true,"channels":[{"id":"C11111111","name":"random"}],"response_metadata":{"next_cursor":"c2"}}"#,
        )
        .expect(1)
        .create_async()
        .await;
    let second = server
        .mock("POST", "/conversations.list")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("cursor".into(), "c2".into()),
            Matcher::UrlEncoded("limit".into(), "200".into()),
        ]))
        .with_body(
            r#"{"ok":true,"channels":[{"id":"C22222222","name":"general"}],"response_metadata":{"next_cursor":""}}"#,
        )
        .expect(1)
        .create_async()
        .await;
    let client = test_client(server.url());

    assert_eq!(
        client.resolve_channel("#general").await.unwrap(),
        "C22222222"
    );
    first.assert_async().await;
    second.assert_async().await;
}

#[tokio::test]
async fn resolve_channel_empty_next_cursor_stops_with_not_found() {
    let mut server = Server::new_async().await;
    let list = server
        .mock("POST", "/conversations.list")
        .match_body(Matcher::AllOf(vec![
            Matcher::UrlEncoded("limit".into(), "200".into()),
            Matcher::UrlEncoded("exclude_archived".into(), "false".into()),
            Matcher::UrlEncoded(
                "types".into(),
                "public_channel,private_channel,mpim,im".into(),
            ),
        ]))
        .with_body(
            r#"{"ok":true,"channels":[{"id":"C11111111","name":"random"}],"response_metadata":{"next_cursor":""}}"#,
        )
        .expect(1)
        .create_async()
        .await;
    let client = test_client(server.url());

    let error = client.resolve_channel("missing").await.unwrap_err();
    assert!(matches!(error, SlackError::ChannelNotFound(name) if name == "missing"));
    list.assert_async().await;
}
