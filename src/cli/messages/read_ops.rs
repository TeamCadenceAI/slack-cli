//! Helpers for bounded message reads and optional user-name resolution.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;

use crate::error::{Result, SlackError};
use crate::models::{Message, User};
use crate::utils::TimeLimit;

/// A normalized Slack timestamp and its exact microsecond value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Bound {
    pub(super) timestamp: String,
    micros: i128,
}

/// Parse a date, RFC3339 instant, or Slack decimal timestamp.
pub(super) fn parse_bound(input: &str) -> Result<Bound> {
    let input = input.trim();
    if input.is_empty() {
        return invalid_bound(input);
    }

    if let Ok(date) = NaiveDate::parse_from_str(input, "%Y-%m-%d") {
        let datetime = date
            .and_hms_opt(0, 0, 0)
            .expect("midnight is always a valid time")
            .and_utc();
        return from_datetime(datetime, input);
    }

    if let Ok(datetime) = DateTime::parse_from_rfc3339(input) {
        return from_datetime(datetime.with_timezone(&Utc), input);
    }

    parse_decimal_bound(input)
}

fn from_datetime(datetime: DateTime<Utc>, input: &str) -> Result<Bound> {
    let seconds = datetime.timestamp();
    if seconds < 0 {
        return invalid_bound(input);
    }
    let subsec_micros = datetime.timestamp_subsec_micros();
    Ok(Bound {
        timestamp: format!("{}.{:06}", seconds, subsec_micros),
        micros: i128::from(seconds) * 1_000_000 + i128::from(subsec_micros),
    })
}

fn parse_decimal_bound(input: &str) -> Result<Bound> {
    let mut pieces = input.split('.');
    let seconds = pieces.next().unwrap_or_default();
    let fraction = pieces.next();
    if pieces.next().is_some()
        || seconds.is_empty()
        || !seconds.bytes().all(|byte| byte.is_ascii_digit())
    {
        return invalid_bound(input);
    }

    let micros = match fraction {
        Some(value)
            if !value.is_empty()
                && value.len() <= 6
                && value.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            let mut padded = value.to_string();
            while padded.len() < 6 {
                padded.push('0');
            }
            padded.parse::<u32>().map_err(|_| usage_for_bound(input))?
        }
        Some(_) => return invalid_bound(input),
        None => 0,
    };

    let seconds = seconds.parse::<u64>().map_err(|_| usage_for_bound(input))?;
    let exact = i128::from(seconds) * 1_000_000 + i128::from(micros);
    Ok(Bound {
        timestamp: format!("{}.{:06}", seconds, micros),
        micros: exact,
    })
}

fn usage_for_bound(input: &str) -> SlackError {
    SlackError::Usage(format!(
        "Invalid time bound '{input}'; expected YYYY-MM-DD, RFC3339, or a nonnegative Slack timestamp"
    ))
}

fn invalid_bound<T>(input: &str) -> Result<T> {
    Err(usage_for_bound(input))
}

/// Combine explicit bounds with a duration-style `--limit` oldest bound.
pub(super) fn list_bounds(
    limit: &TimeLimit,
    since: Option<&str>,
    until: Option<&str>,
) -> Result<(Option<String>, Option<String>)> {
    let duration_oldest = match limit {
        TimeLimit::Timestamp(timestamp) => Some(parse_bound(timestamp)?),
        TimeLimit::Count(_) => None,
    };
    let explicit_oldest = since.map(parse_bound).transpose()?;
    let latest = until.map(parse_bound).transpose()?;

    let oldest = match (duration_oldest, explicit_oldest) {
        (Some(duration), Some(explicit)) => Some(if duration.micros >= explicit.micros {
            duration
        } else {
            explicit
        }),
        (Some(duration), None) => Some(duration),
        (None, Some(explicit)) => Some(explicit),
        (None, None) => None,
    };

    if let (Some(oldest), Some(latest)) = (&oldest, &latest) {
        if oldest.micros >= latest.micros {
            return Err(SlackError::Usage(
                "The oldest message bound must be earlier than the latest bound".to_string(),
            ));
        }
    }

    Ok((
        oldest.map(|bound| bound.timestamp),
        latest.map(|bound| bound.timestamp),
    ))
}

/// Workspace user names keyed by Slack user ID.
#[derive(Debug, Default)]
pub(super) struct UserDirectory {
    names: HashMap<String, String>,
}

impl UserDirectory {
    pub(super) fn from_users(users: Vec<User>) -> Self {
        let names = users
            .into_iter()
            .map(|user| {
                let id = user.id.clone();
                let name = user
                    .name
                    .as_deref()
                    .filter(|name| !name.is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        let display_name = user.display_name();
                        if display_name.is_empty() {
                            id.clone()
                        } else {
                            display_name
                        }
                    });
                (id, name)
            })
            .collect();
        Self { names }
    }

    /// Return the resolved name, retaining an unknown ID as its own fallback.
    pub(super) fn name_for<'a>(&'a self, user_id: Option<&'a str>) -> Option<&'a str> {
        user_id.map(|id| self.names.get(id).map(String::as_str).unwrap_or(id))
    }

    pub(super) fn replace_mentions(&self, text: &str) -> String {
        let mut output = String::with_capacity(text.len());
        let mut remaining = text;

        while let Some(start) = remaining.find("<@") {
            output.push_str(&remaining[..start]);
            let token = &remaining[start..];
            let Some(end) = token.find('>') else {
                output.push_str(token);
                return output;
            };

            let complete = &token[..=end];
            let inner = &token[2..end];
            let id = inner.split('|').next().unwrap_or_default();
            if id.starts_with('U') && id.len() > 1 {
                if let Some(name) = self.names.get(id) {
                    output.push('@');
                    output.push_str(name);
                } else {
                    output.push_str(complete);
                }
            } else {
                output.push_str(complete);
            }
            remaining = &token[end + 1..];
        }

        output.push_str(remaining);
        output
    }

    pub(super) fn resolve_texts(&self, messages: &[Message]) -> Vec<Message> {
        messages
            .iter()
            .cloned()
            .map(|mut message| {
                if let Some(text) = message.text.take() {
                    message.text = Some(self.replace_mentions(&text));
                }
                message
            })
            .collect()
    }
}

/// JSON message view that preserves the API model and adds the requested name.
#[derive(Serialize)]
pub(super) struct ResolvedMessageView<'a> {
    #[serde(flatten)]
    pub(super) message: &'a Message,
    pub(super) user_name: Option<&'a str>,
}

pub(super) fn resolved_views<'a>(
    messages: &'a [Message],
    directory: &'a UserDirectory,
) -> Vec<ResolvedMessageView<'a>> {
    messages
        .iter()
        .map(|message| ResolvedMessageView {
            message,
            user_name: directory.name_for(message.user.as_deref()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::UserProfile;

    #[test]
    fn parses_supported_bounds_without_losing_microseconds() {
        assert_eq!(
            parse_bound("2025-01-01").unwrap().timestamp,
            "1735689600.000000"
        );
        assert_eq!(
            parse_bound("2025-01-01T01:02:03.123456+01:00")
                .unwrap()
                .timestamp,
            "1735689723.123456"
        );
        assert_eq!(
            parse_bound("1735689600.000001").unwrap().timestamp,
            "1735689600.000001"
        );
        assert_eq!(parse_bound("1.2").unwrap().timestamp, "1.200000");
    }

    #[test]
    fn rejects_invalid_bounds() {
        for input in ["", "-1", "1.", ".1", "1.1234567", "not-a-date"] {
            assert!(parse_bound(input).is_err(), "accepted {input}");
        }
    }

    #[test]
    fn validates_and_intersects_bounds() {
        let duration = TimeLimit::Timestamp("20.000000".to_string());
        assert_eq!(
            list_bounds(&duration, Some("10"), Some("30")).unwrap(),
            (Some("20.000000".to_string()), Some("30.000000".to_string()))
        );
        assert!(list_bounds(&duration, Some("30"), Some("30")).is_err());
    }

    #[test]
    fn resolves_preferred_names_and_mentions() {
        let users = vec![
            User {
                id: "U123456789".to_string(),
                name: Some("alice".to_string()),
                ..Default::default()
            },
            User {
                id: "U987654321".to_string(),
                profile: Some(UserProfile {
                    display_name: Some("Bob B".to_string()),
                    ..Default::default()
                }),
                ..Default::default()
            },
        ];
        let directory = UserDirectory::from_users(users);
        assert_eq!(directory.name_for(Some("U123456789")), Some("alice"));
        assert_eq!(directory.name_for(Some("U000000000")), Some("U000000000"));
        assert_eq!(directory.name_for(None), None);
        assert_eq!(
            directory.replace_mentions(
                "Hi <@U123456789> and <@U987654321|old>; <@U000000000> <#C123|general>"
            ),
            "Hi @alice and @Bob B; <@U000000000> <#C123|general>"
        );
    }
}
