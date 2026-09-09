//! OAuth2 flow for Slack CLI
//!
//! Implements both interactive browser-based OAuth and manual OAuth flows.
//! Uses a local HTTP server to receive the OAuth callback.

use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use url::Url;

use crate::auth::tokens::TokenSet;
use crate::error::{Result, SlackError};

/// Default OAuth scopes to request
pub const DEFAULT_SCOPES: &[&str] = &[
    "channels:read",
    "channels:history",
    "groups:read",
    "groups:history",
    "im:read",
    "im:history",
    "mpim:read",
    "mpim:history",
    "users:read",
    "search:read",
    "chat:write",
    "reactions:read",
    "reactions:write",
    "files:read",
    "users.profile:read",
    "users.profile:write",
];

/// Slack OAuth configuration
pub struct OAuthConfig {
    /// OAuth client ID
    pub client_id: String,
    /// OAuth client secret
    pub client_secret: String,
    /// OAuth scopes to request
    pub scopes: Vec<String>,
    /// Local port for callback server
    pub redirect_port: u16,
    /// Callback timeout in seconds
    pub timeout_secs: u64,
    /// Token exchange URL (for testing)
    pub token_url: String,
}

/// Default token exchange URL
pub const DEFAULT_TOKEN_URL: &str = "https://slack.com/api/oauth.v2.access";

impl Default for OAuthConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret: String::new(),
            scopes: DEFAULT_SCOPES.iter().map(|s| s.to_string()).collect(),
            redirect_port: 8765,
            timeout_secs: 120,
            token_url: DEFAULT_TOKEN_URL.to_string(),
        }
    }
}

impl OAuthConfig {
    /// Create a new OAuth configuration with client credentials
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
            ..Default::default()
        }
    }

    /// Set custom scopes
    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
    }

    /// Set custom redirect port
    pub fn with_port(mut self, port: u16) -> Self {
        self.redirect_port = port;
        self
    }

    /// Set callback timeout
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Set custom token exchange URL (for testing)
    pub fn with_token_url(mut self, url: String) -> Self {
        self.token_url = url;
        self
    }
}

/// OAuth flow result containing tokens
pub struct OAuthResult {
    /// Access token (xoxp-* or xoxb-*)
    pub access_token: String,
    /// Token type (typically "bearer")
    pub token_type: String,
    /// OAuth scopes granted
    pub scope: String,
    /// Team ID
    pub team_id: String,
    /// Team name
    pub team_name: String,
    /// User ID (for user tokens)
    pub user_id: Option<String>,
    /// Bot user ID (for bot tokens)
    pub bot_user_id: Option<String>,
}

/// OAuth2 flow handler
pub struct OAuthFlow {
    config: OAuthConfig,
}

impl OAuthFlow {
    /// Create a new OAuth flow with the given configuration
    pub fn new(config: OAuthConfig) -> Self {
        Self { config }
    }

    /// Generate a random state token for CSRF protection
    pub fn generate_state() -> String {
        let mut bytes = [0u8; 32];
        // Use simple pseudo-random for state token
        // In production, use a proper CSPRNG
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = ((seed >> (i % 16 * 8)) & 0xFF) as u8 ^ (i as u8).wrapping_mul(17);
        }
        URL_SAFE_NO_PAD.encode(bytes)
    }

    /// Build the authorization URL
    pub fn build_auth_url(&self, state: &str) -> Result<String> {
        let redirect_uri = format!("http://localhost:{}/callback", self.config.redirect_port);
        let scope = self.config.scopes.join(",");

        // Use url crate for proper percent-encoding
        // This URL is a known-valid constant, but we handle the error anyway to avoid unwrap
        let mut url = Url::parse("https://slack.com/oauth/v2/authorize")
            .map_err(|e| SlackError::Other(format!("Failed to parse OAuth URL: {}", e)))?;
        url.query_pairs_mut()
            .append_pair("client_id", &self.config.client_id)
            .append_pair("scope", &scope)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("state", state);

        Ok(url.to_string())
    }

    /// Start interactive OAuth flow (opens browser)
    ///
    /// 1. Generate state token
    /// 2. Start local HTTP server
    /// 3. Open browser to authorization URL
    /// 4. Wait for callback with authorization code
    /// 5. Exchange code for tokens
    pub fn authorize(&self) -> Result<TokenSet> {
        let state = Self::generate_state();
        let auth_url = self.build_auth_url(&state)?;

        // Start the callback server in a thread
        let (tx, rx) = mpsc::channel();
        let port = self.config.redirect_port;
        let expected_state = state.clone();

        let server_handle = thread::spawn(move || start_callback_server(port, &expected_state, tx));

        // Give the server a moment to start
        thread::sleep(Duration::from_millis(100));

        // Open the browser
        eprintln!("\nOpening browser for Slack authorization...");
        eprintln!("If the browser doesn't open, visit this URL:\n");
        eprintln!("  {}\n", auth_url);

        if let Err(e) = open::that(&auth_url) {
            eprintln!("Warning: Could not open browser automatically: {}", e);
            eprintln!("Please open the URL above manually.");
        }

        // Wait for the authorization code
        eprintln!(
            "Waiting for authorization (timeout: {}s)...",
            self.config.timeout_secs
        );

        let code = rx
            .recv_timeout(Duration::from_secs(self.config.timeout_secs))
            .map_err(|_| SlackError::Other("OAuth callback timed out".into()))??;

        // Wait for server thread to finish
        let _ = server_handle.join();

        // Exchange the code for tokens
        self.exchange_code(&code)
    }

    /// Manual OAuth flow (no browser)
    ///
    /// 1. Generate state token
    /// 2. Print authorization URL
    /// 3. Prompt user to paste redirect URL
    /// 4. Extract code from URL
    /// 5. Exchange code for tokens
    pub fn authorize_manual(&self) -> Result<TokenSet> {
        let state = Self::generate_state();
        let stdin = io::stdin();
        self.authorize_manual_with(stdin.lock(), &state, |_| Ok(()))
    }

    /// Run the manual flow with injectable input and URL handling.
    ///
    /// The callback is deliberately a no-op in the production wrapper: manual mode prints the
    /// authorization URL but never opens a browser automatically. Keeping it injectable lets the
    /// flow be exercised without touching a real browser.
    fn authorize_manual_with<R, F>(
        &self,
        mut reader: R,
        state: &str,
        mut browser_opener: F,
    ) -> Result<TokenSet>
    where
        R: BufRead,
        F: FnMut(&str) -> Result<()>,
    {
        let auth_url = self.build_auth_url(state)?;

        eprintln!("\n=== Manual OAuth Flow ===\n");
        eprintln!("1. Open this URL in your browser:\n");
        eprintln!("   {}\n", auth_url);
        eprintln!("2. Authorize the application");
        eprintln!("3. You'll be redirected to a localhost URL (may show an error page)");
        eprintln!("4. Copy the FULL URL from your browser's address bar (or just the code)");
        eprintln!("\nPaste the redirect URL here:");
        browser_opener(&auth_url)?;

        io::stdout().flush().map_err(SlackError::Io)?;

        let mut input = String::new();
        reader.read_line(&mut input).map_err(SlackError::Io)?;

        let input = input.trim();
        if input.is_empty() {
            return Err(SlackError::Usage("No URL or code provided".into()));
        }

        // Slack may display the code separately when localhost cannot be reached. Accept that
        // value directly; full redirect URLs retain state verification and OAuth error handling.
        let code = if input.starts_with("http://") || input.starts_with("https://") {
            Self::code_from_redirect_url(input, state)?
        } else {
            input.to_string()
        };

        self.exchange_code(&code)
    }

    fn code_from_redirect_url(redirect_url: &str, expected_state: &str) -> Result<String> {
        let url = Url::parse(redirect_url)
            .map_err(|e| SlackError::Usage(format!("Invalid URL: {}", e)))?;

        let mut code = None;
        let mut returned_state = None;

        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "code" => code = Some(value.to_string()),
                "state" => returned_state = Some(value.to_string()),
                "error" => {
                    let error_desc = url
                        .query_pairs()
                        .find(|(k, _)| k == "error_description")
                        .map(|(_, v)| v.to_string())
                        .unwrap_or_else(|| value.to_string());
                    return Err(SlackError::Api {
                        error: value.to_string(),
                        detail: Some(error_desc),
                    });
                }
                _ => {}
            }
        }

        match returned_state {
            Some(ref state) if state == expected_state => {}
            Some(_) => {
                return Err(SlackError::Other(
                    "State mismatch - possible CSRF attack".into(),
                ))
            }
            None => {
                return Err(SlackError::Usage(
                    "No state parameter in redirect URL".into(),
                ))
            }
        }

        code.ok_or_else(|| SlackError::Usage("No authorization code in redirect URL".into()))
    }

    /// Exchange authorization code for access tokens
    pub(crate) fn exchange_code(&self, code: &str) -> Result<TokenSet> {
        // We need to make a blocking HTTP request here
        // Using reqwest's blocking client for simplicity in the OAuth flow
        let client = reqwest::blocking::Client::new();
        let redirect_uri = format!("http://localhost:{}/callback", self.config.redirect_port);

        let response = client
            .post(&self.config.token_url)
            .form(&[
                ("client_id", self.config.client_id.as_str()),
                ("client_secret", self.config.client_secret.as_str()),
                ("code", code),
                ("redirect_uri", redirect_uri.as_str()),
            ])
            .send()
            .map_err(SlackError::Network)?;

        let body: serde_json::Value = response.json().map_err(SlackError::Network)?;

        parse_oauth_response(&body)
    }
}

/// Parse an OAuth response JSON into a TokenSet
///
/// This is a pure function that handles all the response parsing logic,
/// extracted from exchange_code for testability without network dependencies.
pub fn parse_oauth_response(body: &serde_json::Value) -> Result<TokenSet> {
    if !body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
        let error = body
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown_error");
        return Err(SlackError::Api {
            error: error.to_string(),
            detail: None,
        });
    }

    // Extract token info - prefer top-level access_token (bot), fall back to authed_user (user)
    let access_token = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| {
            body.get("authed_user")
                .and_then(|u| u.get("access_token"))
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .ok_or_else(|| SlackError::Other("No access token in response".into()))?;

    let team = body.get("team").unwrap_or(&serde_json::Value::Null);
    let team_id = team
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let team_name = team
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let authed_user = body.get("authed_user").unwrap_or(&serde_json::Value::Null);
    let user_id = authed_user
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from);

    let bot_user_id = body
        .get("bot_user_id")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Determine user_id: prefer authed_user.id, then bot_user_id, else fallback
    let final_user_id = user_id
        .or(bot_user_id)
        .unwrap_or_else(|| "unknown".to_string());

    // Parse scopes (comma-separated string to Vec<String>)
    let scopes: Vec<String> = body
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    // Create TokenSet with appropriate type based on token prefix
    TokenSet::new_oauth(access_token, team_id, team_name, final_user_id, scopes)
}

/// Start a local HTTP server to receive the OAuth callback
fn start_callback_server(
    port: u16,
    expected_state: &str,
    tx: mpsc::Sender<Result<String>>,
) -> Result<()> {
    let addr = format!("127.0.0.1:{}", port);
    let server = tiny_http::Server::http(&addr)
        .map_err(|e| SlackError::Other(format!("Failed to start callback server: {}", e)))?;

    // Accept one request
    if let Some(request) = server.recv_timeout(Duration::from_secs(120))? {
        let url_str = format!("http://localhost{}", request.url());
        let url = match Url::parse(&url_str) {
            Ok(u) => u,
            Err(e) => {
                let _ = tx.send(Err(SlackError::Usage(format!(
                    "Invalid callback URL: {}",
                    e
                ))));
                let response =
                    tiny_http::Response::from_string("Invalid callback URL").with_status_code(400);
                let _ = request.respond(response);
                return Ok(());
            }
        };

        let mut code = None;
        let mut state = None;
        let mut error = None;
        let mut error_desc = None;

        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "code" => code = Some(value.to_string()),
                "state" => state = Some(value.to_string()),
                "error" => error = Some(value.to_string()),
                "error_description" => error_desc = Some(value.to_string()),
                _ => {}
            }
        }

        // Check for errors
        if let Some(err) = error {
            let _ = tx.send(Err(SlackError::Api {
                error: err,
                detail: error_desc,
            }));
            let response = tiny_http::Response::from_string(
                "Authorization failed. You can close this window.",
            )
            .with_status_code(400);
            let _ = request.respond(response);
            return Ok(());
        }

        // Verify state
        match state {
            Some(ref s) if s == expected_state => {}
            _ => {
                let _ = tx.send(Err(SlackError::Other("State mismatch".into())));
                let response =
                    tiny_http::Response::from_string("State mismatch. Authorization failed.")
                        .with_status_code(400);
                let _ = request.respond(response);
                return Ok(());
            }
        }

        // Send the code
        if let Some(c) = code {
            let _ = tx.send(Ok(c));
            let html = r#"<!DOCTYPE html>
<html>
<head><title>Slack CLI Authorization</title></head>
<body style="font-family: sans-serif; text-align: center; padding: 50px;">
<h1>✓ Authorization Successful!</h1>
<p>You can close this window and return to the terminal.</p>
</body>
</html>"#;
            let header = tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html"[..])
                .map_err(|_| SlackError::Other("Failed to create HTTP header".into()))?;
            let response = tiny_http::Response::from_string(html)
                .with_header(header)
                .with_status_code(200);
            let _ = request.respond(response);
        } else {
            let _ = tx.send(Err(SlackError::Usage(
                "No authorization code received".into(),
            )));
            let response = tiny_http::Response::from_string("No authorization code received.")
                .with_status_code(400);
            let _ = request.respond(response);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_state() {
        let state1 = OAuthFlow::generate_state();
        let state2 = OAuthFlow::generate_state();

        // State should be non-empty
        assert!(!state1.is_empty());
        // States should be different (with high probability)
        // Note: There's a tiny chance they could be the same if generated in the same nanosecond
        // For testing purposes, we just verify they're valid base64
        assert!(state1
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert!(state2
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn test_build_auth_url() {
        let config = OAuthConfig::new("test_client_id".into(), "test_secret".into())
            .with_scopes(vec!["channels:read".into(), "users:read".into()])
            .with_port(8888);

        let flow = OAuthFlow::new(config);
        let state = "test_state_token";
        let url = flow.build_auth_url(state).unwrap();

        assert!(url.starts_with("https://slack.com/oauth/v2/authorize"));
        assert!(url.contains("client_id=test_client_id"));
        assert!(url.contains("scope=channels%3Aread%2Cusers%3Aread"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A8888%2Fcallback"));
        assert!(url.contains("state=test_state_token"));
    }

    #[test]
    fn test_oauth_config_default() {
        let config = OAuthConfig::default();

        assert!(config.client_id.is_empty());
        assert!(config.client_secret.is_empty());
        assert!(!config.scopes.is_empty());
        assert_eq!(config.redirect_port, 8765);
        assert_eq!(config.timeout_secs, 120);
    }

    #[test]
    fn test_oauth_config_builder() {
        let config = OAuthConfig::new("id".into(), "secret".into())
            .with_scopes(vec!["scope1".into()])
            .with_port(9999)
            .with_timeout(60);

        assert_eq!(config.client_id, "id");
        assert_eq!(config.client_secret, "secret");
        assert_eq!(config.scopes, vec!["scope1"]);
        assert_eq!(config.redirect_port, 9999);
        assert_eq!(config.timeout_secs, 60);
    }

    #[test]
    fn test_default_scopes() {
        assert!(DEFAULT_SCOPES.contains(&"channels:read"));
        assert!(DEFAULT_SCOPES.contains(&"users:read"));
        assert!(DEFAULT_SCOPES.contains(&"search:read"));
        assert!(DEFAULT_SCOPES.contains(&"chat:write"));
    }

    #[test]
    fn test_oauth_config_with_token_url() {
        let config = OAuthConfig::new("id".into(), "secret".into())
            .with_token_url("http://localhost:1234/token".into());

        assert_eq!(config.token_url, "http://localhost:1234/token");
    }

    // Tests for parse_oauth_response (pure function, no network dependencies)

    #[test]
    fn test_parse_oauth_response_success_bot_token() {
        let body: serde_json::Value = serde_json::json!({
            "ok": true,
            "access_token": "xoxb-123456789-0123456789-abcdefghijklmnop",
            "token_type": "bot",
            "scope": "channels:read,users:read",
            "bot_user_id": "UBOT12345",
            "team": {
                "id": "T12345678",
                "name": "Test Workspace"
            }
        });

        let result = super::parse_oauth_response(&body);

        assert!(result.is_ok());
        let token_set = result.unwrap();
        assert_eq!(
            token_set.access_token,
            "xoxb-123456789-0123456789-abcdefghijklmnop"
        );
        assert_eq!(token_set.team_id, "T12345678");
        assert_eq!(token_set.team_name, "Test Workspace");
        assert_eq!(token_set.user_id, "UBOT12345");
        assert_eq!(token_set.scopes, vec!["channels:read", "users:read"]);
        assert_eq!(token_set.token_type, crate::auth::TokenType::BotOAuth);
    }

    #[test]
    fn test_parse_oauth_response_success_user_token() {
        let body: serde_json::Value = serde_json::json!({
            "ok": true,
            "access_token": "xoxp-123456789-0123456789-0123456789-abcdef",
            "token_type": "user",
            "scope": "channels:read,chat:write",
            "authed_user": {
                "id": "U12345678",
                "access_token": "xoxp-123456789-0123456789-0123456789-abcdef"
            },
            "team": {
                "id": "T98765432",
                "name": "User Workspace"
            }
        });

        let result = super::parse_oauth_response(&body);

        assert!(result.is_ok());
        let token_set = result.unwrap();
        assert_eq!(
            token_set.access_token,
            "xoxp-123456789-0123456789-0123456789-abcdef"
        );
        assert_eq!(token_set.team_id, "T98765432");
        assert_eq!(token_set.team_name, "User Workspace");
        assert_eq!(token_set.user_id, "U12345678");
        assert_eq!(token_set.scopes, vec!["channels:read", "chat:write"]);
        assert_eq!(token_set.token_type, crate::auth::TokenType::UserOAuth);
    }

    #[test]
    fn test_parse_oauth_response_error_response() {
        let body: serde_json::Value = serde_json::json!({
            "ok": false,
            "error": "invalid_code"
        });

        let result = super::parse_oauth_response(&body);

        assert!(result.is_err());
        match result.unwrap_err() {
            crate::error::SlackError::Api { error, .. } => {
                assert_eq!(error, "invalid_code");
            }
            _ => panic!("Expected SlackError::Api"),
        }
    }

    #[test]
    fn test_parse_oauth_response_with_authed_user_fallback() {
        // Test case where access_token is only in authed_user (user token flow)
        let body: serde_json::Value = serde_json::json!({
            "ok": true,
            "token_type": "user",
            "scope": "search:read",
            "authed_user": {
                "id": "UUSER1234",
                "access_token": "xoxp-user-token-here-abcdef123"
            },
            "team": {
                "id": "TUSER1234",
                "name": "User Team"
            }
        });

        let result = super::parse_oauth_response(&body);

        assert!(result.is_ok());
        let token_set = result.unwrap();
        assert_eq!(token_set.access_token, "xoxp-user-token-here-abcdef123");
        assert_eq!(token_set.user_id, "UUSER1234");
        assert_eq!(token_set.token_type, crate::auth::TokenType::UserOAuth);
    }

    #[test]
    fn test_parse_oauth_response_unknown_user_fallback() {
        // Test case where neither user_id nor bot_user_id is present
        let body: serde_json::Value = serde_json::json!({
            "ok": true,
            "access_token": "xoxb-minimal-token-here-1234",
            "token_type": "bot",
            "scope": "",
            "team": {
                "id": "TMINIMAL",
                "name": "Minimal Team"
            }
        });

        let result = super::parse_oauth_response(&body);

        assert!(result.is_ok());
        let token_set = result.unwrap();
        assert_eq!(token_set.user_id, "unknown");
        assert!(token_set.scopes.is_empty());
    }

    #[test]
    fn test_parse_oauth_response_no_access_token() {
        // Test case where no access_token is provided at all
        let body: serde_json::Value = serde_json::json!({
            "ok": true,
            "team": {
                "id": "T12345",
                "name": "Test"
            }
        });

        let result = super::parse_oauth_response(&body);

        assert!(result.is_err());
        match result.unwrap_err() {
            crate::error::SlackError::Other(msg) => {
                assert!(msg.contains("No access token"));
            }
            _ => panic!("Expected SlackError::Other"),
        }
    }

    fn test_flow(token_url: String, port: u16) -> OAuthFlow {
        OAuthFlow::new(
            OAuthConfig::new("client-id".into(), "client-secret".into())
                .with_port(port)
                .with_token_url(token_url),
        )
    }

    #[test]
    fn exchange_code_posts_form_and_maps_success() {
        use mockito::Matcher;

        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/oauth.v2.access")
            .match_header("content-type", "application/x-www-form-urlencoded")
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("client_id".into(), "client-id".into()),
                Matcher::UrlEncoded("client_secret".into(), "client-secret".into()),
                Matcher::UrlEncoded("code".into(), "oauth-code".into()),
                Matcher::UrlEncoded(
                    "redirect_uri".into(),
                    "http://localhost:9123/callback".into(),
                ),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "ok": true,
                    "access_token": "xoxb-test-access-token",
                    "scope": "channels:read,chat:write",
                    "bot_user_id": "UBOT",
                    "team": {"id": "TTEAM", "name": "OAuth Team"}
                }"#,
            )
            .create();
        let flow = test_flow(format!("{}/oauth.v2.access", server.url()), 9123);

        let token = flow.exchange_code("oauth-code").unwrap();

        mock.assert();
        assert_eq!(token.access_token, "xoxb-test-access-token");
        assert_eq!(token.team_id, "TTEAM");
        assert_eq!(token.team_name, "OAuth Team");
        assert_eq!(token.user_id, "UBOT");
        assert_eq!(token.scopes, ["channels:read", "chat:write"]);
    }

    #[test]
    fn exchange_code_maps_slack_error() {
        use mockito::Matcher;

        let mut server = mockito::Server::new();
        let mock = server
            .mock("POST", "/oauth.v2.access")
            .match_body(Matcher::UrlEncoded("code".into(), "bad-code".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ok":false,"error":"invalid_code"}"#)
            .create();
        let flow = test_flow(format!("{}/oauth.v2.access", server.url()), 8765);

        let error = flow.exchange_code("bad-code").unwrap_err();

        mock.assert();
        match error {
            SlackError::Api { error, detail } => {
                assert_eq!(error, "invalid_code");
                assert_eq!(detail, None);
            }
            other => panic!("expected API error, got {other:?}"),
        }
    }

    #[test]
    fn malformed_oauth_response_uses_safe_error() {
        let error = parse_oauth_response(&serde_json::json!({"unexpected": true})).unwrap_err();
        match error {
            SlackError::Api { error, detail } => {
                assert_eq!(error, "unknown_error");
                assert_eq!(detail, None);
            }
            other => panic!("expected API error, got {other:?}"),
        }
    }

    fn unused_local_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    fn request_callback(path: &str, expected_state: &str) -> (u16, String, Result<String>) {
        let port = unused_local_port();
        let (tx, rx) = mpsc::channel();
        let expected_state = expected_state.to_string();
        let handle = thread::spawn(move || start_callback_server(port, &expected_state, tx));
        let url = format!("http://127.0.0.1:{port}{path}");
        let client = reqwest::blocking::Client::new();

        let response = (0..20)
            .find_map(|_| match client.get(&url).send() {
                Ok(response) => Some(response),
                Err(_) => {
                    thread::sleep(Duration::from_millis(10));
                    None
                }
            })
            .expect("callback server did not start");
        let status = response.status().as_u16();
        let body = response.text().unwrap();
        let callback = rx.recv_timeout(Duration::from_secs(1)).unwrap();
        handle.join().unwrap().unwrap();

        (status, body, callback)
    }

    #[test]
    fn callback_server_returns_code_and_success_html() {
        let (status, body, callback) =
            request_callback("/callback?code=code-123&state=expected", "expected");

        assert_eq!(status, 200);
        assert!(body.contains("Authorization Successful"));
        assert_eq!(callback.unwrap(), "code-123");
    }

    #[test]
    fn callback_server_rejects_state_mismatch_with_html() {
        let (status, body, callback) =
            request_callback("/callback?code=code-123&state=wrong", "expected");

        assert_eq!(status, 400);
        assert!(body.contains("State mismatch"));
        match callback.unwrap_err() {
            SlackError::Other(message) => assert_eq!(message, "State mismatch"),
            other => panic!("expected state error, got {other:?}"),
        }
    }

    #[test]
    fn callback_server_returns_oauth_denial_and_html() {
        let (status, body, callback) = request_callback(
            "/callback?error=access_denied&error_description=User%20declined",
            "expected",
        );

        assert_eq!(status, 400);
        assert!(body.contains("Authorization failed"));
        match callback.unwrap_err() {
            SlackError::Api { error, detail } => {
                assert_eq!(error, "access_denied");
                assert_eq!(detail.as_deref(), Some("User declined"));
            }
            other => panic!("expected API error, got {other:?}"),
        }
    }

    #[test]
    fn callback_server_reports_bind_failure() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, _rx) = mpsc::channel();

        let error = start_callback_server(port, "state", tx).unwrap_err();

        assert!(error
            .to_string()
            .contains("Failed to start callback server"));
    }

    fn oauth_success_mock(server: &mut mockito::Server, expected_code: &str) -> mockito::Mock {
        use mockito::Matcher;

        server
            .mock("POST", "/oauth.v2.access")
            .match_body(Matcher::UrlEncoded(
                "code".into(),
                expected_code.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                    "ok": true,
                    "access_token": "xoxp-manual-access-token",
                    "scope": "users:read",
                    "authed_user": {"id": "UMANUAL"},
                    "team": {"id": "TMANUAL", "name": "Manual Team"}
                }"#,
            )
            .create()
    }

    #[test]
    fn manual_authorization_accepts_pasted_code_without_opening_browser() {
        let mut server = mockito::Server::new();
        let mock = oauth_success_mock(&mut server, "pasted-code");
        let flow = test_flow(format!("{}/oauth.v2.access", server.url()), 8765);
        let input = io::Cursor::new(b"pasted-code\n");
        let mut presented_url = None;

        let token = flow
            .authorize_manual_with(input, "known-state", |url| {
                presented_url = Some(url.to_string());
                Ok(())
            })
            .unwrap();

        mock.assert();
        assert_eq!(token.access_token, "xoxp-manual-access-token");
        assert!(presented_url.unwrap().contains("state=known-state"));
    }

    #[test]
    fn manual_authorization_accepts_full_redirect_url() {
        let mut server = mockito::Server::new();
        let mock = oauth_success_mock(&mut server, "url-code");
        let flow = test_flow(format!("{}/oauth.v2.access", server.url()), 8765);
        let input =
            io::Cursor::new(b"http://localhost:8765/callback?code=url-code&state=known-state\n");

        let token = flow
            .authorize_manual_with(input, "known-state", |_| Ok(()))
            .unwrap();

        mock.assert();
        assert_eq!(token.team_id, "TMANUAL");
        assert_eq!(token.user_id, "UMANUAL");
    }

    #[test]
    fn manual_redirect_validation_errors_are_preserved() {
        let missing_state =
            OAuthFlow::code_from_redirect_url("http://localhost/callback?code=code", "expected")
                .unwrap_err();
        assert!(matches!(missing_state, SlackError::Usage(_)));

        let wrong_state = OAuthFlow::code_from_redirect_url(
            "http://localhost/callback?code=code&state=wrong",
            "expected",
        )
        .unwrap_err();
        assert!(matches!(wrong_state, SlackError::Other(_)));

        let denied = OAuthFlow::code_from_redirect_url(
            "http://localhost/callback?error=access_denied&error_description=Nope&state=expected",
            "expected",
        )
        .unwrap_err();
        assert!(matches!(denied, SlackError::Api { .. }));
    }
}
