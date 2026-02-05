//! CLI module for Slack CLI
//!
//! Contains command-line interface definitions and handlers.

pub mod auth;
pub mod channels;
pub mod completions;
pub mod files;
pub mod messages;
pub mod reactions;
pub mod reminders;
pub mod root;
pub mod status;
pub mod users;

pub use auth::AuthCmd;
pub use channels::ChannelsCmd;
pub use completions::{generate_completions, CompletionsArgs};
pub use files::FilesCmd;
pub use messages::MessagesCmd;
pub use reactions::ReactionsCmd;
pub use reminders::RemindersCmd;
pub use root::{Cli, Commands};
pub use status::StatusCmd;
pub use users::UsersCmd;
