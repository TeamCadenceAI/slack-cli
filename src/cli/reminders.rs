//! Reminders CLI commands for Slack CLI
//!
//! Handles reminder operations: list, add, complete, delete.

use clap::{Args, Subcommand};

/// Reminder operations commands
#[derive(Args, Debug)]
pub struct RemindersCmd {
    #[command(subcommand)]
    pub command: RemindersCommands,
}

/// Reminder subcommands
#[derive(Subcommand, Debug)]
pub enum RemindersCommands {
    /// List all active reminders
    List,

    /// Add a new reminder
    Add {
        /// Reminder text
        text: String,

        /// When to remind (e.g., "in 30m", "in 1h", "tomorrow at 9am", "2024-01-15 14:00")
        #[arg(long)]
        when: String,
    },

    /// Mark a reminder as complete
    Complete {
        /// Reminder ID
        id: String,
    },

    /// Delete a reminder
    Delete {
        /// Reminder ID
        id: String,
    },
}

/// Run the reminders command
pub async fn run(
    cmd: &RemindersCmd,
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
        RemindersCommands::List => {
            list_reminders(&client, output_mode).await?;
        }

        RemindersCommands::Add { text, when } => {
            add_reminder(&client, text, when).await?;
        }

        RemindersCommands::Complete { id } => {
            complete_reminder(&client, id).await?;
        }

        RemindersCommands::Delete { id } => {
            delete_reminder(&client, id).await?;
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

/// Parse natural time expression into Unix timestamp
fn parse_when(when: &str) -> crate::error::Result<i64> {
    use chrono::{Duration, Local, NaiveTime};

    let when_lower = when.to_lowercase();
    let now = Local::now();

    // Handle "in X" format
    if let Some(duration_str) = when_lower.strip_prefix("in ") {
        return parse_relative_duration(duration_str);
    }

    // Handle "tomorrow" variants
    if when_lower.starts_with("tomorrow") {
        let tomorrow = now.date_naive() + Duration::days(1);

        // Check for "tomorrow at HH:MM"
        if when_lower.contains(" at ") {
            let time_part = when_lower.split(" at ").nth(1).unwrap_or("9:00");
            let time = parse_time(time_part)?;
            let datetime = tomorrow.and_time(time);
            return datetime
                .and_local_timezone(Local)
                .single()
                .map(|dt| dt.timestamp())
                .ok_or_else(|| crate::error::SlackError::Usage("Invalid timezone".into()));
        }

        // Default to 9am tomorrow
        let time = NaiveTime::from_hms_opt(9, 0, 0).ok_or_else(|| {
            crate::error::SlackError::Other("Failed to create default time 9:00".into())
        })?;
        let datetime = tomorrow.and_time(time);
        return datetime
            .and_local_timezone(Local)
            .single()
            .map(|dt| dt.timestamp())
            .ok_or_else(|| crate::error::SlackError::Usage("Invalid timezone".into()));
    }

    // Handle "today at HH:MM"
    if when_lower.starts_with("today") {
        let today = now.date_naive();

        if when_lower.contains(" at ") {
            let time_part = when_lower.split(" at ").nth(1).unwrap_or("17:00");
            let time = parse_time(time_part)?;
            let datetime = today.and_time(time);
            return datetime
                .and_local_timezone(Local)
                .single()
                .map(|dt| dt.timestamp())
                .ok_or_else(|| crate::error::SlackError::Usage("Invalid timezone".into()));
        }

        // Default to 5pm today
        let time = NaiveTime::from_hms_opt(17, 0, 0).ok_or_else(|| {
            crate::error::SlackError::Other("Failed to create default time 17:00".into())
        })?;
        let datetime = today.and_time(time);
        return datetime
            .and_local_timezone(Local)
            .single()
            .map(|dt| dt.timestamp())
            .ok_or_else(|| crate::error::SlackError::Usage("Invalid timezone".into()));
    }

    // Try to parse as a Unix timestamp
    if let Ok(ts) = when.parse::<i64>() {
        return Ok(ts);
    }

    // Try to parse as ISO 8601 or similar format
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(when) {
        return Ok(dt.timestamp());
    }

    // Try "YYYY-MM-DD HH:MM" format
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(when, "%Y-%m-%d %H:%M") {
        return dt
            .and_local_timezone(Local)
            .single()
            .map(|dt| dt.timestamp())
            .ok_or_else(|| crate::error::SlackError::Usage("Invalid timezone".into()));
    }

    // Try "YYYY-MM-DD" format (defaults to 9am)
    if let Ok(d) = chrono::NaiveDate::parse_from_str(when, "%Y-%m-%d") {
        let time = NaiveTime::from_hms_opt(9, 0, 0).ok_or_else(|| {
            crate::error::SlackError::Other("Failed to create default time 9:00".into())
        })?;
        let datetime = d.and_time(time);
        return datetime
            .and_local_timezone(Local)
            .single()
            .map(|dt| dt.timestamp())
            .ok_or_else(|| crate::error::SlackError::Usage("Invalid timezone".into()));
    }

    Err(crate::error::SlackError::Usage(format!(
        "Could not parse time: '{}'. Try: 'in 30m', 'tomorrow at 9am', '2024-01-15 14:00'",
        when
    )))
}

/// Parse relative duration string like "30m", "1h", "2d"
fn parse_relative_duration(duration_str: &str) -> crate::error::Result<i64> {
    use chrono::{Duration, Local};

    let now = Local::now();
    let trimmed = duration_str.trim();

    let (num_str, unit) = if trimmed.ends_with('m') || trimmed.ends_with('M') {
        (&trimmed[..trimmed.len() - 1], 'm')
    } else if trimmed.ends_with('h') || trimmed.ends_with('H') {
        (&trimmed[..trimmed.len() - 1], 'h')
    } else if trimmed.ends_with('d') || trimmed.ends_with('D') {
        (&trimmed[..trimmed.len() - 1], 'd')
    } else if trimmed.ends_with("min") || trimmed.ends_with("mins") || trimmed.ends_with("minutes")
    {
        let num_end = trimmed
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(trimmed.len());
        (&trimmed[..num_end], 'm')
    } else if trimmed.ends_with("hour") || trimmed.ends_with("hours") {
        let num_end = trimmed
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(trimmed.len());
        (&trimmed[..num_end], 'h')
    } else if trimmed.ends_with("day") || trimmed.ends_with("days") {
        let num_end = trimmed
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(trimmed.len());
        (&trimmed[..num_end], 'd')
    } else {
        return Err(crate::error::SlackError::Usage(format!(
            "Invalid duration: '{}'. Use format like '30m', '1h', '2d'",
            duration_str
        )));
    };

    let num: i64 = num_str.trim().parse().map_err(|_| {
        crate::error::SlackError::Usage(format!("Invalid number in duration: '{}'", num_str))
    })?;

    let duration = match unit {
        'm' => Duration::minutes(num),
        'h' => Duration::hours(num),
        'd' => Duration::days(num),
        _ => unreachable!(),
    };

    Ok((now + duration).timestamp())
}

/// Parse time string like "9am", "9:00", "14:30", "2pm"
fn parse_time(time_str: &str) -> crate::error::Result<chrono::NaiveTime> {
    use chrono::NaiveTime;

    let time_str = time_str.trim().to_lowercase();

    // Handle "9am", "10pm" format
    if time_str.ends_with("am") {
        let hour_str = &time_str[..time_str.len() - 2];
        let hour: u32 = hour_str
            .parse()
            .map_err(|_| crate::error::SlackError::Usage(format!("Invalid hour: {}", hour_str)))?;
        let hour = if hour == 12 { 0 } else { hour };
        return NaiveTime::from_hms_opt(hour, 0, 0)
            .ok_or_else(|| crate::error::SlackError::Usage(format!("Invalid time: {}", time_str)));
    }

    if time_str.ends_with("pm") {
        let hour_str = &time_str[..time_str.len() - 2];
        let hour: u32 = hour_str
            .parse()
            .map_err(|_| crate::error::SlackError::Usage(format!("Invalid hour: {}", hour_str)))?;
        let hour = if hour == 12 { 12 } else { hour + 12 };
        return NaiveTime::from_hms_opt(hour, 0, 0)
            .ok_or_else(|| crate::error::SlackError::Usage(format!("Invalid time: {}", time_str)));
    }

    // Handle "HH:MM" format
    if time_str.contains(':') {
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() == 2 {
            let hour: u32 = parts[0].parse().map_err(|_| {
                crate::error::SlackError::Usage(format!("Invalid hour: {}", parts[0]))
            })?;
            let minute: u32 = parts[1].parse().map_err(|_| {
                crate::error::SlackError::Usage(format!("Invalid minute: {}", parts[1]))
            })?;
            return NaiveTime::from_hms_opt(hour, minute, 0).ok_or_else(|| {
                crate::error::SlackError::Usage(format!("Invalid time: {}", time_str))
            });
        }
    }

    // Try to parse as just an hour
    if let Ok(hour) = time_str.parse::<u32>() {
        return NaiveTime::from_hms_opt(hour, 0, 0)
            .ok_or_else(|| crate::error::SlackError::Usage(format!("Invalid time: {}", time_str)));
    }

    Err(crate::error::SlackError::Usage(format!(
        "Could not parse time: '{}'. Try '9am', '14:30', or '2pm'",
        time_str
    )))
}

/// List reminders
async fn list_reminders(
    client: &crate::api::SlackClient,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use crate::output::write_json;

    let response = client.reminders_list().await?;

    if output_mode == crate::output::OutputMode::Plain {
        if response.reminders.is_empty() {
            println!("No reminders");
        } else {
            for reminder in &response.reminders {
                let text = reminder.text.as_deref().unwrap_or("");
                let time = reminder.time.unwrap_or(0);
                let complete = if reminder.complete_ts.is_some() {
                    "complete"
                } else {
                    "pending"
                };
                println!("{}\t{}\t{}\t{}", reminder.id, time, complete, text);
            }
        }
    } else {
        write_json(&response.reminders)?;
    }

    Ok(())
}

/// Add a reminder
async fn add_reminder(
    client: &crate::api::SlackClient,
    text: &str,
    when: &str,
) -> crate::error::Result<()> {
    let time = parse_when(when)?;

    let response = client.reminders_add(text, time).await?;

    eprintln!("Reminder created: {}", response.reminder.id);
    Ok(())
}

/// Complete a reminder
async fn complete_reminder(
    client: &crate::api::SlackClient,
    reminder_id: &str,
) -> crate::error::Result<()> {
    client.reminders_complete(reminder_id).await?;

    eprintln!("Reminder {} marked as complete", reminder_id);
    Ok(())
}

/// Delete a reminder
async fn delete_reminder(
    client: &crate::api::SlackClient,
    reminder_id: &str,
) -> crate::error::Result<()> {
    client.reminders_delete(reminder_id).await?;

    eprintln!("Reminder {} deleted", reminder_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;
    use clap::{CommandFactory, Parser};

    use crate::cli::Cli;

    #[test]
    fn test_reminders_cmd_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn test_parse_reminders_list() {
        let cli = Cli::try_parse_from(["slack", "reminders", "list"]).unwrap();
        if let crate::cli::Commands::Reminders(reminders_cmd) = cli.command {
            if let RemindersCommands::List = reminders_cmd.command {
                // Success
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Reminders command");
        }
    }

    #[test]
    fn test_parse_reminders_add() {
        let cli =
            Cli::try_parse_from(["slack", "reminders", "add", "Review PR", "--when", "in 30m"])
                .unwrap();
        if let crate::cli::Commands::Reminders(reminders_cmd) = cli.command {
            if let RemindersCommands::Add { text, when } = reminders_cmd.command {
                assert_eq!(text, "Review PR");
                assert_eq!(when, "in 30m");
            } else {
                panic!("Expected Add command");
            }
        } else {
            panic!("Expected Reminders command");
        }
    }

    #[test]
    fn test_parse_reminders_complete() {
        let cli = Cli::try_parse_from(["slack", "reminders", "complete", "Rm123456"]).unwrap();
        if let crate::cli::Commands::Reminders(reminders_cmd) = cli.command {
            if let RemindersCommands::Complete { id } = reminders_cmd.command {
                assert_eq!(id, "Rm123456");
            } else {
                panic!("Expected Complete command");
            }
        } else {
            panic!("Expected Reminders command");
        }
    }

    #[test]
    fn test_parse_reminders_delete() {
        let cli = Cli::try_parse_from(["slack", "reminders", "delete", "Rm123456"]).unwrap();
        if let crate::cli::Commands::Reminders(reminders_cmd) = cli.command {
            if let RemindersCommands::Delete { id } = reminders_cmd.command {
                assert_eq!(id, "Rm123456");
            } else {
                panic!("Expected Delete command");
            }
        } else {
            panic!("Expected Reminders command");
        }
    }

    #[test]
    fn test_parse_when_in_minutes() {
        let result = parse_when("in 30m");
        assert!(result.is_ok());
        let now = chrono::Local::now().timestamp();
        let parsed = result.unwrap();
        // Should be roughly 30 minutes from now (allow 1 minute tolerance)
        assert!(parsed >= now + 29 * 60);
        assert!(parsed <= now + 31 * 60);
    }

    #[test]
    fn test_parse_when_in_hours() {
        let result = parse_when("in 2h");
        assert!(result.is_ok());
        let now = chrono::Local::now().timestamp();
        let parsed = result.unwrap();
        // Should be roughly 2 hours from now
        assert!(parsed >= now + 119 * 60);
        assert!(parsed <= now + 121 * 60);
    }

    #[test]
    fn test_parse_when_tomorrow() {
        let result = parse_when("tomorrow");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_when_tomorrow_at_time() {
        let result = parse_when("tomorrow at 9am");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_when_date() {
        let result = parse_when("2025-12-25 14:00");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_time_am() {
        let result = parse_time("9am");
        assert!(result.is_ok());
        let time = result.unwrap();
        assert_eq!(time.hour(), 9);
        assert_eq!(time.minute(), 0);
    }

    #[test]
    fn test_parse_time_pm() {
        let result = parse_time("2pm");
        assert!(result.is_ok());
        let time = result.unwrap();
        assert_eq!(time.hour(), 14);
        assert_eq!(time.minute(), 0);
    }

    #[test]
    fn test_parse_time_24h() {
        let result = parse_time("14:30");
        assert!(result.is_ok());
        let time = result.unwrap();
        assert_eq!(time.hour(), 14);
        assert_eq!(time.minute(), 30);
    }

    #[test]
    fn test_parse_relative_duration() {
        assert!(parse_relative_duration("30m").is_ok());
        assert!(parse_relative_duration("1h").is_ok());
        assert!(parse_relative_duration("2d").is_ok());
        assert!(parse_relative_duration("invalid").is_err());
    }
}
