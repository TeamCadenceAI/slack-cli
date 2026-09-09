//! Handlers for message mutations and scheduled messages.

use std::collections::HashSet;

use crate::api::chat_ops::{ChatScheduleMessageParams, ScheduledMessage};
use crate::api::{ConversationsMarkParams, SlackClient};
use crate::error::{Result, SlackError};
use crate::output::{write_json, OutputMode};

use super::{parse_message_identifier, MessageFormat};

pub(super) async fn edit_message(
    client: &SlackClient,
    identifier: &str,
    text: &str,
    format: MessageFormat,
    output_mode: OutputMode,
) -> Result<()> {
    if text.trim().is_empty() {
        return Err(SlackError::Usage(
            "Message text cannot be empty".to_string(),
        ));
    }
    let (channel, ts) = parse_message_identifier(identifier)?;
    let channel = client.resolve_channel(&channel).await?;
    let (text, mrkdwn) = match format {
        MessageFormat::Markdown => (crate::utils::markdown_to_mrkdwn(text), true),
        MessageFormat::Plain => (text.to_string(), false),
    };
    client.chat_update(&channel, &ts, &text, mrkdwn).await?;

    if output_mode == OutputMode::Plain {
        println!("{}", ts);
    } else {
        write_json(&serde_json::json!({
            "ok": true,
            "channel": channel,
            "ts": ts,
            "text": text,
        }))?;
    }
    Ok(())
}

pub(super) async fn delete_message(
    client: &SlackClient,
    identifier: &str,
    output_mode: OutputMode,
) -> Result<()> {
    let (channel, ts) = parse_message_identifier(identifier)?;
    let channel = client.resolve_channel(&channel).await?;
    client.chat_delete(&channel, &ts).await?;

    if output_mode == OutputMode::Plain {
        println!("{}", ts);
    } else {
        write_json(&serde_json::json!({"ok": true, "channel": channel, "ts": ts}))?;
    }
    Ok(())
}

pub(super) async fn permalink_message(
    client: &SlackClient,
    identifier: &str,
    output_mode: OutputMode,
) -> Result<()> {
    let (channel, ts) = parse_message_identifier(identifier)?;
    let channel = client.resolve_channel(&channel).await?;
    let response = client.chat_get_permalink(&channel, &ts).await?;

    if output_mode == OutputMode::Plain {
        println!("{}", response.permalink);
    } else {
        write_json(&serde_json::json!({
            "ok": true,
            "channel": channel,
            "ts": ts,
            "permalink": response.permalink,
        }))?;
    }
    Ok(())
}

pub(super) async fn mark_message(
    client: &SlackClient,
    channel: &str,
    ts: &str,
    output_mode: OutputMode,
) -> Result<()> {
    let channel = client.resolve_channel(channel).await?;
    client
        .conversations_mark(ConversationsMarkParams::new(&channel, ts))
        .await?;

    if output_mode == OutputMode::Plain {
        println!("{}", ts);
    } else {
        write_json(&serde_json::json!({"ok": true, "channel": channel, "ts": ts}))?;
    }
    Ok(())
}

pub(super) struct ScheduleOptions<'a> {
    pub text: Option<&'a str>,
    pub thread_ts: Option<&'a str>,
    pub format: MessageFormat,
    pub blocks: Option<serde_json::Value>,
}

pub(super) async fn schedule_message(
    client: &SlackClient,
    channel: &str,
    post_at: i64,
    options: ScheduleOptions<'_>,
    output_mode: OutputMode,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    let latest = now + 120 * 24 * 60 * 60;
    if post_at <= now {
        return Err(SlackError::Usage(
            "Scheduled time must be in the future".to_string(),
        ));
    }
    if post_at > latest {
        return Err(SlackError::Usage(
            "Scheduled time must be no more than 120 days ahead".to_string(),
        ));
    }

    let channel = client.resolve_channel(channel).await?;
    let mut params = ChatScheduleMessageParams::new(&channel, post_at);
    if let Some(text) = options.text {
        params = params.with_text(text);
    }
    if let Some(thread_ts) = options.thread_ts {
        params = params.in_thread(thread_ts);
    }
    if let Some(blocks) = options.blocks {
        params = params.with_blocks(blocks);
    }
    if matches!(options.format, MessageFormat::Plain) {
        params.mrkdwn = Some(false);
    }

    let response = client.chat_schedule_message(params).await?;
    if output_mode == OutputMode::Plain {
        println!("{}", response.scheduled_message_id);
    } else {
        write_json(&serde_json::json!({
            "ok": true,
            "channel": response.channel,
            "scheduled_message_id": response.scheduled_message_id,
            "post_at": response.post_at,
            "text": options.text,
            "permalink": null,
        }))?;
    }
    Ok(())
}

pub(super) async fn list_scheduled_messages(
    client: &SlackClient,
    output_mode: OutputMode,
) -> Result<()> {
    let mut messages = Vec::new();
    let mut cursor: Option<String> = None;
    let mut seen = HashSet::new();

    loop {
        let response = client
            .chat_scheduled_messages_list(100, cursor.as_deref())
            .await?;
        messages.extend(response.scheduled_messages);
        let next = response
            .response_metadata
            .and_then(|metadata| metadata.next_cursor)
            .filter(|value| !value.is_empty());
        match next {
            Some(next) if !seen.insert(next.clone()) => {
                return Err(SlackError::Api {
                    error: "repeated_cursor".to_string(),
                    detail: Some(
                        "chat.scheduledMessages.list returned a repeated cursor".to_string(),
                    ),
                });
            }
            Some(next) => cursor = Some(next),
            None => break,
        }
    }

    if output_mode == OutputMode::Plain {
        for message in &messages {
            print_scheduled_message(message);
        }
    } else {
        write_json(&serde_json::json!({"scheduled_messages": messages}))?;
    }
    Ok(())
}

fn print_scheduled_message(message: &ScheduledMessage) {
    let text = message
        .text
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    println!(
        "{}\t{}\t{}\t{}",
        message.id, message.channel_id, message.post_at, text
    );
}

pub(super) async fn delete_scheduled_message(
    client: &SlackClient,
    channel: &str,
    scheduled_message_id: &str,
    output_mode: OutputMode,
) -> Result<()> {
    let channel = client.resolve_channel(channel).await?;
    client
        .chat_delete_scheduled_message(&channel, scheduled_message_id)
        .await?;
    if output_mode == OutputMode::Plain {
        println!("{}", scheduled_message_id);
    } else {
        write_json(&serde_json::json!({
            "ok": true,
            "channel": channel,
            "scheduled_message_id": scheduled_message_id,
        }))?;
    }
    Ok(())
}

pub(super) async fn best_effort_permalink(
    client: &SlackClient,
    channel: &str,
    ts: &str,
    existing: Option<&str>,
) -> Option<String> {
    if let Some(permalink) = existing.filter(|value| !value.is_empty()) {
        return Some(permalink.to_string());
    }
    match client.chat_get_permalink(channel, ts).await {
        Ok(response) if !response.permalink.is_empty() => Some(response.permalink),
        Ok(_) => {
            eprintln!("warning: could not enrich message permalink");
            None
        }
        Err(error) => {
            eprintln!("warning: could not enrich message permalink: {}", error);
            None
        }
    }
}
