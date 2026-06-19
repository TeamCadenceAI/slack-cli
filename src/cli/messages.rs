//! Messages CLI commands for Slack CLI
//!
//! Handles message operations: list, thread, send, search, get.

use clap::{Args, Subcommand, ValueEnum};
use tokio::io::{self, AsyncBufReadExt, BufReader};

use crate::api::{
    ChatPostMessageParams, ConversationsHistoryParams, ConversationsMarkParams,
    ConversationsRepliesParams, SearchMessagesParams, SlackClient,
};
use crate::error::{Result, SlackError};
use crate::models::Message;
use crate::output::{write_json, write_messages_plain, MessagePlain, OutputMode};
use crate::utils::{parse_time_limit, TimeLimit};

/// Message operations commands
#[derive(Args, Debug)]
pub struct MessagesCmd {
    #[command(subcommand)]
    pub command: MessagesCommands,
}

/// Message subcommands
#[derive(Subcommand, Debug)]
pub enum MessagesCommands {
    /// List messages in a channel
    List {
        /// Channel name or ID
        channel: String,

        /// Limit: message count (e.g., "50") or time period (e.g., "1d", "7d", "1m", "90d")
        #[arg(long, short = 'l', default_value = "50")]
        limit: String,

        /// Include activity messages (join/leave/topic changes)
        #[arg(long)]
        include_activity: bool,

        /// Pagination cursor for next page
        #[arg(long)]
        cursor: Option<String>,
    },

    /// Show thread replies
    Thread {
        /// Channel name or ID
        channel: String,

        /// Thread parent timestamp
        thread_ts: String,

        /// Limit: message count (e.g., "50") or time period (e.g., "1d", "7d")
        #[arg(long, short = 'l', default_value = "100")]
        limit: String,

        /// Include activity messages
        #[arg(long)]
        include_activity: bool,

        /// Pagination cursor for next page
        #[arg(long)]
        cursor: Option<String>,
    },

    /// Send a message
    Send {
        /// Channel name or ID
        channel: String,

        /// Message text (optional if using --stdin)
        text: Option<String>,

        /// Read message from stdin
        #[arg(long)]
        stdin: bool,

        /// Reply to thread (thread parent timestamp)
        #[arg(long)]
        thread_ts: Option<String>,

        /// Message format
        #[arg(long, value_enum, default_value = "markdown")]
        format: MessageFormat,

        /// Mark channel as read after sending (sets read marker to the sent message)
        #[arg(long)]
        mark_read: bool,
    },

    /// Search messages
    Search {
        /// Search query
        query: String,

        /// Search in specific channel
        #[arg(long)]
        in_channel: Option<String>,

        /// Search in DMs with specific user
        #[arg(long)]
        in_dm: Option<String>,

        /// Search messages from specific user
        #[arg(long)]
        from: Option<String>,

        /// Search messages mentioning specific user
        #[arg(long, name = "with")]
        with_user: Option<String>,

        /// Search messages before date (YYYY-MM-DD)
        #[arg(long)]
        before: Option<String>,

        /// Search messages after date (YYYY-MM-DD)
        #[arg(long)]
        after: Option<String>,

        /// Only search in threads
        #[arg(long)]
        threads_only: bool,

        /// Number of results to return
        #[arg(long, default_value = "20")]
        count: u32,

        /// Page number (1-indexed)
        #[arg(long, default_value = "1")]
        page: u32,
    },

    /// Get a single message by URL or channel:timestamp
    Get {
        /// Message identifier: permalink URL or "channel:timestamp" format
        message: String,
    },
}

/// Message format options
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum MessageFormat {
    #[default]
    Markdown,
    Plain,
}

/// Run the messages command
pub async fn run(
    cmd: &MessagesCmd,
    plain: bool,
    workspace: Option<&str>,
    token_override: Option<&str>,
) -> Result<()> {
    let output_mode = OutputMode::from_flags(plain);

    // Get the token
    let token = get_token(workspace, token_override)?;
    let client = SlackClient::new(token)?;

    match &cmd.command {
        MessagesCommands::List {
            channel,
            limit,
            include_activity,
            cursor,
        } => {
            list_messages(
                &client,
                channel,
                limit,
                *include_activity,
                cursor.as_deref(),
                output_mode,
            )
            .await?;
        }

        MessagesCommands::Thread {
            channel,
            thread_ts,
            limit,
            include_activity,
            cursor,
        } => {
            thread_replies(
                &client,
                channel,
                thread_ts,
                limit,
                *include_activity,
                cursor.as_deref(),
                output_mode,
            )
            .await?;
        }

        MessagesCommands::Send {
            channel,
            text,
            stdin,
            thread_ts,
            format,
            mark_read,
        } => {
            send_message(
                &client,
                channel,
                text.clone(),
                *stdin,
                thread_ts.as_deref(),
                *format,
                *mark_read,
                output_mode,
            )
            .await?;
        }

        MessagesCommands::Search {
            query,
            in_channel,
            in_dm,
            from,
            with_user,
            before,
            after,
            threads_only,
            count,
            page,
        } => {
            let query_params = SearchQueryParams {
                query,
                in_channel: in_channel.as_deref(),
                in_dm: in_dm.as_deref(),
                from: from.as_deref(),
                with_user: with_user.as_deref(),
                before: before.as_deref(),
                after: after.as_deref(),
                threads_only: *threads_only,
            };
            let search_params = SearchParams {
                query_params,
                count: *count,
                page: *page,
            };
            search_messages(&client, search_params, output_mode).await?;
        }

        MessagesCommands::Get { message } => {
            get_message(&client, message, output_mode).await?;
        }
    }

    Ok(())
}

/// Get the authentication token
fn get_token(
    workspace: Option<&str>,
    token_override: Option<&str>,
) -> Result<crate::auth::TokenSet> {
    use crate::auth::{get_token_store, TokenSet, TokenType};

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

/// List messages in a channel
async fn list_messages(
    client: &SlackClient,
    channel: &str,
    limit_str: &str,
    include_activity: bool,
    cursor: Option<&str>,
    output_mode: OutputMode,
) -> Result<()> {
    // Resolve channel name to ID
    let channel_id = client.resolve_channel(channel).await?;

    // Parse the limit
    let time_limit = parse_time_limit(limit_str)?;

    let mut params = ConversationsHistoryParams::new(&channel_id);

    match &time_limit {
        TimeLimit::Count(count) => {
            params = params.with_limit(*count);
        }
        TimeLimit::Timestamp(ts) => {
            params = params.with_oldest(ts);
            // When using timestamp, get up to 100 messages per page
            params = params.with_limit(100);
        }
    }

    if let Some(c) = cursor {
        params = params.with_cursor(c);
    }

    let response = client.conversations_history(params).await?;

    // Filter out activity messages if not requested
    let messages: Vec<Message> = if include_activity {
        response.messages
    } else {
        response
            .messages
            .into_iter()
            .filter(|m| !is_activity_message(m))
            .collect()
    };

    output_messages(
        &messages,
        &channel_id,
        output_mode,
        response.has_more,
        response.response_metadata,
    )?;

    Ok(())
}

/// Show thread replies
async fn thread_replies(
    client: &SlackClient,
    channel: &str,
    thread_ts: &str,
    limit_str: &str,
    include_activity: bool,
    cursor: Option<&str>,
    output_mode: OutputMode,
) -> Result<()> {
    let channel_id = client.resolve_channel(channel).await?;
    let time_limit = parse_time_limit(limit_str)?;

    let mut params = ConversationsRepliesParams::new(&channel_id, thread_ts);

    match &time_limit {
        TimeLimit::Count(count) => {
            params = params.with_limit(*count);
        }
        TimeLimit::Timestamp(_ts) => {
            // Note: conversations.replies doesn't support oldest/latest, so we use limit
            params = params.with_limit(100);
        }
    }

    if let Some(c) = cursor {
        params = params.with_cursor(c);
    }

    let response = client.conversations_replies(params).await?;

    let messages: Vec<Message> = if include_activity {
        response.messages
    } else {
        response
            .messages
            .into_iter()
            .filter(|m| !is_activity_message(m))
            .collect()
    };

    output_messages(
        &messages,
        &channel_id,
        output_mode,
        response.has_more,
        response.response_metadata,
    )?;

    Ok(())
}

/// Send a message
///
/// If `mark_read` is true, marks the channel as read after sending.
#[allow(clippy::too_many_arguments)]
async fn send_message(
    client: &SlackClient,
    channel: &str,
    text: Option<String>,
    from_stdin: bool,
    thread_ts: Option<&str>,
    format: MessageFormat,
    mark_read: bool,
    output_mode: OutputMode,
) -> Result<()> {
    let channel_id = client.resolve_channel(channel).await?;

    // Get message text
    let message_text = if from_stdin {
        read_stdin().await?
    } else {
        text.ok_or_else(|| {
            SlackError::Usage("Message text required. Provide TEXT or use --stdin".to_string())
        })?
    };

    if message_text.trim().is_empty() {
        return Err(SlackError::Usage(
            "Message text cannot be empty".to_string(),
        ));
    }

    let mut params = ChatPostMessageParams::new(&channel_id).with_text(&message_text);

    if let Some(ts) = thread_ts {
        params = params.in_thread(ts);
    }

    // Set markdown based on format
    match format {
        MessageFormat::Markdown => {
            // mrkdwn is enabled by default in Slack, no need to set
        }
        MessageFormat::Plain => {
            params.mrkdwn = Some(false);
        }
    }

    let response = client.chat_post_message(params).await?;

    // Mark channel as read if requested
    if mark_read {
        let mark_params = ConversationsMarkParams::new(&channel_id, &response.ts);
        client.conversations_mark(mark_params).await?;
    }

    if output_mode == OutputMode::Plain {
        // Just output the timestamp
        println!("{}", response.ts);
    } else {
        write_json(&serde_json::json!({
            "ok": true,
            "channel": response.channel,
            "ts": response.ts,
            "message": response.message,
        }))?;
    }

    Ok(())
}

/// Read message content from stdin
async fn read_stdin() -> Result<String> {
    let stdin = io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();
    let mut content = String::new();

    while let Some(line) = lines.next_line().await? {
        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(&line);
    }

    Ok(content)
}

/// Parameters for building a search query
#[derive(Debug, Default)]
struct SearchQueryParams<'a> {
    query: &'a str,
    in_channel: Option<&'a str>,
    in_dm: Option<&'a str>,
    from: Option<&'a str>,
    with_user: Option<&'a str>,
    before: Option<&'a str>,
    after: Option<&'a str>,
    threads_only: bool,
}

impl<'a> SearchQueryParams<'a> {
    /// Build the full search query string
    fn build(&self) -> String {
        let mut parts = vec![self.query.to_string()];

        if let Some(channel) = self.in_channel {
            // Strip # prefix if present
            let ch = channel.strip_prefix('#').unwrap_or(channel);
            parts.push(format!("in:{}", ch));
        }

        if let Some(dm) = self.in_dm {
            let user = dm.strip_prefix('@').unwrap_or(dm);
            parts.push(format!("in:@{}", user));
        }

        if let Some(user) = self.from {
            let u = user.strip_prefix('@').unwrap_or(user);
            parts.push(format!("from:{}", u));
        }

        if let Some(user) = self.with_user {
            let u = user.strip_prefix('@').unwrap_or(user);
            parts.push(format!("to:{}", u));
        }

        if let Some(date) = self.before {
            parts.push(format!("before:{}", date));
        }

        if let Some(date) = self.after {
            parts.push(format!("after:{}", date));
        }

        if self.threads_only {
            parts.push("has:thread".to_string());
        }

        parts.join(" ")
    }
}

/// Parameters for the search_messages function
struct SearchParams<'a> {
    query_params: SearchQueryParams<'a>,
    count: u32,
    page: u32,
}

/// Search messages
async fn search_messages(
    client: &SlackClient,
    params: SearchParams<'_>,
    output_mode: OutputMode,
) -> Result<()> {
    // Check if search is available (user token required)
    if !client.supports_search() {
        return Err(SlackError::SearchNotAvailable);
    }

    // Build the search query
    let full_query = params.query_params.build();

    let api_params = SearchMessagesParams::new(&full_query)
        .with_count(params.count)
        .with_page(params.page)
        .with_sort("timestamp", "desc");

    let response = client.search_messages(api_params).await?;

    if output_mode == OutputMode::Plain {
        let plain_messages: Vec<MessagePlain> = response
            .messages
            .matches
            .iter()
            .map(|m| MessagePlain {
                timestamp: &m.ts,
                user_id: m.user.as_deref().unwrap_or(""),
                channel: m.channel.as_ref().map(|c| c.id.as_str()).unwrap_or(""),
                text: m.text.as_deref().unwrap_or(""),
            })
            .collect();
        write_messages_plain(&plain_messages)?;
    } else {
        write_json(&serde_json::json!({
            "total": response.messages.total,
            "pagination": response.messages.pagination,
            "messages": response.messages.matches,
        }))?;
    }

    Ok(())
}

/// Get a single message by URL or channel:timestamp
async fn get_message(
    client: &SlackClient,
    identifier: &str,
    output_mode: OutputMode,
) -> Result<()> {
    let (channel_id, ts) = parse_message_identifier(identifier)?;

    // Resolve channel if needed
    let resolved_channel = client.resolve_channel(&channel_id).await?;

    // Fetch the message using conversations.history with inclusive timestamp range
    let params = ConversationsHistoryParams::new(&resolved_channel)
        .with_oldest(&ts)
        .with_latest(&ts)
        .inclusive(true)
        .with_limit(1);

    let response = client.conversations_history(params).await?;

    let message = response
        .messages
        .into_iter()
        .next()
        .ok_or_else(|| SlackError::Api {
            error: "message_not_found".to_string(),
            detail: Some(format!("Message {} not found in channel", ts)),
        })?;

    if output_mode == OutputMode::Plain {
        println!("ts\t{}", message.ts);
        if let Some(user) = &message.user {
            println!("user\t{}", user);
        }
        if let Some(text) = &message.text {
            println!("text\t{}", text.replace('\n', "\\n"));
        }
        if let Some(thread_ts) = &message.thread_ts {
            println!("thread_ts\t{}", thread_ts);
        }
        if let Some(reply_count) = message.reply_count {
            println!("reply_count\t{}", reply_count);
        }
    } else {
        write_json(&message)?;
    }

    Ok(())
}

/// Parse a message identifier (URL or channel:timestamp format)
fn parse_message_identifier(identifier: &str) -> Result<(String, String)> {
    // Check if it's a Slack permalink URL
    if identifier.starts_with("https://") || identifier.starts_with("http://") {
        return parse_slack_permalink(identifier);
    }

    // Check for channel:timestamp format
    if let Some(pos) = identifier.rfind(':') {
        let channel = &identifier[..pos];
        let ts = &identifier[pos + 1..];

        if channel.is_empty() || ts.is_empty() {
            return Err(SlackError::Usage(
                "Invalid format. Use 'channel:timestamp' or Slack permalink URL".to_string(),
            ));
        }

        return Ok((channel.to_string(), ts.to_string()));
    }

    Err(SlackError::Usage(
        "Invalid message identifier. Use 'channel:timestamp' or Slack permalink URL".to_string(),
    ))
}

/// Parse a Slack permalink URL to extract channel and timestamp
///
/// Formats:
/// - `https://workspace.slack.com/archives/C123ABC/p1234567890123456`
/// - `https://workspace.slack.com/archives/C123ABC/p1234567890123456?thread_ts=...`
fn parse_slack_permalink(url: &str) -> Result<(String, String)> {
    // Parse the URL
    let parsed =
        url::Url::parse(url).map_err(|_| SlackError::Usage(format!("Invalid URL: {}", url)))?;

    // Get path segments
    let path_segments: Vec<&str> = parsed
        .path_segments()
        .map(|s| s.collect())
        .unwrap_or_default();

    // Expected format: /archives/{channel_id}/p{timestamp}
    if path_segments.len() < 3 || path_segments[0] != "archives" {
        return Err(SlackError::Usage(
            "Invalid Slack permalink format. Expected: https://workspace.slack.com/archives/CHANNEL/pTIMESTAMP".to_string(),
        ));
    }

    let channel_id = path_segments[1].to_string();
    let p_timestamp = path_segments[2];

    // The timestamp in URLs is formatted as p{seconds}{microseconds} without the dot
    // We need to convert p1234567890123456 to 1234567890.123456
    if !p_timestamp.starts_with('p') || p_timestamp.len() < 11 {
        return Err(SlackError::Usage(
            "Invalid timestamp in permalink".to_string(),
        ));
    }

    let ts_digits = &p_timestamp[1..]; // Remove 'p' prefix

    // Split into seconds (10 digits) and microseconds (remaining)
    if ts_digits.len() >= 10 {
        let seconds = &ts_digits[..10];
        let micros = if ts_digits.len() > 10 {
            &ts_digits[10..]
        } else {
            "000000"
        };
        // Pad micros to 6 digits
        let micros_padded = format!("{:0<6}", micros);
        let ts = format!("{}.{}", seconds, micros_padded);
        return Ok((channel_id, ts));
    }

    Err(SlackError::Usage(
        "Invalid timestamp format in permalink".to_string(),
    ))
}

/// Check if a message is an activity message (join/leave/topic change, etc.)
fn is_activity_message(msg: &Message) -> bool {
    if let Some(subtype) = &msg.subtype {
        matches!(
            subtype.as_str(),
            "channel_join"
                | "channel_leave"
                | "channel_topic"
                | "channel_purpose"
                | "channel_name"
                | "channel_archive"
                | "channel_unarchive"
                | "group_join"
                | "group_leave"
                | "group_topic"
                | "group_purpose"
                | "group_name"
                | "group_archive"
                | "group_unarchive"
                | "pinned_item"
                | "unpinned_item"
        )
    } else {
        false
    }
}

/// Output messages in the appropriate format
fn output_messages(
    messages: &[Message],
    channel_id: &str,
    output_mode: OutputMode,
    has_more: bool,
    response_metadata: Option<crate::api::ResponseMetadata>,
) -> Result<()> {
    if output_mode == OutputMode::Plain {
        let plain_messages: Vec<MessagePlain> = messages
            .iter()
            .map(|m| MessagePlain {
                timestamp: &m.ts,
                user_id: m.user.as_deref().unwrap_or(""),
                channel: channel_id,
                text: m.text.as_deref().unwrap_or(""),
            })
            .collect();
        write_messages_plain(&plain_messages)?;
    } else {
        write_json(&serde_json::json!({
            "messages": messages,
            "has_more": has_more,
            "response_metadata": response_metadata,
        }))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;

    #[test]
    fn test_parse_messages_list_default() {
        let cli = Cli::try_parse_from(["slack", "messages", "list", "general"]).unwrap();
        if let crate::cli::Commands::Messages(cmd) = cli.command {
            if let MessagesCommands::List {
                channel,
                limit,
                include_activity,
                cursor,
            } = cmd.command
            {
                assert_eq!(channel, "general");
                assert_eq!(limit, "50");
                assert!(!include_activity);
                assert!(cursor.is_none());
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Messages command");
        }
    }

    #[test]
    fn test_parse_messages_list_with_limit() {
        let cli =
            Cli::try_parse_from(["slack", "messages", "list", "general", "--limit", "7d"]).unwrap();
        if let crate::cli::Commands::Messages(cmd) = cli.command {
            if let MessagesCommands::List { channel, limit, .. } = cmd.command {
                assert_eq!(channel, "general");
                assert_eq!(limit, "7d");
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Messages command");
        }
    }

    #[test]
    fn test_parse_messages_list_with_cursor() {
        let cli =
            Cli::try_parse_from(["slack", "messages", "list", "general", "--cursor", "abc123"])
                .unwrap();
        if let crate::cli::Commands::Messages(cmd) = cli.command {
            if let MessagesCommands::List { cursor, .. } = cmd.command {
                assert_eq!(cursor, Some("abc123".to_string()));
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Messages command");
        }
    }

    #[test]
    fn test_parse_messages_list_include_activity() {
        let cli =
            Cli::try_parse_from(["slack", "messages", "list", "general", "--include-activity"])
                .unwrap();
        if let crate::cli::Commands::Messages(cmd) = cli.command {
            if let MessagesCommands::List {
                include_activity, ..
            } = cmd.command
            {
                assert!(include_activity);
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Messages command");
        }
    }

    #[test]
    fn test_parse_messages_thread() {
        let cli = Cli::try_parse_from([
            "slack",
            "messages",
            "thread",
            "general",
            "1234567890.123456",
        ])
        .unwrap();
        if let crate::cli::Commands::Messages(cmd) = cli.command {
            if let MessagesCommands::Thread {
                channel, thread_ts, ..
            } = cmd.command
            {
                assert_eq!(channel, "general");
                assert_eq!(thread_ts, "1234567890.123456");
            } else {
                panic!("Expected Thread command");
            }
        } else {
            panic!("Expected Messages command");
        }
    }

    #[test]
    fn test_parse_messages_send_with_text() {
        let cli =
            Cli::try_parse_from(["slack", "messages", "send", "general", "Hello world"]).unwrap();
        if let crate::cli::Commands::Messages(cmd) = cli.command {
            if let MessagesCommands::Send {
                channel,
                text,
                stdin,
                ..
            } = cmd.command
            {
                assert_eq!(channel, "general");
                assert_eq!(text, Some("Hello world".to_string()));
                assert!(!stdin);
            } else {
                panic!("Expected Send command");
            }
        } else {
            panic!("Expected Messages command");
        }
    }

    #[test]
    fn test_parse_messages_send_with_stdin() {
        let cli = Cli::try_parse_from(["slack", "messages", "send", "general", "--stdin"]).unwrap();
        if let crate::cli::Commands::Messages(cmd) = cli.command {
            if let MessagesCommands::Send { stdin, text, .. } = cmd.command {
                assert!(stdin);
                assert!(text.is_none());
            } else {
                panic!("Expected Send command");
            }
        } else {
            panic!("Expected Messages command");
        }
    }

    #[test]
    fn test_parse_messages_send_with_thread() {
        let cli = Cli::try_parse_from([
            "slack",
            "messages",
            "send",
            "general",
            "Reply",
            "--thread-ts",
            "1234567890.123456",
        ])
        .unwrap();
        if let crate::cli::Commands::Messages(cmd) = cli.command {
            if let MessagesCommands::Send { thread_ts, .. } = cmd.command {
                assert_eq!(thread_ts, Some("1234567890.123456".to_string()));
            } else {
                panic!("Expected Send command");
            }
        } else {
            panic!("Expected Messages command");
        }
    }

    #[test]
    fn test_parse_messages_send_format_plain() {
        let cli = Cli::try_parse_from([
            "slack", "messages", "send", "general", "Hello", "--format", "plain",
        ])
        .unwrap();
        if let crate::cli::Commands::Messages(cmd) = cli.command {
            if let MessagesCommands::Send { format, .. } = cmd.command {
                assert!(matches!(format, MessageFormat::Plain));
            } else {
                panic!("Expected Send command");
            }
        } else {
            panic!("Expected Messages command");
        }
    }

    #[test]
    fn test_parse_messages_send_mark_read() {
        let cli = Cli::try_parse_from([
            "slack",
            "messages",
            "send",
            "general",
            "Hello",
            "--mark-read",
        ])
        .unwrap();
        if let crate::cli::Commands::Messages(cmd) = cli.command {
            if let MessagesCommands::Send { mark_read, .. } = cmd.command {
                assert!(mark_read);
            } else {
                panic!("Expected Send command");
            }
        } else {
            panic!("Expected Messages command");
        }
    }

    #[test]
    fn test_parse_messages_send_mark_read_default() {
        let cli = Cli::try_parse_from(["slack", "messages", "send", "general", "Hello"]).unwrap();
        if let crate::cli::Commands::Messages(cmd) = cli.command {
            if let MessagesCommands::Send { mark_read, .. } = cmd.command {
                assert!(!mark_read);
            } else {
                panic!("Expected Send command");
            }
        } else {
            panic!("Expected Messages command");
        }
    }

    #[test]
    fn test_parse_messages_search_basic() {
        let cli = Cli::try_parse_from(["slack", "messages", "search", "hello world"]).unwrap();
        if let crate::cli::Commands::Messages(cmd) = cli.command {
            if let MessagesCommands::Search {
                query, count, page, ..
            } = cmd.command
            {
                assert_eq!(query, "hello world");
                assert_eq!(count, 20);
                assert_eq!(page, 1);
            } else {
                panic!("Expected Search command");
            }
        } else {
            panic!("Expected Messages command");
        }
    }

    #[test]
    fn test_parse_messages_search_with_filters() {
        let cli = Cli::try_parse_from([
            "slack",
            "messages",
            "search",
            "test",
            "--in-channel",
            "general",
            "--from",
            "@john",
            "--after",
            "2024-01-01",
            "--before",
            "2024-12-31",
            "--threads-only",
        ])
        .unwrap();
        if let crate::cli::Commands::Messages(cmd) = cli.command {
            if let MessagesCommands::Search {
                query,
                in_channel,
                from,
                after,
                before,
                threads_only,
                ..
            } = cmd.command
            {
                assert_eq!(query, "test");
                assert_eq!(in_channel, Some("general".to_string()));
                assert_eq!(from, Some("@john".to_string()));
                assert_eq!(after, Some("2024-01-01".to_string()));
                assert_eq!(before, Some("2024-12-31".to_string()));
                assert!(threads_only);
            } else {
                panic!("Expected Search command");
            }
        } else {
            panic!("Expected Messages command");
        }
    }

    #[test]
    fn test_parse_messages_get() {
        let cli =
            Cli::try_parse_from(["slack", "messages", "get", "C123:1234567890.123456"]).unwrap();
        if let crate::cli::Commands::Messages(cmd) = cli.command {
            if let MessagesCommands::Get { message } = cmd.command {
                assert_eq!(message, "C123:1234567890.123456");
            } else {
                panic!("Expected Get command");
            }
        } else {
            panic!("Expected Messages command");
        }
    }

    #[test]
    fn test_parse_messages_alias_m() {
        let cli = Cli::try_parse_from(["slack", "m", "list", "general"]).unwrap();
        assert!(matches!(cli.command, crate::cli::Commands::Messages(_)));
    }

    #[test]
    fn test_parse_messages_alias_msg() {
        let cli = Cli::try_parse_from(["slack", "msg", "list", "general"]).unwrap();
        assert!(matches!(cli.command, crate::cli::Commands::Messages(_)));
    }

    #[test]
    fn test_build_search_query_basic() {
        let params = SearchQueryParams {
            query: "hello",
            ..Default::default()
        };
        assert_eq!(params.build(), "hello");
    }

    #[test]
    fn test_build_search_query_with_channel() {
        let params = SearchQueryParams {
            query: "hello",
            in_channel: Some("#general"),
            ..Default::default()
        };
        assert_eq!(params.build(), "hello in:general");
    }

    #[test]
    fn test_build_search_query_with_from() {
        let params = SearchQueryParams {
            query: "hello",
            from: Some("@john"),
            ..Default::default()
        };
        assert_eq!(params.build(), "hello from:john");
    }

    #[test]
    fn test_build_search_query_full() {
        let params = SearchQueryParams {
            query: "test",
            in_channel: Some("general"),
            in_dm: None,
            from: Some("john"),
            with_user: Some("jane"),
            before: Some("2024-01-01"),
            after: Some("2024-06-01"),
            threads_only: true,
        };
        assert_eq!(
            params.build(),
            "test in:general from:john to:jane before:2024-01-01 after:2024-06-01 has:thread"
        );
    }

    #[test]
    fn test_parse_message_identifier_channel_ts() {
        let (channel, ts) = parse_message_identifier("C123ABC:1234567890.123456").unwrap();
        assert_eq!(channel, "C123ABC");
        assert_eq!(ts, "1234567890.123456");
    }

    #[test]
    fn test_parse_message_identifier_name_ts() {
        let (channel, ts) = parse_message_identifier("general:1234567890.123456").unwrap();
        assert_eq!(channel, "general");
        assert_eq!(ts, "1234567890.123456");
    }

    #[test]
    fn test_parse_message_identifier_invalid() {
        let result = parse_message_identifier("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_slack_permalink() {
        let (channel, ts) = parse_slack_permalink(
            "https://myworkspace.slack.com/archives/C123ABC456/p1234567890123456",
        )
        .unwrap();
        assert_eq!(channel, "C123ABC456");
        assert_eq!(ts, "1234567890.123456");
    }

    #[test]
    fn test_parse_slack_permalink_short_ts() {
        let (channel, ts) =
            parse_slack_permalink("https://myworkspace.slack.com/archives/C123ABC456/p1234567890")
                .unwrap();
        assert_eq!(channel, "C123ABC456");
        assert_eq!(ts, "1234567890.000000");
    }

    #[test]
    fn test_parse_slack_permalink_invalid() {
        let result = parse_slack_permalink("https://example.com/invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_is_activity_message() {
        let mut msg = Message {
            ts: "1234567890.123456".to_string(),
            msg_type: Some("message".to_string()),
            subtype: Some("channel_join".to_string()),
            user: None,
            text: None,
            thread_ts: None,
            reply_count: None,
            reply_users: None,
            reply_users_count: None,
            latest_reply: None,
            subscribed: false,
            reactions: None,
            files: None,
            attachments: None,
            blocks: None,
            bot_id: None,
            bot_profile: None,
            app_id: None,
            username: None,
            icons: None,
            edited: None,
            channel: None,
            permalink: None,
            is_starred: false,
            pinned_to: None,
        };

        assert!(is_activity_message(&msg));

        msg.subtype = Some("channel_leave".to_string());
        assert!(is_activity_message(&msg));

        msg.subtype = None;
        assert!(!is_activity_message(&msg));

        msg.subtype = Some("bot_message".to_string());
        assert!(!is_activity_message(&msg));
    }
}
