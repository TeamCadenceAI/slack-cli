//! Slack Web API operations for channel membership and lifecycle management.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::error::{Result, SlackError};
use crate::models::Channel;

use super::client::SlackClient;
use super::types::ResponseMetadata;

#[derive(Debug, Serialize)]
struct ChannelParams<'a> {
    channel: &'a str,
}

#[derive(Debug, Serialize)]
struct CreateParams<'a> {
    name: &'a str,
    is_private: bool,
}

#[derive(Debug, Serialize)]
struct InviteParams<'a> {
    channel: &'a str,
    users: &'a str,
}

#[derive(Debug, Serialize)]
struct TextParams<'a> {
    channel: &'a str,
    #[serde(flatten)]
    text: TextField<'a>,
}

#[derive(Debug, Serialize)]
enum TextField<'a> {
    #[serde(rename = "topic")]
    Topic(&'a str),
    #[serde(rename = "purpose")]
    Purpose(&'a str),
    #[serde(rename = "name")]
    Name(&'a str),
}

#[derive(Debug, Serialize)]
struct MembersParams<'a> {
    channel: &'a str,
    limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<&'a str>,
}

/// Response returned by channel mutations that include a channel object.
#[derive(Debug, Deserialize)]
pub struct ChannelMutationResponse {
    /// The created or updated channel.
    pub channel: Channel,
}

#[derive(Debug, Deserialize)]
struct EmptyResponse {}

/// One page from `conversations.members`.
#[derive(Debug, Deserialize)]
pub struct ConversationsMembersResponse {
    /// User IDs in Slack's page order.
    #[serde(default)]
    pub members: Vec<String>,
    /// Cursor metadata for the next page.
    #[serde(default)]
    pub response_metadata: Option<ResponseMetadata>,
}

/// Unread-count fields returned inside `conversations.info`.
#[derive(Debug, Deserialize)]
pub struct UnreadCounts {
    /// Slack's display-oriented unread count, when exposed to the token.
    #[serde(default)]
    pub unread_count_display: Option<u64>,
    /// Slack's raw unread count, when exposed to the token.
    #[serde(default)]
    pub unread_count: Option<u64>,
}

/// Dedicated response wrapper for capability-dependent unread fields.
#[derive(Debug, Deserialize)]
pub struct ConversationsUnreadInfoResponse {
    /// Unread fields for the requested conversation.
    pub channel: UnreadCounts,
}

impl SlackClient {
    /// Fetch one page of channel member IDs.
    pub async fn conversations_members(
        &self,
        channel: &str,
        cursor: Option<&str>,
    ) -> Result<ConversationsMembersResponse> {
        self.request(
            "conversations.members",
            &MembersParams {
                channel,
                limit: 200,
                cursor,
            },
        )
        .await
    }

    /// Fetch all channel member IDs, preserving order and removing duplicates.
    pub async fn conversations_members_all(&self, channel: &str) -> Result<Vec<String>> {
        let mut members = Vec::new();
        let mut member_ids = HashSet::new();
        let mut seen_cursors = HashSet::new();
        let mut cursor: Option<String> = None;

        loop {
            let response = self
                .conversations_members(channel, cursor.as_deref())
                .await?;
            for member in response.members {
                if member_ids.insert(member.clone()) {
                    members.push(member);
                }
            }

            let next = response
                .response_metadata
                .and_then(|metadata| metadata.next_cursor)
                .filter(|value| !value.is_empty());
            match next {
                Some(next_cursor) => {
                    if !seen_cursors.insert(next_cursor.clone()) {
                        return Err(SlackError::Api {
                            error: "pagination_cursor_loop".to_string(),
                            detail: Some(
                                "conversations.members returned a repeated pagination cursor"
                                    .to_string(),
                            ),
                        });
                    }
                    cursor = Some(next_cursor);
                }
                None => break,
            }
        }

        Ok(members)
    }

    /// Create a public or private channel.
    pub async fn conversations_create(
        &self,
        name: &str,
        is_private: bool,
    ) -> Result<ChannelMutationResponse> {
        self.request("conversations.create", &CreateParams { name, is_private })
            .await
    }

    /// Join a public channel.
    pub async fn conversations_join(&self, channel: &str) -> Result<ChannelMutationResponse> {
        self.request("conversations.join", &ChannelParams { channel })
            .await
    }

    /// Leave a conversation.
    pub async fn conversations_leave(&self, channel: &str) -> Result<()> {
        let _: EmptyResponse = self
            .request("conversations.leave", &ChannelParams { channel })
            .await?;
        Ok(())
    }

    /// Archive a conversation.
    pub async fn conversations_archive(&self, channel: &str) -> Result<()> {
        let _: EmptyResponse = self
            .request("conversations.archive", &ChannelParams { channel })
            .await?;
        Ok(())
    }

    /// Restore an archived conversation.
    pub async fn conversations_unarchive(&self, channel: &str) -> Result<()> {
        let _: EmptyResponse = self
            .request("conversations.unarchive", &ChannelParams { channel })
            .await?;
        Ok(())
    }

    /// Invite a comma-separated set of user IDs to a conversation.
    pub async fn conversations_invite(
        &self,
        channel: &str,
        users: &str,
    ) -> Result<ChannelMutationResponse> {
        self.request("conversations.invite", &InviteParams { channel, users })
            .await
    }

    /// Set or clear a channel topic.
    pub async fn conversations_set_topic(
        &self,
        channel: &str,
        topic: &str,
    ) -> Result<ChannelMutationResponse> {
        self.request(
            "conversations.setTopic",
            &TextParams {
                channel,
                text: TextField::Topic(topic),
            },
        )
        .await
    }

    /// Set or clear a channel purpose.
    pub async fn conversations_set_purpose(
        &self,
        channel: &str,
        purpose: &str,
    ) -> Result<ChannelMutationResponse> {
        self.request(
            "conversations.setPurpose",
            &TextParams {
                channel,
                text: TextField::Purpose(purpose),
            },
        )
        .await
    }

    /// Rename a channel.
    pub async fn conversations_rename(
        &self,
        channel: &str,
        name: &str,
    ) -> Result<ChannelMutationResponse> {
        self.request(
            "conversations.rename",
            &TextParams {
                channel,
                text: TextField::Name(name),
            },
        )
        .await
    }

    /// Fetch unread-count fields without deserializing through the shared channel model.
    pub async fn conversations_info_unread(
        &self,
        channel: &str,
    ) -> Result<ConversationsUnreadInfoResponse> {
        self.request("conversations.info", &ChannelParams { channel })
            .await
    }
}
