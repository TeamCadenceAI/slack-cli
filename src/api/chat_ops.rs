//! Slack Web API methods for message mutations and scheduled messages.

use serde::{Deserialize, Serialize};

use crate::error::Result;

use super::{ResponseMetadata, SlackClient};

/// Parameters for `chat.scheduleMessage`.
#[derive(Debug, Serialize)]
pub struct ChatScheduleMessageParams {
    pub channel: String,
    pub post_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_ts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mrkdwn: Option<bool>,
}

impl ChatScheduleMessageParams {
    /// Create scheduling parameters for a channel and Unix timestamp.
    pub fn new(channel: impl Into<String>, post_at: i64) -> Self {
        Self {
            channel: channel.into(),
            post_at,
            text: None,
            thread_ts: None,
            blocks: None,
            mrkdwn: None,
        }
    }

    /// Set fallback/message text.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Schedule a thread reply.
    pub fn in_thread(mut self, thread_ts: impl Into<String>) -> Self {
        self.thread_ts = Some(thread_ts.into());
        self
    }

    /// Set Block Kit blocks.
    pub fn with_blocks(mut self, blocks: serde_json::Value) -> Self {
        self.blocks = Some(blocks);
        self
    }
}

/// Response from `chat.getPermalink`.
#[derive(Debug, Deserialize)]
pub struct ChatPermalinkResponse {
    pub channel: String,
    pub permalink: String,
}

/// Response from `chat.scheduleMessage`.
#[derive(Debug, Deserialize)]
pub struct ChatScheduleMessageResponse {
    pub channel: String,
    pub scheduled_message_id: String,
    pub post_at: i64,
}

/// A queued Slack message returned by `chat.scheduledMessages.list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledMessage {
    pub id: String,
    pub channel_id: String,
    pub post_at: i64,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub date_created: Option<i64>,
}

/// One page from `chat.scheduledMessages.list`.
#[derive(Debug, Deserialize)]
pub struct ScheduledMessagesResponse {
    #[serde(default)]
    pub scheduled_messages: Vec<ScheduledMessage>,
    #[serde(default)]
    pub response_metadata: Option<ResponseMetadata>,
}

#[derive(Serialize)]
struct MessageTarget<'a> {
    channel: &'a str,
    ts: &'a str,
}

impl SlackClient {
    /// Update a message with `chat.update`.
    pub async fn chat_update(
        &self,
        channel: &str,
        ts: &str,
        text: &str,
        mrkdwn: bool,
    ) -> Result<()> {
        #[derive(Serialize)]
        struct Params<'a> {
            channel: &'a str,
            ts: &'a str,
            text: &'a str,
            mrkdwn: bool,
        }

        let _: serde_json::Value = self
            .request(
                "chat.update",
                &Params {
                    channel,
                    ts,
                    text,
                    mrkdwn,
                },
            )
            .await?;
        Ok(())
    }

    /// Delete a message with `chat.delete`.
    pub async fn chat_delete(&self, channel: &str, ts: &str) -> Result<()> {
        let _: serde_json::Value = self
            .request("chat.delete", &MessageTarget { channel, ts })
            .await?;
        Ok(())
    }

    /// Get a message permalink with `chat.getPermalink`.
    pub async fn chat_get_permalink(
        &self,
        channel: &str,
        message_ts: &str,
    ) -> Result<ChatPermalinkResponse> {
        #[derive(Serialize)]
        struct Params<'a> {
            channel: &'a str,
            message_ts: &'a str,
        }

        self.request(
            "chat.getPermalink",
            &Params {
                channel,
                message_ts,
            },
        )
        .await
    }

    /// Schedule a message with `chat.scheduleMessage`.
    pub async fn chat_schedule_message(
        &self,
        params: ChatScheduleMessageParams,
    ) -> Result<ChatScheduleMessageResponse> {
        self.request("chat.scheduleMessage", &params).await
    }

    /// List one page of scheduled messages.
    pub async fn chat_scheduled_messages_list(
        &self,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<ScheduledMessagesResponse> {
        #[derive(Serialize)]
        struct Params<'a> {
            limit: u32,
            #[serde(skip_serializing_if = "Option::is_none")]
            cursor: Option<&'a str>,
        }

        self.request("chat.scheduledMessages.list", &Params { limit, cursor })
            .await
    }

    /// Delete a queued message with `chat.deleteScheduledMessage`.
    pub async fn chat_delete_scheduled_message(
        &self,
        channel: &str,
        scheduled_message_id: &str,
    ) -> Result<()> {
        #[derive(Serialize)]
        struct Params<'a> {
            channel: &'a str,
            scheduled_message_id: &'a str,
        }

        let _: serde_json::Value = self
            .request(
                "chat.deleteScheduledMessage",
                &Params {
                    channel,
                    scheduled_message_id,
                },
            )
            .await?;
        Ok(())
    }
}
