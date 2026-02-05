use thiserror::Error;

#[derive(Error, Debug)]
pub enum SlackError {
    #[error("Authentication required. Run: slack auth add")]
    AuthRequired,

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Workspace not found: {0}")]
    WorkspaceNotFound(String),

    #[error("Channel not found: {0}")]
    ChannelNotFound(String),

    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("Slack API error: {error}")]
    Api {
        error: String,
        detail: Option<String>,
    },

    #[error("Rate limited. Retry after {0} seconds")]
    RateLimited(u64),

    #[error("Search not available for bot tokens")]
    SearchNotAvailable,

    #[error("File too large (max 5MB)")]
    FileTooLarge,

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Keyring error: {0}")]
    Keyring(#[from] keyring::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Usage(String),

    #[error("{0}")]
    Other(String),
}

impl SlackError {
    pub fn exit_code(&self) -> i32 {
        match self {
            SlackError::Usage(_) => 2,
            _ => 1,
        }
    }

    pub fn code(&self) -> &str {
        match self {
            SlackError::AuthRequired => "auth_required",
            SlackError::InvalidToken(_) => "invalid_token",
            SlackError::WorkspaceNotFound(_) => "workspace_not_found",
            SlackError::ChannelNotFound(_) => "channel_not_found",
            SlackError::UserNotFound(_) => "user_not_found",
            SlackError::Api { .. } => "api_error",
            SlackError::RateLimited(_) => "rate_limited",
            SlackError::SearchNotAvailable => "search_not_available",
            SlackError::FileTooLarge => "file_too_large",
            SlackError::Network(_) => "network_error",
            SlackError::Config(_) => "config_error",
            SlackError::Keyring(_) => "keyring_error",
            SlackError::Serialization(_) => "serialization_error",
            SlackError::Io(_) => "io_error",
            SlackError::Usage(_) => "usage_error",
            SlackError::Other(_) => "unknown_error",
        }
    }

    pub fn detail(&self) -> Option<String> {
        match self {
            SlackError::Api { detail, .. } => detail.clone(),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, SlackError>;
