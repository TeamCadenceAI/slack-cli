//! Channels CLI commands for Slack CLI
//!
//! Handles channel operations: listing, membership, lifecycle, and unread counts.

use std::collections::{HashMap, HashSet};

use clap::{Args, Subcommand};
use serde::Serialize;

/// Channel operations commands
#[derive(Args, Debug)]
pub struct ChannelsCmd {
    #[command(subcommand)]
    pub command: ChannelsCommands,
}

/// Channel subcommands
#[derive(Subcommand, Debug)]
pub enum ChannelsCommands {
    /// List channels in the workspace
    List {
        /// Channel types to include (comma-separated)
        /// Options: public_channel, private_channel, mpim, im
        #[arg(long, default_value = "public_channel,private_channel")]
        types: String,

        /// Maximum number of channels to return
        #[arg(long)]
        limit: Option<u32>,

        /// Pagination cursor for next page
        #[arg(long)]
        cursor: Option<String>,

        /// Exclude archived channels
        #[arg(long)]
        exclude_archived: bool,

        /// Sort by popularity (member count, descending)
        #[arg(long)]
        sort_popularity: bool,
    },

    /// Show channel info
    Info {
        /// Channel name or ID
        channel: String,
    },

    /// List direct messages
    Dms {
        /// Include multi-party DMs (mpim)
        #[arg(long)]
        include_mpim: bool,
    },

    /// List members of a channel
    Members {
        /// Channel name or ID
        channel: String,

        /// Resolve member IDs to user names
        #[arg(long)]
        resolve: bool,
    },

    /// Create a channel
    Create {
        /// Channel name
        name: String,

        /// Create a private channel
        #[arg(long)]
        private: bool,
    },

    /// Join a channel
    Join {
        /// Channel name or ID
        channel: String,
    },

    /// Leave a channel
    Leave {
        /// Channel name or ID
        channel: String,
    },

    /// Archive a channel
    Archive {
        /// Channel name or ID
        channel: String,
    },

    /// Restore an archived channel
    Unarchive {
        /// Channel name or ID
        channel: String,
    },

    /// Invite one or more users to a channel
    Invite {
        /// Channel name or ID
        channel: String,

        /// User names or IDs
        #[arg(required = true, num_args = 1..)]
        users: Vec<String>,
    },

    /// Set or clear a channel topic
    SetTopic {
        /// Channel name or ID
        channel: String,

        /// New topic; pass an empty string to clear it
        text: String,
    },

    /// Set or clear a channel purpose
    SetPurpose {
        /// Channel name or ID
        channel: String,

        /// New purpose; pass an empty string to clear it
        text: String,
    },

    /// Rename a channel
    Rename {
        /// Channel name or ID
        channel: String,

        /// New channel name
        new_name: String,
    },

    /// Show channels with unread messages when Slack exposes counts
    Unread,

    /// Export all channels to CSV
    Export {
        /// Output file (stdout if not specified)
        #[arg(long, short = 'o')]
        output: Option<String>,

        /// Channel types to include
        #[arg(long, default_value = "public_channel,private_channel")]
        types: String,
    },
}

/// Run the channels command
pub async fn run(
    cmd: &ChannelsCmd,
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
        ChannelsCommands::List {
            types,
            limit,
            cursor,
            exclude_archived,
            sort_popularity,
        } => {
            list_channels(
                &client,
                types,
                *limit,
                cursor.as_deref(),
                *exclude_archived,
                *sort_popularity,
                output_mode,
            )
            .await?;
        }

        ChannelsCommands::Info { channel } => {
            info_channel(&client, channel, output_mode).await?;
        }

        ChannelsCommands::Dms { include_mpim } => {
            list_dms(&client, *include_mpim, output_mode).await?;
        }

        ChannelsCommands::Members { channel, resolve } => {
            list_members(&client, channel, *resolve, output_mode).await?;
        }

        ChannelsCommands::Create { name, private } => {
            create_channel(&client, name, *private, output_mode).await?;
        }

        ChannelsCommands::Join { channel } => {
            channel_mutation(&client, ChannelMutation::Join, channel, None, output_mode).await?;
        }

        ChannelsCommands::Leave { channel } => {
            channel_mutation(&client, ChannelMutation::Leave, channel, None, output_mode).await?;
        }

        ChannelsCommands::Archive { channel } => {
            channel_mutation(
                &client,
                ChannelMutation::Archive,
                channel,
                None,
                output_mode,
            )
            .await?;
        }

        ChannelsCommands::Unarchive { channel } => {
            channel_mutation(
                &client,
                ChannelMutation::Unarchive,
                channel,
                None,
                output_mode,
            )
            .await?;
        }

        ChannelsCommands::Invite { channel, users } => {
            invite_users(&client, channel, users, output_mode).await?;
        }

        ChannelsCommands::SetTopic { channel, text } => {
            channel_mutation(
                &client,
                ChannelMutation::SetTopic,
                channel,
                Some(text),
                output_mode,
            )
            .await?;
        }

        ChannelsCommands::SetPurpose { channel, text } => {
            channel_mutation(
                &client,
                ChannelMutation::SetPurpose,
                channel,
                Some(text),
                output_mode,
            )
            .await?;
        }

        ChannelsCommands::Rename { channel, new_name } => {
            if new_name.is_empty() {
                return Err(crate::error::SlackError::Usage(
                    "channel name cannot be empty".to_string(),
                ));
            }
            channel_mutation(
                &client,
                ChannelMutation::Rename,
                channel,
                Some(new_name),
                output_mode,
            )
            .await?;
        }

        ChannelsCommands::Unread => {
            unread_channels(&client, output_mode).await?;
        }

        ChannelsCommands::Export { output, types } => {
            export_channels(&client, types, output.as_deref()).await?;
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
        // Token override provided on command line
        let token_type = TokenType::from_prefix(token_str).ok_or_else(|| {
            SlackError::InvalidToken("Token must start with xoxp-, xoxb-, or xoxc-".into())
        })?;

        if token_type == TokenType::Browser {
            return Err(SlackError::InvalidToken(
                "Browser tokens require --xoxc and --xoxd flags in 'auth add'".into(),
            ));
        }

        // Create a temporary token set
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
            // Workspace specified
            let workspaces = store.get_workspace_info()?;
            let ws = workspaces
                .iter()
                .find(|w| {
                    crate::auth::workspace_matches(ws_name, &w.team_id, w.team_domain.as_deref())
                })
                .ok_or_else(|| SlackError::WorkspaceNotFound(ws_name.to_string()))?;
            store
                .get_token(&ws.team_id)?
                .ok_or(SlackError::AuthRequired)
        } else {
            // Use default or first workspace
            store
                .get_default_or_first()?
                .ok_or(SlackError::AuthRequired)
        }
    }
}

/// List channels
async fn list_channels(
    client: &crate::api::SlackClient,
    types: &str,
    limit: Option<u32>,
    cursor: Option<&str>,
    exclude_archived: bool,
    sort_popularity: bool,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use crate::api::ConversationsListParams;
    use crate::output::{write_channels_plain, write_json, ChannelPlain};

    let mut params = ConversationsListParams::new()
        .with_types(types)
        .exclude_archived(exclude_archived);

    if let Some(l) = limit {
        params = params.with_limit(l);
    }

    if let Some(c) = cursor {
        params = params.with_cursor(c);
    }

    let response = client.conversations_list(params).await?;

    // Get channels, optionally sorting by popularity (member count)
    let mut channels = response.channels;
    if sort_popularity {
        // Sort by num_members descending (highest first)
        channels.sort_by(|a, b| {
            let a_members = a.num_members.unwrap_or(0);
            let b_members = b.num_members.unwrap_or(0);
            b_members.cmp(&a_members)
        });
    }

    if output_mode == crate::output::OutputMode::Plain {
        let plain_channels: Vec<ChannelPlain> = channels
            .iter()
            .map(|ch| ChannelPlain {
                id: &ch.id,
                name: ch.name.as_deref(),
                num_members: ch.num_members,
                is_private: ch.is_private || ch.is_group,
            })
            .collect();
        write_channels_plain(&plain_channels)?;
    } else {
        // For JSON output, include the full response with pagination metadata
        write_json(&serde_json::json!({
            "channels": channels,
            "response_metadata": response.response_metadata,
        }))?;
    }

    Ok(())
}

/// Show channel info
async fn info_channel(
    client: &crate::api::SlackClient,
    channel_identifier: &str,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use crate::output::write_json;

    // Resolve channel name to ID if needed, then get full info
    let channel = client.resolve_channel_info(channel_identifier).await?;

    if output_mode == crate::output::OutputMode::Plain {
        // For plain output, print key fields on separate lines
        println!("id\t{}", channel.id);
        if let Some(name) = &channel.name {
            println!("name\t{}", name);
        }
        println!("type\t{}", channel.channel_type());
        println!("is_private\t{}", channel.is_private || channel.is_group);
        println!("is_archived\t{}", channel.is_archived);
        println!("is_member\t{}", channel.is_member);
        if let Some(num_members) = channel.num_members {
            println!("num_members\t{}", num_members);
        }
        if let Some(topic) = &channel.topic {
            if !topic.value.is_empty() {
                println!("topic\t{}", topic.value.replace('\n', "\\n"));
            }
        }
        if let Some(purpose) = &channel.purpose {
            if !purpose.value.is_empty() {
                println!("purpose\t{}", purpose.value.replace('\n', "\\n"));
            }
        }
        if let Some(creator) = &channel.creator {
            println!("creator\t{}", creator);
        }
        if let Some(created) = channel.created {
            println!("created\t{}", created);
        }
    } else {
        write_json(&channel)?;
    }

    Ok(())
}

/// List direct messages
async fn list_dms(
    client: &crate::api::SlackClient,
    include_mpim: bool,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use crate::output::{write_channels_plain, write_json, ChannelPlain};

    let types = if include_mpim { "im,mpim" } else { "im" };

    // Get all DMs (paginated)
    let channels = client.conversations_list_all(Some(types), false).await?;

    if output_mode == crate::output::OutputMode::Plain {
        let plain_channels: Vec<ChannelPlain> = channels
            .iter()
            .map(|ch| ChannelPlain {
                id: &ch.id,
                name: ch.name.as_deref(),
                num_members: ch.num_members,
                is_private: true, // DMs are always private
            })
            .collect();
        write_channels_plain(&plain_channels)?;
    } else {
        write_json(&channels)?;
    }

    Ok(())
}

#[derive(Serialize)]
struct ResolvedMember {
    id: String,
    user_name: String,
}

#[derive(Serialize)]
struct UnreadChannel {
    id: String,
    name: Option<String>,
    is_im: bool,
    is_mpim: bool,
    user: Option<String>,
    unread_count: u64,
}

#[derive(Clone, Copy)]
enum ChannelMutation {
    Join,
    Leave,
    Archive,
    Unarchive,
    SetTopic,
    SetPurpose,
    Rename,
}

fn escape_tsv(value: &str) -> String {
    value
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn member_user_name(user: &crate::models::User) -> String {
    user.name
        .as_deref()
        .filter(|name| !name.is_empty())
        .or_else(|| {
            user.profile
                .as_ref()
                .and_then(|profile| profile.display_name.as_deref())
                .filter(|name| !name.is_empty())
        })
        .unwrap_or(&user.id)
        .to_string()
}

async fn list_members(
    client: &crate::api::SlackClient,
    channel: &str,
    resolve: bool,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    let channel_id = client.resolve_channel(channel).await?;
    let members = client.conversations_members_all(&channel_id).await?;

    if resolve {
        let users = client.users_list_all().await?;
        let names: HashMap<String, String> = users
            .iter()
            .map(|user| (user.id.clone(), member_user_name(user)))
            .collect();
        let resolved: Vec<ResolvedMember> = members
            .iter()
            .map(|id| ResolvedMember {
                id: id.clone(),
                user_name: names.get(id).cloned().unwrap_or_else(|| id.clone()),
            })
            .collect();

        if output_mode == crate::output::OutputMode::Plain {
            for member in &resolved {
                println!("{}\t{}", member.id, escape_tsv(&member.user_name));
            }
        } else {
            crate::output::write_json(&serde_json::json!({
                "channel": channel_id,
                "members": resolved,
            }))?;
        }
    } else if output_mode == crate::output::OutputMode::Plain {
        for member in &members {
            println!("{}", member);
        }
    } else {
        crate::output::write_json(&serde_json::json!({
            "channel": channel_id,
            "members": members,
        }))?;
    }

    Ok(())
}

async fn create_channel(
    client: &crate::api::SlackClient,
    name: &str,
    private: bool,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    if name.is_empty() {
        return Err(crate::error::SlackError::Usage(
            "channel name cannot be empty".to_string(),
        ));
    }
    let response = client.conversations_create(name, private).await?;
    write_channel_mutation(response.channel, output_mode)
}

fn write_channel_mutation(
    channel: crate::models::Channel,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    if output_mode == crate::output::OutputMode::Plain {
        println!("{}", channel.id);
    } else {
        crate::output::write_json(&serde_json::json!({
            "ok": true,
            "channel": channel,
        }))?;
    }
    Ok(())
}

fn write_id_mutation(
    channel_id: &str,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    if output_mode == crate::output::OutputMode::Plain {
        println!("{}", channel_id);
    } else {
        crate::output::write_json(&serde_json::json!({
            "ok": true,
            "channel": channel_id,
        }))?;
    }
    Ok(())
}

async fn channel_mutation(
    client: &crate::api::SlackClient,
    mutation: ChannelMutation,
    channel: &str,
    value: Option<&str>,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    let channel_id = client.resolve_channel(channel).await?;
    match mutation {
        ChannelMutation::Join => {
            let response = client.conversations_join(&channel_id).await?;
            write_channel_mutation(response.channel, output_mode)
        }
        ChannelMutation::Leave => {
            client.conversations_leave(&channel_id).await?;
            write_id_mutation(&channel_id, output_mode)
        }
        ChannelMutation::Archive => {
            client.conversations_archive(&channel_id).await?;
            write_id_mutation(&channel_id, output_mode)
        }
        ChannelMutation::Unarchive => {
            client.conversations_unarchive(&channel_id).await?;
            write_id_mutation(&channel_id, output_mode)
        }
        ChannelMutation::SetTopic => {
            let response = client
                .conversations_set_topic(&channel_id, value.unwrap_or(""))
                .await?;
            write_channel_mutation(response.channel, output_mode)
        }
        ChannelMutation::SetPurpose => {
            let response = client
                .conversations_set_purpose(&channel_id, value.unwrap_or(""))
                .await?;
            write_channel_mutation(response.channel, output_mode)
        }
        ChannelMutation::Rename => {
            let response = client
                .conversations_rename(&channel_id, value.unwrap_or(""))
                .await?;
            write_channel_mutation(response.channel, output_mode)
        }
    }
}

async fn invite_users(
    client: &crate::api::SlackClient,
    channel: &str,
    users: &[String],
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    let channel_id = client.resolve_channel(channel).await?;
    let mut user_ids = Vec::new();
    let mut seen = HashSet::new();
    for user in users {
        let user_id = client.resolve_user(user).await?;
        if seen.insert(user_id.clone()) {
            user_ids.push(user_id);
        }
    }

    let response = client
        .conversations_invite(&channel_id, &user_ids.join(","))
        .await?;
    write_channel_mutation(response.channel, output_mode)
}

async fn unread_channels(
    client: &crate::api::SlackClient,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    let channels = client
        .conversations_list_all(Some("public_channel,private_channel,mpim,im"), true)
        .await?;
    let eligible: Vec<_> = channels
        .into_iter()
        .filter(|channel| {
            !channel.is_archived && (channel.is_im || channel.is_mpim || channel.is_member)
        })
        .collect();

    if eligible.is_empty() {
        if output_mode == crate::output::OutputMode::Plain {
            return Ok(());
        }
        crate::output::write_json(&serde_json::json!({
            "channels": [],
            "unavailable_channels": [],
        }))?;
        return Ok(());
    }

    let mut unread = Vec::new();
    let mut unavailable = Vec::new();
    let mut known_counts = 0usize;
    for channel in eligible {
        let response = client.conversations_info_unread(&channel.id).await?;
        let count = response
            .channel
            .unread_count_display
            .or(response.channel.unread_count);
        match count {
            Some(count) => {
                known_counts += 1;
                if count > 0 {
                    unread.push(UnreadChannel {
                        id: channel.id,
                        name: channel.name,
                        is_im: channel.is_im,
                        is_mpim: channel.is_mpim,
                        user: channel.user,
                        unread_count: count,
                    });
                }
            }
            None => unavailable.push(channel.id),
        }
    }

    if known_counts == 0 {
        return Err(crate::error::SlackError::Api {
            error: "unread_unavailable".to_string(),
            detail: Some(
                "this workspace/token does not expose unread counts through the Web API"
                    .to_string(),
            ),
        });
    }

    unread.sort_by(|left, right| {
        right
            .unread_count
            .cmp(&left.unread_count)
            .then_with(|| left.id.cmp(&right.id))
    });

    if output_mode == crate::output::OutputMode::Plain {
        for channel in &unread {
            let label = channel
                .name
                .as_deref()
                .filter(|name| !name.is_empty())
                .or(channel.user.as_deref().filter(|user| !user.is_empty()))
                .unwrap_or(&channel.id);
            println!(
                "{}\t{}\t{}",
                channel.id,
                escape_tsv(label),
                channel.unread_count
            );
        }
        if !unavailable.is_empty() {
            eprintln!(
                "warning: unread counts unavailable for {} channel(s)",
                unavailable.len()
            );
        }
    } else {
        crate::output::write_json(&serde_json::json!({
            "channels": unread,
            "unavailable_channels": unavailable,
        }))?;
    }

    Ok(())
}

/// Export channels to CSV
async fn export_channels(
    client: &crate::api::SlackClient,
    types: &str,
    output_path: Option<&str>,
) -> crate::error::Result<()> {
    // Get all channels (paginated)
    let channels = client.conversations_list_all(Some(types), false).await?;

    // Prepare CSV content
    let mut csv_content = String::from("id,name,num_members,is_private\n");

    for channel in &channels {
        let name = channel.name.as_deref().unwrap_or("");
        let num_members = channel.num_members.unwrap_or(0);
        let is_private = channel.is_private || channel.is_group;

        // Escape name if it contains comma or quotes
        let escaped_name = if name.contains(',') || name.contains('"') || name.contains('\n') {
            format!("\"{}\"", name.replace('"', "\"\""))
        } else {
            name.to_string()
        };

        csv_content.push_str(&format!(
            "{},{},{},{}\n",
            channel.id, escaped_name, num_members, is_private
        ));
    }

    // Write to file or stdout
    if let Some(path) = output_path {
        std::fs::write(path, &csv_content)?;
        eprintln!("Exported {} channels to {}", channels.len(), path);
    } else {
        print!("{}", csv_content);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    // Import the parent Cli for full parsing tests
    use crate::cli::Cli;

    #[test]
    fn test_channels_cmd_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn test_parse_channels_list_default() {
        let cli = Cli::try_parse_from(["slack", "channels", "list"]).unwrap();
        if let crate::cli::Commands::Channels(channels_cmd) = cli.command {
            if let ChannelsCommands::List {
                types,
                limit,
                cursor,
                exclude_archived,
                sort_popularity,
            } = channels_cmd.command
            {
                assert_eq!(types, "public_channel,private_channel");
                assert!(limit.is_none());
                assert!(cursor.is_none());
                assert!(!exclude_archived);
                assert!(!sort_popularity);
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Channels command");
        }
    }

    #[test]
    fn test_parse_channels_list_with_flags() {
        let cli = Cli::try_parse_from([
            "slack",
            "channels",
            "list",
            "--types",
            "public_channel",
            "--limit",
            "50",
            "--exclude-archived",
        ])
        .unwrap();
        if let crate::cli::Commands::Channels(channels_cmd) = cli.command {
            if let ChannelsCommands::List {
                types,
                limit,
                cursor,
                exclude_archived,
                sort_popularity,
            } = channels_cmd.command
            {
                assert_eq!(types, "public_channel");
                assert_eq!(limit, Some(50));
                assert!(cursor.is_none());
                assert!(exclude_archived);
                assert!(!sort_popularity);
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Channels command");
        }
    }

    #[test]
    fn test_parse_channels_list_with_cursor() {
        let cli = Cli::try_parse_from(["slack", "channels", "list", "--cursor", "abc123"]).unwrap();
        if let crate::cli::Commands::Channels(channels_cmd) = cli.command {
            if let ChannelsCommands::List { cursor, .. } = channels_cmd.command {
                assert_eq!(cursor, Some("abc123".to_string()));
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Channels command");
        }
    }

    #[test]
    fn test_parse_channels_list_with_sort_popularity() {
        let cli = Cli::try_parse_from(["slack", "channels", "list", "--sort-popularity"]).unwrap();
        if let crate::cli::Commands::Channels(channels_cmd) = cli.command {
            if let ChannelsCommands::List {
                sort_popularity, ..
            } = channels_cmd.command
            {
                assert!(sort_popularity);
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Channels command");
        }
    }

    #[test]
    fn test_parse_channels_list_with_sort_popularity_and_limit() {
        let cli = Cli::try_parse_from([
            "slack",
            "channels",
            "list",
            "--sort-popularity",
            "--limit",
            "10",
        ])
        .unwrap();
        if let crate::cli::Commands::Channels(channels_cmd) = cli.command {
            if let ChannelsCommands::List {
                sort_popularity,
                limit,
                ..
            } = channels_cmd.command
            {
                assert!(sort_popularity);
                assert_eq!(limit, Some(10));
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Channels command");
        }
    }

    #[test]
    fn test_parse_channels_info() {
        let cli = Cli::try_parse_from(["slack", "channels", "info", "C123456789"]).unwrap();
        if let crate::cli::Commands::Channels(channels_cmd) = cli.command {
            if let ChannelsCommands::Info { channel } = channels_cmd.command {
                assert_eq!(channel, "C123456789");
            } else {
                panic!("Expected Info command");
            }
        } else {
            panic!("Expected Channels command");
        }
    }

    #[test]
    fn test_parse_channels_info_by_name() {
        let cli = Cli::try_parse_from(["slack", "channels", "info", "#general"]).unwrap();
        if let crate::cli::Commands::Channels(channels_cmd) = cli.command {
            if let ChannelsCommands::Info { channel } = channels_cmd.command {
                assert_eq!(channel, "#general");
            } else {
                panic!("Expected Info command");
            }
        } else {
            panic!("Expected Channels command");
        }
    }

    #[test]
    fn test_parse_channels_dms_default() {
        let cli = Cli::try_parse_from(["slack", "channels", "dms"]).unwrap();
        if let crate::cli::Commands::Channels(channels_cmd) = cli.command {
            if let ChannelsCommands::Dms { include_mpim } = channels_cmd.command {
                assert!(!include_mpim);
            } else {
                panic!("Expected Dms command");
            }
        } else {
            panic!("Expected Channels command");
        }
    }

    #[test]
    fn test_parse_channels_dms_with_mpim() {
        let cli = Cli::try_parse_from(["slack", "channels", "dms", "--include-mpim"]).unwrap();
        if let crate::cli::Commands::Channels(channels_cmd) = cli.command {
            if let ChannelsCommands::Dms { include_mpim } = channels_cmd.command {
                assert!(include_mpim);
            } else {
                panic!("Expected Dms command");
            }
        } else {
            panic!("Expected Channels command");
        }
    }

    #[test]
    fn test_parse_channels_export_default() {
        let cli = Cli::try_parse_from(["slack", "channels", "export"]).unwrap();
        if let crate::cli::Commands::Channels(channels_cmd) = cli.command {
            if let ChannelsCommands::Export { output, types } = channels_cmd.command {
                assert!(output.is_none());
                assert_eq!(types, "public_channel,private_channel");
            } else {
                panic!("Expected Export command");
            }
        } else {
            panic!("Expected Channels command");
        }
    }

    #[test]
    fn test_parse_channels_export_with_output() {
        let cli =
            Cli::try_parse_from(["slack", "channels", "export", "-o", "channels.csv"]).unwrap();
        if let crate::cli::Commands::Channels(channels_cmd) = cli.command {
            if let ChannelsCommands::Export { output, types } = channels_cmd.command {
                assert_eq!(output, Some("channels.csv".to_string()));
                assert_eq!(types, "public_channel,private_channel");
            } else {
                panic!("Expected Export command");
            }
        } else {
            panic!("Expected Channels command");
        }
    }

    #[test]
    fn test_parse_channels_export_with_types() {
        let cli = Cli::try_parse_from([
            "slack",
            "channels",
            "export",
            "--types",
            "public_channel",
            "--output",
            "out.csv",
        ])
        .unwrap();
        if let crate::cli::Commands::Channels(channels_cmd) = cli.command {
            if let ChannelsCommands::Export { output, types } = channels_cmd.command {
                assert_eq!(output, Some("out.csv".to_string()));
                assert_eq!(types, "public_channel");
            } else {
                panic!("Expected Export command");
            }
        } else {
            panic!("Expected Channels command");
        }
    }

    #[test]
    fn test_parse_channels_members_and_create() {
        let unresolved = Cli::try_parse_from(["slack", "channels", "members", "general"]).unwrap();
        let crate::cli::Commands::Channels(unresolved) = unresolved.command else {
            panic!("Expected Channels command");
        };
        assert!(matches!(
            unresolved.command,
            ChannelsCommands::Members { resolve: false, .. }
        ));

        let cli =
            Cli::try_parse_from(["slack", "channels", "members", "general", "--resolve"]).unwrap();
        let crate::cli::Commands::Channels(cmd) = cli.command else {
            panic!("Expected Channels command");
        };
        assert!(matches!(
            cmd.command,
            ChannelsCommands::Members { channel, resolve }
                if channel == "general" && resolve
        ));

        let public = Cli::try_parse_from(["slack", "channels", "create", "public-room"]).unwrap();
        let crate::cli::Commands::Channels(public) = public.command else {
            panic!("Expected Channels command");
        };
        assert!(matches!(
            public.command,
            ChannelsCommands::Create { private: false, .. }
        ));

        let cli = Cli::try_parse_from(["slack", "channels", "create", "private-room", "--private"])
            .unwrap();
        let crate::cli::Commands::Channels(cmd) = cli.command else {
            panic!("Expected Channels command");
        };
        assert!(matches!(
            cmd.command,
            ChannelsCommands::Create { name, private }
                if name == "private-room" && private
        ));
    }

    #[test]
    fn test_parse_channels_lifecycle_variants() {
        for subcommand in ["join", "leave", "archive", "unarchive"] {
            Cli::try_parse_from(["slack", "channels", subcommand, "C123456789"]).unwrap();
        }
        Cli::try_parse_from(["slack", "channels", "set-topic", "general", "topic"]).unwrap();
        Cli::try_parse_from(["slack", "channels", "set-purpose", "general", "purpose"]).unwrap();
        Cli::try_parse_from(["slack", "channels", "rename", "general", "new-name"]).unwrap();
        Cli::try_parse_from(["slack", "channels", "unread"]).unwrap();
    }

    #[test]
    fn test_parse_channels_invite_requires_users() {
        let cli = Cli::try_parse_from([
            "slack",
            "channels",
            "invite",
            "general",
            "alice",
            "U123456789",
        ])
        .unwrap();
        let crate::cli::Commands::Channels(cmd) = cli.command else {
            panic!("Expected Channels command");
        };
        assert!(matches!(
            cmd.command,
            ChannelsCommands::Invite { channel, users }
                if channel == "general" && users == ["alice", "U123456789"]
        ));
        assert!(Cli::try_parse_from(["slack", "channels", "invite", "general"]).is_err());
    }

    #[test]
    fn test_parse_channels_alias() {
        let cli = Cli::try_parse_from(["slack", "c", "list"]).unwrap();
        if let crate::cli::Commands::Channels(_) = cli.command {
            // Success
        } else {
            panic!("Expected Channels command");
        }
    }
}
