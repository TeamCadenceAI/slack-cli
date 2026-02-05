mod types;

pub use types::{Result, SlackError};

use serde_json::json;

/// Format error as JSON for agent consumption
pub fn format_error_json(err: &SlackError) -> serde_json::Value {
    json!({
        "error": true,
        "code": err.code(),
        "message": err.to_string(),
        "detail": err.detail(),
    })
}

/// Format error for human-readable output (stderr)
pub fn format_error_human(err: &SlackError) -> String {
    match err {
        SlackError::AuthRequired => {
            format!(
                "{}\n\n{}\n  {}",
                "Authentication required.", "To authorize a workspace, run:", "slack auth add"
            )
        }
        SlackError::Api { error, detail } => {
            let mut msg = format!("Slack API error: {}", error);
            if let Some(d) = detail {
                msg.push_str(&format!("\n{}", d));
            }
            msg
        }
        _ => err.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        assert_eq!(SlackError::AuthRequired.code(), "auth_required");
        assert_eq!(
            SlackError::InvalidToken("test".to_string()).code(),
            "invalid_token"
        );
        assert_eq!(
            SlackError::ChannelNotFound("#test".to_string()).code(),
            "channel_not_found"
        );
    }

    #[test]
    fn test_exit_codes() {
        assert_eq!(SlackError::AuthRequired.exit_code(), 1);
        assert_eq!(SlackError::Usage("test".to_string()).exit_code(), 2);
    }

    #[test]
    fn test_json_formatting() {
        let err = SlackError::AuthRequired;
        let json = format_error_json(&err);
        assert_eq!(json["error"], true);
        assert_eq!(json["code"], "auth_required");
    }

    #[test]
    fn test_api_error_with_detail() {
        let err = SlackError::Api {
            error: "channel_not_found".to_string(),
            detail: Some("The channel does not exist".to_string()),
        };
        assert_eq!(err.code(), "api_error");
        assert_eq!(err.detail(), Some("The channel does not exist".to_string()));
    }
}
