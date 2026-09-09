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

mod read_ops;
mod write_ops;

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
        #[arg(long, conflicts_with = "all")]
        cursor: Option<String>,

        /// Read messages newer than this exclusive UTC bound
        #[arg(long)]
        since: Option<String>,

        /// Read messages older than this exclusive UTC bound
        #[arg(long)]
        until: Option<String>,

        /// Fetch all pages (the numeric --limit is ignored)
        #[arg(long)]
        all: bool,

        /// Resolve author IDs and user mentions with one paginated user-directory load
        #[arg(long)]
        resolve_users: bool,
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

        /// Resolve author IDs and user mentions with one paginated user-directory load
        #[arg(long)]
        resolve_users: bool,
    },

    /// Send a message
    Send {
        /// Channel name, channel ID, @user, or user ID
        channel: String,

        /// Message text (optional if using --stdin or --blocks)
        text: Option<String>,

        /// Read message from stdin
        #[arg(long)]
        stdin: bool,

        /// Reply to thread (thread parent timestamp)
        #[arg(long)]
        thread_ts: Option<String>,

        /// Message format: `markdown` converts standard Markdown to Slack
        /// mrkdwn (**bold**, [text](url), lists, etc.); `plain` sends the text
        /// verbatim with mrkdwn parsing disabled.
        #[arg(long, value_enum, default_value = "markdown")]
        format: MessageFormat,

        /// Mark channel as read after sending (sets read marker to the sent message)
        #[arg(long)]
        mark_read: bool,

        /// Broadcast a thread reply to the channel
        #[arg(long)]
        broadcast: bool,

        /// Read a Block Kit JSON array from a file, or from stdin with `-`
        #[arg(long, value_name = "FILE.JSON|-", conflicts_with_all = ["stdin"])]
        blocks: Option<String>,

        /// Schedule delivery using a Unix timestamp, RFC3339, or natural expression
        #[arg(long, value_name = "WHEN")]
        schedule: Option<String>,
    },

    /// Edit an existing message
    Edit {
        /// Message identifier: permalink URL or "channel:timestamp" format
        message: String,
        /// Replacement message text
        text: String,
        /// Replacement text format
        #[arg(long, value_enum, default_value = "markdown")]
        format: MessageFormat,
    },

    /// Delete an existing message
    Delete {
        /// Message identifier: permalink URL or "channel:timestamp" format
        message: String,
    },

    /// Get a permalink for an existing message
    Permalink {
        /// Message identifier: permalink URL or "channel:timestamp" format
        message: String,
    },

    /// Mark a channel read through a timestamp
    Mark {
        /// Channel name or ID
        channel: String,
        /// Message timestamp
        ts: String,
    },

    /// Manage scheduled messages
    Scheduled {
        #[command(subcommand)]
        command: ScheduledCommands,
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

        /// Result ordering field
        #[arg(long, value_enum, default_value = "timestamp")]
        sort: SearchSort,

        /// Result ordering direction
        #[arg(long, value_enum, default_value = "desc")]
        sort_dir: SearchSortDirection,

        /// Resolve author IDs and user mentions with one paginated user-directory load
        #[arg(long)]
        resolve_users: bool,
    },

    /// Get a single message by URL or channel:timestamp
    Get {
        /// Message identifier: permalink URL or "channel:timestamp" format
        message: String,
    },
}

/// Scheduled-message subcommands.
#[derive(Subcommand, Debug)]
pub enum ScheduledCommands {
    /// List all scheduled messages
    List,
    /// Delete a scheduled message
    Delete {
        /// Channel name or ID
        channel: String,
        /// Scheduled message ID
        scheduled_message_id: String,
    },
}

/// Search result ordering fields.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum SearchSort {
    /// Order by Slack relevance score.
    Score,
    /// Order by message timestamp.
    #[default]
    Timestamp,
}

impl SearchSort {
    fn as_str(self) -> &'static str {
        match self {
            Self::Score => "score",
            Self::Timestamp => "timestamp",
        }
    }
}

/// Search result ordering directions.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum SearchSortDirection {
    /// Oldest or lowest-score results first.
    Asc,
    /// Newest or highest-score results first.
    #[default]
    Desc,
}

impl SearchSortDirection {
    fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

/// Message format options
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum MessageFormat {
    /// Convert standard Markdown to Slack mrkdwn before sending (default).
    #[default]
    Markdown,
    /// Send text verbatim with Slack mrkdwn parsing disabled.
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
    let token = crate::auth::resolve_token(workspace, token_override)?;
    let client = SlackClient::new(token)?;

    match &cmd.command {
        MessagesCommands::List {
            channel,
            limit,
            include_activity,
            cursor,
            since,
            until,
            all,
            resolve_users,
        } => {
            let options = ListOptions {
                limit,
                include_activity: *include_activity,
                cursor: cursor.as_deref(),
                since: since.as_deref(),
                until: until.as_deref(),
                all: *all,
                resolve_users: *resolve_users,
            };
            list_messages(&client, channel, options, output_mode).await?;
        }

        MessagesCommands::Thread {
            channel,
            thread_ts,
            limit,
            include_activity,
            cursor,
            resolve_users,
        } => {
            let options = ThreadOptions {
                limit,
                include_activity: *include_activity,
                cursor: cursor.as_deref(),
                resolve_users: *resolve_users,
            };
            thread_replies(&client, channel, thread_ts, options, output_mode).await?;
        }

        MessagesCommands::Send {
            channel,
            text,
            stdin,
            thread_ts,
            format,
            mark_read,
            broadcast,
            blocks,
            schedule,
        } => {
            let options = SendOptions {
                from_stdin: *stdin,
                thread_ts: thread_ts.as_deref(),
                format: *format,
                mark_read: *mark_read,
                broadcast: *broadcast,
                blocks: blocks.as_deref(),
                schedule: schedule.as_deref(),
            };
            send_message(&client, channel, text.clone(), options, output_mode).await?;
        }

        MessagesCommands::Edit {
            message,
            text,
            format,
        } => {
            write_ops::edit_message(&client, message, text, *format, output_mode).await?;
        }

        MessagesCommands::Delete { message } => {
            write_ops::delete_message(&client, message, output_mode).await?;
        }

        MessagesCommands::Permalink { message } => {
            write_ops::permalink_message(&client, message, output_mode).await?;
        }

        MessagesCommands::Mark { channel, ts } => {
            write_ops::mark_message(&client, channel, ts, output_mode).await?;
        }

        MessagesCommands::Scheduled { command } => match command {
            ScheduledCommands::List => {
                write_ops::list_scheduled_messages(&client, output_mode).await?;
            }
            ScheduledCommands::Delete {
                channel,
                scheduled_message_id,
            } => {
                write_ops::delete_scheduled_message(
                    &client,
                    channel,
                    scheduled_message_id,
                    output_mode,
                )
                .await?;
            }
        },

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
            sort,
            sort_dir,
            resolve_users,
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
                sort: *sort,
                sort_dir: *sort_dir,
                resolve_users: *resolve_users,
            };
            search_messages(&client, search_params, output_mode).await?;
        }

        MessagesCommands::Get { message } => {
            get_message(&client, message, output_mode).await?;
        }
    }

    Ok(())
}

/// Options for a channel history read.
struct ListOptions<'a> {
    limit: &'a str,
    include_activity: bool,
    cursor: Option<&'a str>,
    since: Option<&'a str>,
    until: Option<&'a str>,
    all: bool,
    resolve_users: bool,
}

/// List messages in a channel.
async fn list_messages(
    client: &SlackClient,
    channel: &str,
    options: ListOptions<'_>,
    output_mode: OutputMode,
) -> Result<()> {
    let time_limit = parse_time_limit(options.limit)?;
    let (oldest, latest) = read_ops::list_bounds(&time_limit, options.since, options.until)?;
    let channel_id = client.resolve_channel(channel).await?;

    let (messages, has_more, response_metadata) = if options.all {
        let messages = client
            .conversations_history_all(&channel_id, oldest.as_deref(), latest.as_deref())
            .await?;
        (messages, false, None)
    } else {
        let mut params = ConversationsHistoryParams::new(&channel_id);
        params = match &time_limit {
            TimeLimit::Count(count) => params.with_limit(*count),
            TimeLimit::Timestamp(_) => params.with_limit(100),
        };
        if let Some(oldest) = &oldest {
            params = params.with_oldest(oldest);
        }
        if let Some(latest) = &latest {
            params = params.with_latest(latest);
        }
        if let Some(cursor) = options.cursor {
            params = params.with_cursor(cursor);
        }

        let response = client.conversations_history(params).await?;
        (
            response.messages,
            response.has_more,
            response.response_metadata,
        )
    };

    let messages = filter_activity(messages, options.include_activity);
    let directory = load_user_directory(client, options.resolve_users, messages.is_empty()).await?;
    output_messages(
        &messages,
        &channel_id,
        output_mode,
        has_more,
        response_metadata,
        directory.as_ref(),
    )
}

/// Options for reading one page of thread replies.
struct ThreadOptions<'a> {
    limit: &'a str,
    include_activity: bool,
    cursor: Option<&'a str>,
    resolve_users: bool,
}

/// Show thread replies.
async fn thread_replies(
    client: &SlackClient,
    channel: &str,
    thread_ts: &str,
    options: ThreadOptions<'_>,
    output_mode: OutputMode,
) -> Result<()> {
    let time_limit = parse_time_limit(options.limit)?;
    let channel_id = client.resolve_channel(channel).await?;
    let mut params = ConversationsRepliesParams::new(&channel_id, thread_ts);

    params = match &time_limit {
        TimeLimit::Count(count) => params.with_limit(*count),
        // conversations.replies does not support duration limits; preserve the
        // existing page-size behavior.
        TimeLimit::Timestamp(_) => params.with_limit(100),
    };

    if let Some(cursor) = options.cursor {
        params = params.with_cursor(cursor);
    }

    let response = client.conversations_replies(params).await?;
    let messages = filter_activity(response.messages, options.include_activity);
    let directory = load_user_directory(client, options.resolve_users, messages.is_empty()).await?;
    output_messages(
        &messages,
        &channel_id,
        output_mode,
        response.has_more,
        response.response_metadata,
        directory.as_ref(),
    )
}

fn filter_activity(messages: Vec<Message>, include_activity: bool) -> Vec<Message> {
    if include_activity {
        messages
    } else {
        messages
            .into_iter()
            .filter(|message| !is_activity_message(message))
            .collect()
    }
}

async fn load_user_directory(
    client: &SlackClient,
    resolve_users: bool,
    messages_empty: bool,
) -> Result<Option<read_ops::UserDirectory>> {
    if !resolve_users || messages_empty {
        return Ok(None);
    }
    Ok(Some(read_ops::UserDirectory::from_users(
        client.users_list_all().await?,
    )))
}

/// Options for sending immediately or scheduling a message.
struct SendOptions<'a> {
    from_stdin: bool,
    thread_ts: Option<&'a str>,
    format: MessageFormat,
    mark_read: bool,
    broadcast: bool,
    blocks: Option<&'a str>,
    schedule: Option<&'a str>,
}

/// Send a message immediately or schedule it for later.
async fn send_message(
    client: &SlackClient,
    channel: &str,
    text: Option<String>,
    options: SendOptions<'_>,
    output_mode: OutputMode,
) -> Result<()> {
    if options.broadcast && options.thread_ts.is_none() {
        return Err(SlackError::Usage(
            "--broadcast requires --thread-ts".to_string(),
        ));
    }
    if options.schedule.is_some() && options.mark_read {
        return Err(SlackError::Usage(
            "--schedule cannot be used with --mark-read".to_string(),
        ));
    }
    if options.schedule.is_some() && options.broadcast {
        return Err(SlackError::Usage(
            "--schedule cannot be used with --broadcast".to_string(),
        ));
    }
    if options.from_stdin && options.blocks == Some("-") {
        return Err(SlackError::Usage(
            "--blocks - cannot be used with --stdin".to_string(),
        ));
    }

    let message_text = if options.from_stdin {
        Some(read_stdin().await?)
    } else {
        text
    };
    let blocks = match options.blocks {
        Some(path) => Some(read_blocks(path).await?),
        None => None,
    };

    if message_text.is_none() && blocks.is_none() {
        return Err(SlackError::Usage(
            "Message text or --blocks is required. Provide TEXT or use --stdin".to_string(),
        ));
    }
    if message_text
        .as_deref()
        .map(|value| value.trim().is_empty())
        .unwrap_or(false)
        && blocks.is_none()
    {
        return Err(SlackError::Usage(
            "Message text cannot be empty".to_string(),
        ));
    }

    let outgoing_text = message_text.map(|message_text| match options.format {
        MessageFormat::Markdown => crate::utils::markdown_to_mrkdwn(&message_text),
        MessageFormat::Plain => message_text,
    });

    if let Some(when) = options.schedule {
        let post_at = crate::cli::reminders::parse_when(when)?;
        let schedule_options = write_ops::ScheduleOptions {
            text: outgoing_text.as_deref(),
            thread_ts: options.thread_ts,
            format: options.format,
            blocks,
        };
        return write_ops::schedule_message(
            client,
            channel,
            post_at,
            schedule_options,
            output_mode,
        )
        .await;
    }

    let channel_id = client.resolve_channel(channel).await?;
    let mut params = ChatPostMessageParams::new(&channel_id);
    if let Some(text) = &outgoing_text {
        params = params.with_text(text);
    }
    if let Some(ts) = options.thread_ts {
        params = params.in_thread(ts);
    }
    if options.broadcast {
        params = params.reply_broadcast(true);
    }
    if let Some(blocks) = blocks {
        params = params.with_blocks(blocks);
    }
    if matches!(options.format, MessageFormat::Plain) {
        params.mrkdwn = Some(false);
    }

    let response = client.chat_post_message(params).await?;
    if options.mark_read {
        let mark_params = ConversationsMarkParams::new(&channel_id, &response.ts);
        client.conversations_mark(mark_params).await?;
    }

    if output_mode == OutputMode::Plain {
        println!("{}", response.ts);
    } else {
        let permalink = write_ops::best_effort_permalink(
            client,
            &response.channel,
            &response.ts,
            response.message.permalink.as_deref(),
        )
        .await;
        write_json(&serde_json::json!({
            "ok": true,
            "channel": response.channel,
            "ts": response.ts,
            "message": response.message,
            "permalink": permalink,
        }))?;
    }

    Ok(())
}

async fn read_blocks(path: &str) -> Result<serde_json::Value> {
    let content = if path == "-" {
        read_stdin().await?
    } else {
        std::fs::read_to_string(path).map_err(|error| {
            SlackError::Usage(format!("Could not read blocks JSON '{}': {}", path, error))
        })?
    };
    let value: serde_json::Value = serde_json::from_str(&content)
        .map_err(|error| SlackError::Usage(format!("Invalid blocks JSON: {}", error)))?;
    match value.as_array() {
        Some(items) if !items.is_empty() => Ok(value),
        Some(_) => Err(SlackError::Usage(
            "Blocks JSON array cannot be empty".to_string(),
        )),
        None => Err(SlackError::Usage(
            "Blocks JSON must be a non-empty array".to_string(),
        )),
    }
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
    sort: SearchSort,
    sort_dir: SearchSortDirection,
    resolve_users: bool,
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
        .with_sort(params.sort.as_str(), params.sort_dir.as_str());

    let response = client.search_messages(api_params).await?;
    let directory = load_user_directory(
        client,
        params.resolve_users,
        response.messages.matches.is_empty(),
    )
    .await?;
    output_search_messages(
        &response.messages.matches,
        response.messages.total,
        response.messages.pagination,
        output_mode,
        directory.as_ref(),
    )
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

    let mut message = response
        .messages
        .into_iter()
        .next()
        .ok_or_else(|| SlackError::Api {
            error: "message_not_found".to_string(),
            detail: Some(format!("Message {} not found in channel", ts)),
        })?;

    if output_mode != OutputMode::Plain {
        message.permalink = write_ops::best_effort_permalink(
            client,
            &resolved_channel,
            &message.ts,
            message.permalink.as_deref(),
        )
        .await;
    }

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
pub(super) fn parse_message_identifier(identifier: &str) -> Result<(String, String)> {
    if identifier.starts_with("https://") || identifier.starts_with("http://") {
        return parse_slack_permalink(identifier);
    }

    if let Some(pos) = identifier.rfind(':') {
        let channel = &identifier[..pos];
        let ts = &identifier[pos + 1..];
        if !channel.is_empty() && valid_slack_timestamp(ts) {
            return Ok((channel.to_string(), ts.to_string()));
        }
        return Err(SlackError::Usage(
            "Invalid format. Use 'channel:timestamp' or Slack permalink URL".to_string(),
        ));
    }

    Err(SlackError::Usage(
        "Invalid message identifier. Use 'channel:timestamp' or Slack permalink URL".to_string(),
    ))
}

fn valid_slack_timestamp(ts: &str) -> bool {
    let mut parts = ts.split('.');
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(seconds), Some(micros), None)
            if !seconds.is_empty()
                && !micros.is_empty()
                && seconds.bytes().all(|byte| byte.is_ascii_digit())
                && micros.len() <= 6
                && micros.bytes().all(|byte| byte.is_ascii_digit())
    )
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
    if path_segments.len() != 3 || path_segments[0] != "archives" || path_segments[1].is_empty() {
        return Err(SlackError::Usage(
            "Invalid Slack permalink format. Expected: https://workspace.slack.com/archives/CHANNEL/pTIMESTAMP".to_string(),
        ));
    }

    let channel_id = path_segments[1].to_string();
    let p_timestamp = path_segments[2];
    let ts_digits = p_timestamp
        .strip_prefix('p')
        .ok_or_else(|| SlackError::Usage("Invalid timestamp in permalink".to_string()))?;
    if !(10..=16).contains(&ts_digits.len()) || !ts_digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(SlackError::Usage(
            "Invalid timestamp in permalink".to_string(),
        ));
    }

    // ASCII validation above makes these byte offsets safe.
    let seconds = &ts_digits[..10];
    let micros = &ts_digits[10..];
    let micros_padded = format!("{:0<6}", micros);
    Ok((channel_id, format!("{}.{}", seconds, micros_padded)))
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
    directory: Option<&read_ops::UserDirectory>,
) -> Result<()> {
    if let Some(directory) = directory {
        let resolved = directory.resolve_texts(messages);
        if output_mode == OutputMode::Plain {
            write_plain_messages(&resolved, channel_id, directory)
        } else {
            write_json(&serde_json::json!({
                "messages": read_ops::resolved_views(&resolved, directory),
                "has_more": has_more,
                "response_metadata": response_metadata,
            }))
        }
    } else if output_mode == OutputMode::Plain {
        let plain_messages: Vec<MessagePlain> = messages
            .iter()
            .map(|message| MessagePlain {
                timestamp: &message.ts,
                user_id: message.user.as_deref().unwrap_or(""),
                channel: channel_id,
                text: message.text.as_deref().unwrap_or(""),
            })
            .collect();
        write_messages_plain(&plain_messages)
    } else {
        write_json(&serde_json::json!({
            "messages": messages,
            "has_more": has_more,
            "response_metadata": response_metadata,
        }))
    }
}

fn write_plain_messages(
    messages: &[Message],
    default_channel: &str,
    directory: &read_ops::UserDirectory,
) -> Result<()> {
    // The shared TSV writer handles tabs and line feeds. Normalize carriage
    // returns here as well so resolved output remains exactly four columns.
    let texts: Vec<String> = messages
        .iter()
        .map(|message| message.text.as_deref().unwrap_or("").replace('\r', "\\r"))
        .collect();
    let plain_messages: Vec<MessagePlain> = messages
        .iter()
        .zip(&texts)
        .map(|(message, text)| MessagePlain {
            timestamp: &message.ts,
            user_id: directory.name_for(message.user.as_deref()).unwrap_or(""),
            channel: message
                .channel
                .as_ref()
                .map(|channel| channel.id.as_str())
                .unwrap_or(default_channel),
            text,
        })
        .collect();
    write_messages_plain(&plain_messages)
}

fn output_search_messages(
    messages: &[Message],
    total: u32,
    pagination: Option<crate::api::SearchPagination>,
    output_mode: OutputMode,
    directory: Option<&read_ops::UserDirectory>,
) -> Result<()> {
    if let Some(directory) = directory {
        let resolved = directory.resolve_texts(messages);
        if output_mode == OutputMode::Plain {
            write_plain_messages(&resolved, "", directory)
        } else {
            write_json(&serde_json::json!({
                "total": total,
                "pagination": pagination,
                "messages": read_ops::resolved_views(&resolved, directory),
            }))
        }
    } else if output_mode == OutputMode::Plain {
        let plain_messages: Vec<MessagePlain> = messages
            .iter()
            .map(|message| MessagePlain {
                timestamp: &message.ts,
                user_id: message.user.as_deref().unwrap_or(""),
                channel: message
                    .channel
                    .as_ref()
                    .map(|channel| channel.id.as_str())
                    .unwrap_or(""),
                text: message.text.as_deref().unwrap_or(""),
            })
            .collect();
        write_messages_plain(&plain_messages)
    } else {
        write_json(&serde_json::json!({
            "total": total,
            "pagination": pagination,
            "messages": messages,
        }))
    }
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
                since,
                until,
                all,
                resolve_users,
            } = cmd.command
            {
                assert_eq!(channel, "general");
                assert_eq!(limit, "50");
                assert!(!include_activity);
                assert!(cursor.is_none());
                assert!(since.is_none());
                assert!(until.is_none());
                assert!(!all);
                assert!(!resolve_users);
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Messages command");
        }
    }

    #[test]
    fn test_parse_messages_list_read_options() {
        let cli = Cli::try_parse_from([
            "slack",
            "messages",
            "list",
            "general",
            "--since",
            "2025-01-01",
            "--until",
            "2025-02-01",
            "--all",
            "--resolve-users",
        ])
        .unwrap();
        if let crate::cli::Commands::Messages(cmd) = cli.command {
            if let MessagesCommands::List {
                since,
                until,
                all,
                resolve_users,
                ..
            } = cmd.command
            {
                assert_eq!(since.as_deref(), Some("2025-01-01"));
                assert_eq!(until.as_deref(), Some("2025-02-01"));
                assert!(all);
                assert!(resolve_users);
            } else {
                panic!("Expected List command");
            }
        }
    }

    #[test]
    fn test_parse_messages_list_all_conflicts_with_cursor() {
        assert!(Cli::try_parse_from([
            "slack", "messages", "list", "general", "--all", "--cursor", "next"
        ])
        .is_err());
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
    fn test_parse_messages_thread_resolve_users() {
        let cli = Cli::try_parse_from([
            "slack",
            "messages",
            "thread",
            "general",
            "1234567890.123456",
            "--resolve-users",
        ])
        .unwrap();
        if let crate::cli::Commands::Messages(cmd) = cli.command {
            if let MessagesCommands::Thread { resolve_users, .. } = cmd.command {
                assert!(resolve_users);
            } else {
                panic!("Expected Thread command");
            }
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
                query,
                count,
                page,
                sort,
                sort_dir,
                resolve_users,
                ..
            } = cmd.command
            {
                assert_eq!(query, "hello world");
                assert_eq!(count, 20);
                assert_eq!(page, 1);
                assert!(matches!(sort, SearchSort::Timestamp));
                assert!(matches!(sort_dir, SearchSortDirection::Desc));
                assert!(!resolve_users);
            } else {
                panic!("Expected Search command");
            }
        } else {
            panic!("Expected Messages command");
        }
    }

    #[test]
    fn test_parse_messages_search_sort_and_resolution() {
        let cli = Cli::try_parse_from([
            "slack",
            "messages",
            "search",
            "hello",
            "--sort",
            "score",
            "--sort-dir",
            "asc",
            "--resolve-users",
        ])
        .unwrap();
        if let crate::cli::Commands::Messages(cmd) = cli.command {
            if let MessagesCommands::Search {
                sort,
                sort_dir,
                resolve_users,
                ..
            } = cmd.command
            {
                assert!(matches!(sort, SearchSort::Score));
                assert!(matches!(sort_dir, SearchSortDirection::Asc));
                assert!(resolve_users);
            } else {
                panic!("Expected Search command");
            }
        }
        assert!(
            Cli::try_parse_from(["slack", "messages", "search", "hello", "--sort", "newest"])
                .is_err()
        );
        assert!(Cli::try_parse_from([
            "slack",
            "messages",
            "search",
            "hello",
            "--sort-dir",
            "sideways"
        ])
        .is_err());
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
    fn test_parse_messages_write_commands_and_send_flags() {
        let cli = Cli::try_parse_from([
            "slack",
            "messages",
            "send",
            "C123456789",
            "fallback",
            "--thread-ts",
            "1234567890.123456",
            "--broadcast",
            "--blocks",
            "blocks.json",
            "--schedule",
            "in 1h",
            "--format",
            "plain",
        ])
        .unwrap();
        match cli.command {
            crate::cli::Commands::Messages(MessagesCmd {
                command:
                    MessagesCommands::Send {
                        broadcast,
                        blocks,
                        schedule,
                        format,
                        ..
                    },
            }) => {
                assert!(broadcast);
                assert_eq!(blocks.as_deref(), Some("blocks.json"));
                assert_eq!(schedule.as_deref(), Some("in 1h"));
                assert!(matches!(format, MessageFormat::Plain));
            }
            _ => panic!("Expected Send command"),
        }

        for args in [
            vec![
                "slack",
                "messages",
                "edit",
                "C123456789:1234567890.123456",
                "new",
            ],
            vec![
                "slack",
                "messages",
                "delete",
                "C123456789:1234567890.123456",
            ],
            vec![
                "slack",
                "messages",
                "permalink",
                "C123456789:1234567890.123456",
            ],
            vec![
                "slack",
                "messages",
                "mark",
                "C123456789",
                "1234567890.123456",
            ],
            vec!["slack", "messages", "scheduled", "list"],
            vec![
                "slack",
                "messages",
                "scheduled",
                "delete",
                "C123456789",
                "Q123",
            ],
        ] {
            assert!(Cli::try_parse_from(args).is_ok());
        }
    }

    #[test]
    fn test_parse_message_identifier_rejects_malformed_unicode_timestamp() {
        for identifier in [
            "C123456789:１２３.456",
            "C123456789:1234567890.1234567",
            "C123456789:1234",
            "https://workspace.slack.com/archives/C123456789/p1234567890💥",
        ] {
            assert!(
                parse_message_identifier(identifier).is_err(),
                "{identifier}"
            );
        }
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
