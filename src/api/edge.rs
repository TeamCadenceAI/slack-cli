//! Edge API client for Slack CLI
//!
//! The Edge API provides access to Slack data using browser tokens (xoxc/xoxd).
//! It's a "stealth" mode that doesn't require creating a Slack app.
//!
//! Base URL: https://edgeapi.slack.com/cache/{team_id}/

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, COOKIE};
use serde::{Deserialize, Serialize};

use crate::auth::{TokenSet, TokenType};
use crate::error::{Result, SlackError};

/// Default base URL for Slack Edge API
const DEFAULT_EDGE_API_BASE: &str = "https://edgeapi.slack.com/cache";

/// Environment variable to override the Edge API base URL (for testing)
const EDGE_API_BASE_ENV: &str = "SLACK_EDGE_API_BASE_URL";

/// Get the Edge API base URL, allowing override via environment variable
fn get_edge_api_base_url() -> String {
    std::env::var(EDGE_API_BASE_ENV).unwrap_or_else(|_| DEFAULT_EDGE_API_BASE.to_string())
}

/// Edge API client
///
/// Only available for browser tokens (xoxc/xoxd).
/// Provides access to some Slack data without requiring an app.
pub struct EdgeClient {
    http: reqwest::Client,
    token: TokenSet,
    team_id: String,
    base_url: String,
}

impl EdgeClient {
    /// Create a new Edge API client
    ///
    /// Returns an error if the token is not a browser token.
    pub fn new(token: TokenSet) -> Result<Self> {
        Self::with_base_url(token, get_edge_api_base_url())
    }

    /// Create a new Edge API client with a custom base URL (for testing)
    pub fn with_base_url(token: TokenSet, base_url: String) -> Result<Self> {
        // Edge API only works with browser tokens
        if token.token_type != TokenType::Browser {
            return Err(SlackError::InvalidToken(
                "Edge API requires browser tokens (xoxc/xoxd)".into(),
            ));
        }

        token.validate()?;

        let team_id = token.team_id.clone();

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(SlackError::Network)?;

        Ok(Self {
            http,
            token,
            team_id,
            base_url,
        })
    }

    /// Get the base URL for API requests
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Get the team ID
    pub fn team_id(&self) -> &str {
        &self.team_id
    }

    /// Build authentication headers for Edge API requests
    fn build_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();

        // Authorization header
        let auth_value = format!("Bearer {}", self.token.access_token);
        if let Ok(value) = HeaderValue::from_str(&auth_value) {
            headers.insert(AUTHORIZATION, value);
        }

        // Cookie header (required for Edge API)
        if let Some(xoxd) = &self.token.xoxd_cookie {
            let cookie_value = format!("d={}", xoxd);
            if let Ok(value) = HeaderValue::from_str(&cookie_value) {
                headers.insert(COOKIE, value);
            }
        }

        headers
    }

    /// Build the Edge API URL for a given endpoint
    fn build_url(&self, endpoint: &str) -> String {
        format!("{}/{}/{}", self.base_url, self.team_id, endpoint)
    }

    /// Initialize client session (client.boot)
    ///
    /// Returns basic information about the authenticated user and team.
    pub async fn client_boot(&self) -> Result<ClientBootResponse> {
        let url = self.build_url("client.boot");
        let headers = self.build_headers();

        let response = self
            .http
            .post(&url)
            .headers(headers)
            .json(&ClientBootRequest::default())
            .send()
            .await
            .map_err(SlackError::Network)?;

        let body: EdgeApiResponse<ClientBootResponse> =
            response.json().await.map_err(SlackError::Network)?;

        body.into_result()
    }

    /// Get conversation/channel information via Edge API
    ///
    /// Similar to conversations.view but through the Edge API.
    pub async fn conversations_view(&self, channel_id: &str) -> Result<ConversationViewResponse> {
        let url = self.build_url("conversations.view");
        let headers = self.build_headers();

        let request = ConversationViewRequest {
            channel: channel_id.to_string(),
        };

        let response = self
            .http
            .post(&url)
            .headers(headers)
            .json(&request)
            .send()
            .await
            .map_err(SlackError::Network)?;

        let body: EdgeApiResponse<ConversationViewResponse> =
            response.json().await.map_err(SlackError::Network)?;

        body.into_result()
    }

    /// Search channels via Edge API
    pub async fn search_channels(&self, query: &str, limit: u32) -> Result<SearchChannelsResponse> {
        let url = self.build_url("channels.search");
        let headers = self.build_headers();

        let request = SearchChannelsRequest {
            query: query.to_string(),
            count: limit,
        };

        let response = self
            .http
            .post(&url)
            .headers(headers)
            .json(&request)
            .send()
            .await
            .map_err(SlackError::Network)?;

        let body: EdgeApiResponse<SearchChannelsResponse> =
            response.json().await.map_err(SlackError::Network)?;

        body.into_result()
    }

    /// Make a generic Edge API request
    pub async fn request<T, P>(&self, endpoint: &str, params: &P) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
        P: Serialize,
    {
        let url = self.build_url(endpoint);
        let headers = self.build_headers();

        let response = self
            .http
            .post(&url)
            .headers(headers)
            .json(params)
            .send()
            .await
            .map_err(SlackError::Network)?;

        let body: EdgeApiResponse<T> = response.json().await.map_err(SlackError::Network)?;

        body.into_result()
    }
}

/// Generic Edge API response wrapper
#[derive(Debug, Deserialize)]
struct EdgeApiResponse<T> {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(flatten)]
    data: Option<T>,
}

impl<T> EdgeApiResponse<T> {
    fn into_result(self) -> Result<T> {
        if self.ok {
            self.data
                .ok_or_else(|| SlackError::Other("Empty response from Edge API".into()))
        } else {
            Err(SlackError::Api {
                error: self.error.unwrap_or_else(|| "unknown_error".into()),
                detail: None,
            })
        }
    }
}

/// Request for client.boot
#[derive(Debug, Serialize, Default)]
struct ClientBootRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
}

/// Response from client.boot
#[derive(Debug, Deserialize, Clone)]
pub struct ClientBootResponse {
    /// Self user information
    #[serde(rename = "self")]
    pub self_user: Option<EdgeSelfUser>,
    /// Team information
    pub team: Option<EdgeTeam>,
}

/// Self user info from Edge API
#[derive(Debug, Deserialize, Clone)]
pub struct EdgeSelfUser {
    pub id: String,
    pub name: Option<String>,
    pub real_name: Option<String>,
}

/// Team info from Edge API
#[derive(Debug, Deserialize, Clone)]
pub struct EdgeTeam {
    pub id: String,
    pub name: Option<String>,
    pub domain: Option<String>,
}

/// Request for conversations.view
#[derive(Debug, Serialize)]
struct ConversationViewRequest {
    channel: String,
}

/// Response from conversations.view
#[derive(Debug, Deserialize, Clone)]
pub struct ConversationViewResponse {
    pub channel: Option<EdgeChannel>,
}

/// Channel info from Edge API
#[derive(Debug, Deserialize, Clone)]
pub struct EdgeChannel {
    pub id: String,
    pub name: Option<String>,
    pub is_channel: Option<bool>,
    pub is_group: Option<bool>,
    pub is_im: Option<bool>,
    pub is_mpim: Option<bool>,
    pub is_private: Option<bool>,
    pub is_member: Option<bool>,
}

/// Request for channels.search
#[derive(Debug, Serialize)]
struct SearchChannelsRequest {
    query: String,
    count: u32,
}

/// Response from channels.search
#[derive(Debug, Deserialize, Clone)]
pub struct SearchChannelsResponse {
    pub channels: Option<Vec<EdgeChannelSearchResult>>,
}

/// Channel search result from Edge API
#[derive(Debug, Deserialize, Clone)]
pub struct EdgeChannelSearchResult {
    pub id: String,
    pub name: Option<String>,
    pub is_private: Option<bool>,
    pub num_members: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_browser_token() -> TokenSet {
        TokenSet {
            token_type: TokenType::Browser,
            access_token: "xoxc-1234567890-0123456789-abcdefghij".to_string(),
            xoxd_cookie: Some("xoxd-test-cookie-value".to_string()),
            team_id: "T12345".to_string(),
            team_name: "Test Team".to_string(),
            user_id: "U12345".to_string(),
            created_at: Utc::now(),
            scopes: vec![],
        }
    }

    fn create_oauth_token() -> TokenSet {
        TokenSet {
            token_type: TokenType::UserOAuth,
            access_token: "xoxp-1234567890-0123456789-abcdefghij".to_string(),
            xoxd_cookie: None,
            team_id: "T12345".to_string(),
            team_name: "Test Team".to_string(),
            user_id: "U12345".to_string(),
            created_at: Utc::now(),
            scopes: vec![],
        }
    }

    #[test]
    fn test_edge_client_requires_browser_token() {
        let browser_token = create_browser_token();
        let result = EdgeClient::new(browser_token);
        assert!(result.is_ok());

        let oauth_token = create_oauth_token();
        let result = EdgeClient::new(oauth_token);
        assert!(result.is_err());
        assert!(matches!(result, Err(SlackError::InvalidToken(_))));
    }

    #[test]
    fn test_edge_client_team_id() {
        let token = create_browser_token();
        let client = EdgeClient::new(token).unwrap();
        assert_eq!(client.team_id(), "T12345");
    }

    #[test]
    fn test_build_url() {
        let token = create_browser_token();
        let client = EdgeClient::new(token).unwrap();

        let url = client.build_url("client.boot");
        assert!(url.contains("/T12345/client.boot"));

        let url = client.build_url("conversations.view");
        assert!(url.contains("/T12345/conversations.view"));
    }

    #[test]
    fn test_edge_client_with_custom_base_url() {
        let token = create_browser_token();
        let custom_url = "http://localhost:8080".to_string();
        let client = EdgeClient::with_base_url(token, custom_url.clone()).unwrap();
        assert_eq!(client.base_url(), custom_url);

        let url = client.build_url("client.boot");
        assert_eq!(url, "http://localhost:8080/T12345/client.boot");
    }

    #[test]
    fn test_build_headers() {
        let token = create_browser_token();
        let client = EdgeClient::new(token).unwrap();
        let headers = client.build_headers();

        assert!(headers.contains_key(AUTHORIZATION));
        assert!(headers.contains_key(COOKIE));

        let auth = headers.get(AUTHORIZATION).unwrap().to_str().unwrap();
        assert!(auth.starts_with("Bearer xoxc-"));

        let cookie = headers.get(COOKIE).unwrap().to_str().unwrap();
        assert!(cookie.starts_with("d="));
    }

    #[test]
    fn test_edge_api_response_ok() {
        let response: EdgeApiResponse<ClientBootResponse> = EdgeApiResponse {
            ok: true,
            error: None,
            data: Some(ClientBootResponse {
                self_user: None,
                team: None,
            }),
        };

        assert!(response.into_result().is_ok());
    }

    #[test]
    fn test_edge_api_response_error() {
        let response: EdgeApiResponse<ClientBootResponse> = EdgeApiResponse {
            ok: false,
            error: Some("invalid_token".into()),
            data: None,
        };

        let result = response.into_result();
        assert!(result.is_err());
        match result {
            Err(SlackError::Api { error, .. }) => assert_eq!(error, "invalid_token"),
            _ => panic!("Expected SlackError::Api"),
        }
    }

    #[test]
    fn test_edge_api_response_empty_data() {
        let response: EdgeApiResponse<ClientBootResponse> = EdgeApiResponse {
            ok: true,
            error: None,
            data: None,
        };

        let result = response.into_result();
        assert!(result.is_err());
    }
}
