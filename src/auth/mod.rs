//! Authentication module for Slack CLI
//!
//! Provides token types, validation, keyring storage, OAuth flow, and browser token support.

pub mod browser;
pub mod extract;
pub mod oauth;
mod resolve;
mod storage;
pub mod store;
mod tokens;

pub use browser::{print_extraction_instructions, BrowserTokens};
pub use extract::{
    discover_workspaces, extract_workspaces, DiscoveredWorkspace, ExtractOptions,
    ExtractedWorkspace,
};
pub use oauth::{OAuthConfig, OAuthFlow, DEFAULT_SCOPES};
pub use resolve::resolve_token;
pub use storage::{KeyringStore, WorkspaceInfo};
pub use store::{
    get_token_store, FileTokenStore, KeyringTokenStore, TokenStore, TOKEN_STORE_PATH_ENV,
};
pub use tokens::{normalize_workspace_domain, workspace_matches, TokenSet, TokenType};
