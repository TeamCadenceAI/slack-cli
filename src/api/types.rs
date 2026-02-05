//! API response types and error handling
//!
//! Wrapper types for Slack API responses and error mapping.

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::error::SlackError;

/// Generic wrapper for Slack API responses
///
/// All Slack API responses include an `ok` field indicating success.
/// On success, additional data is included. On failure, `error` contains the error code.
#[derive(Debug, Deserialize)]
pub struct SlackResponse<T> {
    pub ok: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub warning: Option<String>,
    #[serde(default)]
    pub response_metadata: Option<ResponseMetadata>,
    #[serde(flatten)]
    pub data: Option<T>,
}

impl<T: DeserializeOwned> SlackResponse<T> {
    /// Convert this response into a Result
    pub fn into_result(self) -> Result<T, SlackError> {
        if self.ok {
            self.data.ok_or_else(|| SlackError::Api {
                error: "missing_data".to_string(),
                detail: Some("Response was ok but contained no data".to_string()),
            })
        } else {
            Err(SlackError::Api {
                error: self.error.unwrap_or_else(|| "unknown_error".to_string()),
                detail: self
                    .response_metadata
                    .and_then(|m| m.messages.map(|msgs| msgs.join(", "))),
            })
        }
    }
}

/// Metadata included in some API responses
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ResponseMetadata {
    /// Cursor for pagination
    #[serde(default)]
    pub next_cursor: Option<String>,
    /// Warning messages
    #[serde(default)]
    pub messages: Option<Vec<String>>,
}

/// Request parameters for paginated endpoints
#[derive(Debug, Serialize, Default)]
pub struct PaginationParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

impl PaginationParams {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn with_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }
}

/// Response from auth.test endpoint
#[derive(Debug, Deserialize)]
pub struct AuthTestResponse {
    pub url: String,
    pub team: String,
    pub user: String,
    pub team_id: String,
    pub user_id: String,
    #[serde(default)]
    pub bot_id: Option<String>,
    #[serde(default)]
    pub is_enterprise_install: bool,
}

/// Response containing a list of channels
#[derive(Debug, Deserialize)]
pub struct ConversationsListResponse {
    pub channels: Vec<crate::models::Channel>,
    #[serde(default)]
    pub response_metadata: Option<ResponseMetadata>,
}

/// Response containing message history
#[derive(Debug, Deserialize)]
pub struct ConversationsHistoryResponse {
    pub messages: Vec<crate::models::Message>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub response_metadata: Option<ResponseMetadata>,
}

/// Response containing thread replies
#[derive(Debug, Deserialize)]
pub struct ConversationsRepliesResponse {
    pub messages: Vec<crate::models::Message>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub response_metadata: Option<ResponseMetadata>,
}

/// Response from chat.postMessage
#[derive(Debug, Deserialize)]
pub struct ChatPostMessageResponse {
    pub channel: String,
    pub ts: String,
    pub message: crate::models::Message,
}

/// Response from search.messages
#[derive(Debug, Deserialize)]
pub struct SearchMessagesResponse {
    pub messages: SearchResults,
}

/// Search results container
#[derive(Debug, Deserialize)]
pub struct SearchResults {
    pub total: u32,
    #[serde(default)]
    pub pagination: Option<SearchPagination>,
    pub matches: Vec<crate::models::Message>,
}

/// Pagination info for search results
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchPagination {
    pub total_count: u32,
    pub page: u32,
    pub per_page: u32,
    pub page_count: u32,
    pub first: u32,
    pub last: u32,
}

/// Response containing a list of users
#[derive(Debug, Deserialize)]
pub struct UsersListResponse {
    pub members: Vec<crate::models::User>,
    #[serde(default)]
    pub response_metadata: Option<ResponseMetadata>,
}

/// Response containing a single user
#[derive(Debug, Deserialize)]
pub struct UsersInfoResponse {
    pub user: crate::models::User,
}

/// Response from reactions.add/remove
#[derive(Debug, Deserialize)]
pub struct ReactionsResponse {
    // These endpoints just return ok: true on success
}

/// Response from files.info
#[derive(Debug, Deserialize)]
pub struct FilesInfoResponse {
    pub file: crate::models::File,
}

/// Response from conversations.info
#[derive(Debug, Deserialize)]
pub struct ConversationsInfoResponse {
    pub channel: crate::models::Channel,
}

/// Response from conversations.mark
#[derive(Debug, Deserialize)]
pub struct ConversationsMarkResponse {
    // This endpoint just returns ok: true on success
}

/// Response from files.list
#[derive(Debug, Deserialize)]
pub struct FilesListResponse {
    pub files: Vec<crate::models::File>,
    #[serde(default)]
    pub paging: Option<FilesPaging>,
}

/// Paging info for files.list
#[derive(Debug, Serialize, Deserialize)]
pub struct FilesPaging {
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub page: u32,
    #[serde(default)]
    pub pages: u32,
}

/// Response from reactions.get
#[derive(Debug, Deserialize)]
pub struct ReactionsGetResponse {
    pub message: crate::models::Message,
}

/// Response from reminders.list
#[derive(Debug, Deserialize)]
pub struct RemindersListResponse {
    pub reminders: Vec<Reminder>,
}

/// Response from reminders.add
#[derive(Debug, Deserialize)]
pub struct RemindersAddResponse {
    pub reminder: Reminder,
}

/// Response from reminders.complete
#[derive(Debug, Deserialize)]
pub struct RemindersCompleteResponse {
    // Just returns ok: true on success
}

/// Response from reminders.delete
#[derive(Debug, Deserialize)]
pub struct RemindersDeleteResponse {
    // Just returns ok: true on success
}

/// A Slack reminder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reminder {
    pub id: String,
    #[serde(default)]
    pub creator: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub recurring: bool,
    #[serde(default)]
    pub time: Option<i64>,
    #[serde(default)]
    pub complete_ts: Option<i64>,
}

/// Response from users.profile.get
#[derive(Debug, Deserialize)]
pub struct UsersProfileGetResponse {
    pub profile: crate::models::UserProfile,
}

/// Response from users.profile.set
#[derive(Debug, Deserialize)]
pub struct UsersProfileSetResponse {
    pub profile: crate::models::UserProfile,
}

/// Response from users.setPresence
#[derive(Debug, Deserialize)]
pub struct UsersSetPresenceResponse {
    // Just returns ok: true on success
}

/// Response from users.getPresence
#[derive(Debug, Deserialize)]
pub struct UsersGetPresenceResponse {
    pub presence: String,
    #[serde(default)]
    pub online: bool,
    #[serde(default)]
    pub auto_away: bool,
    #[serde(default)]
    pub manual_away: bool,
    #[serde(default)]
    pub connection_count: Option<u32>,
    #[serde(default)]
    pub last_activity: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slack_response_success() {
        let json = r#"{"ok": true, "team_id": "T12345"}"#;
        let response: SlackResponse<serde_json::Value> = serde_json::from_str(json).unwrap();
        assert!(response.ok);
        assert!(response.error.is_none());
    }

    #[test]
    fn test_slack_response_error() {
        let json = r#"{"ok": false, "error": "invalid_auth"}"#;
        let response: SlackResponse<serde_json::Value> = serde_json::from_str(json).unwrap();
        assert!(!response.ok);
        assert_eq!(response.error, Some("invalid_auth".to_string()));
    }

    #[test]
    fn test_slack_response_into_result_success() {
        #[derive(Debug, Deserialize)]
        struct TestData {
            value: String,
        }

        let json = r#"{"ok": true, "value": "test"}"#;
        let response: SlackResponse<TestData> = serde_json::from_str(json).unwrap();
        let result = response.into_result();
        assert!(result.is_ok());
        assert_eq!(result.unwrap().value, "test");
    }

    #[test]
    fn test_slack_response_into_result_error() {
        #[derive(Debug, Deserialize)]
        #[allow(dead_code)]
        struct TestData {
            value: String,
        }

        let json = r#"{"ok": false, "error": "channel_not_found"}"#;
        let response: SlackResponse<TestData> = serde_json::from_str(json).unwrap();
        let result = response.into_result();
        assert!(result.is_err());

        match result.unwrap_err() {
            SlackError::Api { error, .. } => assert_eq!(error, "channel_not_found"),
            _ => panic!("Expected Api error"),
        }
    }

    #[test]
    fn test_response_metadata_with_cursor() {
        let json = r#"{"next_cursor": "abc123", "messages": ["warning1"]}"#;
        let meta: ResponseMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.next_cursor, Some("abc123".to_string()));
        assert_eq!(meta.messages, Some(vec!["warning1".to_string()]));
    }

    #[test]
    fn test_pagination_params_serialization() {
        let params = PaginationParams::new().with_limit(100).with_cursor("abc");
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("\"cursor\":\"abc\""));
        assert!(json.contains("\"limit\":100"));
    }

    #[test]
    fn test_pagination_params_skip_none() {
        let params = PaginationParams::new();
        let json = serde_json::to_string(&params).unwrap();
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_auth_test_response() {
        let json = r#"{
            "url": "https://myteam.slack.com/",
            "team": "My Team",
            "user": "testuser",
            "team_id": "T12345",
            "user_id": "U12345"
        }"#;
        let response: AuthTestResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.team_id, "T12345");
        assert_eq!(response.user_id, "U12345");
        assert!(response.bot_id.is_none());
    }

    #[test]
    fn test_auth_test_response_with_bot() {
        let json = r#"{
            "url": "https://myteam.slack.com/",
            "team": "My Team",
            "user": "testbot",
            "team_id": "T12345",
            "user_id": "U12345",
            "bot_id": "B12345",
            "is_enterprise_install": true
        }"#;
        let response: AuthTestResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.bot_id, Some("B12345".to_string()));
        assert!(response.is_enterprise_install);
    }
}
