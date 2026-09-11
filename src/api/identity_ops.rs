//! Slack Web API operations used for identity and user-group commands.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::models::{Channel, User};

use super::client::SlackClient;

/// Parameters for `conversations.open`.
#[derive(Debug, Serialize)]
pub struct ConversationsOpenParams<'a> {
    /// Comma-separated user IDs. Direct-message resolution supplies one ID.
    pub users: &'a str,
}

/// Response from `conversations.open`.
#[derive(Debug, Deserialize)]
pub struct ConversationsOpenResponse {
    /// The opened or existing direct-message conversation.
    pub channel: Channel,
}

/// Parameters for `users.lookupByEmail`.
#[derive(Debug, Serialize)]
pub struct UsersLookupByEmailParams<'a> {
    /// Email address to look up.
    pub email: &'a str,
}

/// Response from `users.lookupByEmail`.
#[derive(Debug, Deserialize)]
pub struct UsersLookupByEmailResponse {
    /// Matching user.
    pub user: User,
}

/// Parameters for `usergroups.list`.
#[derive(Debug, Serialize)]
pub struct UsergroupsListParams {
    /// Include disabled user groups.
    pub include_disabled: bool,
    /// Include each group's member count.
    pub include_count: bool,
    /// Include the member ID array on each group.
    pub include_users: bool,
}

/// A Slack user group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usergroup {
    /// User-group ID.
    pub id: String,
    /// User-group handle, without `@`.
    #[serde(default)]
    pub handle: String,
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Number of users, when requested.
    #[serde(default, deserialize_with = "deserialize_user_count")]
    pub user_count: u64,
    /// Additional fields returned by Slack.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn deserialize_user_count<'de, D>(deserializer: D) -> std::result::Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Null => Ok(0),
        serde_json::Value::Number(value) => value
            .as_u64()
            .ok_or_else(|| D::Error::custom("user_count must be a nonnegative integer")),
        serde_json::Value::String(value) => value.parse().map_err(D::Error::custom),
        _ => Err(D::Error::custom("user_count must be an integer or string")),
    }
}

/// Response from `usergroups.list`.
#[derive(Debug, Deserialize)]
pub struct UsergroupsListResponse {
    /// Workspace user groups.
    #[serde(default)]
    pub usergroups: Vec<Usergroup>,
}

/// Parameters for `usergroups.users.list`.
#[derive(Debug, Serialize)]
pub struct UsergroupsUsersListParams<'a> {
    /// User-group ID.
    pub usergroup: &'a str,
    /// Include disabled users.
    pub include_disabled: bool,
}

/// Response from `usergroups.users.list`.
#[derive(Debug, Deserialize)]
pub struct UsergroupsUsersListResponse {
    /// IDs of users in the group.
    #[serde(default)]
    pub users: Vec<String>,
}

impl SlackClient {
    /// Open or find a direct-message conversation with one user.
    pub async fn conversations_open(&self, user_id: &str) -> Result<ConversationsOpenResponse> {
        self.request(
            "conversations.open",
            &ConversationsOpenParams { users: user_id },
        )
        .await
    }

    /// Look up a user by email address.
    pub async fn users_lookup_by_email(&self, email: &str) -> Result<User> {
        let response: UsersLookupByEmailResponse = self
            .request("users.lookupByEmail", &UsersLookupByEmailParams { email })
            .await?;
        Ok(response.user)
    }

    /// List workspace user groups.
    pub async fn usergroups_list(
        &self,
        params: UsergroupsListParams,
    ) -> Result<UsergroupsListResponse> {
        self.request("usergroups.list", &params).await
    }

    /// List member IDs for a user group.
    pub async fn usergroups_users_list(
        &self,
        params: UsergroupsUsersListParams<'_>,
    ) -> Result<UsergroupsUsersListResponse> {
        self.request("usergroups.users.list", &params).await
    }
}
