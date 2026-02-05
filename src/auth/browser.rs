//! Browser token support for Slack CLI
//!
//! Provides extraction instructions and validation for browser-based tokens (xoxc/xoxd).
//! Browser tokens allow access to Slack without creating an app - they use the same
//! session as your logged-in browser.

use crate::error::{Result, SlackError};

/// Browser tokens (xoxc token + xoxd cookie)
#[derive(Debug, Clone)]
pub struct BrowserTokens {
    /// The xoxc token (starts with xoxc-)
    pub xoxc: String,
    /// The xoxd cookie value (starts with xoxd-)
    pub xoxd: String,
}

impl BrowserTokens {
    /// Create a new BrowserTokens instance
    pub fn new(xoxc: String, xoxd: String) -> Self {
        Self { xoxc, xoxd }
    }

    /// Validate the browser tokens format
    pub fn validate(&self) -> Result<()> {
        // Validate xoxc token
        if !self.xoxc.starts_with("xoxc-") {
            return Err(SlackError::InvalidToken(
                "xoxc token must start with 'xoxc-'".into(),
            ));
        }

        if self.xoxc.len() < 20 {
            return Err(SlackError::InvalidToken("xoxc token is too short".into()));
        }

        // Check for invalid characters in xoxc (should be alphanumeric with dashes)
        if self
            .xoxc
            .chars()
            .any(|c| !c.is_ascii_alphanumeric() && c != '-')
        {
            return Err(SlackError::InvalidToken(
                "xoxc token contains invalid characters".into(),
            ));
        }

        // Validate xoxd cookie
        if self.xoxd.is_empty() {
            return Err(SlackError::InvalidToken(
                "xoxd cookie cannot be empty".into(),
            ));
        }

        // xoxd cookies should not contain newlines or carriage returns
        if self.xoxd.contains('\n') || self.xoxd.contains('\r') {
            return Err(SlackError::InvalidToken(
                "xoxd cookie contains invalid characters (newlines)".into(),
            ));
        }

        // Modern xoxd cookies start with "xoxd-" but older ones might not
        // We'll accept both but warn if it doesn't look right
        if !self.xoxd.starts_with("xoxd-") && self.xoxd.len() < 50 {
            // Very short non-xoxd cookie is likely wrong
            return Err(SlackError::InvalidToken(
                "xoxd cookie appears invalid - should start with 'xoxd-' or be a long encoded value".into(),
            ));
        }

        Ok(())
    }

    /// Check if the tokens appear to be valid format (non-strict)
    pub fn is_valid_format(&self) -> bool {
        self.validate().is_ok()
    }
}

/// Print step-by-step instructions for extracting browser tokens
pub fn print_extraction_instructions() {
    println!(
        r#"
╔══════════════════════════════════════════════════════════════════════════════╗
║                    BROWSER TOKEN EXTRACTION GUIDE                            ║
╚══════════════════════════════════════════════════════════════════════════════╝

Browser tokens allow you to use the Slack CLI with the same access as your
logged-in browser session - no Slack app required!

STEP 1: Open Slack in your browser
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
   • Go to https://app.slack.com
   • Sign in to your workspace
   • Make sure you're on the workspace you want to use

STEP 2: Open Developer Tools
━━━━━━━━━━━━━━━━━━━━━━━━━━━━
   • Windows/Linux: Press F12 or Ctrl+Shift+I
   • Mac: Press Cmd+Option+I
   • Or right-click anywhere and select "Inspect"

STEP 3: Get the xoxc token
━━━━━━━━━━━━━━━━━━━━━━━━━━
   1. Click the "Application" tab (or "Storage" in Firefox)
   2. In the left sidebar, expand "Local Storage"
   3. Click on "https://app.slack.com"
   4. Find the key named "localConfig_v2"
   5. Click on it to see the value
   6. Look for "token" in the JSON and copy the value
      (It starts with "xoxc-")

   Alternative method:
   1. Click the "Console" tab
   2. Type: JSON.parse(localStorage.getItem('localConfig_v2')).teams
   3. Find your team and copy the "token" value

STEP 4: Get the xoxd cookie
━━━━━━━━━━━━━━━━━━━━━━━━━━━
   1. In the Application tab, expand "Cookies"
   2. Click on "https://app.slack.com"
   3. Find the cookie named "d"
   4. Copy its value (starts with "xoxd-")

STEP 5: Add the tokens to slack-cli
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
   Run this command with your tokens:

   slack auth add --xoxc <YOUR_XOXC_TOKEN> --xoxd <YOUR_XOXD_COOKIE>

   Example:
   slack auth add --xoxc xoxc-1234567890-... --xoxd xoxd-abc123...

╔══════════════════════════════════════════════════════════════════════════════╗
║                             IMPORTANT NOTES                                  ║
╠══════════════════════════════════════════════════════════════════════════════╣
║ • Browser tokens have the same access as your logged-in session             ║
║ • Tokens may expire when you log out or after extended inactivity           ║
║ • Keep these tokens secure - they provide full access to your account       ║
║ • For automated/programmatic access, consider using OAuth instead:          ║
║   slack auth add --oauth                                                    ║
╚══════════════════════════════════════════════════════════════════════════════╝
"#
    );
}

/// Print compact extraction instructions (for CLI help)
pub fn print_compact_instructions() {
    println!(
        r#"
To extract browser tokens:

1. Open Slack in your browser (https://app.slack.com)
2. Open Developer Tools (F12)
3. Get xoxc token: Application → Local Storage → localConfig_v2 → "token"
4. Get xoxd cookie: Application → Cookies → "d"
5. Run: slack auth add --xoxc <TOKEN> --xoxd <COOKIE>

For detailed instructions: slack auth browser-help
"#
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_tokens_valid() {
        let tokens = BrowserTokens::new(
            "xoxc-1234567890-abcdef123456789012345678901234567890".into(),
            "xoxd-abcdefghijklmnopqrstuvwxyz1234567890".into(),
        );

        assert!(tokens.validate().is_ok());
        assert!(tokens.is_valid_format());
    }

    #[test]
    fn test_browser_tokens_invalid_xoxc_prefix() {
        let tokens = BrowserTokens::new(
            "xoxp-1234567890-abcdef123456789012345678901234567890".into(),
            "xoxd-abcdefghijklmnopqrstuvwxyz1234567890".into(),
        );

        let result = tokens.validate();
        assert!(result.is_err());
        assert!(matches!(result, Err(SlackError::InvalidToken(_))));
    }

    #[test]
    fn test_browser_tokens_xoxc_too_short() {
        let tokens = BrowserTokens::new(
            "xoxc-123".into(),
            "xoxd-abcdefghijklmnopqrstuvwxyz1234567890".into(),
        );

        let result = tokens.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_browser_tokens_xoxc_invalid_chars() {
        let tokens = BrowserTokens::new(
            "xoxc-1234567890-abcdef!@#$%^&*()".into(),
            "xoxd-abcdefghijklmnopqrstuvwxyz1234567890".into(),
        );

        let result = tokens.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_browser_tokens_empty_xoxd() {
        let tokens = BrowserTokens::new(
            "xoxc-1234567890-abcdef123456789012345678901234567890".into(),
            "".into(),
        );

        let result = tokens.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_browser_tokens_xoxd_with_newlines() {
        let tokens = BrowserTokens::new(
            "xoxc-1234567890-abcdef123456789012345678901234567890".into(),
            "xoxd-abc\ndef".into(),
        );

        let result = tokens.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_browser_tokens_xoxd_carriage_return() {
        let tokens = BrowserTokens::new(
            "xoxc-1234567890-abcdef123456789012345678901234567890".into(),
            "xoxd-abc\rdef".into(),
        );

        let result = tokens.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_browser_tokens_legacy_xoxd_format() {
        // Older xoxd cookies might not start with xoxd-
        // but should be long encoded values
        let tokens = BrowserTokens::new(
            "xoxc-1234567890-abcdef123456789012345678901234567890".into(),
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c".into(),
        );

        // Long JWT-like cookie should be accepted
        assert!(tokens.validate().is_ok());
    }

    #[test]
    fn test_browser_tokens_short_non_xoxd() {
        // Short non-xoxd cookie is likely wrong
        let tokens = BrowserTokens::new(
            "xoxc-1234567890-abcdef123456789012345678901234567890".into(),
            "shortcookie".into(),
        );

        let result = tokens.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_is_valid_format() {
        let valid = BrowserTokens::new(
            "xoxc-1234567890-abcdef123456789012345678901234567890".into(),
            "xoxd-abcdefghijklmnopqrstuvwxyz1234567890".into(),
        );
        assert!(valid.is_valid_format());

        let invalid = BrowserTokens::new("invalid".into(), "invalid".into());
        assert!(!invalid.is_valid_format());
    }
}
