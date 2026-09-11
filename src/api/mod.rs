//! Slack API client and related modules
//!
//! This module provides:
//! - `SlackClient` - HTTP client for Slack's Web API
//! - `RateLimiter` - Rate limiting for API requests
//! - `EdgeClient` - Edge API client for browser tokens
//! - Response types and API method parameters
//! - Name resolution helpers

pub mod bookmark_ops;
pub mod channel_ops;
pub mod chat_ops;
mod client;
pub mod edge;
pub mod file_ops;
pub mod identity_ops;
pub mod pin_emoji_ops;
mod rate_limiter;
mod resolve;
pub mod types;
mod web;

pub use client::SlackClient;
pub use edge::EdgeClient;
pub use rate_limiter::RateLimiter;
pub use types::{
    AuthTestResponse, ChatPostMessageResponse, ConversationsHistoryResponse,
    ConversationsInfoResponse, ConversationsListResponse, ConversationsRepliesResponse,
    FilesInfoResponse, PaginationParams, ReactionsResponse, ResponseMetadata,
    SearchMessagesResponse, SearchPagination, SearchResults, SlackResponse, UsersInfoResponse,
    UsersListResponse,
};
pub use web::{
    ChatPostMessageParams, ConversationsHistoryParams, ConversationsListParams,
    ConversationsMarkParams, ConversationsRepliesParams, SearchMessagesParams,
};
