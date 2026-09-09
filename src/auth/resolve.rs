//! Shared token resolution for CLI commands
//!
//! Resolves the authentication token to use for an API call, honoring (in
//! order of precedence):
//! 1. An explicit `--token` override (xoxp-/xoxb- only; browser tokens are
//!    rejected because they also require the xoxd cookie via `auth add`)
//! 2. A `-w/--workspace` selection matched against stored workspaces
//! 3. The default (or first) stored workspace

use crate::auth::{get_token_store, workspace_matches, TokenSet, TokenType};
use crate::error::{Result, SlackError};

/// Resolve the authentication token for a command invocation.
///
/// `token_override` takes precedence over `workspace`. Browser tokens
/// (xoxc-*) cannot be supplied as overrides because they require the paired
/// xoxd cookie, which is only available via `auth add`.
pub fn resolve_token(workspace: Option<&str>, token_override: Option<&str>) -> Result<TokenSet> {
    if let Some(token_str) = token_override {
        let token_type = TokenType::from_prefix(token_str).ok_or_else(|| {
            SlackError::InvalidToken("Token must start with xoxp-, xoxb-, or xoxc-".into())
        })?;

        if token_type == TokenType::Browser {
            return Err(SlackError::InvalidToken(
                "Browser tokens require --xoxc and --xoxd flags in 'auth add'".into(),
            ));
        }

        TokenSet::new_oauth(
            token_str.to_string(),
            "unknown".into(),
            "unknown".into(),
            "unknown".into(),
            vec![],
        )
    } else {
        let store = get_token_store();

        if let Some(ws_name) = workspace {
            let workspaces = store.get_workspace_info()?;
            let ws = workspaces
                .iter()
                .find(|w| workspace_matches(ws_name, &w.team_id, w.team_domain.as_deref()))
                .ok_or_else(|| SlackError::WorkspaceNotFound(ws_name.to_string()))?;
            store
                .get_token(&ws.team_id)?
                .ok_or(SlackError::AuthRequired)
        } else {
            store
                .get_default_or_first()?
                .ok_or(SlackError::AuthRequired)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests only exercise the `token_override` code path, which never
    // touches the token store or environment, so no process-wide env
    // mutations are needed.

    #[test]
    fn test_override_user_oauth_token() {
        let token = resolve_token(None, Some("xoxp-1234567890-abcdef")).unwrap();
        assert_eq!(token.token_type, TokenType::UserOAuth);
        assert_eq!(token.access_token, "xoxp-1234567890-abcdef");
        assert_eq!(token.team_id, "unknown");
        assert_eq!(token.team_name, "unknown");
        assert_eq!(token.user_id, "unknown");
        assert!(token.scopes.is_empty());
        assert!(token.xoxd_cookie.is_none());
    }

    #[test]
    fn test_override_bot_oauth_token() {
        let token = resolve_token(None, Some("xoxb-1234567890-abcdef")).unwrap();
        assert_eq!(token.token_type, TokenType::BotOAuth);
        assert_eq!(token.access_token, "xoxb-1234567890-abcdef");
    }

    #[test]
    fn test_override_takes_precedence_over_workspace() {
        // Even with a workspace selection, the override wins and the store is
        // never consulted (a nonexistent workspace would otherwise error).
        let token = resolve_token(
            Some("definitely-not-a-real-workspace"),
            Some("xoxp-1234567890-abcdef"),
        )
        .unwrap();
        assert_eq!(token.access_token, "xoxp-1234567890-abcdef");
    }

    #[test]
    fn test_override_browser_token_rejected() {
        let err = resolve_token(None, Some("xoxc-1234567890-abcdef")).unwrap_err();
        match err {
            SlackError::InvalidToken(msg) => {
                assert!(msg.contains("--xoxc and --xoxd"), "unexpected msg: {}", msg);
            }
            other => panic!("Expected InvalidToken, got {:?}", other),
        }
    }

    #[test]
    fn test_override_invalid_prefix_rejected() {
        let err = resolve_token(None, Some("not-a-token")).unwrap_err();
        match err {
            SlackError::InvalidToken(msg) => {
                assert!(
                    msg.contains("xoxp-, xoxb-, or xoxc-"),
                    "unexpected msg: {}",
                    msg
                );
            }
            other => panic!("Expected InvalidToken, got {:?}", other),
        }
    }

    #[test]
    fn test_override_malformed_token_rejected() {
        // Valid prefix but invalid characters fails TokenSet validation.
        let err = resolve_token(None, Some("xoxp-bad token!")).unwrap_err();
        assert!(matches!(err, SlackError::InvalidToken(_)));
    }
}
