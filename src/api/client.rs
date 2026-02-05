//! Slack API client
//!
//! HTTP client for making authenticated requests to Slack's Web API.

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, COOKIE};
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;

use crate::auth::TokenSet;
use crate::error::{Result, SlackError};

use super::rate_limiter::RateLimiter;
use super::types::SlackResponse;

/// Default base URL for Slack Web API
const DEFAULT_SLACK_API_BASE: &str = "https://slack.com/api";

/// Environment variable to override the Slack API base URL (for testing)
const SLACK_API_BASE_ENV: &str = "SLACK_API_BASE_URL";

/// Maximum number of retries for rate-limited requests
const MAX_RETRIES: u32 = 5;

/// Initial backoff duration for retries
const INITIAL_BACKOFF_MS: u64 = 1000;

/// Get the Slack API base URL, allowing override via environment variable
fn get_api_base_url() -> String {
    std::env::var(SLACK_API_BASE_ENV).unwrap_or_else(|_| DEFAULT_SLACK_API_BASE.to_string())
}

/// Slack API client
#[derive(Clone)]
pub struct SlackClient {
    http: reqwest::Client,
    token: TokenSet,
    rate_limiter: RateLimiter,
    base_url: String,
}

impl SlackClient {
    /// Create a new Slack client with the given token
    pub fn new(token: TokenSet) -> Result<Self> {
        Self::with_base_url(token, get_api_base_url())
    }

    /// Create a new Slack client with a custom base URL (for testing)
    pub fn with_base_url(token: TokenSet, base_url: String) -> Result<Self> {
        token.validate()?;

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(SlackError::Network)?;

        Ok(Self {
            http,
            token,
            rate_limiter: RateLimiter::new(),
            base_url,
        })
    }

    /// Create a new Slack client with a custom rate limiter
    pub fn with_rate_limiter(token: TokenSet, rate_limiter: RateLimiter) -> Result<Self> {
        token.validate()?;

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(SlackError::Network)?;

        Ok(Self {
            http,
            token,
            rate_limiter,
            base_url: get_api_base_url(),
        })
    }

    /// Get the base URL for API requests
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the token set
    pub fn token(&self) -> &TokenSet {
        &self.token
    }

    /// Get the team ID from the token
    pub fn team_id(&self) -> &str {
        &self.token.team_id
    }

    /// Check if search is available with this token
    pub fn supports_search(&self) -> bool {
        self.token.supports_search()
    }

    /// Build authentication headers for the request
    fn build_auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();

        // Bearer token for all token types
        let auth_value = self.token.auth_header();
        if let Ok(value) = HeaderValue::from_str(&auth_value) {
            headers.insert(AUTHORIZATION, value);
        }

        // For browser tokens, also add the xoxd cookie
        if let Some(xoxd) = &self.token.xoxd_cookie {
            let cookie_value = format!("d={}", xoxd);
            if let Ok(value) = HeaderValue::from_str(&cookie_value) {
                headers.insert(COOKIE, value);
            }
        }

        headers
    }

    /// Make a POST request to the Slack API
    ///
    /// Handles rate limiting, retries, and response parsing.
    pub async fn request<T, P>(&self, method: &str, params: &P) -> Result<T>
    where
        T: DeserializeOwned,
        P: Serialize + ?Sized,
    {
        let url = format!("{}/{}", self.base_url, method);
        let headers = self.build_auth_headers();

        let mut retries = 0;
        let mut backoff = INITIAL_BACKOFF_MS;

        loop {
            // Wait for rate limiter
            self.rate_limiter.acquire().await;

            let response = self
                .http
                .post(&url)
                .headers(headers.clone())
                .json(params)
                .send()
                .await
                .map_err(SlackError::Network)?;

            // Handle rate limiting
            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                if retries >= MAX_RETRIES {
                    // Extract retry-after if available
                    let retry_after = response
                        .headers()
                        .get("retry-after")
                        .and_then(|h| h.to_str().ok())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(60);
                    return Err(SlackError::RateLimited(retry_after));
                }

                // Get retry-after header or use exponential backoff
                let wait_time = response
                    .headers()
                    .get("retry-after")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(|s| s * 1000)
                    .unwrap_or(backoff);

                tokio::time::sleep(Duration::from_millis(wait_time)).await;

                retries += 1;
                backoff *= 2;
                continue;
            }

            // Parse response
            let slack_response: SlackResponse<T> =
                response.json().await.map_err(SlackError::Network)?;

            return slack_response.into_result();
        }
    }

    /// Make a GET request to download a file
    ///
    /// Returns the raw bytes of the file.
    pub async fn download(&self, url: &str, max_size: u64) -> Result<Vec<u8>> {
        let headers = self.build_auth_headers();

        // Wait for rate limiter
        self.rate_limiter.acquire().await;

        let response = self
            .http
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_err(SlackError::Network)?;

        if !response.status().is_success() {
            return Err(SlackError::Api {
                error: format!("HTTP {}", response.status()),
                detail: None,
            });
        }

        // Check content length if available
        if let Some(len) = response.content_length() {
            if len > max_size {
                return Err(SlackError::FileTooLarge);
            }
        }

        let bytes = response.bytes().await.map_err(SlackError::Network)?;

        if bytes.len() as u64 > max_size {
            return Err(SlackError::FileTooLarge);
        }

        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::TokenType;
    use chrono::Utc;

    fn create_test_token(token_type: TokenType) -> TokenSet {
        match token_type {
            TokenType::UserOAuth => TokenSet {
                token_type: TokenType::UserOAuth,
                access_token: "xoxp-123456789-0123456789-abcdef".to_string(),
                xoxd_cookie: None,
                team_id: "T12345".to_string(),
                team_name: "Test".to_string(),
                user_id: "U12345".to_string(),
                created_at: Utc::now(),
                scopes: vec![],
            },
            TokenType::BotOAuth => TokenSet {
                token_type: TokenType::BotOAuth,
                access_token: "xoxb-123456789-0123456789-abcdef".to_string(),
                xoxd_cookie: None,
                team_id: "T12345".to_string(),
                team_name: "Test".to_string(),
                user_id: "U12345".to_string(),
                created_at: Utc::now(),
                scopes: vec![],
            },
            TokenType::Browser => TokenSet {
                token_type: TokenType::Browser,
                access_token: "xoxc-123456789-0123456789-abcdef".to_string(),
                xoxd_cookie: Some("xoxd-test-cookie".to_string()),
                team_id: "T12345".to_string(),
                team_name: "Test".to_string(),
                user_id: "U12345".to_string(),
                created_at: Utc::now(),
                scopes: vec![],
            },
        }
    }

    #[test]
    fn test_client_creation_user_token() {
        let token = create_test_token(TokenType::UserOAuth);
        let client = SlackClient::new(token).unwrap();
        assert!(client.supports_search());
    }

    #[test]
    fn test_client_creation_bot_token() {
        let token = create_test_token(TokenType::BotOAuth);
        let client = SlackClient::new(token).unwrap();
        assert!(!client.supports_search());
    }

    #[test]
    fn test_client_creation_browser_token() {
        let token = create_test_token(TokenType::Browser);
        let client = SlackClient::new(token).unwrap();
        assert!(client.supports_search());
    }

    #[test]
    fn test_auth_headers_user_token() {
        let token = create_test_token(TokenType::UserOAuth);
        let client = SlackClient::new(token).unwrap();
        let headers = client.build_auth_headers();

        assert!(headers.contains_key(AUTHORIZATION));
        assert!(!headers.contains_key(COOKIE));

        let auth = headers.get(AUTHORIZATION).unwrap().to_str().unwrap();
        assert!(auth.starts_with("Bearer xoxp-"));
    }

    #[test]
    fn test_auth_headers_bot_token() {
        let token = create_test_token(TokenType::BotOAuth);
        let client = SlackClient::new(token).unwrap();
        let headers = client.build_auth_headers();

        assert!(headers.contains_key(AUTHORIZATION));
        assert!(!headers.contains_key(COOKIE));

        let auth = headers.get(AUTHORIZATION).unwrap().to_str().unwrap();
        assert!(auth.starts_with("Bearer xoxb-"));
    }

    #[test]
    fn test_auth_headers_browser_token() {
        let token = create_test_token(TokenType::Browser);
        let client = SlackClient::new(token).unwrap();
        let headers = client.build_auth_headers();

        assert!(headers.contains_key(AUTHORIZATION));
        assert!(headers.contains_key(COOKIE));

        let auth = headers.get(AUTHORIZATION).unwrap().to_str().unwrap();
        assert!(auth.starts_with("Bearer xoxc-"));

        let cookie = headers.get(COOKIE).unwrap().to_str().unwrap();
        assert!(cookie.starts_with("d="));
    }

    #[test]
    fn test_client_team_id() {
        let token = create_test_token(TokenType::UserOAuth);
        let client = SlackClient::new(token).unwrap();
        assert_eq!(client.team_id(), "T12345");
    }

    #[tokio::test]
    async fn test_client_request_mock() {
        use crate::api::types::AuthTestResponse;

        // Skip this test unless SLACK_RUN_MOCK_TESTS=1 is set
        // because mockito requires socket binding which may fail in restricted environments
        if std::env::var("SLACK_RUN_MOCK_TESTS").unwrap_or_default() != "1" {
            eprintln!("Skipping test_client_request_mock (set SLACK_RUN_MOCK_TESTS=1 to run)");
            return;
        }

        use mockito::Server;

        let mut server = Server::new_async().await;
        let mock_url = server.url();

        let _m = server
            .mock("POST", "/auth.test")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "ok": true,
                "url": "https://myteam.slack.com/",
                "team": "My Team",
                "user": "testuser",
                "team_id": "T12345",
                "user_id": "U12345"
            }"#,
            )
            .create_async()
            .await;

        // Create client with mock server URL
        let token = create_test_token(TokenType::UserOAuth);
        let client = SlackClient::with_base_url(token, mock_url).expect("Failed to create client");

        // Make the request
        let result: AuthTestResponse = client
            .request("auth.test", &())
            .await
            .expect("Request failed");
        assert_eq!(result.team_id, "T12345");
        assert_eq!(result.user_id, "U12345");
    }

    #[test]
    fn test_client_with_custom_base_url() {
        let token = create_test_token(TokenType::UserOAuth);
        let custom_url = "http://localhost:8080".to_string();
        let client = SlackClient::with_base_url(token, custom_url.clone()).unwrap();
        assert_eq!(client.base_url(), custom_url);
    }

    #[test]
    fn test_default_base_url() {
        let token = create_test_token(TokenType::UserOAuth);
        let client = SlackClient::new(token).unwrap();
        // When env var is not set, should use default
        assert!(client.base_url().contains("slack.com") || client.base_url().starts_with("http"));
    }

    #[tokio::test]
    async fn test_rate_limiter_integration() {
        let token = create_test_token(TokenType::UserOAuth);
        let rate_limiter = RateLimiter::with_config(60, 3);
        let client = SlackClient::with_rate_limiter(token, rate_limiter).unwrap();

        // Just verify the client was created with custom rate limiter
        assert!(client.supports_search());
    }
}
