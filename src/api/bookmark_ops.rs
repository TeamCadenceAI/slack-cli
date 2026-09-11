//! Slack Web API operations for channel bookmarks.

use serde::{Deserialize, Serialize};

use crate::error::Result;

use super::client::SlackClient;

#[derive(Debug, Serialize)]
struct BookmarkListParams<'a> {
    channel_id: &'a str,
}

#[derive(Debug, Serialize)]
struct BookmarkAddParams<'a> {
    channel_id: &'a str,
    title: &'a str,
    #[serde(rename = "type")]
    bookmark_type: &'static str,
    link: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    emoji: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct BookmarkRemoveParams<'a> {
    channel_id: &'a str,
    bookmark_id: &'a str,
}

#[derive(Debug, Deserialize)]
struct BookmarkRemoveResponse {}

/// A channel bookmark returned by Slack.
///
/// Slack may add type-specific or workspace-specific fields to bookmark
/// objects. Unknown fields are retained so JSON output does not discard that
/// metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    /// Bookmark ID.
    pub id: String,
    /// Bookmark title.
    pub title: String,
    /// Bookmark destination URL.
    pub link: String,
    /// Bookmark emoji, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    /// Additional metadata returned by Slack.
    #[serde(flatten)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

/// Response from `bookmarks.list`.
#[derive(Debug, Serialize, Deserialize)]
pub struct BookmarksListResponse {
    /// Bookmarks in Slack API order.
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
}

/// Response from `bookmarks.add`.
#[derive(Debug, Serialize, Deserialize)]
pub struct BookmarksAddResponse {
    /// The newly created bookmark.
    pub bookmark: Bookmark,
}

impl SlackClient {
    /// List all bookmarks in a channel.
    pub async fn bookmarks_list(&self, channel_id: &str) -> Result<BookmarksListResponse> {
        self.request("bookmarks.list", &BookmarkListParams { channel_id })
            .await
    }

    /// Add a link bookmark to a channel.
    pub async fn bookmarks_add(
        &self,
        channel_id: &str,
        title: &str,
        link: &str,
        emoji: Option<&str>,
    ) -> Result<BookmarksAddResponse> {
        self.request(
            "bookmarks.add",
            &BookmarkAddParams {
                channel_id,
                title,
                bookmark_type: "link",
                link,
                emoji,
            },
        )
        .await
    }

    /// Remove a bookmark from a channel.
    pub async fn bookmarks_remove(&self, channel_id: &str, bookmark_id: &str) -> Result<()> {
        let _: BookmarkRemoveResponse = self
            .request(
                "bookmarks.remove",
                &BookmarkRemoveParams {
                    channel_id,
                    bookmark_id,
                },
            )
            .await?;
        Ok(())
    }
}
