//! Status CLI commands for Slack CLI
//!
//! Handles status operations: get, set, clear, presence.

use clap::{Args, Subcommand};

/// Status operations commands
#[derive(Args, Debug)]
pub struct StatusCmd {
    #[command(subcommand)]
    pub command: StatusCommands,
}

/// Status subcommands
#[derive(Subcommand, Debug)]
pub enum StatusCommands {
    /// Show current status and emoji
    Get,

    /// Set status
    Set {
        /// Status text
        text: String,

        /// Status emoji (without colons, e.g., "coffee")
        #[arg(long)]
        emoji: Option<String>,

        /// Expiration time (30m, 1h, 4h, today, tomorrow)
        #[arg(long)]
        expires: Option<String>,
    },

    /// Clear current status
    Clear,

    /// Set presence status
    Presence {
        /// Presence status (auto or away)
        #[arg(value_parser = ["auto", "away"])]
        status: String,
    },
}

/// Run the status command
pub async fn run(
    cmd: &StatusCmd,
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
        StatusCommands::Get => {
            get_status(&client, output_mode).await?;
        }

        StatusCommands::Set {
            text,
            emoji,
            expires,
        } => {
            set_status(&client, text, emoji.as_deref(), expires.as_deref()).await?;
        }

        StatusCommands::Clear => {
            clear_status(&client).await?;
        }

        StatusCommands::Presence { status } => {
            set_presence(&client, status).await?;
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

/// Normalize emoji name (add colons if not present)
fn normalize_emoji_for_api(emoji: &str) -> String {
    let emoji = emoji.strip_prefix(':').unwrap_or(emoji);
    let emoji = emoji.strip_suffix(':').unwrap_or(emoji);
    format!(":{emoji}:")
}

/// Parse expiration duration into Unix timestamp
fn parse_expiration(expires: &str) -> crate::error::Result<i64> {
    use chrono::{Duration, Local, NaiveTime};

    let now = Local::now();

    let expiration =
        match expires.to_lowercase().as_str() {
            "30m" => now + Duration::minutes(30),
            "1h" => now + Duration::hours(1),
            "4h" => now + Duration::hours(4),
            "today" => {
                // End of today (midnight)
                let end_of_day = NaiveTime::from_hms_opt(23, 59, 59).ok_or_else(|| {
                    crate::error::SlackError::Other("Failed to create end-of-day time".into())
                })?;
                let today_end = now.date_naive().and_time(end_of_day);
                today_end
                    .and_local_timezone(Local)
                    .single()
                    .ok_or_else(|| crate::error::SlackError::Usage("Invalid timezone".into()))?
            }
            "tomorrow" => {
                // End of tomorrow (midnight)
                let end_of_day = NaiveTime::from_hms_opt(23, 59, 59).ok_or_else(|| {
                    crate::error::SlackError::Other("Failed to create end-of-day time".into())
                })?;
                let tomorrow = now.date_naive() + Duration::days(1);
                let tomorrow_end = tomorrow.and_time(end_of_day);
                tomorrow_end
                    .and_local_timezone(Local)
                    .single()
                    .ok_or_else(|| crate::error::SlackError::Usage("Invalid timezone".into()))?
            }
            s if s.ends_with('m') => {
                // Parse as minutes
                let mins: i64 = s.trim_end_matches('m').parse().map_err(|_| {
                    crate::error::SlackError::Usage(format!("Invalid duration: {s}"))
                })?;
                now + Duration::minutes(mins)
            }
            s if s.ends_with('h') => {
                // Parse as hours
                let hours: i64 = s.trim_end_matches('h').parse().map_err(|_| {
                    crate::error::SlackError::Usage(format!("Invalid duration: {s}"))
                })?;
                now + Duration::hours(hours)
            }
            _ => {
                return Err(crate::error::SlackError::Usage(format!(
                    "Invalid expiration format: {expires}. Use 30m, 1h, 4h, today, or tomorrow."
                )));
            }
        };

    Ok(expiration.timestamp())
}

/// Get current status
async fn get_status(
    client: &crate::api::SlackClient,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use crate::output::write_json;

    let profile = client.users_profile_get().await?;
    let presence = client.users_get_presence(None).await?;

    if output_mode == crate::output::OutputMode::Plain {
        if let Some(text) = &profile.status_text {
            if !text.is_empty() {
                println!("status_text\t{}", text);
            }
        }
        if let Some(emoji) = &profile.status_emoji {
            if !emoji.is_empty() {
                println!("status_emoji\t{}", emoji);
            }
        }
        if let Some(exp) = profile.status_expiration {
            if exp > 0 {
                println!("status_expiration\t{}", exp);
            }
        }
        println!("presence\t{}", presence.presence);
        if presence.auto_away {
            println!("auto_away\ttrue");
        }
        if presence.manual_away {
            println!("manual_away\ttrue");
        }
    } else {
        write_json(&serde_json::json!({
            "status_text": profile.status_text,
            "status_emoji": profile.status_emoji,
            "status_expiration": profile.status_expiration,
            "presence": presence.presence,
            "auto_away": presence.auto_away,
            "manual_away": presence.manual_away,
        }))?;
    }

    Ok(())
}

/// Set status
async fn set_status(
    client: &crate::api::SlackClient,
    text: &str,
    emoji: Option<&str>,
    expires: Option<&str>,
) -> crate::error::Result<()> {
    let emoji = emoji.map(normalize_emoji_for_api);
    let expiration = expires.map(parse_expiration).transpose()?;

    client
        .users_profile_set(Some(text), emoji.as_deref(), expiration)
        .await?;

    eprintln!("Status updated");
    Ok(())
}

/// Clear status
async fn clear_status(client: &crate::api::SlackClient) -> crate::error::Result<()> {
    client
        .users_profile_set(Some(""), Some(""), Some(0))
        .await?;

    eprintln!("Status cleared");
    Ok(())
}

/// Set presence
async fn set_presence(
    client: &crate::api::SlackClient,
    presence: &str,
) -> crate::error::Result<()> {
    client.users_set_presence(presence).await?;

    eprintln!("Presence set to {}", presence);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    use crate::cli::Cli;

    #[test]
    fn test_status_cmd_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn test_parse_status_get() {
        let cli = Cli::try_parse_from(["slack", "status", "get"]).unwrap();
        if let crate::cli::Commands::Status(status_cmd) = cli.command {
            if let StatusCommands::Get = status_cmd.command {
                // Success
            } else {
                panic!("Expected Get command");
            }
        } else {
            panic!("Expected Status command");
        }
    }

    #[test]
    fn test_parse_status_set() {
        let cli = Cli::try_parse_from(["slack", "status", "set", "Working from home"]).unwrap();
        if let crate::cli::Commands::Status(status_cmd) = cli.command {
            if let StatusCommands::Set {
                text,
                emoji,
                expires,
            } = status_cmd.command
            {
                assert_eq!(text, "Working from home");
                assert!(emoji.is_none());
                assert!(expires.is_none());
            } else {
                panic!("Expected Set command");
            }
        } else {
            panic!("Expected Status command");
        }
    }

    #[test]
    fn test_parse_status_set_with_options() {
        let cli = Cli::try_parse_from([
            "slack",
            "status",
            "set",
            "In meeting",
            "--emoji",
            "meeting",
            "--expires",
            "1h",
        ])
        .unwrap();
        if let crate::cli::Commands::Status(status_cmd) = cli.command {
            if let StatusCommands::Set {
                text,
                emoji,
                expires,
            } = status_cmd.command
            {
                assert_eq!(text, "In meeting");
                assert_eq!(emoji, Some("meeting".to_string()));
                assert_eq!(expires, Some("1h".to_string()));
            } else {
                panic!("Expected Set command");
            }
        } else {
            panic!("Expected Status command");
        }
    }

    #[test]
    fn test_parse_status_clear() {
        let cli = Cli::try_parse_from(["slack", "status", "clear"]).unwrap();
        if let crate::cli::Commands::Status(status_cmd) = cli.command {
            if let StatusCommands::Clear = status_cmd.command {
                // Success
            } else {
                panic!("Expected Clear command");
            }
        } else {
            panic!("Expected Status command");
        }
    }

    #[test]
    fn test_parse_status_presence_auto() {
        let cli = Cli::try_parse_from(["slack", "status", "presence", "auto"]).unwrap();
        if let crate::cli::Commands::Status(status_cmd) = cli.command {
            if let StatusCommands::Presence { status } = status_cmd.command {
                assert_eq!(status, "auto");
            } else {
                panic!("Expected Presence command");
            }
        } else {
            panic!("Expected Status command");
        }
    }

    #[test]
    fn test_parse_status_presence_away() {
        let cli = Cli::try_parse_from(["slack", "status", "presence", "away"]).unwrap();
        if let crate::cli::Commands::Status(status_cmd) = cli.command {
            if let StatusCommands::Presence { status } = status_cmd.command {
                assert_eq!(status, "away");
            } else {
                panic!("Expected Presence command");
            }
        } else {
            panic!("Expected Status command");
        }
    }

    #[test]
    fn test_parse_status_alias() {
        let cli = Cli::try_parse_from(["slack", "s", "get"]).unwrap();
        if let crate::cli::Commands::Status(_) = cli.command {
            // Success
        } else {
            panic!("Expected Status command");
        }
    }

    #[test]
    fn test_normalize_emoji_for_api() {
        assert_eq!(normalize_emoji_for_api("coffee"), ":coffee:");
        assert_eq!(normalize_emoji_for_api(":coffee:"), ":coffee:");
        assert_eq!(normalize_emoji_for_api(":coffee"), ":coffee:");
        assert_eq!(normalize_emoji_for_api("coffee:"), ":coffee:");
    }

    #[test]
    fn test_parse_expiration_minutes() {
        let result = parse_expiration("30m");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_expiration_hours() {
        let result = parse_expiration("4h");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_expiration_today() {
        let result = parse_expiration("today");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_expiration_tomorrow() {
        let result = parse_expiration("tomorrow");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_expiration_invalid() {
        let result = parse_expiration("invalid");
        assert!(result.is_err());
    }
}
