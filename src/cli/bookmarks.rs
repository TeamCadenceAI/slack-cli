//! Channel bookmark commands for Slack CLI.

use std::io::{self, Write};

use clap::{Args, Subcommand};
use url::Url;

use crate::api::SlackClient;
use crate::error::{Result, SlackError};
use crate::output::{write_json, OutputMode};

/// Channel bookmark operations.
#[derive(Args, Debug)]
pub struct BookmarksCmd {
    /// Bookmark command to run.
    #[command(subcommand)]
    pub command: BookmarksCommands,
}

/// Bookmark subcommands.
#[derive(Subcommand, Debug)]
pub enum BookmarksCommands {
    /// List bookmarks in a channel.
    List {
        /// Channel name or ID.
        channel: String,
    },

    /// Add a link bookmark to a channel.
    Add {
        /// Channel name or ID.
        channel: String,
        /// Bookmark title.
        title: String,
        /// Absolute HTTP or HTTPS link.
        link: String,
        /// Emoji name, with or without surrounding colons.
        #[arg(long)]
        emoji: Option<String>,
    },

    /// Remove a bookmark from a channel.
    Remove {
        /// Channel name or ID.
        channel: String,
        /// Bookmark ID.
        bookmark_id: String,
    },
}

/// Run a bookmark command.
pub async fn run(
    cmd: &BookmarksCmd,
    plain: bool,
    workspace: Option<&str>,
    token_override: Option<&str>,
) -> Result<()> {
    let token = crate::auth::resolve_token(workspace, token_override)?;
    let client = SlackClient::new(token)?;
    let output_mode = OutputMode::from_flags(plain);

    match &cmd.command {
        BookmarksCommands::List { channel } => list_bookmarks(&client, channel, output_mode).await,
        BookmarksCommands::Add {
            channel,
            title,
            link,
            emoji,
        } => add_bookmark(&client, channel, title, link, emoji.as_deref(), output_mode).await,
        BookmarksCommands::Remove {
            channel,
            bookmark_id,
        } => remove_bookmark(&client, channel, bookmark_id, output_mode).await,
    }
}

async fn list_bookmarks(
    client: &SlackClient,
    channel: &str,
    output_mode: OutputMode,
) -> Result<()> {
    let channel = client.resolve_channel(channel).await?;
    let response = client.bookmarks_list(&channel).await?;

    if output_mode == OutputMode::Plain {
        let stdout = io::stdout();
        let mut output = stdout.lock();
        for bookmark in &response.bookmarks {
            writeln!(
                output,
                "{}\t{}\t{}\t{}",
                escape_tsv(&bookmark.id),
                escape_tsv(&bookmark.title),
                escape_tsv(&bookmark.link),
                escape_tsv(bookmark.emoji.as_deref().unwrap_or(""))
            )?;
        }
    } else {
        write_json(&serde_json::json!({
            "channel": channel,
            "bookmarks": response.bookmarks,
        }))?;
    }
    Ok(())
}

async fn add_bookmark(
    client: &SlackClient,
    channel: &str,
    title: &str,
    link: &str,
    emoji: Option<&str>,
    output_mode: OutputMode,
) -> Result<()> {
    validate_title(title)?;
    validate_link(link)?;
    let emoji = emoji.map(normalize_emoji).transpose()?;

    let channel = client.resolve_channel(channel).await?;
    let response = client
        .bookmarks_add(&channel, title, link, emoji.as_deref())
        .await?;

    if output_mode == OutputMode::Plain {
        let stdout = io::stdout();
        writeln!(stdout.lock(), "{}", escape_tsv(&response.bookmark.id))?;
    } else {
        write_json(&serde_json::json!({
            "ok": true,
            "channel": channel,
            "bookmark": response.bookmark,
        }))?;
    }
    Ok(())
}

async fn remove_bookmark(
    client: &SlackClient,
    channel: &str,
    bookmark_id: &str,
    output_mode: OutputMode,
) -> Result<()> {
    if bookmark_id.trim().is_empty() {
        return Err(SlackError::Usage(
            "bookmark ID must not be empty".to_string(),
        ));
    }

    let channel = client.resolve_channel(channel).await?;
    client.bookmarks_remove(&channel, bookmark_id).await?;

    if output_mode == OutputMode::Plain {
        let stdout = io::stdout();
        writeln!(stdout.lock(), "{}", escape_tsv(bookmark_id))?;
    } else {
        write_json(&serde_json::json!({
            "ok": true,
            "channel": channel,
            "bookmark_id": bookmark_id,
        }))?;
    }
    Ok(())
}

fn validate_title(title: &str) -> Result<()> {
    if title.trim().is_empty() {
        Err(SlackError::Usage(
            "bookmark title must not be empty".to_string(),
        ))
    } else {
        Ok(())
    }
}

fn validate_link(link: &str) -> Result<()> {
    let url = Url::parse(link).map_err(|_| {
        SlackError::Usage("bookmark link must be an absolute HTTP(S) URL".to_string())
    })?;
    if link.trim() != link || !matches!(url.scheme(), "http" | "https") || !url.has_host() {
        return Err(SlackError::Usage(
            "bookmark link must be an absolute HTTP(S) URL".to_string(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() || authority_contains_at_sign(link) {
        return Err(SlackError::Usage(
            "bookmark link must not contain user information".to_string(),
        ));
    }
    Ok(())
}

fn authority_contains_at_sign(link: &str) -> bool {
    link.split_once(':')
        .map(|(_, rest)| rest.trim_start_matches(['/', '\\']))
        .and_then(|rest| rest.split(['/', '\\', '?', '#']).next())
        .map(|authority| authority.contains('@'))
        .unwrap_or(false)
}

fn normalize_emoji(emoji: &str) -> Result<String> {
    if emoji.is_empty() || emoji.trim() != emoji {
        return Err(invalid_emoji());
    }

    let name = match (emoji.strip_prefix(':'), emoji.strip_suffix(':')) {
        (Some(without_prefix), Some(_)) if emoji.len() >= 2 => {
            without_prefix.strip_suffix(':').unwrap_or(without_prefix)
        }
        (None, None) => emoji,
        _ => return Err(invalid_emoji()),
    };

    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '+'))
    {
        return Err(invalid_emoji());
    }

    Ok(format!(":{}:", name))
}

fn invalid_emoji() -> SlackError {
    SlackError::Usage("emoji must be a non-empty name with optional surrounding colons".to_string())
}

fn escape_tsv(value: &str) -> String {
    value
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::cli::{Cli, Commands};

    #[test]
    fn parse_bookmarks_list() {
        let cli = Cli::try_parse_from(["slack", "bookmarks", "list", "#general"]).unwrap();
        match cli.command {
            Commands::Bookmarks(BookmarksCmd {
                command: BookmarksCommands::List { channel },
            }) => assert_eq!(channel, "#general"),
            _ => panic!("expected bookmarks list command"),
        }
    }

    #[test]
    fn parse_bookmarks_add_defaults_emoji_to_absent() {
        let cli = Cli::try_parse_from([
            "slack",
            "bookmarks",
            "add",
            "C12345678",
            "Docs",
            "https://example.com/docs",
        ])
        .unwrap();
        match cli.command {
            Commands::Bookmarks(BookmarksCmd {
                command:
                    BookmarksCommands::Add {
                        channel,
                        title,
                        link,
                        emoji,
                    },
            }) => {
                assert_eq!(channel, "C12345678");
                assert_eq!(title, "Docs");
                assert_eq!(link, "https://example.com/docs");
                assert_eq!(emoji, None);
            }
            _ => panic!("expected bookmarks add command"),
        }
    }

    #[test]
    fn parse_bookmarks_add_with_long_emoji() {
        let cli = Cli::try_parse_from([
            "slack",
            "bookmarks",
            "add",
            "C12345678",
            "Docs",
            "https://example.com",
            "--emoji",
            ":books:",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Commands::Bookmarks(BookmarksCmd {
                command: BookmarksCommands::Add {
                    emoji: Some(ref value),
                    ..
                }
            }) if value == ":books:"
        ));
        assert!(Cli::try_parse_from([
            "slack",
            "bookmarks",
            "add",
            "C12345678",
            "Docs",
            "https://example.com",
            "-e",
            "books",
        ])
        .is_err());
    }

    #[test]
    fn parse_bookmarks_remove() {
        let cli = Cli::try_parse_from(["slack", "bookmarks", "remove", "G12345678", "Bk12345678"])
            .unwrap();
        assert!(matches!(
            cli.command,
            Commands::Bookmarks(BookmarksCmd {
                command: BookmarksCommands::Remove { .. }
            })
        ));
    }

    #[test]
    fn validates_and_normalizes_inputs() {
        assert!(validate_title("title").is_ok());
        assert!(validate_title(" \t\n").is_err());
        assert!(validate_link("https://example.com/path").is_ok());
        assert!(validate_link("http://example.com").is_ok());
        assert!(validate_link("relative/path").is_err());
        assert!(validate_link("ftp://example.com").is_err());
        assert!(validate_link("https://user@example.com").is_err());
        assert!(validate_link("https://@example.com").is_err());
        assert!(validate_link(" https://example.com").is_err());
        assert!(validate_link("https://example.com/path@user").is_ok());
        assert_eq!(normalize_emoji("books").unwrap(), ":books:");
        assert_eq!(normalize_emoji(":books:").unwrap(), ":books:");
        assert!(normalize_emoji("").is_err());
        assert!(normalize_emoji(":books").is_err());
        assert!(normalize_emoji("bad emoji").is_err());
    }

    #[test]
    fn escapes_all_tsv_controls() {
        assert_eq!(escape_tsv("a\tb\r\nc"), "a\\tb\\r\\nc");
    }
}
