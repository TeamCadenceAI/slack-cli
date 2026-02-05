//! Authentication module for Slack CLI
//!
//! Provides token types, validation, keyring storage, OAuth flow, and browser token support.

pub mod browser;
pub mod oauth;
mod storage;
pub mod store;
mod tokens;

pub use browser::{print_extraction_instructions, BrowserTokens};
pub use oauth::{OAuthConfig, OAuthFlow, DEFAULT_SCOPES};
pub use storage::{KeyringStore, WorkspaceInfo};
pub use store::{
    get_token_store, FileTokenStore, KeyringTokenStore, TokenStore, TOKEN_STORE_PATH_ENV,
};
pub use tokens::{TokenSet, TokenType};
