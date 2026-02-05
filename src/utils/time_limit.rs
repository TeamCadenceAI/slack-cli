//! Time limit parsing utilities for Slack CLI
//!
//! Parses --limit values like "1d", "7d", "1m", "90d" into timestamps or message counts.

use chrono::{Duration, Utc};

use crate::error::{Result, SlackError};

/// Represents a parsed time limit for message queries
#[derive(Debug, Clone, PartialEq)]
pub enum TimeLimit {
    /// A timestamp (Unix epoch) for the oldest message to fetch
    Timestamp(String),
    /// A count of messages to fetch
    Count(u32),
}

impl TimeLimit {
    /// Get the limit as a message count, if applicable
    pub fn as_count(&self) -> Option<u32> {
        match self {
            TimeLimit::Count(c) => Some(*c),
            TimeLimit::Timestamp(_) => None,
        }
    }

    /// Get the limit as a timestamp string, if applicable
    pub fn as_timestamp(&self) -> Option<&str> {
        match self {
            TimeLimit::Timestamp(ts) => Some(ts),
            TimeLimit::Count(_) => None,
        }
    }
}

/// Parse a time limit string into a TimeLimit
///
/// Supported formats:
/// - `1d`, `7d`, `30d`, `90d` - days ago
/// - `1w`, `2w`, `4w` - weeks ago
/// - `1m`, `3m`, `6m`, `12m` - months ago (30 days per month)
/// - Pure numbers - treated as message count (e.g., "50", "100")
///
/// # Examples
///
/// ```
/// use slack_cli::utils::parse_time_limit;
///
/// // Time-based limits
/// let limit = parse_time_limit("1d").unwrap();
/// assert!(limit.as_timestamp().is_some());
///
/// // Count-based limits
/// let limit = parse_time_limit("50").unwrap();
/// assert_eq!(limit.as_count(), Some(50));
/// ```
pub fn parse_time_limit(input: &str) -> Result<TimeLimit> {
    let input = input.trim().to_lowercase();

    if input.is_empty() {
        return Err(SlackError::Usage("Empty time limit".to_string()));
    }

    // Check if it's a pure number (message count)
    if let Ok(count) = input.parse::<u32>() {
        if count == 0 {
            return Err(SlackError::Usage(
                "Message count must be greater than 0".to_string(),
            ));
        }
        return Ok(TimeLimit::Count(count));
    }

    // Parse time-based formats
    let (value_str, unit) = parse_duration_parts(&input)?;
    let value: i64 = value_str
        .parse()
        .map_err(|_| SlackError::Usage(format!("Invalid number in time limit: {}", value_str)))?;

    if value <= 0 {
        return Err(SlackError::Usage(
            "Time limit value must be positive".to_string(),
        ));
    }

    let duration = match unit {
        'd' => Duration::days(value),
        'w' => Duration::weeks(value),
        'm' => Duration::days(value * 30), // Approximate months
        'h' => Duration::hours(value),
        _ => {
            return Err(SlackError::Usage(format!(
                "Unknown time unit '{}'. Use d (days), w (weeks), m (months), or h (hours)",
                unit
            )));
        }
    };

    let oldest = Utc::now() - duration;
    let timestamp = format!("{}.000000", oldest.timestamp());

    Ok(TimeLimit::Timestamp(timestamp))
}

/// Parse a duration string into its numeric and unit parts
fn parse_duration_parts(input: &str) -> Result<(&str, char)> {
    // Find the position of the unit character
    let unit_pos = input
        .chars()
        .position(|c| c.is_alphabetic())
        .ok_or_else(|| {
            SlackError::Usage(format!(
                "Invalid time limit format '{}'. Expected format like '7d', '1w', '1m'",
                input
            ))
        })?;

    let value_str = &input[..unit_pos];
    let unit = input
        .chars()
        .nth(unit_pos)
        .ok_or_else(|| SlackError::Usage("Missing time unit".to_string()))?;

    if value_str.is_empty() {
        return Err(SlackError::Usage(
            "Missing numeric value in time limit".to_string(),
        ));
    }

    Ok((value_str, unit))
}

/// Convert a Slack timestamp to Unix timestamp (seconds only)
pub fn slack_ts_to_unix(ts: &str) -> Option<i64> {
    let parts: Vec<&str> = ts.split('.').collect();
    parts.first().and_then(|s| s.parse().ok())
}

/// Convert a Unix timestamp to Slack timestamp format
pub fn unix_to_slack_ts(unix: i64) -> String {
    format!("{}.000000", unix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_parse_time_limit_days() {
        let limit = parse_time_limit("1d").unwrap();
        assert!(matches!(limit, TimeLimit::Timestamp(_)));

        if let TimeLimit::Timestamp(ts) = limit {
            let ts_unix = slack_ts_to_unix(&ts).unwrap();
            let now = Utc::now().timestamp();
            // Should be approximately 1 day ago (within 5 seconds tolerance)
            let expected = now - (24 * 60 * 60);
            assert!((ts_unix - expected).abs() < 5);
        }
    }

    #[test]
    fn test_parse_time_limit_7_days() {
        let limit = parse_time_limit("7d").unwrap();
        if let TimeLimit::Timestamp(ts) = limit {
            let ts_unix = slack_ts_to_unix(&ts).unwrap();
            let now = Utc::now().timestamp();
            let expected = now - (7 * 24 * 60 * 60);
            assert!((ts_unix - expected).abs() < 5);
        }
    }

    #[test]
    fn test_parse_time_limit_weeks() {
        let limit = parse_time_limit("2w").unwrap();
        if let TimeLimit::Timestamp(ts) = limit {
            let ts_unix = slack_ts_to_unix(&ts).unwrap();
            let now = Utc::now().timestamp();
            let expected = now - (14 * 24 * 60 * 60);
            assert!((ts_unix - expected).abs() < 5);
        }
    }

    #[test]
    fn test_parse_time_limit_months() {
        let limit = parse_time_limit("1m").unwrap();
        if let TimeLimit::Timestamp(ts) = limit {
            let ts_unix = slack_ts_to_unix(&ts).unwrap();
            let now = Utc::now().timestamp();
            // 1 month = 30 days
            let expected = now - (30 * 24 * 60 * 60);
            assert!((ts_unix - expected).abs() < 5);
        }
    }

    #[test]
    fn test_parse_time_limit_hours() {
        let limit = parse_time_limit("12h").unwrap();
        if let TimeLimit::Timestamp(ts) = limit {
            let ts_unix = slack_ts_to_unix(&ts).unwrap();
            let now = Utc::now().timestamp();
            let expected = now - (12 * 60 * 60);
            assert!((ts_unix - expected).abs() < 5);
        }
    }

    #[test]
    fn test_parse_time_limit_count() {
        let limit = parse_time_limit("50").unwrap();
        assert_eq!(limit, TimeLimit::Count(50));
        assert_eq!(limit.as_count(), Some(50));
    }

    #[test]
    fn test_parse_time_limit_count_100() {
        let limit = parse_time_limit("100").unwrap();
        assert_eq!(limit, TimeLimit::Count(100));
    }

    #[test]
    fn test_parse_time_limit_uppercase() {
        // Should work with uppercase
        let limit = parse_time_limit("7D").unwrap();
        assert!(matches!(limit, TimeLimit::Timestamp(_)));
    }

    #[test]
    fn test_parse_time_limit_whitespace() {
        let limit = parse_time_limit("  7d  ").unwrap();
        assert!(matches!(limit, TimeLimit::Timestamp(_)));
    }

    #[test]
    fn test_parse_time_limit_90_days() {
        let limit = parse_time_limit("90d").unwrap();
        if let TimeLimit::Timestamp(ts) = limit {
            let ts_unix = slack_ts_to_unix(&ts).unwrap();
            let now = Utc::now().timestamp();
            let expected = now - (90 * 24 * 60 * 60);
            assert!((ts_unix - expected).abs() < 5);
        }
    }

    #[test]
    fn test_parse_time_limit_empty() {
        let result = parse_time_limit("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_time_limit_zero_count() {
        let result = parse_time_limit("0");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_time_limit_negative() {
        let result = parse_time_limit("-7d");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_time_limit_invalid_unit() {
        let result = parse_time_limit("7x");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_time_limit_no_number() {
        let result = parse_time_limit("d");
        assert!(result.is_err());
    }

    #[test]
    fn test_slack_ts_to_unix() {
        assert_eq!(slack_ts_to_unix("1577836800.000100"), Some(1577836800));
        assert_eq!(slack_ts_to_unix("1577836800"), Some(1577836800));
        assert_eq!(slack_ts_to_unix("invalid"), None);
    }

    #[test]
    fn test_unix_to_slack_ts() {
        let ts = unix_to_slack_ts(1577836800);
        assert_eq!(ts, "1577836800.000000");
    }

    #[test]
    fn test_time_limit_methods() {
        let count = TimeLimit::Count(50);
        assert_eq!(count.as_count(), Some(50));
        assert_eq!(count.as_timestamp(), None);

        let timestamp = TimeLimit::Timestamp("1234567890.000000".to_string());
        assert_eq!(timestamp.as_count(), None);
        assert_eq!(timestamp.as_timestamp(), Some("1234567890.000000"));
    }
}
