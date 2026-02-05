//! Data models for Slack API responses
//!
//! These structs are designed to deserialize Slack API JSON responses
//! with serde. Most fields are optional to handle partial responses.

mod channel;
mod file;
mod message;
mod reaction;
mod user;

pub use channel::{Channel, ChannelPurpose, ChannelTopic};
pub use file::File;
pub use message::{
    Attachment, AttachmentField, BotIcons, BotProfile, ChannelInfo, EditInfo, Message, MessageFile,
    MessageIcons,
};
pub use reaction::Reaction;
pub use user::{EnterpriseUser, User, UserProfile};
