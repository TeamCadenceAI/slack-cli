//! JSON output helpers for Slack CLI
//!
//! Provides functions for writing JSON output to stdout.

use serde::Serialize;
use std::io::{self, Write};

use crate::error::Result;

/// Write any serializable value as pretty-printed JSON to stdout
pub fn write_json<T: Serialize>(value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value)?;
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{}", json)?;
    Ok(())
}

/// Write any serializable value as compact JSON to stdout (no extra whitespace)
pub fn write_json_compact<T: Serialize>(value: &T) -> Result<()> {
    let json = serde_json::to_string(value)?;
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    writeln!(handle, "{}", json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestData {
        id: String,
        name: String,
        count: u32,
    }

    #[test]
    fn test_write_json_serializes_correctly() {
        // Test that serialization works (doesn't panic)
        let data = TestData {
            id: "C123".to_string(),
            name: "general".to_string(),
            count: 42,
        };

        // Serialize to verify the format
        let json = serde_json::to_string_pretty(&data).unwrap();
        assert!(json.contains("\"id\": \"C123\""));
        assert!(json.contains("\"name\": \"general\""));
        assert!(json.contains("\"count\": 42"));
    }

    #[test]
    fn test_write_json_compact_no_whitespace() {
        let data = TestData {
            id: "C123".to_string(),
            name: "general".to_string(),
            count: 42,
        };

        let json = serde_json::to_string(&data).unwrap();
        // Compact JSON should not contain newlines or indentation
        assert!(!json.contains('\n'));
        assert!(!json.contains("  "));
    }

    #[test]
    fn test_write_json_handles_nested_structures() {
        #[derive(Serialize)]
        struct Nested {
            items: Vec<TestData>,
            total: u32,
        }

        let nested = Nested {
            items: vec![
                TestData {
                    id: "C1".to_string(),
                    name: "a".to_string(),
                    count: 1,
                },
                TestData {
                    id: "C2".to_string(),
                    name: "b".to_string(),
                    count: 2,
                },
            ],
            total: 2,
        };

        let json = serde_json::to_string_pretty(&nested).unwrap();
        assert!(json.contains("\"items\""));
        assert!(json.contains("\"total\": 2"));
    }

    #[test]
    fn test_write_json_handles_empty_arrays() {
        let empty: Vec<TestData> = vec![];
        let json = serde_json::to_string_pretty(&empty).unwrap();
        assert_eq!(json, "[]");
    }

    #[test]
    fn test_write_json_handles_optional_fields() {
        #[derive(Serialize)]
        struct WithOptional {
            id: String,
            name: Option<String>,
        }

        let with_some = WithOptional {
            id: "C1".to_string(),
            name: Some("test".to_string()),
        };
        let json_some = serde_json::to_string_pretty(&with_some).unwrap();
        assert!(json_some.contains("\"name\": \"test\""));

        let with_none = WithOptional {
            id: "C2".to_string(),
            name: None,
        };
        let json_none = serde_json::to_string_pretty(&with_none).unwrap();
        assert!(json_none.contains("\"name\": null"));
    }
}
