//! Token types and validation for Slack CLI
//!
//! Supports three types of Slack tokens:
//! - User OAuth (xoxp-*): Full access with user permissions
//! - Bot OAuth (xoxb-*): App-level access to invited channels
//! - Browser (xoxc-* + xoxd-*): Stealth mode, no app required

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Result, SlackError};

/// The type of Slack token
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    /// User OAuth token (xoxp-*)
    UserOAuth,
    /// Bot OAuth token (xoxb-*)
    BotOAuth,
    /// Browser token (xoxc-* with xoxd-* cookie)
    Browser,
}

impl TokenType {
    /// Detect token type from prefix
    pub fn from_prefix(token: &str) -> Option<Self> {
        if token.starts_with("xoxp-") {
            Some(TokenType::UserOAuth)
        } else if token.starts_with("xoxb-") {
            Some(TokenType::BotOAuth)
        } else if token.starts_with("xoxc-") {
            Some(TokenType::Browser)
        } else {
            None
        }
    }

    /// Get the expected token prefix for this type
    pub fn expected_prefix(&self) -> &'static str {
        match self {
            TokenType::UserOAuth => "xoxp-",
            TokenType::BotOAuth => "xoxb-",
            TokenType::Browser => "xoxc-",
        }
    }
}

/// A set of Slack tokens for a workspace
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenSet {
    /// The type of token
    pub token_type: TokenType,
    /// The main access token (xoxp-*, xoxb-*, or xoxc-*)
    pub access_token: String,
    /// The xoxd cookie (required for Browser tokens)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xoxd_cookie: Option<String>,
    /// The team/workspace ID
    pub team_id: String,
    /// The team/workspace name
    pub team_name: String,
    /// The workspace domain (the `<sub>` in `<sub>.slack.com`), when known.
    /// Populated from the `auth.test` `url` at store time; used for stable
    /// `-w <subdomain>` selection (names are too volatile to match on).
    #[serde(default)]
    pub team_domain: Option<String>,
    /// The user ID associated with this token
    pub user_id: String,
    /// When the token was created/stored
    pub created_at: DateTime<Utc>,
    /// OAuth scopes granted (if known)
    #[serde(default)]
    pub scopes: Vec<String>,
}

impl TokenSet {
    /// Create a new TokenSet for a direct token (xoxp-* or xoxb-*)
    pub fn new_oauth(
        access_token: String,
        team_id: String,
        team_name: String,
        user_id: String,
        scopes: Vec<String>,
    ) -> Result<Self> {
        let token_type = TokenType::from_prefix(&access_token).ok_or_else(|| {
            SlackError::InvalidToken("Token must start with xoxp- or xoxb-".into())
        })?;

        if token_type == TokenType::Browser {
            return Err(SlackError::InvalidToken(
                "Browser tokens require xoxd cookie, use new_browser()".into(),
            ));
        }

        validate_token_format(&access_token)?;

        Ok(Self {
            token_type,
            access_token,
            xoxd_cookie: None,
            team_id,
            team_name,
            team_domain: None,
            user_id,
            created_at: Utc::now(),
            scopes,
        })
    }

    /// Create a new TokenSet for browser tokens (xoxc-* with xoxd-*)
    pub fn new_browser(
        xoxc_token: String,
        xoxd_cookie: String,
        team_id: String,
        team_name: String,
        user_id: String,
    ) -> Result<Self> {
        validate_token_format(&xoxc_token)?;
        validate_xoxd_cookie(&xoxd_cookie)?;

        if !xoxc_token.starts_with("xoxc-") {
            return Err(SlackError::InvalidToken(
                "Browser token must start with xoxc-".into(),
            ));
        }

        Ok(Self {
            token_type: TokenType::Browser,
            access_token: xoxc_token,
            xoxd_cookie: Some(xoxd_cookie),
            team_id,
            team_name,
            team_domain: None,
            user_id,
            created_at: Utc::now(),
            scopes: vec![],
        })
    }

    /// Attach the workspace domain (chainable), normalizing any full URL or
    /// `<sub>.slack.com` down to the bare leading label.
    pub fn with_domain(mut self, domain: impl AsRef<str>) -> Self {
        self.team_domain = normalize_workspace_domain(domain.as_ref());
        self
    }

    /// Validate that this token set is valid
    pub fn validate(&self) -> Result<()> {
        validate_token_format(&self.access_token)?;

        // Check token prefix matches type
        let expected_prefix = self.token_type.expected_prefix();
        if !self.access_token.starts_with(expected_prefix) {
            return Err(SlackError::InvalidToken(format!(
                "Token type {:?} requires prefix {}",
                self.token_type, expected_prefix
            )));
        }

        // Browser tokens require xoxd cookie
        if self.token_type == TokenType::Browser {
            match &self.xoxd_cookie {
                Some(cookie) => validate_xoxd_cookie(cookie)?,
                None => {
                    return Err(SlackError::InvalidToken(
                        "Browser tokens require xoxd cookie".into(),
                    ))
                }
            }
        }

        Ok(())
    }

    /// Check if this token supports search
    pub fn supports_search(&self) -> bool {
        // Bot tokens don't support search.messages
        self.token_type != TokenType::BotOAuth
    }

    /// Get the authentication header value
    pub fn auth_header(&self) -> String {
        format!("Bearer {}", self.access_token)
    }
}

/// Validate token format (basic checks)
fn validate_token_format(token: &str) -> Result<()> {
    // Check for minimum length (prefix + some characters)
    if token.len() < 10 {
        return Err(SlackError::InvalidToken("Token too short".into()));
    }

    // Check for valid prefix
    if !token.starts_with("xoxp-") && !token.starts_with("xoxb-") && !token.starts_with("xoxc-") {
        return Err(SlackError::InvalidToken(
            "Token must start with xoxp-, xoxb-, or xoxc-".into(),
        ));
    }

    // Check for invalid characters (tokens should be alphanumeric with dashes)
    if token
        .chars()
        .any(|c| !c.is_ascii_alphanumeric() && c != '-')
    {
        return Err(SlackError::InvalidToken(
            "Token contains invalid characters".into(),
        ));
    }

    Ok(())
}

/// Validate xoxd cookie format
fn validate_xoxd_cookie(cookie: &str) -> Result<()> {
    if cookie.is_empty() {
        return Err(SlackError::InvalidToken("xoxd cookie is required".into()));
    }

    // xoxd cookies typically start with "xoxd-" but older ones might not
    // Just check it's not empty and doesn't contain obviously invalid chars
    if cookie.contains('\n') || cookie.contains('\r') {
        return Err(SlackError::InvalidToken(
            "xoxd cookie contains invalid characters".into(),
        ));
    }

    Ok(())
}

/// Normalize a user- or `auth.test`-supplied workspace reference to its bare
/// domain label (the `<sub>` in `<sub>.slack.com`).
///
/// Accepts `cadence-app`, `cadence-app.slack.com`, `https://cadence-app.slack.com/`,
/// etc., and returns `Some("cadence-app")` (lowercased). Returns `None` when no
/// meaningful label can be extracted.
pub fn normalize_workspace_domain(input: &str) -> Option<String> {
    let mut s = input.trim();

    // Strip scheme.
    if let Some((_, rest)) = s.split_once("://") {
        s = rest;
    }
    // Strip path / query / fragment / port.
    s = s
        .split(['/', '?', '#', ':'])
        .next()
        .unwrap_or(s)
        .trim()
        .trim_end_matches('.');

    // Take the leading label (before the first dot): `cadence-app.slack.com`
    // -> `cadence-app`; a bare `cadence-app` is returned as-is.
    let label = s.split('.').next().unwrap_or(s).trim();
    if label.is_empty() {
        None
    } else {
        Some(label.to_ascii_lowercase())
    }
}

/// Return `true` if `query` selects the workspace identified by `team_id` /
/// `team_domain`.
///
/// Matching is intentionally limited to the **team ID** (exact,
/// case-insensitive) and the **domain/subdomain** (normalized via
/// [`normalize_workspace_domain`]). Team *names* are deliberately not matched:
/// they are user-editable and too volatile to be a stable selector.
pub fn workspace_matches(query: &str, team_id: &str, team_domain: Option<&str>) -> bool {
    if query.eq_ignore_ascii_case(team_id) {
        return true;
    }
    match (normalize_workspace_domain(query), team_domain) {
        (Some(q), Some(d)) => q.eq_ignore_ascii_case(d),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_domain_variants() {
        assert_eq!(
            normalize_workspace_domain("cadence-app").as_deref(),
            Some("cadence-app")
        );
        assert_eq!(
            normalize_workspace_domain("cadence-app.slack.com").as_deref(),
            Some("cadence-app")
        );
        assert_eq!(
            normalize_workspace_domain("https://cadence-app.slack.com/").as_deref(),
            Some("cadence-app")
        );
        assert_eq!(
            normalize_workspace_domain("HTTPS://Cadence-App.slack.com/messages").as_deref(),
            Some("cadence-app")
        );
        assert_eq!(normalize_workspace_domain("").as_deref(), None);
        assert_eq!(normalize_workspace_domain("   ").as_deref(), None);
    }

    #[test]
    fn workspace_matches_by_id_and_domain_only() {
        // team_id, case-insensitive
        assert!(workspace_matches(
            "T04U8BDD0KC",
            "T04U8BDD0KC",
            Some("cadence-app")
        ));
        assert!(workspace_matches(
            "t04u8bdd0kc",
            "T04U8BDD0KC",
            Some("cadence-app")
        ));
        // domain / subdomain / full url
        assert!(workspace_matches(
            "cadence-app",
            "T04U8BDD0KC",
            Some("cadence-app")
        ));
        assert!(workspace_matches(
            "cadence-app.slack.com",
            "T04U8BDD0KC",
            Some("cadence-app")
        ));
        // team NAME must NOT match
        assert!(!workspace_matches(
            "Cadence",
            "T04U8BDD0KC",
            Some("cadence-app")
        ));
        assert!(!workspace_matches(
            "antiburn",
            "T04U8BDD0KC",
            Some("cadence-app")
        ));
        // no domain stored -> only id matches
        assert!(!workspace_matches("cadence-app", "T04U8BDD0KC", None));
    }

    #[test]
    fn test_token_type_from_prefix_user() {
        assert_eq!(
            TokenType::from_prefix("xoxp-123456789-0"),
            Some(TokenType::UserOAuth)
        );
    }

    #[test]
    fn test_token_type_from_prefix_bot() {
        assert_eq!(
            TokenType::from_prefix("xoxb-123456789-0"),
            Some(TokenType::BotOAuth)
        );
    }

    #[test]
    fn test_token_type_from_prefix_browser() {
        assert_eq!(
            TokenType::from_prefix("xoxc-123456789-0"),
            Some(TokenType::Browser)
        );
    }

    #[test]
    fn test_token_type_from_prefix_invalid() {
        assert_eq!(TokenType::from_prefix("invalid-token"), None);
        assert_eq!(TokenType::from_prefix("xoxa-123456789-0"), None);
        assert_eq!(TokenType::from_prefix(""), None);
    }

    #[test]
    fn test_token_type_expected_prefix() {
        assert_eq!(TokenType::UserOAuth.expected_prefix(), "xoxp-");
        assert_eq!(TokenType::BotOAuth.expected_prefix(), "xoxb-");
        assert_eq!(TokenType::Browser.expected_prefix(), "xoxc-");
    }

    #[test]
    fn test_validate_token_format_valid() {
        assert!(validate_token_format("xoxp-123456789-0123456789-abcdef").is_ok());
        assert!(validate_token_format("xoxb-123456789-0123456789-abcdef").is_ok());
        assert!(validate_token_format("xoxc-123456789-0123456789-abcdef").is_ok());
    }

    #[test]
    fn test_validate_token_format_too_short() {
        let result = validate_token_format("xoxp-123");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SlackError::InvalidToken(_)));
    }

    #[test]
    fn test_validate_token_format_invalid_prefix() {
        let result = validate_token_format("invalid-token-here");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_token_format_invalid_chars() {
        let result = validate_token_format("xoxp-123456789!@#$%");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_xoxd_cookie_valid() {
        assert!(validate_xoxd_cookie("xoxd-abcdef123456").is_ok());
        assert!(validate_xoxd_cookie("some-cookie-value").is_ok());
    }

    #[test]
    fn test_validate_xoxd_cookie_empty() {
        let result = validate_xoxd_cookie("");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_xoxd_cookie_newlines() {
        let result = validate_xoxd_cookie("cookie\nwith\nnewlines");
        assert!(result.is_err());
    }

    #[test]
    fn test_token_set_new_oauth_user() {
        let result = TokenSet::new_oauth(
            "xoxp-123456789-0123456789-abcdef".into(),
            "T12345".into(),
            "Test Workspace".into(),
            "U12345".into(),
            vec!["channels:read".into()],
        );
        assert!(result.is_ok());
        let token_set = result.unwrap();
        assert_eq!(token_set.token_type, TokenType::UserOAuth);
        assert!(token_set.xoxd_cookie.is_none());
    }

    #[test]
    fn test_token_set_new_oauth_bot() {
        let result = TokenSet::new_oauth(
            "xoxb-123456789-0123456789-abcdef".into(),
            "T12345".into(),
            "Test Workspace".into(),
            "U12345".into(),
            vec!["chat:write".into()],
        );
        assert!(result.is_ok());
        let token_set = result.unwrap();
        assert_eq!(token_set.token_type, TokenType::BotOAuth);
    }

    #[test]
    fn test_token_set_new_oauth_rejects_browser_token() {
        let result = TokenSet::new_oauth(
            "xoxc-123456789-0123456789-abcdef".into(),
            "T12345".into(),
            "Test Workspace".into(),
            "U12345".into(),
            vec![],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_token_set_new_browser() {
        let result = TokenSet::new_browser(
            "xoxc-123456789-0123456789-abcdef".into(),
            "xoxd-cookie-value".into(),
            "T12345".into(),
            "Test Workspace".into(),
            "U12345".into(),
        );
        assert!(result.is_ok());
        let token_set = result.unwrap();
        assert_eq!(token_set.token_type, TokenType::Browser);
        assert!(token_set.xoxd_cookie.is_some());
    }

    #[test]
    fn test_token_set_new_browser_rejects_non_xoxc() {
        let result = TokenSet::new_browser(
            "xoxp-123456789-0123456789-abcdef".into(),
            "xoxd-cookie-value".into(),
            "T12345".into(),
            "Test Workspace".into(),
            "U12345".into(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_token_set_validate() {
        let token_set = TokenSet::new_oauth(
            "xoxp-123456789-0123456789-abcdef".into(),
            "T12345".into(),
            "Test Workspace".into(),
            "U12345".into(),
            vec![],
        )
        .unwrap();
        assert!(token_set.validate().is_ok());
    }

    #[test]
    fn test_token_set_supports_search() {
        let user_token = TokenSet::new_oauth(
            "xoxp-123456789-0123456789-abcdef".into(),
            "T12345".into(),
            "Test".into(),
            "U12345".into(),
            vec![],
        )
        .unwrap();
        assert!(user_token.supports_search());

        let bot_token = TokenSet::new_oauth(
            "xoxb-123456789-0123456789-abcdef".into(),
            "T12345".into(),
            "Test".into(),
            "U12345".into(),
            vec![],
        )
        .unwrap();
        assert!(!bot_token.supports_search());

        let browser_token = TokenSet::new_browser(
            "xoxc-123456789-0123456789-abcdef".into(),
            "xoxd-cookie".into(),
            "T12345".into(),
            "Test".into(),
            "U12345".into(),
        )
        .unwrap();
        assert!(browser_token.supports_search());
    }

    #[test]
    fn test_token_set_auth_header() {
        let token_set = TokenSet::new_oauth(
            "xoxp-123456789-0123456789-abcdef".into(),
            "T12345".into(),
            "Test".into(),
            "U12345".into(),
            vec![],
        )
        .unwrap();
        assert_eq!(
            token_set.auth_header(),
            "Bearer xoxp-123456789-0123456789-abcdef"
        );
    }

    #[test]
    fn test_token_set_serialization_roundtrip() {
        let original = TokenSet::new_oauth(
            "xoxp-123456789-0123456789-abcdef".into(),
            "T12345".into(),
            "Test Workspace".into(),
            "U12345".into(),
            vec!["channels:read".into(), "users:read".into()],
        )
        .unwrap();

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: TokenSet = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.token_type, original.token_type);
        assert_eq!(deserialized.access_token, original.access_token);
        assert_eq!(deserialized.team_id, original.team_id);
        assert_eq!(deserialized.team_name, original.team_name);
        assert_eq!(deserialized.user_id, original.user_id);
        assert_eq!(deserialized.scopes, original.scopes);
    }

    #[test]
    fn test_token_set_browser_serialization_roundtrip() {
        let original = TokenSet::new_browser(
            "xoxc-123456789-0123456789-abcdef".into(),
            "xoxd-cookie-value".into(),
            "T12345".into(),
            "Test Workspace".into(),
            "U12345".into(),
        )
        .unwrap();

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: TokenSet = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.token_type, TokenType::Browser);
        assert_eq!(deserialized.xoxd_cookie, original.xoxd_cookie);
    }
}
