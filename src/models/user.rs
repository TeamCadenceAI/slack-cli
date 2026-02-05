//! User model for Slack API

use serde::{Deserialize, Serialize};

/// A Slack user
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct User {
    /// User ID (e.g., "U1234567890")
    pub id: String,

    /// Team/workspace ID
    #[serde(default)]
    pub team_id: Option<String>,

    /// Username (without @)
    #[serde(default)]
    pub name: Option<String>,

    /// Whether this user has been deleted/deactivated
    #[serde(default)]
    pub deleted: bool,

    /// Display color (hex without #)
    #[serde(default)]
    pub color: Option<String>,

    /// Real name
    #[serde(default)]
    pub real_name: Option<String>,

    /// Timezone identifier (e.g., "America/Los_Angeles")
    #[serde(default)]
    pub tz: Option<String>,

    /// Timezone label (e.g., "Pacific Standard Time")
    #[serde(default)]
    pub tz_label: Option<String>,

    /// Timezone offset in seconds
    #[serde(default)]
    pub tz_offset: Option<i32>,

    /// User profile information
    #[serde(default)]
    pub profile: Option<UserProfile>,

    /// Whether this user is an admin
    #[serde(default)]
    pub is_admin: bool,

    /// Whether this user is an owner
    #[serde(default)]
    pub is_owner: bool,

    /// Whether this user is the primary owner
    #[serde(default)]
    pub is_primary_owner: bool,

    /// Whether this user is restricted
    #[serde(default)]
    pub is_restricted: bool,

    /// Whether this user is ultra restricted (single-channel guest)
    #[serde(default)]
    pub is_ultra_restricted: bool,

    /// Whether this user is a bot
    #[serde(default)]
    pub is_bot: bool,

    /// Whether this user is an app user
    #[serde(default)]
    pub is_app_user: bool,

    /// Unix timestamp when the user was last updated
    #[serde(default)]
    pub updated: Option<i64>,

    /// Whether 2FA is enabled
    #[serde(default)]
    pub has_2fa: bool,

    /// Enterprise user info
    #[serde(default)]
    pub enterprise_user: Option<EnterpriseUser>,

    /// Whether this is a Slack-internal account
    #[serde(default)]
    pub is_stranger: Option<bool>,
}

impl User {
    /// Get the display name (real name or username)
    pub fn display_name(&self) -> String {
        if let Some(profile) = &self.profile {
            if let Some(dn) = &profile.display_name {
                if !dn.is_empty() {
                    return dn.clone();
                }
            }
            if let Some(rn) = &profile.real_name {
                if !rn.is_empty() {
                    return rn.clone();
                }
            }
        }
        if let Some(rn) = &self.real_name {
            if !rn.is_empty() {
                return rn.clone();
            }
        }
        self.name.clone().unwrap_or_else(|| self.id.clone())
    }

    /// Get the user's status emoji
    pub fn status_emoji(&self) -> Option<&str> {
        self.profile
            .as_ref()
            .and_then(|p| p.status_emoji.as_deref())
            .filter(|s| !s.is_empty())
    }

    /// Get the user's status text
    pub fn status_text(&self) -> Option<&str> {
        self.profile
            .as_ref()
            .and_then(|p| p.status_text.as_deref())
            .filter(|s| !s.is_empty())
    }

    /// Check if the user is active (not deleted)
    pub fn is_active(&self) -> bool {
        !self.deleted
    }

    /// Check if the user is a guest
    pub fn is_guest(&self) -> bool {
        self.is_restricted || self.is_ultra_restricted
    }
}

/// User profile information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserProfile {
    /// User's title/job position
    #[serde(default)]
    pub title: Option<String>,

    /// Phone number
    #[serde(default)]
    pub phone: Option<String>,

    /// Skype handle
    #[serde(default)]
    pub skype: Option<String>,

    /// Real name
    #[serde(default)]
    pub real_name: Option<String>,

    /// Normalized real name
    #[serde(default)]
    pub real_name_normalized: Option<String>,

    /// Display name
    #[serde(default)]
    pub display_name: Option<String>,

    /// Normalized display name
    #[serde(default)]
    pub display_name_normalized: Option<String>,

    /// Custom fields
    #[serde(default)]
    pub fields: Option<serde_json::Value>,

    /// Status text
    #[serde(default)]
    pub status_text: Option<String>,

    /// Status emoji
    #[serde(default)]
    pub status_emoji: Option<String>,

    /// Status expiration (Unix timestamp)
    #[serde(default)]
    pub status_expiration: Option<i64>,

    /// Avatar hash
    #[serde(default)]
    pub avatar_hash: Option<String>,

    /// Whether a custom image was uploaded
    #[serde(default)]
    pub image_original: Option<String>,

    /// Whether email is confirmed
    #[serde(default)]
    pub is_custom_image: bool,

    /// Email address
    #[serde(default)]
    pub email: Option<String>,

    /// First name
    #[serde(default)]
    pub first_name: Option<String>,

    /// Last name
    #[serde(default)]
    pub last_name: Option<String>,

    /// Avatar URLs at various sizes
    #[serde(default)]
    pub image_24: Option<String>,
    #[serde(default)]
    pub image_32: Option<String>,
    #[serde(default)]
    pub image_48: Option<String>,
    #[serde(default)]
    pub image_72: Option<String>,
    #[serde(default)]
    pub image_192: Option<String>,
    #[serde(default)]
    pub image_512: Option<String>,
    #[serde(default)]
    pub image_1024: Option<String>,

    /// Team ID
    #[serde(default)]
    pub team: Option<String>,

    /// Huddles status
    #[serde(default)]
    pub huddle_state: Option<String>,
    #[serde(default)]
    pub huddle_state_expiration_ts: Option<i64>,
}

/// Enterprise user information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnterpriseUser {
    /// Enterprise ID
    pub id: String,
    /// Enterprise name
    #[serde(default)]
    pub enterprise_id: Option<String>,
    /// Enterprise name
    #[serde(default)]
    pub enterprise_name: Option<String>,
    /// Whether user is admin of enterprise
    #[serde(default)]
    pub is_admin: bool,
    /// Whether user is owner of enterprise
    #[serde(default)]
    pub is_owner: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_deserialization_basic() {
        let json = r#"{
            "id": "U1234567890",
            "team_id": "T12345",
            "name": "johndoe",
            "deleted": false,
            "real_name": "John Doe",
            "tz": "America/Los_Angeles",
            "tz_label": "Pacific Standard Time",
            "tz_offset": -28800,
            "is_admin": false,
            "is_owner": false,
            "is_bot": false
        }"#;

        let user: User = serde_json::from_str(json).unwrap();
        assert_eq!(user.id, "U1234567890");
        assert_eq!(user.name, Some("johndoe".to_string()));
        assert_eq!(user.real_name, Some("John Doe".to_string()));
        assert!(!user.deleted);
        assert!(!user.is_admin);
        assert!(!user.is_bot);
    }

    #[test]
    fn test_user_deserialization_with_profile() {
        let json = r#"{
            "id": "U1234567890",
            "name": "johndoe",
            "profile": {
                "title": "Software Engineer",
                "phone": "555-1234",
                "real_name": "John Doe",
                "display_name": "JD",
                "status_text": "Working from home",
                "status_emoji": ":house:",
                "email": "john@example.com",
                "image_48": "https://example.com/avatar.png"
            }
        }"#;

        let user: User = serde_json::from_str(json).unwrap();
        assert!(user.profile.is_some());
        let profile = user.profile.unwrap();
        assert_eq!(profile.title, Some("Software Engineer".to_string()));
        assert_eq!(profile.display_name, Some("JD".to_string()));
        assert_eq!(profile.status_emoji, Some(":house:".to_string()));
    }

    #[test]
    fn test_user_deserialization_bot() {
        let json = r#"{
            "id": "U1234567890",
            "name": "mybot",
            "is_bot": true,
            "is_app_user": true,
            "deleted": false
        }"#;

        let user: User = serde_json::from_str(json).unwrap();
        assert!(user.is_bot);
        assert!(user.is_app_user);
    }

    #[test]
    fn test_user_deserialization_admin() {
        let json = r#"{
            "id": "U1234567890",
            "name": "admin",
            "is_admin": true,
            "is_owner": true,
            "is_primary_owner": true
        }"#;

        let user: User = serde_json::from_str(json).unwrap();
        assert!(user.is_admin);
        assert!(user.is_owner);
        assert!(user.is_primary_owner);
    }

    #[test]
    fn test_user_deserialization_guest() {
        let json = r#"{
            "id": "U1234567890",
            "name": "guest",
            "is_restricted": true,
            "is_ultra_restricted": false
        }"#;

        let user: User = serde_json::from_str(json).unwrap();
        assert!(user.is_restricted);
        assert!(!user.is_ultra_restricted);
        assert!(user.is_guest());
    }

    #[test]
    fn test_user_display_name() {
        // With display name in profile
        let user = User {
            id: "U123".to_string(),
            name: Some("johndoe".to_string()),
            real_name: Some("John Doe".to_string()),
            profile: Some(UserProfile {
                display_name: Some("JD".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(user.display_name(), "JD");

        // Without display name, use real name
        let user2 = User {
            id: "U123".to_string(),
            name: Some("johndoe".to_string()),
            real_name: Some("John Doe".to_string()),
            profile: Some(UserProfile {
                display_name: Some("".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(user2.display_name(), "John Doe");

        // Without profile, use top-level real name
        let user3 = User {
            id: "U123".to_string(),
            name: Some("johndoe".to_string()),
            real_name: Some("John Doe".to_string()),
            profile: None,
            ..Default::default()
        };
        assert_eq!(user3.display_name(), "John Doe");

        // Without real name, use username
        let user4 = User {
            id: "U123".to_string(),
            name: Some("johndoe".to_string()),
            real_name: None,
            profile: None,
            ..Default::default()
        };
        assert_eq!(user4.display_name(), "johndoe");

        // Without username, use ID
        let user5 = User {
            id: "U123".to_string(),
            name: None,
            real_name: None,
            profile: None,
            ..Default::default()
        };
        assert_eq!(user5.display_name(), "U123");
    }

    #[test]
    fn test_user_status() {
        let user = User {
            id: "U123".to_string(),
            profile: Some(UserProfile {
                status_emoji: Some(":coffee:".to_string()),
                status_text: Some("On a break".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        assert_eq!(user.status_emoji(), Some(":coffee:"));
        assert_eq!(user.status_text(), Some("On a break"));
    }

    #[test]
    fn test_user_is_active() {
        let active = User {
            id: "U123".to_string(),
            deleted: false,
            ..Default::default()
        };
        assert!(active.is_active());

        let deleted = User {
            id: "U123".to_string(),
            deleted: true,
            ..Default::default()
        };
        assert!(!deleted.is_active());
    }

    #[test]
    fn test_user_minimal() {
        let json = r#"{"id": "U123"}"#;
        let user: User = serde_json::from_str(json).unwrap();
        assert_eq!(user.id, "U123");
        assert!(user.name.is_none());
        assert!(!user.deleted);
    }
}
