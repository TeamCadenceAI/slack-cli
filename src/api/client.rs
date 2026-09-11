//! Slack API client
//!
//! HTTP client for making authenticated requests to Slack's Web API.

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, COOKIE};
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;

use crate::auth::TokenSet;
use crate::error::{Result, SlackError};

use super::rate_limiter::RateLimiter;
use super::types::parse_slack_response_value;

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

/// Check whether `s` is a syntactically valid Slack Web API method name
/// (e.g. `chat.postMessage`, `admin.users.list`, `api.test`).
///
/// Segments of ASCII alphanumerics/underscores separated by single dots.
/// This rejects path separators, percent-encoding, whitespace, traversal
/// sequences and every other URL trick by construction.
fn is_valid_method_name(s: &str) -> bool {
    !s.is_empty()
        && s.split('.').all(|seg| {
            !seg.is_empty() && seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}

/// Normalize a user-supplied endpoint into a bare Slack method name.
///
/// Accepts either:
/// - a bare method name (`chat.postMessage`), or
/// - a full canonical URL `https://slack.com/api/<method>`.
///
/// Full URLs are strictly validated (https only, exact `slack.com` host, no
/// userinfo, no non-default port, no query, no fragment, exactly one method
/// path segment under `/api/`) and normalized to the bare method name so the
/// request is always issued against the configured base URL (which keeps
/// `SLACK_API_BASE_URL` usable for mocks). Everything else is rejected
/// before any HTTP request is made.
pub(crate) fn normalize_api_endpoint(endpoint: &str) -> Result<String> {
    let endpoint = endpoint.trim();

    if endpoint.is_empty() {
        return Err(SlackError::Usage(
            "API method must not be empty".to_string(),
        ));
    }

    // Bare method name: the common, safe case.
    if is_valid_method_name(endpoint) {
        return Ok(endpoint.to_string());
    }

    // Otherwise it must be a full canonical Slack API URL.
    let url = url::Url::parse(endpoint).map_err(|_| {
        SlackError::Usage(format!(
            "invalid API method '{}': expected a method name like 'chat.postMessage' \
             or a full URL like 'https://slack.com/api/chat.postMessage'",
            endpoint
        ))
    })?;

    if url.scheme() != "https" {
        return Err(SlackError::Usage(format!(
            "invalid API URL '{}': only https:// URLs are allowed",
            endpoint
        )));
    }

    if !url.username().is_empty() || url.password().is_some() {
        return Err(SlackError::Usage(format!(
            "invalid API URL '{}': credentials in the URL are not allowed",
            endpoint
        )));
    }

    if url.host_str() != Some("slack.com") {
        return Err(SlackError::Usage(format!(
            "invalid API URL '{}': host must be exactly slack.com",
            endpoint
        )));
    }

    // `Url` drops the default port (443) during parsing, so any remaining
    // explicit port is a non-default one.
    if url.port().is_some() {
        return Err(SlackError::Usage(format!(
            "invalid API URL '{}': a custom port is not allowed",
            endpoint
        )));
    }

    if url.query().is_some() {
        return Err(SlackError::Usage(format!(
            "invalid API URL '{}': query strings are not allowed (pass parameters as fields)",
            endpoint
        )));
    }

    if url.fragment().is_some() {
        return Err(SlackError::Usage(format!(
            "invalid API URL '{}': fragments are not allowed",
            endpoint
        )));
    }

    // Path must be exactly /api/<method>. `Url::parse` has already resolved
    // `.`/`..` segments, and percent-encoded characters remain encoded in
    // `path()` so they fail the method-name check below.
    let method = url
        .path()
        .strip_prefix("/api/")
        .filter(|m| is_valid_method_name(m))
        .ok_or_else(|| {
            SlackError::Usage(format!(
                "invalid API URL '{}': path must be exactly /api/<method>",
                endpoint
            ))
        })?;

    Ok(method.to_string())
}

/// Truncate a string to at most `max_chars` characters for error details.
fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}… (truncated)", truncated)
    }
}

/// Build the HTTP clients used by `SlackClient`.
///
/// Returns `(default, no_redirect)`. The default client follows redirects
/// (needed for file downloads); the no-redirect client is used for generic
/// `api_request` calls so an attacker-controlled redirect can never leak the
/// Authorization header or session cookie to another host.
fn build_http_clients() -> Result<(reqwest::Client, reqwest::Client)> {
    let default = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(SlackError::Network)?;
    let no_redirect = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(SlackError::Network)?;
    Ok((default, no_redirect))
}

/// Convert arbitrary serializable params into form fields for
/// `application/x-www-form-urlencoded` requests.
///
/// The Slack Web API expects form-encoded bodies; with browser (xoxc) tokens
/// several endpoints silently ignore JSON bodies entirely (responding with
/// `invalid_arguments` / `missing_charset`). Scalar values are stringified and
/// nested arrays/objects (e.g. `blocks`, `attachments`, `profile`) are encoded
/// as JSON strings, which is what the Web API expects for those fields.
pub(crate) fn to_form_params<P>(params: &P) -> Result<Vec<(String, String)>>
where
    P: Serialize + ?Sized,
{
    let value = serde_json::to_value(params)
        .map_err(|e| SlackError::Other(format!("failed to serialize request parameters: {}", e)))?;

    let mut pairs = Vec::new();
    if let serde_json::Value::Object(map) = value {
        for (key, v) in map {
            match v {
                serde_json::Value::Null => {}
                serde_json::Value::String(s) => pairs.push((key, s)),
                serde_json::Value::Bool(b) => pairs.push((key, b.to_string())),
                serde_json::Value::Number(n) => pairs.push((key, n.to_string())),
                // Arrays and objects are sent as JSON strings (blocks, attachments, ...)
                other => pairs.push((key, other.to_string())),
            }
        }
    }
    Ok(pairs)
}

/// Slack API client
#[derive(Clone)]
pub struct SlackClient {
    http: reqwest::Client,
    /// Client with redirects disabled, used for generic `api_request` calls
    /// so credentials can never follow a redirect off-host.
    http_no_redirect: reqwest::Client,
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

        let (http, http_no_redirect) = build_http_clients()?;

        Ok(Self {
            http,
            http_no_redirect,
            token,
            rate_limiter: RateLimiter::new(),
            base_url,
        })
    }

    /// Create a new Slack client with a custom rate limiter
    pub fn with_rate_limiter(token: TokenSet, rate_limiter: RateLimiter) -> Result<Self> {
        token.validate()?;

        let (http, http_no_redirect) = build_http_clients()?;

        Ok(Self {
            http,
            http_no_redirect,
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
        let form_params = to_form_params(params)?;

        let mut retries = 0;
        let mut backoff = INITIAL_BACKOFF_MS;

        loop {
            // Wait for rate limiter
            self.rate_limiter.acquire().await;

            let response = self
                .http
                .post(&url)
                .headers(headers.clone())
                .form(&form_params)
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

            // Parse response: deserialize to a Value first so that payload
            // deserialization failures produce a real error instead of being
            // silently swallowed into `None` by `#[serde(flatten)]`.
            let body = response.text().await.map_err(SlackError::Network)?;
            let value: serde_json::Value =
                serde_json::from_str(&body).map_err(|e| SlackError::Api {
                    error: "invalid_response".to_string(),
                    detail: Some(format!("response was not valid JSON: {}", e)),
                })?;

            return parse_slack_response_value(value);
        }
    }

    /// Make a generic request to an arbitrary Slack Web API method and return
    /// the full raw JSON response (escape hatch, `slack api`).
    ///
    /// `endpoint` is a bare method name (`chat.postMessage`) or a full
    /// canonical `https://slack.com/api/<method>` URL, validated and
    /// normalized before any HTTP request via `normalize_api_endpoint`.
    ///
    /// Only GET and POST are supported. GET sends `params` as query
    /// parameters; POST sends them form-encoded — both use the same encoding
    /// as `to_form_params`: scalars stringified, nested arrays/objects
    /// encoded as JSON strings, and explicit JSON `null` values omitted
    /// entirely.
    ///
    /// Redirects are disabled so tokens/cookies cannot leak to another host.
    /// Rate-limit (429) responses are retried with bounded backoff, exactly
    /// like [`SlackClient::request`]; no other status (including 5xx) is
    /// retried, so mutations are never replayed.
    pub async fn api_request(
        &self,
        endpoint: &str,
        http_method: reqwest::Method,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        // Validate everything before any network I/O.
        let method_name = normalize_api_endpoint(endpoint)?;

        if http_method != reqwest::Method::GET && http_method != reqwest::Method::POST {
            return Err(SlackError::Usage(format!(
                "unsupported HTTP method '{}': only GET and POST are allowed",
                http_method
            )));
        }

        match params {
            serde_json::Value::Null | serde_json::Value::Object(_) => {}
            _ => {
                return Err(SlackError::Usage(
                    "request parameters must be a JSON object".to_string(),
                ))
            }
        }

        let pairs = to_form_params(params)?;
        let url = format!("{}/{}", self.base_url, method_name);
        let headers = self.build_auth_headers();

        let mut retries = 0;
        let mut backoff = INITIAL_BACKOFF_MS;

        loop {
            // Wait for rate limiter
            self.rate_limiter.acquire().await;

            let builder = if http_method == reqwest::Method::GET {
                self.http_no_redirect.get(&url).query(&pairs)
            } else {
                self.http_no_redirect.post(&url).form(&pairs)
            };

            let response = builder
                .headers(headers.clone())
                .send()
                .await
                .map_err(SlackError::Network)?;

            let status = response.status();

            // Bounded retries on 429, mirroring `request`. 429 means the
            // request was not executed, so retrying is safe for mutations.
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                if retries >= MAX_RETRIES {
                    let retry_after = response
                        .headers()
                        .get("retry-after")
                        .and_then(|h| h.to_str().ok())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(60);
                    return Err(SlackError::RateLimited(retry_after));
                }

                let retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok());
                // Do not overflow or block indefinitely on an excessive server
                // delay. Return it to the caller rather than retrying too early.
                if let Some(seconds) = retry_after {
                    if seconds > 60 {
                        return Err(SlackError::RateLimited(seconds));
                    }
                }
                let wait_time = retry_after.map(|s| s * 1000).unwrap_or(backoff);

                tokio::time::sleep(Duration::from_millis(wait_time)).await;

                retries += 1;
                backoff *= 2;
                continue;
            }

            // Redirects are disabled: a 3xx here means the server tried to
            // send us elsewhere. Fail loudly instead of following.
            if status.is_redirection() {
                return Err(SlackError::Api {
                    error: format!("HTTP {}", status),
                    detail: Some(
                        "server attempted a redirect; redirects are disabled for generic API \
                         requests to protect credentials"
                            .to_string(),
                    ),
                });
            }

            let body = response.text().await.map_err(SlackError::Network)?;

            let value: serde_json::Value = match serde_json::from_str(&body) {
                Ok(v) => v,
                Err(e) => {
                    // Malformed body: report the HTTP status when it already
                    // indicates failure, otherwise the parse problem.
                    if !status.is_success() {
                        return Err(SlackError::Api {
                            error: format!("HTTP {}", status),
                            detail: Some(format!(
                                "response body was not valid JSON: {}",
                                truncate_str(&body, 300)
                            )),
                        });
                    }
                    return Err(SlackError::Api {
                        error: "invalid_response".to_string(),
                        detail: Some(format!("response was not valid JSON: {}", e)),
                    });
                }
            };

            // Slack-level failure takes precedence regardless of HTTP status.
            if value.get("ok").and_then(|v| v.as_bool()) == Some(false) {
                let error = value
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown_error")
                    .to_string();
                let detail = value
                    .get("response_metadata")
                    .and_then(|m| m.get("messages"))
                    .and_then(|m| m.as_array())
                    .map(|msgs| {
                        msgs.iter()
                            .filter_map(|m| m.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .filter(|s| !s.is_empty());
                return Err(SlackError::Api { error, detail });
            }

            // Valid JSON without `ok: false` but a failing HTTP status is
            // still a failure (e.g. proxies, gateways).
            if !status.is_success() {
                return Err(SlackError::Api {
                    error: format!("HTTP {}", status),
                    detail: Some(truncate_str(&body, 300)),
                });
            }

            // Success: return the full raw JSON response (including `ok`,
            // warnings and metadata) untouched.
            return Ok(value);
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
                team_domain: None,
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
                team_domain: None,
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
                team_domain: None,
                user_id: "U12345".to_string(),
                created_at: Utc::now(),
                scopes: vec![],
            },
        }
    }

    #[test]
    fn test_to_form_params_scalars_and_optionals() {
        use crate::api::web::ConversationsRepliesParams;

        let params = ConversationsRepliesParams::new("C0BQE5V7UHH", "1786937494.427139")
            .with_limit(30)
            .with_cursor("abc");

        let mut pairs = to_form_params(&params).unwrap();
        pairs.sort();

        assert_eq!(
            pairs,
            vec![
                ("channel".to_string(), "C0BQE5V7UHH".to_string()),
                ("cursor".to_string(), "abc".to_string()),
                ("limit".to_string(), "30".to_string()),
                ("ts".to_string(), "1786937494.427139".to_string()),
            ]
        );
    }

    #[test]
    fn test_to_form_params_skips_none_fields() {
        use crate::api::web::ConversationsRepliesParams;

        let params = ConversationsRepliesParams::new("C123", "1.2");
        let pairs = to_form_params(&params).unwrap();

        assert_eq!(pairs.len(), 2);
        assert!(!pairs.iter().any(|(k, _)| k == "limit" || k == "cursor"));
    }

    #[test]
    fn test_to_form_params_bool_and_nested_json() {
        #[derive(Serialize)]
        struct Params {
            channel: String,
            inclusive: bool,
            blocks: serde_json::Value,
        }

        let params = Params {
            channel: "C123".to_string(),
            inclusive: true,
            blocks: serde_json::json!([{"type": "section"}]),
        };

        let pairs = to_form_params(&params).unwrap();
        assert!(pairs.contains(&("inclusive".to_string(), "true".to_string())));
        assert!(pairs.contains(&("blocks".to_string(), r#"[{"type":"section"}]"#.to_string())));
    }

    #[test]
    fn test_to_form_params_unit_params() {
        let pairs = to_form_params(&()).unwrap();
        assert!(pairs.is_empty());
    }

    #[test]
    fn test_to_form_params_json_value_null_fields_omitted() {
        // api_request feeds serde_json::Value params through to_form_params:
        // explicit nulls must be omitted entirely (predictable null handling).
        let params = serde_json::json!({"channel": "C1", "thread_ts": null});
        let pairs = to_form_params(&params).unwrap();
        assert_eq!(pairs, vec![("channel".to_string(), "C1".to_string())]);

        // Null params (no fields at all) encode as an empty pair list.
        assert!(to_form_params(&serde_json::Value::Null).unwrap().is_empty());
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

    // --- normalize_api_endpoint / api_request validation -------------------

    #[test]
    fn test_normalize_endpoint_bare_method_names() {
        for m in [
            "api.test",
            "chat.postMessage",
            "admin.users.list",
            "conversations.history",
            "users.profile.set",
            "team_info", // underscores allowed within a segment
        ] {
            assert_eq!(normalize_api_endpoint(m).unwrap(), m, "method {}", m);
        }
    }

    #[test]
    fn test_normalize_endpoint_trims_whitespace() {
        assert_eq!(
            normalize_api_endpoint("  chat.postMessage ").unwrap(),
            "chat.postMessage"
        );
    }

    #[test]
    fn test_normalize_endpoint_full_url_is_normalized() {
        assert_eq!(
            normalize_api_endpoint("https://slack.com/api/chat.postMessage").unwrap(),
            "chat.postMessage"
        );
        // Host case-insensitivity (Url lowercases the host)
        assert_eq!(
            normalize_api_endpoint("https://SLACK.COM/api/auth.test").unwrap(),
            "auth.test"
        );
        // Explicit default port is normalized away by Url and is fine
        assert_eq!(
            normalize_api_endpoint("https://slack.com:443/api/auth.test").unwrap(),
            "auth.test"
        );
    }

    #[test]
    fn test_normalize_endpoint_rejects_unsafe_inputs() {
        let bad = [
            "",
            "   ",
            ".",
            "..",
            "chat..postMessage",
            ".chat.postMessage",
            "chat.postMessage.",
            "chat/postMessage",
            "../auth.test",
            "chat.postMessage?foo=bar",
            "chat.post Message",
            "chat.postMessage#frag",
            // Wrong scheme
            "http://slack.com/api/auth.test",
            "ftp://slack.com/api/auth.test",
            "file:///etc/passwd",
            // Wrong host / lookalikes
            "https://evil.com/api/auth.test",
            "https://slack.com.evil.com/api/auth.test",
            "https://api.slack.com/api/auth.test",
            "https://slack.com@evil.com/api/auth.test",
            // Userinfo
            "https://user:pass@slack.com/api/auth.test",
            "https://user@slack.com/api/auth.test",
            // Non-default port
            "https://slack.com:8443/api/auth.test",
            // Query / fragment
            "https://slack.com/api/auth.test?token=x",
            "https://slack.com/api/auth.test#frag",
            // Path tricks
            "https://slack.com/api/",
            "https://slack.com/api",
            "https://slack.com/auth.test",
            "https://slack.com/api/auth.test/extra",
            "https://slack.com/api/auth.test/",
            "https://slack.com/api/../admin",
            "https://slack.com/api/%2e%2e/admin",
            "https://slack.com/api/auth%2Etest",
            "https://slack.com//api/auth.test",
        ];
        for input in bad {
            let result = normalize_api_endpoint(input);
            match result {
                Err(SlackError::Usage(_)) => {}
                other => panic!("expected Usage error for {:?}, got {:?}", input, other),
            }
        }
    }

    #[tokio::test]
    async fn test_api_request_rejects_bad_endpoint_before_http() {
        let token = create_test_token(TokenType::UserOAuth);
        // Unroutable base URL: if validation didn't happen first, this would
        // fail with a network error instead of a usage error.
        let client = SlackClient::with_base_url(token, "http://127.0.0.1:1".to_string()).unwrap();
        let err = client
            .api_request(
                "https://evil.com/api/auth.test",
                reqwest::Method::GET,
                &serde_json::Value::Null,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, SlackError::Usage(_)), "got {:?}", err);
    }

    #[tokio::test]
    async fn test_api_request_rejects_unsupported_http_method() {
        let token = create_test_token(TokenType::UserOAuth);
        let client = SlackClient::with_base_url(token, "http://127.0.0.1:1".to_string()).unwrap();
        for method in [
            reqwest::Method::PUT,
            reqwest::Method::DELETE,
            reqwest::Method::PATCH,
            reqwest::Method::HEAD,
        ] {
            let err = client
                .api_request("auth.test", method.clone(), &serde_json::Value::Null)
                .await
                .unwrap_err();
            assert!(
                matches!(err, SlackError::Usage(_)),
                "method {} should be rejected, got {:?}",
                method,
                err
            );
        }
    }

    #[tokio::test]
    async fn test_api_request_rejects_non_object_params() {
        let token = create_test_token(TokenType::UserOAuth);
        let client = SlackClient::with_base_url(token, "http://127.0.0.1:1".to_string()).unwrap();
        for params in [
            serde_json::json!([1, 2]),
            serde_json::json!("str"),
            serde_json::json!(42),
            serde_json::json!(true),
        ] {
            let err = client
                .api_request("auth.test", reqwest::Method::POST, &params)
                .await
                .unwrap_err();
            assert!(matches!(err, SlackError::Usage(_)), "got {:?}", err);
        }
    }

    #[tokio::test]
    async fn test_api_request_get_encodes_query_params() {
        use mockito::{Matcher, Server};

        let mut server = Server::new_async().await;
        let m = server
            .mock("GET", "/conversations.history")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("channel".into(), "C123".into()),
                Matcher::UrlEncoded("limit".into(), "5".into()),
                Matcher::UrlEncoded("inclusive".into(), "true".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ok": true, "messages": []}"#)
            .create_async()
            .await;

        let token = create_test_token(TokenType::UserOAuth);
        let client = SlackClient::with_base_url(token, server.url()).unwrap();

        let params = serde_json::json!({"channel": "C123", "limit": 5, "inclusive": true});
        let value = client
            .api_request("conversations.history", reqwest::Method::GET, &params)
            .await
            .unwrap();

        assert_eq!(value["ok"], serde_json::json!(true));
        m.assert_async().await;
    }

    #[tokio::test]
    async fn test_api_request_post_form_encodes_nested_json_and_skips_null() {
        use mockito::{Matcher, Server};

        let mut server = Server::new_async().await;
        let m = server
            .mock("POST", "/chat.postMessage")
            .match_header(
                "content-type",
                Matcher::Regex("application/x-www-form-urlencoded".to_string()),
            )
            .match_header("authorization", Matcher::Regex("Bearer xoxp-".to_string()))
            .match_body(Matcher::AllOf(vec![
                Matcher::UrlEncoded("channel".into(), "C123".into()),
                Matcher::UrlEncoded("blocks".into(), r#"[{"type":"section"}]"#.into()),
                Matcher::UrlEncoded("count".into(), "3".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ok": true, "ts": "1.2"}"#)
            .create_async()
            .await;

        let token = create_test_token(TokenType::UserOAuth);
        let client = SlackClient::with_base_url(token, server.url()).unwrap();

        let params = serde_json::json!({
            "channel": "C123",
            "blocks": [{"type": "section"}],
            "count": 3,
            "thread_ts": null
        });
        let value = client
            .api_request("chat.postMessage", reqwest::Method::POST, &params)
            .await
            .unwrap();

        assert_eq!(value["ts"], serde_json::json!("1.2"));
        m.assert_async().await;
    }

    #[tokio::test]
    async fn test_api_request_slack_error_maps_to_api_error() {
        use mockito::Server;

        let mut server = Server::new_async().await;
        let _m = server
            .mock("POST", "/chat.postMessage")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"ok": false, "error": "channel_not_found",
                   "response_metadata": {"messages": ["[ERROR] no such channel"]}}"#,
            )
            .create_async()
            .await;

        let token = create_test_token(TokenType::UserOAuth);
        let client = SlackClient::with_base_url(token, server.url()).unwrap();

        let err = client
            .api_request(
                "chat.postMessage",
                reqwest::Method::POST,
                &serde_json::json!({"channel": "C404"}),
            )
            .await
            .unwrap_err();

        match err {
            SlackError::Api { error, detail } => {
                assert_eq!(error, "channel_not_found");
                assert!(detail.unwrap().contains("no such channel"));
            }
            other => panic!("expected Api error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_api_request_http_error_without_ok_field() {
        use mockito::Server;

        let mut server = Server::new_async().await;
        let _m = server
            .mock("GET", "/auth.test")
            .with_status(502)
            .with_header("content-type", "application/json")
            .with_body(r#"{"message": "bad gateway"}"#)
            .expect(1) // 5xx must NOT be retried
            .create_async()
            .await;

        let token = create_test_token(TokenType::UserOAuth);
        let client = SlackClient::with_base_url(token, server.url()).unwrap();

        let err = client
            .api_request("auth.test", reqwest::Method::GET, &serde_json::Value::Null)
            .await
            .unwrap_err();

        match err {
            SlackError::Api { error, .. } => assert!(error.contains("502"), "got {}", error),
            other => panic!("expected Api error, got {:?}", other),
        }
        server.reset();
    }

    #[tokio::test]
    async fn test_api_request_malformed_json_response() {
        use mockito::Server;

        let mut server = Server::new_async().await;
        let _m = server
            .mock("GET", "/auth.test")
            .with_status(200)
            .with_body("<html>not json</html>")
            .create_async()
            .await;

        let token = create_test_token(TokenType::UserOAuth);
        let client = SlackClient::with_base_url(token, server.url()).unwrap();

        let err = client
            .api_request("auth.test", reqwest::Method::GET, &serde_json::Value::Null)
            .await
            .unwrap_err();

        match err {
            SlackError::Api { error, .. } => assert_eq!(error, "invalid_response"),
            other => panic!("expected Api error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_api_request_does_not_follow_redirects() {
        use mockito::Server;

        // "Attacker" server that must never receive our credentials.
        let mut attacker = Server::new_async().await;
        let leak = attacker
            .mock("GET", "/steal")
            .with_status(200)
            .with_body(r#"{"ok": true}"#)
            .expect(0)
            .create_async()
            .await;

        let mut server = Server::new_async().await;
        let _m = server
            .mock("GET", "/auth.test")
            .with_status(302)
            .with_header("location", &format!("{}/steal", attacker.url()))
            .create_async()
            .await;

        let token = create_test_token(TokenType::Browser);
        let client = SlackClient::with_base_url(token, server.url()).unwrap();

        let err = client
            .api_request("auth.test", reqwest::Method::GET, &serde_json::Value::Null)
            .await
            .unwrap_err();

        match err {
            SlackError::Api { error, detail } => {
                assert!(error.contains("302"), "got {}", error);
                assert!(detail.unwrap().contains("redirect"));
            }
            other => panic!("expected Api error, got {:?}", other),
        }

        // The redirect target must never have been contacted.
        leak.assert_async().await;
    }

    #[tokio::test]
    async fn test_api_request_bounded_429_retries() {
        use mockito::Server;

        let mut server = Server::new_async().await;
        // Always 429 with a tiny retry-after; after MAX_RETRIES the client
        // must give up with RateLimited (MAX_RETRIES + 1 total requests).
        let m = server
            .mock("POST", "/chat.postMessage")
            .with_status(429)
            .with_header("retry-after", "0")
            .expect((MAX_RETRIES + 1) as usize)
            .create_async()
            .await;

        let token = create_test_token(TokenType::UserOAuth);
        let client = SlackClient::with_base_url(token, server.url()).unwrap();

        let err = client
            .api_request(
                "chat.postMessage",
                reqwest::Method::POST,
                &serde_json::json!({"channel": "C123"}),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, SlackError::RateLimited(_)), "got {:?}", err);
        m.assert_async().await;
    }

    #[tokio::test]
    async fn test_api_request_browser_token_sends_cookie() {
        use mockito::{Matcher, Server};

        let mut server = Server::new_async().await;
        let m = server
            .mock("GET", "/auth.test")
            .match_header("authorization", Matcher::Regex("Bearer xoxc-".to_string()))
            .match_header("cookie", Matcher::Regex("d=xoxd-test-cookie".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ok": true, "team_id": "T12345"}"#)
            .create_async()
            .await;

        let token = create_test_token(TokenType::Browser);
        let client = SlackClient::with_base_url(token, server.url()).unwrap();

        let value = client
            .api_request("auth.test", reqwest::Method::GET, &serde_json::Value::Null)
            .await
            .unwrap();

        assert_eq!(value["team_id"], serde_json::json!("T12345"));
        m.assert_async().await;
    }

    #[tokio::test]
    async fn test_api_request_full_url_normalized_to_base_url() {
        use mockito::Server;

        // Passing the canonical https://slack.com/api/<method> URL must still
        // hit the configured (mock) base URL, proving normalization to a bare
        // method name.
        let mut server = Server::new_async().await;
        let m = server
            .mock("GET", "/auth.test")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ok": true}"#)
            .create_async()
            .await;

        let token = create_test_token(TokenType::UserOAuth);
        let client = SlackClient::with_base_url(token, server.url()).unwrap();

        let value = client
            .api_request(
                "https://slack.com/api/auth.test",
                reqwest::Method::GET,
                &serde_json::Value::Null,
            )
            .await
            .unwrap();

        assert_eq!(value["ok"], serde_json::json!(true));
        m.assert_async().await;
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
