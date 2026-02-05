//! Reactions CLI commands for Slack CLI
//!
//! Handles reaction operations: add, remove, list.

use clap::{Args, Subcommand};

/// Reaction operations commands
#[derive(Args, Debug)]
pub struct ReactionsCmd {
    #[command(subcommand)]
    pub command: ReactionsCommands,
}

/// Reaction subcommands
#[derive(Subcommand, Debug)]
pub enum ReactionsCommands {
    /// Add a reaction to a message
    Add {
        /// Channel name or ID
        channel: String,

        /// Message timestamp
        timestamp: String,

        /// Emoji name (without colons, e.g., "thumbsup")
        emoji: String,
    },

    /// Remove a reaction from a message
    Remove {
        /// Channel name or ID
        channel: String,

        /// Message timestamp
        timestamp: String,

        /// Emoji name (without colons)
        emoji: String,
    },

    /// List reactions on a message
    List {
        /// Channel name or ID
        channel: String,

        /// Message timestamp
        timestamp: String,
    },
}

/// Run the reactions command
pub async fn run(
    cmd: &ReactionsCmd,
    plain: bool,
    workspace: Option<&str>,
    token_override: Option<&str>,
) -> crate::error::Result<()> {
    use crate::api::SlackClient;
    use crate::output::OutputMode;

    let output_mode = OutputMode::from_flags(plain);

    // Get the token
    let token = get_token(workspace, token_override)?;
    let client = SlackClient::new(token)?;

    match &cmd.command {
        ReactionsCommands::Add {
            channel,
            timestamp,
            emoji,
        } => {
            add_reaction(&client, channel, timestamp, emoji).await?;
        }

        ReactionsCommands::Remove {
            channel,
            timestamp,
            emoji,
        } => {
            remove_reaction(&client, channel, timestamp, emoji).await?;
        }

        ReactionsCommands::List { channel, timestamp } => {
            list_reactions(&client, channel, timestamp, output_mode).await?;
        }
    }

    Ok(())
}

/// Get the authentication token
fn get_token(
    workspace: Option<&str>,
    token_override: Option<&str>,
) -> crate::error::Result<crate::auth::TokenSet> {
    use crate::auth::{get_token_store, TokenSet, TokenType};
    use crate::error::SlackError;

    if let Some(token_str) = token_override {
        let token_type = TokenType::from_prefix(token_str).ok_or_else(|| {
            SlackError::InvalidToken("Token must start with xoxp-, xoxb-, or xoxc-".into())
        })?;

        if token_type == TokenType::Browser {
            return Err(SlackError::InvalidToken(
                "Browser tokens require --xoxc and --xoxd flags in 'auth add'".into(),
            ));
        }

        TokenSet::new_oauth(
            token_str.to_string(),
            "unknown".into(),
            "unknown".into(),
            "unknown".into(),
            vec![],
        )
    } else {
        let store = get_token_store();

        if let Some(ws_name) = workspace {
            let workspaces = store.get_workspace_info()?;
            let ws = workspaces
                .iter()
                .find(|w| w.team_id == *ws_name || w.team_name == *ws_name)
                .ok_or_else(|| SlackError::WorkspaceNotFound(ws_name.to_string()))?;
            store
                .get_token(&ws.team_id)?
                .ok_or(SlackError::AuthRequired)
        } else {
            store
                .get_default_or_first()?
                .ok_or(SlackError::AuthRequired)
        }
    }
}

/// Normalize emoji name (remove surrounding colons if present)
fn normalize_emoji(emoji: &str) -> &str {
    let emoji = emoji.strip_prefix(':').unwrap_or(emoji);
    emoji.strip_suffix(':').unwrap_or(emoji)
}

/// Add a reaction to a message
async fn add_reaction(
    client: &crate::api::SlackClient,
    channel: &str,
    timestamp: &str,
    emoji: &str,
) -> crate::error::Result<()> {
    // Resolve channel if needed
    let channel_id = client.resolve_channel(channel).await?;

    // Normalize emoji
    let emoji = normalize_emoji(emoji);

    // Add the reaction
    client.reactions_add(&channel_id, timestamp, emoji).await?;

    eprintln!("Added :{}:", emoji);
    Ok(())
}

/// Remove a reaction from a message
async fn remove_reaction(
    client: &crate::api::SlackClient,
    channel: &str,
    timestamp: &str,
    emoji: &str,
) -> crate::error::Result<()> {
    // Resolve channel if needed
    let channel_id = client.resolve_channel(channel).await?;

    // Normalize emoji
    let emoji = normalize_emoji(emoji);

    // Remove the reaction
    client
        .reactions_remove(&channel_id, timestamp, emoji)
        .await?;

    eprintln!("Removed :{}:", emoji);
    Ok(())
}

/// List reactions on a message
async fn list_reactions(
    client: &crate::api::SlackClient,
    channel: &str,
    timestamp: &str,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use crate::output::write_json;

    // Resolve channel if needed
    let channel_id = client.resolve_channel(channel).await?;

    // Get message with reactions
    let message = client.reactions_get(&channel_id, timestamp).await?;

    let reactions = message.reactions.as_ref();

    if output_mode == crate::output::OutputMode::Plain {
        if let Some(reactions) = reactions {
            for reaction in reactions {
                println!(
                    ":{}: ({})\t{}",
                    reaction.name,
                    reaction.count,
                    reaction.users.join(",")
                );
            }
        } else {
            println!("No reactions");
        }
    } else {
        write_json(&serde_json::json!({
            "reactions": reactions.unwrap_or(&vec![]),
        }))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    use crate::cli::Cli;

    #[test]
    fn test_reactions_cmd_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn test_parse_reactions_add() {
        let cli = Cli::try_parse_from([
            "slack",
            "reactions",
            "add",
            "C123",
            "1234567890.123456",
            "thumbsup",
        ])
        .unwrap();
        if let crate::cli::Commands::Reactions(reactions_cmd) = cli.command {
            if let ReactionsCommands::Add {
                channel,
                timestamp,
                emoji,
            } = reactions_cmd.command
            {
                assert_eq!(channel, "C123");
                assert_eq!(timestamp, "1234567890.123456");
                assert_eq!(emoji, "thumbsup");
            } else {
                panic!("Expected Add command");
            }
        } else {
            panic!("Expected Reactions command");
        }
    }

    #[test]
    fn test_parse_reactions_remove() {
        let cli = Cli::try_parse_from([
            "slack",
            "reactions",
            "remove",
            "C123",
            "1234567890.123456",
            "thumbsup",
        ])
        .unwrap();
        if let crate::cli::Commands::Reactions(reactions_cmd) = cli.command {
            if let ReactionsCommands::Remove {
                channel,
                timestamp,
                emoji,
            } = reactions_cmd.command
            {
                assert_eq!(channel, "C123");
                assert_eq!(timestamp, "1234567890.123456");
                assert_eq!(emoji, "thumbsup");
            } else {
                panic!("Expected Remove command");
            }
        } else {
            panic!("Expected Reactions command");
        }
    }

    #[test]
    fn test_parse_reactions_list() {
        let cli = Cli::try_parse_from(["slack", "reactions", "list", "C123", "1234567890.123456"])
            .unwrap();
        if let crate::cli::Commands::Reactions(reactions_cmd) = cli.command {
            if let ReactionsCommands::List { channel, timestamp } = reactions_cmd.command {
                assert_eq!(channel, "C123");
                assert_eq!(timestamp, "1234567890.123456");
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Reactions command");
        }
    }

    #[test]
    fn test_parse_reactions_alias() {
        let cli =
            Cli::try_parse_from(["slack", "r", "add", "C123", "1234567890.123456", "thumbsup"])
                .unwrap();
        if let crate::cli::Commands::Reactions(_) = cli.command {
            // Success
        } else {
            panic!("Expected Reactions command");
        }
    }

    #[test]
    fn test_normalize_emoji_no_colons() {
        assert_eq!(normalize_emoji("thumbsup"), "thumbsup");
    }

    #[test]
    fn test_normalize_emoji_with_colons() {
        assert_eq!(normalize_emoji(":thumbsup:"), "thumbsup");
    }

    #[test]
    fn test_normalize_emoji_partial_colons() {
        assert_eq!(normalize_emoji(":thumbsup"), "thumbsup");
        assert_eq!(normalize_emoji("thumbsup:"), "thumbsup");
    }
}
