//! External file upload and file-search Web API operations.

use std::net::IpAddr;
use std::time::Duration;

use reqwest::header::{CONTENT_TYPE, RETRY_AFTER};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use url::Url;

use crate::api::SlackClient;
use crate::error::{Result, SlackError};
use crate::models::File;

const RAW_UPLOAD_TIMEOUT: Duration = Duration::from_secs(30);
const API_BASE_ENV: &str = "SLACK_API_BASE_URL";

/// Response from `files.getUploadURLExternal`.
#[derive(Debug, Serialize)]
pub struct GetUploadUrlResponse {
    /// Short-lived URL that accepts the file bytes.
    pub upload_url: String,
    /// Slack file ID to pass to `files.completeUploadExternal`.
    pub file_id: String,
}

/// A file submitted to `files.completeUploadExternal`.
#[derive(Debug, Serialize)]
pub struct CompleteUploadFile {
    /// File ID returned by `files.getUploadURLExternal`.
    pub id: String,
    /// Optional display title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Response from `files.completeUploadExternal`.
#[derive(Debug, Serialize, Deserialize)]
pub struct CompleteUploadResponse {
    /// Completed Slack file objects.
    pub files: Vec<File>,
}

/// Request parameters for `search.files`.
#[derive(Debug, Serialize)]
pub struct SearchFilesParams {
    /// Slack file-search query.
    pub query: String,
    /// Number of matches per page.
    pub count: u32,
    /// One-based page number.
    pub page: u32,
}

/// Response from `search.files`.
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchFilesResponse {
    /// Search result container returned by Slack.
    pub files: SearchFileResults,
}

/// File-search matches and pagination metadata.
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchFileResults {
    /// Total number of matches.
    pub total: u32,
    /// Slack's page metadata, when returned.
    #[serde(default)]
    pub pagination: Option<crate::api::SearchPagination>,
    /// Files on this page.
    pub matches: Vec<File>,
}

#[derive(Serialize)]
struct GetUploadUrlParams<'a> {
    filename: &'a str,
    length: u64,
}

#[derive(Serialize)]
struct CompleteUploadParams<'a> {
    files: Vec<CompleteUploadFile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_comment: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_ts: Option<&'a str>,
}

impl SlackClient {
    /// Request a short-lived URL for an external file upload.
    pub async fn files_get_upload_url_external(
        &self,
        filename: &str,
        length: u64,
    ) -> Result<GetUploadUrlResponse> {
        // Deserialize through Value so a malformed payload containing a signed
        // upload URL is never included in the generic parser's debug logging.
        let value: serde_json::Value = self
            .request(
                "files.getUploadURLExternal",
                &GetUploadUrlParams { filename, length },
            )
            .await?;
        let upload_url = required_string(&value, "upload_url")?;
        let file_id = required_string(&value, "file_id")?;

        Ok(GetUploadUrlResponse {
            upload_url,
            file_id,
        })
    }

    /// Complete an external upload and optionally share it to a channel.
    pub async fn files_complete_upload_external(
        &self,
        file: CompleteUploadFile,
        channel_id: Option<&str>,
        initial_comment: Option<&str>,
        thread_ts: Option<&str>,
    ) -> Result<CompleteUploadResponse> {
        if file.id.trim().is_empty() {
            return Err(invalid_response(
                "upload response contained an empty file ID",
            ));
        }

        let value: serde_json::Value = self
            .request(
                "files.completeUploadExternal",
                &CompleteUploadParams {
                    files: vec![file],
                    channel_id,
                    initial_comment,
                    thread_ts,
                },
            )
            .await?;
        let response: CompleteUploadResponse = deserialize_response(value)?;
        validate_files(&response.files, true)?;
        Ok(response)
    }

    /// Run all three stages of Slack's external file-upload flow.
    ///
    /// The raw upload is attempted exactly once and is never sent with Slack
    /// authorization headers or browser cookies.
    pub async fn files_upload_external(
        &self,
        filename: &str,
        bytes: Vec<u8>,
        title: Option<&str>,
        channel_id: Option<&str>,
        initial_comment: Option<&str>,
        thread_ts: Option<&str>,
    ) -> Result<CompleteUploadResponse> {
        let length = bytes.len() as u64;
        let target = self.files_get_upload_url_external(filename, length).await?;

        upload_raw_bytes(&target.upload_url, bytes).await?;

        self.files_complete_upload_external(
            CompleteUploadFile {
                id: target.file_id,
                title: title.map(str::to_string),
            },
            channel_id,
            initial_comment,
            thread_ts,
        )
        .await
    }

    /// Search workspace files.
    ///
    /// Slack only supports this method for user and browser tokens.
    pub async fn search_files(&self, params: SearchFilesParams) -> Result<SearchFilesResponse> {
        if !self.supports_search() {
            return Err(SlackError::SearchNotAvailable);
        }

        let value: serde_json::Value = self.request("search.files", &params).await?;
        let response: SearchFilesResponse = deserialize_response(value)?;
        validate_files(&response.files.matches, false)?;
        Ok(response)
    }
}

fn required_string(value: &serde_json::Value, field: &str) -> Result<String> {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| invalid_response(&format!("response omitted required field `{}`", field)))
}

fn deserialize_response<T: DeserializeOwned>(value: serde_json::Value) -> Result<T> {
    serde_json::from_value(value).map_err(|error| SlackError::Api {
        error: "invalid_response".to_string(),
        detail: Some(format!(
            "response omitted or contained invalid required data: {}",
            error
        )),
    })
}

fn validate_files(files: &[File], require_file: bool) -> Result<()> {
    if require_file && files.is_empty() {
        return Err(invalid_response("completion response contained no files"));
    }
    if files.iter().any(|file| file.id.trim().is_empty()) {
        return Err(invalid_response("response contained an empty file ID"));
    }
    Ok(())
}

fn invalid_response(detail: &str) -> SlackError {
    SlackError::Api {
        error: "invalid_response".to_string(),
        detail: Some(detail.to_string()),
    }
}

async fn upload_raw_bytes(upload_url: &str, bytes: Vec<u8>) -> Result<()> {
    let url = validate_upload_url(upload_url)?;
    let client = reqwest::Client::builder()
        .timeout(RAW_UPLOAD_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(false)
        .build()
        .map_err(SlackError::Network)?;

    let response = client
        .post(url)
        .header(CONTENT_TYPE, "application/octet-stream")
        .body(bytes)
        .send()
        .await
        .map_err(|error| SlackError::Network(error.without_url()))?;

    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|header| header.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(60);
        return Err(SlackError::RateLimited(retry_after));
    }

    if !response.status().is_success() {
        return Err(SlackError::Api {
            error: "upload_failed".to_string(),
            detail: Some(format!(
                "raw upload returned HTTP status {}",
                response.status().as_u16()
            )),
        });
    }

    // The upload service's response body is intentionally not read or logged.
    Ok(())
}

fn validate_upload_url(value: &str) -> Result<Url> {
    let url = Url::parse(value)
        .map_err(|_| SlackError::Usage("Slack returned an invalid upload URL".to_string()))?;

    if !url.username().is_empty() || url.password().is_some() {
        return Err(SlackError::Usage(
            "Slack returned an upload URL containing credentials".to_string(),
        ));
    }
    if url.fragment().is_some() {
        return Err(SlackError::Usage(
            "Slack returned an upload URL containing a fragment".to_string(),
        ));
    }

    let secure = url.scheme() == "https";
    let loopback_exception = url.scheme() == "http"
        && is_loopback_url(&url)
        && std::env::var(API_BASE_ENV)
            .ok()
            .and_then(|base| Url::parse(&base).ok())
            .is_some_and(|base| is_loopback_url(&base));

    if !secure && !loopback_exception {
        return Err(SlackError::Usage(
            "Slack upload URLs must use HTTPS".to_string(),
        ));
    }

    if url.host_str().is_none() {
        return Err(SlackError::Usage(
            "Slack returned an upload URL without a host".to_string(),
        ));
    }

    Ok(url)
}

fn is_loopback_url(url: &Url) -> bool {
    match url.host_str() {
        Some(host) if host.eq_ignore_ascii_case("localhost") => true,
        Some(host) => host
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false),
        None => false,
    }
}
