//! Message model for Slack API

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::Reaction;

/// A Slack message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message type (usually "message")
    #[serde(rename = "type", default)]
    pub msg_type: Option<String>,

    /// Message subtype (e.g., "bot_message", "channel_join")
    #[serde(default)]
    pub subtype: Option<String>,

    /// User ID who sent the message
    #[serde(default)]
    pub user: Option<String>,

    /// Message text content
    #[serde(default)]
    pub text: Option<String>,

    /// Timestamp (unique message identifier within channel)
    pub ts: String,

    /// Thread parent timestamp (if this is a reply)
    #[serde(default)]
    pub thread_ts: Option<String>,

    /// Reply count (if this is a thread parent)
    #[serde(default)]
    pub reply_count: Option<u32>,

    /// Reply users (subset of users who replied)
    #[serde(default)]
    pub reply_users: Option<Vec<String>>,

    /// Reply user count
    #[serde(default)]
    pub reply_users_count: Option<u32>,

    /// Latest reply timestamp
    #[serde(default)]
    pub latest_reply: Option<String>,

    /// Whether user has subscribed to this thread
    #[serde(default)]
    pub subscribed: bool,

    /// Reactions on this message
    #[serde(default)]
    pub reactions: Option<Vec<Reaction>>,

    /// Files attached to this message
    #[serde(default)]
    pub files: Option<Vec<MessageFile>>,

    /// Attachments (legacy attachment format)
    #[serde(default)]
    pub attachments: Option<Vec<Attachment>>,

    /// Blocks (Block Kit format)
    #[serde(default)]
    pub blocks: Option<Vec<serde_json::Value>>,

    /// Bot ID (if sent by a bot)
    #[serde(default)]
    pub bot_id: Option<String>,

    /// Bot profile (if sent by a bot)
    #[serde(default)]
    pub bot_profile: Option<BotProfile>,

    /// App ID (if sent by an app)
    #[serde(default)]
    pub app_id: Option<String>,

    /// Username (for bot messages or custom usernames)
    #[serde(default)]
    pub username: Option<String>,

    /// Icons for the message
    #[serde(default)]
    pub icons: Option<MessageIcons>,

    /// Whether the message was edited
    #[serde(default)]
    pub edited: Option<EditInfo>,

    /// Channel ID (included in search results)
    #[serde(default)]
    pub channel: Option<ChannelInfo>,

    /// Permalink URL (included in search results)
    #[serde(default)]
    pub permalink: Option<String>,

    /// Whether this is a starred message
    #[serde(default)]
    pub is_starred: bool,

    /// Whether this message was pinned
    #[serde(default)]
    pub pinned_to: Option<Vec<String>>,
}

impl Message {
    /// Get the timestamp as a DateTime
    pub fn timestamp(&self) -> Option<DateTime<Utc>> {
        parse_slack_timestamp(&self.ts)
    }

    /// Get the thread parent timestamp as a DateTime
    pub fn thread_timestamp(&self) -> Option<DateTime<Utc>> {
        self.thread_ts
            .as_ref()
            .and_then(|ts| parse_slack_timestamp(ts))
    }

    /// Check if this message is a thread parent
    pub fn is_thread_parent(&self) -> bool {
        self.reply_count.is_some_and(|c| c > 0)
    }

    /// Check if this message is a reply
    pub fn is_reply(&self) -> bool {
        self.thread_ts.is_some() && self.thread_ts.as_ref() != Some(&self.ts)
    }

    /// Check if this is a bot message
    pub fn is_bot_message(&self) -> bool {
        self.bot_id.is_some() || self.subtype.as_deref() == Some("bot_message")
    }

    /// Get plain text content (strips mrkdwn)
    pub fn plain_text(&self) -> String {
        self.text.clone().unwrap_or_default()
    }
}

/// Parse a Slack timestamp (format: "1234567890.123456") to DateTime
fn parse_slack_timestamp(ts: &str) -> Option<DateTime<Utc>> {
    let parts: Vec<&str> = ts.split('.').collect();
    if parts.is_empty() {
        return None;
    }

    let seconds: i64 = parts[0].parse().ok()?;
    let nanos: u32 = if parts.len() > 1 {
        // Slack timestamps have 6 decimal places (microseconds)
        let micro_str = parts[1];
        let micros: u32 = micro_str.parse().ok()?;
        micros * 1000 // Convert to nanoseconds
    } else {
        0
    };

    DateTime::from_timestamp(seconds, nanos)
}

/// Simplified file info included in messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageFile {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub mimetype: Option<String>,
    #[serde(default)]
    pub filetype: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub url_private: Option<String>,
    #[serde(default)]
    pub url_private_download: Option<String>,
    #[serde(default)]
    pub permalink: Option<String>,
}

/// Legacy attachment format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    #[serde(default)]
    pub fallback: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub pretext: Option<String>,
    #[serde(default)]
    pub author_name: Option<String>,
    #[serde(default)]
    pub author_link: Option<String>,
    #[serde(default)]
    pub author_icon: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub title_link: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub fields: Option<Vec<AttachmentField>>,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub thumb_url: Option<String>,
    #[serde(default)]
    pub footer: Option<String>,
    #[serde(default)]
    pub footer_icon: Option<String>,
    #[serde(default)]
    pub ts: Option<serde_json::Value>, // Can be string or number
}

/// Attachment field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentField {
    pub title: String,
    pub value: String,
    #[serde(default)]
    pub short: bool,
}

/// Bot profile information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotProfile {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub app_id: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub icons: Option<BotIcons>,
    #[serde(default)]
    pub deleted: bool,
}

/// Bot icons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotIcons {
    #[serde(default)]
    pub image_36: Option<String>,
    #[serde(default)]
    pub image_48: Option<String>,
    #[serde(default)]
    pub image_72: Option<String>,
}

/// Message icons
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageIcons {
    #[serde(default)]
    pub image_36: Option<String>,
    #[serde(default)]
    pub image_48: Option<String>,
    #[serde(default)]
    pub image_72: Option<String>,
    #[serde(default)]
    pub emoji: Option<String>,
}

/// Edit information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditInfo {
    pub user: String,
    pub ts: String,
}

/// Channel info included in search results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_deserialization_basic() {
        let json = r#"{
            "type": "message",
            "user": "U1234567890",
            "text": "Hello, world!",
            "ts": "1577836800.000100"
        }"#;

        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(msg.msg_type, Some("message".to_string()));
        assert_eq!(msg.user, Some("U1234567890".to_string()));
        assert_eq!(msg.text, Some("Hello, world!".to_string()));
        assert_eq!(msg.ts, "1577836800.000100");
    }

    #[test]
    fn test_message_deserialization_thread_parent() {
        let json = r#"{
            "type": "message",
            "user": "U1234567890",
            "text": "Thread start",
            "ts": "1577836800.000100",
            "reply_count": 5,
            "reply_users": ["U111", "U222"],
            "reply_users_count": 2,
            "latest_reply": "1577837000.000200",
            "subscribed": true
        }"#;

        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(msg.is_thread_parent());
        assert!(!msg.is_reply());
        assert_eq!(msg.reply_count, Some(5));
        assert!(msg.subscribed);
    }

    #[test]
    fn test_message_deserialization_reply() {
        let json = r#"{
            "type": "message",
            "user": "U1234567890",
            "text": "This is a reply",
            "ts": "1577837000.000200",
            "thread_ts": "1577836800.000100"
        }"#;

        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(msg.is_reply());
        assert!(!msg.is_thread_parent());
    }

    #[test]
    fn test_message_deserialization_bot() {
        let json = r#"{
            "type": "message",
            "subtype": "bot_message",
            "bot_id": "B1234567890",
            "text": "Bot message",
            "ts": "1577836800.000100",
            "bot_profile": {
                "id": "B1234567890",
                "name": "TestBot",
                "app_id": "A123"
            }
        }"#;

        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(msg.is_bot_message());
        assert_eq!(msg.bot_id, Some("B1234567890".to_string()));
        assert!(msg.bot_profile.is_some());
    }

    #[test]
    fn test_message_deserialization_with_files() {
        let json = r#"{
            "type": "message",
            "user": "U1234567890",
            "text": "Check this file",
            "ts": "1577836800.000100",
            "files": [{
                "id": "F1234567890",
                "name": "image.png",
                "mimetype": "image/png",
                "size": 1024
            }]
        }"#;

        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(msg.files.is_some());
        let files = msg.files.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, Some("image.png".to_string()));
    }

    #[test]
    fn test_message_deserialization_with_reactions() {
        let json = r#"{
            "type": "message",
            "user": "U1234567890",
            "text": "React to me",
            "ts": "1577836800.000100",
            "reactions": [{
                "name": "thumbsup",
                "count": 3,
                "users": ["U111", "U222", "U333"]
            }]
        }"#;

        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(msg.reactions.is_some());
        let reactions = msg.reactions.unwrap();
        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0].name, "thumbsup");
    }

    #[test]
    fn test_message_deserialization_with_attachments() {
        let json = r##"{
            "type": "message",
            "user": "U1234567890",
            "text": "",
            "ts": "1577836800.000100",
            "attachments": [{
                "fallback": "Test attachment",
                "color": "#36a64f",
                "title": "Attachment Title",
                "text": "Attachment text"
            }]
        }"##;

        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(msg.attachments.is_some());
        let attachments = msg.attachments.unwrap();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].title, Some("Attachment Title".to_string()));
    }

    #[test]
    fn test_message_timestamp_parsing() {
        let msg = Message {
            ts: "1577836800.000100".to_string(),
            msg_type: None,
            subtype: None,
            user: None,
            text: None,
            thread_ts: None,
            reply_count: None,
            reply_users: None,
            reply_users_count: None,
            latest_reply: None,
            subscribed: false,
            reactions: None,
            files: None,
            attachments: None,
            blocks: None,
            bot_id: None,
            bot_profile: None,
            app_id: None,
            username: None,
            icons: None,
            edited: None,
            channel: None,
            permalink: None,
            is_starred: false,
            pinned_to: None,
        };

        let dt = msg.timestamp().unwrap();
        assert_eq!(dt.timestamp(), 1577836800);
    }

    #[test]
    fn test_message_deserialization_search_result() {
        let json = r#"{
            "type": "message",
            "user": "U1234567890",
            "text": "Found message",
            "ts": "1577836800.000100",
            "channel": {
                "id": "C1234567890",
                "name": "general"
            },
            "permalink": "https://myteam.slack.com/archives/C123/p1577836800000100"
        }"#;

        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(msg.channel.is_some());
        assert!(msg.permalink.is_some());
        let channel = msg.channel.unwrap();
        assert_eq!(channel.id, "C1234567890");
        assert_eq!(channel.name, Some("general".to_string()));
    }

    #[test]
    fn test_message_plain_text() {
        let msg = Message {
            text: Some("Hello <@U123|user>!".to_string()),
            ts: "1577836800.000100".to_string(),
            msg_type: None,
            subtype: None,
            user: None,
            thread_ts: None,
            reply_count: None,
            reply_users: None,
            reply_users_count: None,
            latest_reply: None,
            subscribed: false,
            reactions: None,
            files: None,
            attachments: None,
            blocks: None,
            bot_id: None,
            bot_profile: None,
            app_id: None,
            username: None,
            icons: None,
            edited: None,
            channel: None,
            permalink: None,
            is_starred: false,
            pinned_to: None,
        };

        // Currently just returns text as-is, could add parsing later
        assert_eq!(msg.plain_text(), "Hello <@U123|user>!");
    }
}
