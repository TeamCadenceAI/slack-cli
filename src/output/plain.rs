//! Plain TSV output helpers for Slack CLI
//!
//! Provides functions for writing plain text (TSV) output for shell scripting.

use std::io::{self, Write};

use crate::error::Result;

/// Escape text for TSV output (replace tabs and newlines)
fn escape_tsv(text: &str) -> String {
    text.replace('\t', "\\t").replace('\n', "\\n")
}

/// Channel data for plain output
#[derive(Debug)]
pub struct ChannelPlain<'a> {
    pub id: &'a str,
    pub name: Option<&'a str>,
    pub num_members: Option<u32>,
    pub is_private: bool,
}

/// Write channels as TSV to stdout
/// Format: `id\tname\tnum_members\ttype`
pub fn write_channels_plain(channels: &[ChannelPlain]) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    for ch in channels {
        writeln!(
            handle,
            "{}\t{}\t{}\t{}",
            ch.id,
            ch.name.unwrap_or(""),
            ch.num_members.unwrap_or(0),
            if ch.is_private { "private" } else { "public" }
        )?;
    }
    Ok(())
}

/// Message data for plain output
#[derive(Debug)]
pub struct MessagePlain<'a> {
    pub timestamp: &'a str,
    pub user_id: &'a str,
    pub channel: &'a str,
    pub text: &'a str,
}

/// Write messages as TSV to stdout
/// Format: `timestamp\tuser_id\tchannel\ttext`
pub fn write_messages_plain(messages: &[MessagePlain]) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    for msg in messages {
        writeln!(
            handle,
            "{}\t{}\t{}\t{}",
            msg.timestamp,
            msg.user_id,
            msg.channel,
            escape_tsv(msg.text)
        )?;
    }
    Ok(())
}

/// User data for plain output
#[derive(Debug)]
pub struct UserPlain<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub real_name: Option<&'a str>,
    pub email: Option<&'a str>,
}

/// Write users as TSV to stdout
/// Format: `id\tname\treal_name\temail`
pub fn write_users_plain(users: &[UserPlain]) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    for user in users {
        writeln!(
            handle,
            "{}\t{}\t{}\t{}",
            user.id,
            user.name,
            user.real_name.unwrap_or(""),
            user.email.unwrap_or("")
        )?;
    }
    Ok(())
}

/// File data for plain output
#[derive(Debug)]
pub struct FilePlain<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub filetype: &'a str,
    pub size: u64,
}

/// Write files as TSV to stdout
/// Format: `id\tname\tfiletype\tsize`
pub fn write_files_plain(files: &[FilePlain]) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    for file in files {
        writeln!(
            handle,
            "{}\t{}\t{}\t{}",
            file.id, file.name, file.filetype, file.size
        )?;
    }
    Ok(())
}

/// Write a simple list of strings to stdout, one per line
pub fn write_list_plain(items: &[&str]) -> Result<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    for item in items {
        writeln!(handle, "{}", item)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_tsv_tabs() {
        let input = "hello\tworld";
        assert_eq!(escape_tsv(input), "hello\\tworld");
    }

    #[test]
    fn test_escape_tsv_newlines() {
        let input = "hello\nworld";
        assert_eq!(escape_tsv(input), "hello\\nworld");
    }

    #[test]
    fn test_escape_tsv_mixed() {
        let input = "line1\nline2\twith\ttabs";
        assert_eq!(escape_tsv(input), "line1\\nline2\\twith\\ttabs");
    }

    #[test]
    fn test_escape_tsv_no_special_chars() {
        let input = "normal text";
        assert_eq!(escape_tsv(input), "normal text");
    }

    #[test]
    fn test_channel_plain_private() {
        let ch = ChannelPlain {
            id: "C123",
            name: Some("secret"),
            num_members: Some(5),
            is_private: true,
        };
        assert!(ch.is_private);
    }

    #[test]
    fn test_channel_plain_public() {
        let ch = ChannelPlain {
            id: "C456",
            name: Some("general"),
            num_members: Some(100),
            is_private: false,
        };
        assert!(!ch.is_private);
    }

    #[test]
    fn test_message_plain_struct() {
        let msg = MessagePlain {
            timestamp: "1234567890.123456",
            user_id: "U123",
            channel: "C456",
            text: "Hello world",
        };
        assert_eq!(msg.timestamp, "1234567890.123456");
        assert_eq!(msg.user_id, "U123");
    }

    #[test]
    fn test_user_plain_struct() {
        let user = UserPlain {
            id: "U123",
            name: "john",
            real_name: Some("John Doe"),
            email: Some("john@example.com"),
        };
        assert_eq!(user.id, "U123");
        assert_eq!(user.real_name, Some("John Doe"));
    }

    #[test]
    fn test_user_plain_optional_none() {
        let user = UserPlain {
            id: "U456",
            name: "bot",
            real_name: None,
            email: None,
        };
        assert_eq!(user.real_name, None);
        assert_eq!(user.email, None);
    }

    #[test]
    fn test_file_plain_struct() {
        let file = FilePlain {
            id: "F123",
            name: "report.pdf",
            filetype: "pdf",
            size: 1024,
        };
        assert_eq!(file.id, "F123");
        assert_eq!(file.size, 1024);
    }
}
