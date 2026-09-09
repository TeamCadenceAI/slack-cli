//! Name resolution helpers for Slack IDs
//!
//! Resolves channel names to IDs and user names to IDs.

use crate::error::{Result, SlackError};
use crate::models::{Channel, User};

use super::client::SlackClient;
use super::types::PaginationParams;
use super::web::ConversationsListParams;

/// Check if a string looks like a Slack channel ID
///
/// Channel IDs start with C (public), D (DM), or G (private/group)
/// and are 9+ characters long.
fn is_channel_id(s: &str) -> bool {
    if s.len() < 9 {
        return false;
    }
    matches!(s.chars().next(), Some('C') | Some('D') | Some('G'))
}

/// Check if a string looks like a Slack user ID
///
/// User IDs start with U and are 9+ characters long.
fn is_user_id(s: &str) -> bool {
    if s.len() < 9 {
        return false;
    }
    matches!(s.chars().next(), Some('U'))
}

/// Return a normalized email address when the identifier has nonempty local
/// and domain components.
fn email_identifier(identifier: &str) -> Option<&str> {
    let trimmed = identifier.trim();
    let (local, domain) = trimmed.split_once('@')?;
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        None
    } else {
        Some(trimmed)
    }
}

impl SlackClient {
    /// Resolve a channel identifier to a channel ID
    ///
    /// If the identifier already looks like a channel ID (starts with C/D/G
    /// and is 9+ characters), it is returned as-is.
    ///
    /// A leading `@` or a valid user ID opens a direct-message conversation
    /// and returns its channel ID. Otherwise, the identifier is treated as a
    /// channel name. The leading `#` is stripped if present before searching.
    ///
    /// # Errors
    ///
    /// Returns `SlackError::ChannelNotFound` if the channel name cannot be resolved.
    pub async fn resolve_channel(&self, identifier: &str) -> Result<String> {
        // If it looks like a channel ID, return as-is
        if is_channel_id(identifier) {
            return Ok(identifier.to_string());
        }

        // A leading @ explicitly selects a user. A valid bare user ID is also
        // unambiguous; ordinary bare names remain channel names.
        let dm_target = if let Some(target) = identifier.strip_prefix('@') {
            let target = target.trim();
            if target.is_empty() {
                return Err(SlackError::Usage(
                    "direct-message target after @ cannot be empty".to_string(),
                ));
            }
            Some(target)
        } else if is_user_id(identifier) {
            Some(identifier)
        } else {
            None
        };

        if let Some(target) = dm_target {
            let user_id = self.resolve_user(target).await?;
            let response = self.conversations_open(&user_id).await?;
            return Ok(response.channel.id);
        }

        // Strip leading # if present
        let name = identifier.strip_prefix('#').unwrap_or(identifier);

        // Search through all channels (paginated)
        let mut cursor: Option<String> = None;

        loop {
            let mut params = ConversationsListParams::new()
                .with_limit(200)
                .exclude_archived(false)
                .with_types("public_channel,private_channel,mpim,im");

            if let Some(c) = cursor {
                params = params.with_cursor(c);
            }

            let response = self.conversations_list(params).await?;

            // Search for matching channel name
            for channel in &response.channels {
                if let Some(channel_name) = &channel.name {
                    if channel_name == name || channel_name == identifier {
                        return Ok(channel.id.clone());
                    }
                }
            }

            // Check for more pages
            cursor = response
                .response_metadata
                .and_then(|m| m.next_cursor)
                .filter(|c| !c.is_empty());

            if cursor.is_none() {
                break;
            }
        }

        Err(SlackError::ChannelNotFound(identifier.to_string()))
    }

    /// Resolve a channel identifier and return the full Channel object
    ///
    /// Similar to `resolve_channel`, but returns the full Channel object
    /// including all metadata from conversations.info.
    pub async fn resolve_channel_info(&self, identifier: &str) -> Result<Channel> {
        let channel_id = self.resolve_channel(identifier).await?;
        self.conversations_info(&channel_id).await
    }

    /// Resolve a user identifier to a user ID
    ///
    /// If the identifier already looks like a user ID (starts with U
    /// and is 9+ characters), it is returned as-is.
    ///
    /// Email addresses are resolved with `users.lookupByEmail`. Otherwise, the
    /// identifier is treated as a username. The leading `@` is stripped if
    /// present, and `users.list` is searched for `name` or
    /// `profile.display_name`.
    ///
    /// # Errors
    ///
    /// Returns `SlackError::UserNotFound` if the username cannot be resolved.
    pub async fn resolve_user(&self, identifier: &str) -> Result<String> {
        // If it looks like a user ID, return as-is
        if is_user_id(identifier) {
            return Ok(identifier.to_string());
        }

        if let Some(email) = email_identifier(identifier) {
            return Ok(self.users_lookup_by_email(email).await?.id);
        }

        // Strip leading @ if present
        let name = identifier.strip_prefix('@').unwrap_or(identifier);
        let name_lower = name.to_lowercase();

        // Search through all users (paginated)
        let mut cursor: Option<String> = None;

        loop {
            let mut params = PaginationParams::new().with_limit(200);

            if let Some(c) = cursor {
                params = params.with_cursor(c);
            }

            let response = self.users_list(params).await?;

            // Search for matching username or display name
            for user in &response.members {
                // Match on name
                if let Some(user_name) = &user.name {
                    if user_name.to_lowercase() == name_lower {
                        return Ok(user.id.clone());
                    }
                }

                // Match on profile display name
                if let Some(profile) = &user.profile {
                    if let Some(display_name) = &profile.display_name {
                        if !display_name.is_empty() && display_name.to_lowercase() == name_lower {
                            return Ok(user.id.clone());
                        }
                    }
                }
            }

            // Check for more pages
            cursor = response
                .response_metadata
                .and_then(|m| m.next_cursor)
                .filter(|c| !c.is_empty());

            if cursor.is_none() {
                break;
            }
        }

        Err(SlackError::UserNotFound(identifier.to_string()))
    }

    /// Resolve a user identifier and return the full User object
    ///
    /// Similar to `resolve_user`, but returns the full user object. Email
    /// lookups return the `users.lookupByEmail` user directly; other
    /// identifiers are fetched with `users.info` after resolution.
    pub async fn resolve_user_info(&self, identifier: &str) -> Result<User> {
        if let Some(email) = email_identifier(identifier) {
            return self.users_lookup_by_email(email).await;
        }

        let user_id = self.resolve_user(identifier).await?;
        self.users_info(&user_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_channel_id_public() {
        assert!(is_channel_id("C123456789"));
        assert!(is_channel_id("C1234567890"));
        assert!(is_channel_id("CAAAABBBB"));
    }

    #[test]
    fn test_is_channel_id_dm() {
        assert!(is_channel_id("D123456789"));
        assert!(is_channel_id("D1234567890"));
    }

    #[test]
    fn test_is_channel_id_private() {
        assert!(is_channel_id("G123456789"));
        assert!(is_channel_id("G1234567890"));
    }

    #[test]
    fn test_is_channel_id_too_short() {
        assert!(!is_channel_id("C12345"));
        assert!(!is_channel_id("D12"));
        assert!(!is_channel_id("G"));
        assert!(!is_channel_id(""));
    }

    #[test]
    fn test_is_channel_id_wrong_prefix() {
        assert!(!is_channel_id("U123456789")); // User ID
        assert!(!is_channel_id("T123456789")); // Team ID
        assert!(!is_channel_id("general")); // Name
        assert!(!is_channel_id("#general")); // Name with hash
    }

    #[test]
    fn test_is_user_id_valid() {
        assert!(is_user_id("U123456789"));
        assert!(is_user_id("U1234567890"));
        assert!(is_user_id("UAAAABBBB"));
    }

    #[test]
    fn test_is_user_id_too_short() {
        assert!(!is_user_id("U12345"));
        assert!(!is_user_id("U"));
        assert!(!is_user_id(""));
    }

    #[test]
    fn test_is_user_id_wrong_prefix() {
        assert!(!is_user_id("C123456789")); // Channel ID
        assert!(!is_user_id("T123456789")); // Team ID
        assert!(!is_user_id("johndoe")); // Name
        assert!(!is_user_id("@johndoe")); // Name with @
    }

    #[test]
    fn test_email_identifier() {
        assert_eq!(
            email_identifier(" alice+ops@example.com "),
            Some("alice+ops@example.com")
        );
        assert_eq!(email_identifier("@example.com"), None);
        assert_eq!(email_identifier("alice@"), None);
        assert_eq!(email_identifier("alice"), None);
        assert_eq!(email_identifier("a@b@example.com"), None);
    }
}
