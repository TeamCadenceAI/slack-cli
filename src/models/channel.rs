//! Channel model for Slack API

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A Slack channel (public, private, or DM)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Channel {
    /// Channel ID (e.g., "C1234567890")
    pub id: String,

    /// Channel name (without # prefix)
    #[serde(default)]
    pub name: Option<String>,

    /// Formatted name for display
    #[serde(default)]
    pub name_normalized: Option<String>,

    /// Whether this is a public channel
    #[serde(default)]
    pub is_channel: bool,

    /// Whether this is a private channel
    #[serde(default)]
    pub is_group: bool,

    /// Whether this is a direct message
    #[serde(default)]
    pub is_im: bool,

    /// Whether this is a multi-party direct message
    #[serde(default)]
    pub is_mpim: bool,

    /// Whether the channel is private
    #[serde(default)]
    pub is_private: bool,

    /// Whether the channel is archived
    #[serde(default)]
    pub is_archived: bool,

    /// Whether the user is a member of this channel
    #[serde(default)]
    pub is_member: bool,

    /// Whether this is a shared channel
    #[serde(default)]
    pub is_shared: bool,

    /// Whether this is an external shared channel
    #[serde(default)]
    pub is_ext_shared: bool,

    /// Whether this is the general channel
    #[serde(default)]
    pub is_general: bool,

    /// Channel creator ID
    #[serde(default)]
    pub creator: Option<String>,

    /// Unix timestamp when the channel was created
    #[serde(default)]
    pub created: Option<i64>,

    /// Number of members in the channel
    #[serde(default)]
    pub num_members: Option<u32>,

    /// Channel topic
    #[serde(default)]
    pub topic: Option<ChannelTopic>,

    /// Channel purpose
    #[serde(default)]
    pub purpose: Option<ChannelPurpose>,

    /// User ID for direct messages
    #[serde(default)]
    pub user: Option<String>,

    /// Whether the channel is open (for DMs)
    #[serde(default)]
    pub is_open: Option<bool>,

    /// Priority for sorting
    #[serde(default)]
    pub priority: Option<f64>,
}

impl Channel {
    /// Get the display name for this channel
    pub fn display_name(&self) -> String {
        if self.is_im {
            self.user.clone().unwrap_or_else(|| "DM".to_string())
        } else {
            self.name.clone().unwrap_or_else(|| self.id.clone())
        }
    }

    /// Get the channel type as a string
    pub fn channel_type(&self) -> &'static str {
        if self.is_im {
            "dm"
        } else if self.is_mpim {
            "mpim"
        } else if self.is_private || self.is_group {
            "private"
        } else {
            "public"
        }
    }

    /// Get the created timestamp as DateTime
    pub fn created_at(&self) -> Option<DateTime<Utc>> {
        self.created
            .map(|ts| DateTime::from_timestamp(ts, 0).unwrap_or_default())
    }
}

/// Channel topic
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelTopic {
    /// The topic text
    pub value: String,
    /// Who set the topic
    #[serde(default)]
    pub creator: Option<String>,
    /// When the topic was set (Unix timestamp)
    #[serde(default)]
    pub last_set: Option<i64>,
}

/// Channel purpose
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelPurpose {
    /// The purpose text
    pub value: String,
    /// Who set the purpose
    #[serde(default)]
    pub creator: Option<String>,
    /// When the purpose was set (Unix timestamp)
    #[serde(default)]
    pub last_set: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_deserialization_public() {
        let json = r#"{
            "id": "C1234567890",
            "name": "general",
            "name_normalized": "general",
            "is_channel": true,
            "is_private": false,
            "is_member": true,
            "is_general": true,
            "creator": "U1234567890",
            "created": 1577836800,
            "num_members": 42,
            "topic": {
                "value": "Company-wide announcements",
                "creator": "U1234567890",
                "last_set": 1577836800
            },
            "purpose": {
                "value": "This channel is for team-wide communication",
                "creator": "U1234567890",
                "last_set": 1577836800
            }
        }"#;

        let channel: Channel = serde_json::from_str(json).unwrap();
        assert_eq!(channel.id, "C1234567890");
        assert_eq!(channel.name, Some("general".to_string()));
        assert!(channel.is_channel);
        assert!(!channel.is_private);
        assert!(channel.is_member);
        assert!(channel.is_general);
        assert_eq!(channel.num_members, Some(42));
        assert!(channel.topic.is_some());
        assert!(channel.purpose.is_some());
    }

    #[test]
    fn test_channel_deserialization_private() {
        let json = r#"{
            "id": "G1234567890",
            "name": "secret-project",
            "is_group": true,
            "is_private": true,
            "is_member": true
        }"#;

        let channel: Channel = serde_json::from_str(json).unwrap();
        assert_eq!(channel.id, "G1234567890");
        assert!(channel.is_group);
        assert!(channel.is_private);
        assert_eq!(channel.channel_type(), "private");
    }

    #[test]
    fn test_channel_deserialization_dm() {
        let json = r#"{
            "id": "D1234567890",
            "is_im": true,
            "user": "U9876543210",
            "is_open": true
        }"#;

        let channel: Channel = serde_json::from_str(json).unwrap();
        assert_eq!(channel.id, "D1234567890");
        assert!(channel.is_im);
        assert_eq!(channel.user, Some("U9876543210".to_string()));
        assert_eq!(channel.channel_type(), "dm");
    }

    #[test]
    fn test_channel_deserialization_mpim() {
        let json = r#"{
            "id": "G1234567890",
            "name": "mpdm-user1--user2--user3-1",
            "is_mpim": true,
            "is_group": true
        }"#;

        let channel: Channel = serde_json::from_str(json).unwrap();
        assert!(channel.is_mpim);
        assert_eq!(channel.channel_type(), "mpim");
    }

    #[test]
    fn test_channel_display_name() {
        let public = Channel {
            id: "C123".to_string(),
            name: Some("general".to_string()),
            is_im: false,
            ..Default::default()
        };
        assert_eq!(public.display_name(), "general");

        let dm = Channel {
            id: "D123".to_string(),
            name: None,
            is_im: true,
            user: Some("U456".to_string()),
            ..Default::default()
        };
        assert_eq!(dm.display_name(), "U456");
    }

    #[test]
    fn test_channel_created_at() {
        let channel = Channel {
            id: "C123".to_string(),
            created: Some(1577836800),
            ..Default::default()
        };
        let dt = channel.created_at().unwrap();
        assert_eq!(dt.timestamp(), 1577836800);
    }

    #[test]
    fn test_channel_minimal() {
        // Test that minimal JSON (only id) deserializes
        let json = r#"{"id": "C123"}"#;
        let channel: Channel = serde_json::from_str(json).unwrap();
        assert_eq!(channel.id, "C123");
        assert!(channel.name.is_none());
        assert!(!channel.is_channel);
    }
}
