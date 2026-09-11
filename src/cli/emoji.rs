//! Custom emoji commands for Slack CLI.

use std::io::{self, Write};

use clap::{Args, Subcommand};

use crate::api::SlackClient;
use crate::error::Result;
use crate::output::{write_json, OutputMode};

/// Custom emoji operations.
#[derive(Args, Debug)]
pub struct EmojiCmd {
    /// Emoji command to run.
    #[command(subcommand)]
    pub command: EmojiCommands,
}

/// Custom emoji subcommands.
#[derive(Subcommand, Debug)]
pub enum EmojiCommands {
    /// List custom workspace emoji.
    List,
}

/// Run a custom emoji command.
pub async fn run(
    cmd: &EmojiCmd,
    plain: bool,
    workspace: Option<&str>,
    token_override: Option<&str>,
) -> Result<()> {
    let token = crate::auth::resolve_token(workspace, token_override)?;
    let client = SlackClient::new(token)?;
    let output_mode = OutputMode::from_flags(plain);

    match &cmd.command {
        EmojiCommands::List => list_emoji(&client, output_mode).await,
    }
}

async fn list_emoji(client: &SlackClient, output_mode: OutputMode) -> Result<()> {
    let response = client.emoji_list().await?;

    if output_mode == OutputMode::Plain {
        let stdout = io::stdout();
        let mut output = stdout.lock();
        for (name, value) in &response.emoji {
            writeln!(output, "{}\t{}", escape_tsv(name), escape_tsv(value))?;
        }
    } else {
        write_json(&serde_json::json!({ "emoji": response.emoji }))?;
    }
    Ok(())
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
    fn parse_emoji_list() {
        let cli = Cli::try_parse_from(["slack", "emoji", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Emoji(EmojiCmd {
                command: EmojiCommands::List
            })
        ));
    }

    #[test]
    fn escapes_all_tsv_controls() {
        assert_eq!(escape_tsv("a\tb\r\nc"), "a\\tb\\r\\nc");
    }
}
