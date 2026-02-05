//! Slack Web API method implementations
//!
//! High-level methods for interacting with Slack's Web API endpoints.

use serde::Serialize;

use crate::error::{Result, SlackError};
use crate::models::{Channel, File, Message, User};

use super::client::SlackClient;
use super::types::{
    AuthTestResponse, ChatPostMessageResponse, ConversationsHistoryResponse,
    ConversationsInfoResponse, ConversationsListResponse, ConversationsRepliesResponse,
    FilesInfoResponse, FilesListResponse, PaginationParams, ReactionsGetResponse,
    ReactionsResponse, RemindersAddResponse, RemindersCompleteResponse, RemindersDeleteResponse,
    RemindersListResponse, SearchMessagesResponse, UsersGetPresenceResponse, UsersInfoResponse,
    UsersListResponse, UsersProfileGetResponse, UsersProfileSetResponse, UsersSetPresenceResponse,
};

/// Maximum file download size (5MB)
const MAX_FILE_SIZE: u64 = 5 * 1024 * 1024;

// ============================================================================
// Auth Methods
// ============================================================================

impl SlackClient {
    /// Test authentication and get information about the token
    ///
    /// API: auth.test
    pub async fn auth_test(&self) -> Result<AuthTestResponse> {
        #[derive(Serialize)]
        struct Params {}

        self.request("auth.test", &Params {}).await
    }
}

// ============================================================================
// Conversations Methods
// ============================================================================

/// Parameters for conversations.list
#[derive(Debug, Serialize, Default)]
pub struct ConversationsListParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_archived: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<String>,
}

impl ConversationsListParams {
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

    pub fn exclude_archived(mut self, exclude: bool) -> Self {
        self.exclude_archived = Some(exclude);
        self
    }

    /// Set channel types to include
    ///
    /// Types can be: public_channel, private_channel, mpim, im
    pub fn with_types(mut self, types: impl Into<String>) -> Self {
        self.types = Some(types.into());
        self
    }
}

/// Parameters for conversations.history
#[derive(Debug, Serialize, Default)]
pub struct ConversationsHistoryParams {
    pub channel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oldest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inclusive: Option<bool>,
}

impl ConversationsHistoryParams {
    pub fn new(channel: impl Into<String>) -> Self {
        Self {
            channel: channel.into(),
            ..Default::default()
        }
    }

    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn with_cursor(mut self, cursor: impl Into<String>) -> Self {
        self.cursor = Some(cursor.into());
        self
    }

    pub fn with_oldest(mut self, ts: impl Into<String>) -> Self {
        self.oldest = Some(ts.into());
        self
    }

    pub fn with_latest(mut self, ts: impl Into<String>) -> Self {
        self.latest = Some(ts.into());
        self
    }

    pub fn inclusive(mut self, inclusive: bool) -> Self {
        self.inclusive = Some(inclusive);
        self
    }
}

/// Parameters for conversations.replies
#[derive(Debug, Serialize, Default)]
pub struct ConversationsRepliesParams {
    pub channel: String,
    pub ts: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oldest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inclusive: Option<bool>,
}

impl ConversationsRepliesParams {
    pub fn new(channel: impl Into<String>, ts: impl Into<String>) -> Self {
        Self {
            channel: channel.into(),
            ts: ts.into(),
            ..Default::default()
        }
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

/// Parameters for conversations.mark
#[derive(Debug, Serialize, Default)]
pub struct ConversationsMarkParams {
    pub channel: String,
    pub ts: String,
}

impl ConversationsMarkParams {
    pub fn new(channel: impl Into<String>, ts: impl Into<String>) -> Self {
        Self {
            channel: channel.into(),
            ts: ts.into(),
        }
    }
}

impl SlackClient {
    /// Mark a channel as read up to a specific message timestamp
    ///
    /// API: conversations.mark
    pub async fn conversations_mark(&self, params: ConversationsMarkParams) -> Result<()> {
        use super::types::ConversationsMarkResponse;
        let _: ConversationsMarkResponse = self.request("conversations.mark", &params).await?;
        Ok(())
    }

    /// List channels the user/bot has access to
    ///
    /// API: conversations.list
    pub async fn conversations_list(
        &self,
        params: ConversationsListParams,
    ) -> Result<ConversationsListResponse> {
        self.request("conversations.list", &params).await
    }

    /// List all channels with automatic pagination
    pub async fn conversations_list_all(
        &self,
        types: Option<&str>,
        exclude_archived: bool,
    ) -> Result<Vec<Channel>> {
        let mut all_channels = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut params = ConversationsListParams::new()
                .with_limit(200)
                .exclude_archived(exclude_archived);

            if let Some(t) = types {
                params = params.with_types(t);
            }

            if let Some(c) = cursor {
                params = params.with_cursor(c);
            }

            let response = self.conversations_list(params).await?;
            all_channels.extend(response.channels);

            cursor = response
                .response_metadata
                .and_then(|m| m.next_cursor)
                .filter(|c| !c.is_empty());

            if cursor.is_none() {
                break;
            }
        }

        Ok(all_channels)
    }

    /// Get information about a channel
    ///
    /// API: conversations.info
    pub async fn conversations_info(&self, channel_id: &str) -> Result<Channel> {
        #[derive(Serialize)]
        struct Params<'a> {
            channel: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            include_num_members: Option<bool>,
        }

        let response: ConversationsInfoResponse = self
            .request(
                "conversations.info",
                &Params {
                    channel: channel_id,
                    include_num_members: Some(true),
                },
            )
            .await?;

        Ok(response.channel)
    }

    /// Get message history for a channel
    ///
    /// API: conversations.history
    pub async fn conversations_history(
        &self,
        params: ConversationsHistoryParams,
    ) -> Result<ConversationsHistoryResponse> {
        self.request("conversations.history", &params).await
    }

    /// Get all messages in a channel with automatic pagination
    pub async fn conversations_history_all(
        &self,
        channel: &str,
        oldest: Option<&str>,
        latest: Option<&str>,
    ) -> Result<Vec<Message>> {
        let mut all_messages = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut params = ConversationsHistoryParams::new(channel).with_limit(200);

            if let Some(o) = oldest {
                params = params.with_oldest(o);
            }
            if let Some(l) = latest {
                params = params.with_latest(l);
            }
            if let Some(c) = cursor {
                params = params.with_cursor(c);
            }

            let response = self.conversations_history(params).await?;
            all_messages.extend(response.messages);

            if !response.has_more {
                break;
            }

            cursor = response
                .response_metadata
                .and_then(|m| m.next_cursor)
                .filter(|c| !c.is_empty());

            if cursor.is_none() {
                break;
            }
        }

        Ok(all_messages)
    }

    /// Get replies in a thread
    ///
    /// API: conversations.replies
    pub async fn conversations_replies(
        &self,
        params: ConversationsRepliesParams,
    ) -> Result<ConversationsRepliesResponse> {
        self.request("conversations.replies", &params).await
    }

    /// Get all replies in a thread with automatic pagination
    pub async fn conversations_replies_all(
        &self,
        channel: &str,
        thread_ts: &str,
    ) -> Result<Vec<Message>> {
        let mut all_messages = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut params = ConversationsRepliesParams::new(channel, thread_ts).with_limit(200);

            if let Some(c) = cursor {
                params = params.with_cursor(c);
            }

            let response = self.conversations_replies(params).await?;
            all_messages.extend(response.messages);

            if !response.has_more {
                break;
            }

            cursor = response
                .response_metadata
                .and_then(|m| m.next_cursor)
                .filter(|c| !c.is_empty());

            if cursor.is_none() {
                break;
            }
        }

        Ok(all_messages)
    }
}

// ============================================================================
// Chat Methods
// ============================================================================

/// Parameters for chat.postMessage
#[derive(Debug, Serialize)]
pub struct ChatPostMessageParams {
    pub channel: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_ts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_broadcast: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unfurl_links: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unfurl_media: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mrkdwn: Option<bool>,
}

impl ChatPostMessageParams {
    pub fn new(channel: impl Into<String>) -> Self {
        Self {
            channel: channel.into(),
            text: None,
            thread_ts: None,
            reply_broadcast: None,
            blocks: None,
            attachments: None,
            unfurl_links: None,
            unfurl_media: None,
            mrkdwn: None,
        }
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn in_thread(mut self, thread_ts: impl Into<String>) -> Self {
        self.thread_ts = Some(thread_ts.into());
        self
    }

    pub fn reply_broadcast(mut self, broadcast: bool) -> Self {
        self.reply_broadcast = Some(broadcast);
        self
    }

    pub fn with_blocks(mut self, blocks: serde_json::Value) -> Self {
        self.blocks = Some(blocks);
        self
    }
}

impl SlackClient {
    /// Post a message to a channel
    ///
    /// API: chat.postMessage
    pub async fn chat_post_message(
        &self,
        params: ChatPostMessageParams,
    ) -> Result<ChatPostMessageResponse> {
        self.request("chat.postMessage", &params).await
    }
}

// ============================================================================
// Search Methods
// ============================================================================

/// Parameters for search.messages
#[derive(Debug, Serialize)]
pub struct SearchMessagesParams {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
}

impl SearchMessagesParams {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            sort: None,
            sort_dir: None,
            count: None,
            page: None,
        }
    }

    pub fn with_sort(mut self, sort: impl Into<String>, dir: impl Into<String>) -> Self {
        self.sort = Some(sort.into());
        self.sort_dir = Some(dir.into());
        self
    }

    pub fn with_count(mut self, count: u32) -> Self {
        self.count = Some(count);
        self
    }

    pub fn with_page(mut self, page: u32) -> Self {
        self.page = Some(page);
        self
    }
}

impl SlackClient {
    /// Search for messages
    ///
    /// API: search.messages
    ///
    /// Note: This method requires a user token (xoxp-*) or browser token (xoxc-*).
    /// Bot tokens do not support search.
    pub async fn search_messages(
        &self,
        params: SearchMessagesParams,
    ) -> Result<SearchMessagesResponse> {
        if !self.supports_search() {
            return Err(SlackError::SearchNotAvailable);
        }
        self.request("search.messages", &params).await
    }
}

// ============================================================================
// Users Methods
// ============================================================================

impl SlackClient {
    /// List all users in the workspace
    ///
    /// API: users.list
    pub async fn users_list(&self, params: PaginationParams) -> Result<UsersListResponse> {
        self.request("users.list", &params).await
    }

    /// List all users with automatic pagination
    pub async fn users_list_all(&self) -> Result<Vec<User>> {
        let mut all_users = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let mut params = PaginationParams::new().with_limit(200);

            if let Some(c) = cursor {
                params = params.with_cursor(c);
            }

            let response = self.users_list(params).await?;
            all_users.extend(response.members);

            cursor = response
                .response_metadata
                .and_then(|m| m.next_cursor)
                .filter(|c| !c.is_empty());

            if cursor.is_none() {
                break;
            }
        }

        Ok(all_users)
    }

    /// Get information about a user
    ///
    /// API: users.info
    pub async fn users_info(&self, user_id: &str) -> Result<User> {
        #[derive(Serialize)]
        struct Params<'a> {
            user: &'a str,
        }

        let response: UsersInfoResponse = self
            .request("users.info", &Params { user: user_id })
            .await?;

        Ok(response.user)
    }
}

// ============================================================================
// Reactions Methods
// ============================================================================

impl SlackClient {
    /// Add a reaction to a message
    ///
    /// API: reactions.add
    pub async fn reactions_add(&self, channel: &str, timestamp: &str, name: &str) -> Result<()> {
        #[derive(Serialize)]
        struct Params<'a> {
            channel: &'a str,
            timestamp: &'a str,
            name: &'a str,
        }

        let _: ReactionsResponse = self
            .request(
                "reactions.add",
                &Params {
                    channel,
                    timestamp,
                    name,
                },
            )
            .await?;

        Ok(())
    }

    /// Remove a reaction from a message
    ///
    /// API: reactions.remove
    pub async fn reactions_remove(&self, channel: &str, timestamp: &str, name: &str) -> Result<()> {
        #[derive(Serialize)]
        struct Params<'a> {
            channel: &'a str,
            timestamp: &'a str,
            name: &'a str,
        }

        let _: ReactionsResponse = self
            .request(
                "reactions.remove",
                &Params {
                    channel,
                    timestamp,
                    name,
                },
            )
            .await?;

        Ok(())
    }
}

// ============================================================================
// Files Methods
// ============================================================================

impl SlackClient {
    /// Get information about a file
    ///
    /// API: files.info
    pub async fn files_info(&self, file_id: &str) -> Result<File> {
        #[derive(Serialize)]
        struct Params<'a> {
            file: &'a str,
        }

        let response: FilesInfoResponse = self
            .request("files.info", &Params { file: file_id })
            .await?;

        Ok(response.file)
    }

    /// Download a file
    ///
    /// This method downloads the file content directly.
    /// Maximum file size is 5MB.
    pub async fn files_download(&self, file: &File) -> Result<Vec<u8>> {
        // Check size limit
        if !file.is_within_download_limit() {
            return Err(SlackError::FileTooLarge);
        }

        let url = file.download_url().ok_or_else(|| SlackError::Api {
            error: "no_download_url".to_string(),
            detail: Some("File does not have a download URL".to_string()),
        })?;

        self.download(url, MAX_FILE_SIZE).await
    }

    /// Download a file by ID
    ///
    /// Convenience method that fetches file info and downloads in one call.
    pub async fn files_download_by_id(&self, file_id: &str) -> Result<Vec<u8>> {
        let file = self.files_info(file_id).await?;
        self.files_download(&file).await
    }

    /// List files
    ///
    /// API: files.list
    pub async fn files_list(
        &self,
        channel: Option<&str>,
        user: Option<&str>,
        limit: Option<u32>,
        cursor: Option<&str>,
    ) -> Result<FilesListResponse> {
        #[derive(Serialize)]
        struct Params<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            channel: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            user: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            count: Option<u32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            page: Option<&'a str>,
        }

        self.request(
            "files.list",
            &Params {
                channel,
                user,
                count: limit,
                page: cursor,
            },
        )
        .await
    }
}

// ============================================================================
// Reactions Get Method
// ============================================================================

impl SlackClient {
    /// Get reactions for a message
    ///
    /// API: reactions.get
    pub async fn reactions_get(&self, channel: &str, timestamp: &str) -> Result<Message> {
        #[derive(Serialize)]
        struct Params<'a> {
            channel: &'a str,
            timestamp: &'a str,
            full: bool,
        }

        let response: ReactionsGetResponse = self
            .request(
                "reactions.get",
                &Params {
                    channel,
                    timestamp,
                    full: true,
                },
            )
            .await?;

        Ok(response.message)
    }
}

// ============================================================================
// Reminders Methods
// ============================================================================

impl SlackClient {
    /// List reminders
    ///
    /// API: reminders.list
    pub async fn reminders_list(&self) -> Result<RemindersListResponse> {
        #[derive(Serialize)]
        struct Params {}

        self.request("reminders.list", &Params {}).await
    }

    /// Add a reminder
    ///
    /// API: reminders.add
    pub async fn reminders_add(&self, text: &str, time: i64) -> Result<RemindersAddResponse> {
        #[derive(Serialize)]
        struct Params<'a> {
            text: &'a str,
            time: i64,
        }

        self.request("reminders.add", &Params { text, time }).await
    }

    /// Complete a reminder
    ///
    /// API: reminders.complete
    pub async fn reminders_complete(&self, reminder_id: &str) -> Result<()> {
        #[derive(Serialize)]
        struct Params<'a> {
            reminder: &'a str,
        }

        let _: RemindersCompleteResponse = self
            .request(
                "reminders.complete",
                &Params {
                    reminder: reminder_id,
                },
            )
            .await?;

        Ok(())
    }

    /// Delete a reminder
    ///
    /// API: reminders.delete
    pub async fn reminders_delete(&self, reminder_id: &str) -> Result<()> {
        #[derive(Serialize)]
        struct Params<'a> {
            reminder: &'a str,
        }

        let _: RemindersDeleteResponse = self
            .request(
                "reminders.delete",
                &Params {
                    reminder: reminder_id,
                },
            )
            .await?;

        Ok(())
    }
}

// ============================================================================
// Status/Presence Methods
// ============================================================================

impl SlackClient {
    /// Get the current user's profile
    ///
    /// API: users.profile.get
    pub async fn users_profile_get(&self) -> Result<crate::models::UserProfile> {
        #[derive(Serialize)]
        struct Params {}

        let response: UsersProfileGetResponse =
            self.request("users.profile.get", &Params {}).await?;

        Ok(response.profile)
    }

    /// Set the current user's profile status
    ///
    /// API: users.profile.set
    pub async fn users_profile_set(
        &self,
        status_text: Option<&str>,
        status_emoji: Option<&str>,
        status_expiration: Option<i64>,
    ) -> Result<crate::models::UserProfile> {
        #[derive(Serialize)]
        struct Profile<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            status_text: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            status_emoji: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            status_expiration: Option<i64>,
        }

        #[derive(Serialize)]
        struct Params<'a> {
            profile: Profile<'a>,
        }

        let response: UsersProfileSetResponse = self
            .request(
                "users.profile.set",
                &Params {
                    profile: Profile {
                        status_text,
                        status_emoji,
                        status_expiration,
                    },
                },
            )
            .await?;

        Ok(response.profile)
    }

    /// Set presence status
    ///
    /// API: users.setPresence
    pub async fn users_set_presence(&self, presence: &str) -> Result<()> {
        #[derive(Serialize)]
        struct Params<'a> {
            presence: &'a str,
        }

        let _: UsersSetPresenceResponse = self
            .request("users.setPresence", &Params { presence })
            .await?;

        Ok(())
    }

    /// Get presence status
    ///
    /// API: users.getPresence
    pub async fn users_get_presence(&self, user: Option<&str>) -> Result<UsersGetPresenceResponse> {
        #[derive(Serialize)]
        struct Params<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            user: Option<&'a str>,
        }

        self.request("users.getPresence", &Params { user }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversations_list_params_builder() {
        let params = ConversationsListParams::new()
            .with_limit(100)
            .with_cursor("cursor123")
            .exclude_archived(true)
            .with_types("public_channel,private_channel");

        assert_eq!(params.limit, Some(100));
        assert_eq!(params.cursor, Some("cursor123".to_string()));
        assert_eq!(params.exclude_archived, Some(true));
        assert_eq!(
            params.types,
            Some("public_channel,private_channel".to_string())
        );
    }

    #[test]
    fn test_conversations_history_params_builder() {
        let params = ConversationsHistoryParams::new("C123")
            .with_limit(50)
            .with_oldest("1234567890.000001")
            .with_latest("1234567890.999999")
            .inclusive(true);

        assert_eq!(params.channel, "C123");
        assert_eq!(params.limit, Some(50));
        assert_eq!(params.oldest, Some("1234567890.000001".to_string()));
        assert_eq!(params.latest, Some("1234567890.999999".to_string()));
        assert_eq!(params.inclusive, Some(true));
    }

    #[test]
    fn test_conversations_replies_params_builder() {
        let params = ConversationsRepliesParams::new("C123", "1234567890.000001")
            .with_limit(100)
            .with_cursor("cursor");

        assert_eq!(params.channel, "C123");
        assert_eq!(params.ts, "1234567890.000001");
        assert_eq!(params.limit, Some(100));
        assert_eq!(params.cursor, Some("cursor".to_string()));
    }

    #[test]
    fn test_chat_post_message_params_builder() {
        let params = ChatPostMessageParams::new("C123")
            .with_text("Hello, world!")
            .in_thread("1234567890.000001")
            .reply_broadcast(true);

        assert_eq!(params.channel, "C123");
        assert_eq!(params.text, Some("Hello, world!".to_string()));
        assert_eq!(params.thread_ts, Some("1234567890.000001".to_string()));
        assert_eq!(params.reply_broadcast, Some(true));
    }

    #[test]
    fn test_search_messages_params_builder() {
        let params = SearchMessagesParams::new("query text")
            .with_sort("timestamp", "desc")
            .with_count(50)
            .with_page(2);

        assert_eq!(params.query, "query text");
        assert_eq!(params.sort, Some("timestamp".to_string()));
        assert_eq!(params.sort_dir, Some("desc".to_string()));
        assert_eq!(params.count, Some(50));
        assert_eq!(params.page, Some(2));
    }

    #[test]
    fn test_params_serialization_skips_none() {
        let params = ConversationsListParams::new().with_limit(100);

        let json = serde_json::to_string(&params).unwrap();

        // Should have limit but not cursor, exclude_archived, or types
        assert!(json.contains("\"limit\":100"));
        assert!(!json.contains("cursor"));
        assert!(!json.contains("exclude_archived"));
        assert!(!json.contains("types"));
    }

    #[test]
    fn test_conversations_mark_params_builder() {
        let params = ConversationsMarkParams::new("C123", "1234567890.000001");

        assert_eq!(params.channel, "C123");
        assert_eq!(params.ts, "1234567890.000001");

        // Verify serialization
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("\"channel\":\"C123\""));
        assert!(json.contains("\"ts\":\"1234567890.000001\""));
    }
}
