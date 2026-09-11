//! Slack Web API operations for pins and custom emoji.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;

use super::client::SlackClient;

#[derive(Debug, Serialize)]
struct PinParams<'a> {
    channel: &'a str,
    timestamp: &'a str,
}

#[derive(Debug, Serialize)]
struct ChannelParams<'a> {
    channel: &'a str,
}

#[derive(Debug, Serialize)]
struct EmptyParams {}

#[derive(Debug, Deserialize)]
struct PinMutationResponse {}

/// One item returned by `pins.list`.
///
/// Pin payloads vary by item type. Keeping the raw object means message, file,
/// file-comment, and future item types retain all metadata returned by Slack.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PinItem(pub serde_json::Value);

/// Response from `pins.list`.
#[derive(Debug, Serialize, Deserialize)]
pub struct PinsListResponse {
    /// Pin items in Slack API order.
    #[serde(default)]
    pub items: Vec<PinItem>,
}

/// Response from `emoji.list`.
#[derive(Debug, Serialize, Deserialize)]
pub struct EmojiListResponse {
    /// Custom emoji names mapped to image URLs or `alias:<name>` values.
    #[serde(default)]
    pub emoji: BTreeMap<String, String>,
}

impl SlackClient {
    /// Add a pin to a message.
    pub async fn pins_add(&self, channel: &str, timestamp: &str) -> Result<()> {
        let _: PinMutationResponse = self
            .request("pins.add", &PinParams { channel, timestamp })
            .await?;
        Ok(())
    }

    /// Remove a pin from a message.
    pub async fn pins_remove(&self, channel: &str, timestamp: &str) -> Result<()> {
        let _: PinMutationResponse = self
            .request("pins.remove", &PinParams { channel, timestamp })
            .await?;
        Ok(())
    }

    /// List all pins in a channel.
    pub async fn pins_list(&self, channel: &str) -> Result<PinsListResponse> {
        self.request("pins.list", &ChannelParams { channel }).await
    }

    /// List custom emoji for the workspace.
    pub async fn emoji_list(&self) -> Result<EmojiListResponse> {
        self.request("emoji.list", &EmptyParams {}).await
    }
}
