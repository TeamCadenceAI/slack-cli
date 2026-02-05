//! Reaction model for Slack API

use serde::{Deserialize, Serialize};

/// A reaction on a Slack message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reaction {
    /// Emoji name (without colons)
    pub name: String,

    /// Number of users who added this reaction
    #[serde(default)]
    pub count: u32,

    /// User IDs who added this reaction
    #[serde(default)]
    pub users: Vec<String>,
}

impl Reaction {
    /// Get the emoji name with colons for display
    pub fn display_name(&self) -> String {
        format!(":{}:", self.name)
    }

    /// Check if a specific user has added this reaction
    pub fn has_user(&self, user_id: &str) -> bool {
        self.users.iter().any(|u| u == user_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reaction_deserialization() {
        let json = r#"{
            "name": "thumbsup",
            "count": 3,
            "users": ["U111", "U222", "U333"]
        }"#;

        let reaction: Reaction = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.name, "thumbsup");
        assert_eq!(reaction.count, 3);
        assert_eq!(reaction.users.len(), 3);
    }

    #[test]
    fn test_reaction_deserialization_minimal() {
        let json = r#"{
            "name": "heart"
        }"#;

        let reaction: Reaction = serde_json::from_str(json).unwrap();
        assert_eq!(reaction.name, "heart");
        assert_eq!(reaction.count, 0);
        assert!(reaction.users.is_empty());
    }

    #[test]
    fn test_reaction_display_name() {
        let reaction = Reaction {
            name: "thumbsup".to_string(),
            count: 1,
            users: vec!["U123".to_string()],
        };
        assert_eq!(reaction.display_name(), ":thumbsup:");
    }

    #[test]
    fn test_reaction_has_user() {
        let reaction = Reaction {
            name: "thumbsup".to_string(),
            count: 2,
            users: vec!["U111".to_string(), "U222".to_string()],
        };

        assert!(reaction.has_user("U111"));
        assert!(reaction.has_user("U222"));
        assert!(!reaction.has_user("U333"));
    }

    #[test]
    fn test_reaction_serialization() {
        let reaction = Reaction {
            name: "fire".to_string(),
            count: 5,
            users: vec!["U1".to_string(), "U2".to_string()],
        };

        let json = serde_json::to_string(&reaction).unwrap();
        assert!(json.contains("\"name\":\"fire\""));
        assert!(json.contains("\"count\":5"));
    }
}
