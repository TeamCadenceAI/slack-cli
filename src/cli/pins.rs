//! Pin commands for Slack CLI.

use std::io::{self, Write};

use clap::{Args, Subcommand};

use crate::api::pin_emoji_ops::PinItem;
use crate::api::SlackClient;
use crate::error::Result;
use crate::output::{write_json, OutputMode};

/// Pin operations.
#[derive(Args, Debug)]
pub struct PinsCmd {
    /// Pin command to run.
    #[command(subcommand)]
    pub command: PinsCommands,
}

/// Pin subcommands.
#[derive(Subcommand, Debug)]
pub enum PinsCommands {
    /// Pin a message in a channel.
    Add {
        /// Channel name or ID.
        channel: String,
        /// Message timestamp.
        ts: String,
    },

    /// Remove a message pin from a channel.
    Remove {
        /// Channel name or ID.
        channel: String,
        /// Message timestamp.
        ts: String,
    },

    /// List pins in a channel.
    List {
        /// Channel name or ID.
        channel: String,
    },
}

/// Run a pin command.
pub async fn run(
    cmd: &PinsCmd,
    plain: bool,
    workspace: Option<&str>,
    token_override: Option<&str>,
) -> Result<()> {
    let token = crate::auth::resolve_token(workspace, token_override)?;
    let client = SlackClient::new(token)?;
    let output_mode = OutputMode::from_flags(plain);

    match &cmd.command {
        PinsCommands::Add { channel, ts } => {
            mutate_pin(&client, channel, ts, true, output_mode).await
        }
        PinsCommands::Remove { channel, ts } => {
            mutate_pin(&client, channel, ts, false, output_mode).await
        }
        PinsCommands::List { channel } => list_pins(&client, channel, output_mode).await,
    }
}

async fn mutate_pin(
    client: &SlackClient,
    channel: &str,
    ts: &str,
    add: bool,
    output_mode: OutputMode,
) -> Result<()> {
    let channel = client.resolve_channel(channel).await?;
    if add {
        client.pins_add(&channel, ts).await?;
    } else {
        client.pins_remove(&channel, ts).await?;
    }

    if output_mode == OutputMode::Plain {
        let stdout = io::stdout();
        writeln!(stdout.lock(), "{}", escape_tsv(ts))?;
    } else {
        write_json(&serde_json::json!({
            "ok": true,
            "channel": channel,
            "ts": ts,
        }))?;
    }
    Ok(())
}

async fn list_pins(client: &SlackClient, channel: &str, output_mode: OutputMode) -> Result<()> {
    let channel = client.resolve_channel(channel).await?;
    let response = client.pins_list(&channel).await?;

    if output_mode == OutputMode::Plain {
        let stdout = io::stdout();
        let mut output = stdout.lock();
        for item in &response.items {
            let (item_type, id, author, text) = plain_fields(item);
            writeln!(
                output,
                "{}\t{}\t{}\t{}",
                escape_tsv(item_type),
                escape_tsv(id),
                escape_tsv(author),
                escape_tsv(text)
            )?;
        }
    } else {
        write_json(&serde_json::json!({
            "channel": channel,
            "items": response.items,
        }))?;
    }
    Ok(())
}

fn plain_fields(item: &PinItem) -> (&str, &str, &str, &str) {
    let value = &item.0;
    let message = value.get("message");
    let file = value.get("file");

    (
        value
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or(""),
        nested_string(message, "ts")
            .or_else(|| nested_string(file, "id"))
            .unwrap_or(""),
        nested_string(message, "user")
            .or_else(|| nested_string(file, "user"))
            .unwrap_or(""),
        nested_string(message, "text")
            .or_else(|| nested_string(file, "title"))
            .unwrap_or(""),
    )
}

fn nested_string<'a>(value: Option<&'a serde_json::Value>, field: &str) -> Option<&'a str> {
    value
        .and_then(|value| value.get(field))
        .and_then(|value| value.as_str())
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
    fn parse_pins_add() {
        let cli = Cli::try_parse_from(["slack", "pins", "add", "general", "123.456"]).unwrap();
        match cli.command {
            Commands::Pins(PinsCmd {
                command: PinsCommands::Add { channel, ts },
            }) => {
                assert_eq!(channel, "general");
                assert_eq!(ts, "123.456");
            }
            _ => panic!("expected pins add command"),
        }
    }

    #[test]
    fn parse_pins_remove() {
        let cli = Cli::try_parse_from(["slack", "pins", "remove", "C12345678", "123.456"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Pins(PinsCmd {
                command: PinsCommands::Remove { .. }
            })
        ));
    }

    #[test]
    fn parse_pins_list() {
        let cli = Cli::try_parse_from(["slack", "pins", "list", "#general"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Pins(PinsCmd {
                command: PinsCommands::List { .. }
            })
        ));
    }

    #[test]
    fn extracts_plain_fields_and_escapes_all_tsv_controls() {
        let item = PinItem(serde_json::json!({
            "type": "message",
            "message": {"ts": "1", "user": "U1", "text": "a\tb\r\nc"}
        }));
        assert_eq!(plain_fields(&item), ("message", "1", "U1", "a\tb\r\nc"));
        assert_eq!(escape_tsv("a\tb\r\nc"), "a\\tb\\r\\nc");
    }
}
