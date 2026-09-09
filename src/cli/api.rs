//! Generic Slack API escape hatch for Slack CLI
//!
//! `slack api <method>` makes an authenticated request to any Slack Web API
//! method, printing the full JSON response. Modeled after `gh api`.

use clap::Args;

/// Generic Slack API request command
#[derive(Args, Debug)]
#[command(after_help = "Examples:\n  \
        slack api conversations.list -X GET -f limit=10\n  \
        slack api chat.postMessage -f channel=C123456 -f text='hello'\n  \
        slack api chat.postMessage --input payload.json\n  \
        echo '{\"channel\":\"C123456\",\"text\":\"hi\"}' | slack api chat.postMessage --input -\n\n\
        Parameters are sent as query parameters for GET and form-encoded for POST.\n\
        Nested JSON values (from -F, or --input) are serialized as JSON strings,\n\
        matching how the built-in commands encode parameters.")]
pub struct ApiCmd {
    /// Slack API method name (e.g. "chat.postMessage") or a full
    /// `https://slack.com/api/<method>` URL
    pub endpoint: String,

    /// HTTP method to use (GET or POST)
    #[arg(
        short = 'X',
        long,
        value_name = "METHOD",
        default_value = "POST",
        value_parser = parse_method
    )]
    pub method: String,

    /// Add a string parameter as key=value (value is always sent as a string;
    /// repeatable)
    #[arg(short = 'f', long = "raw-field", value_name = "KEY=VALUE")]
    pub raw_fields: Vec<String>,

    /// Add a typed parameter as key=value: JSON booleans, numbers, arrays and
    /// objects are parsed; anything else is sent as a string. A literal
    /// "null" is rejected (Slack's form encoding omits null values); use -f
    /// to send the string "null", or omit the field entirely. Repeatable.
    #[arg(short = 'F', long = "field", value_name = "KEY=VALUE")]
    pub fields: Vec<String>,

    /// Read parameters from FILE containing a JSON object ("-" reads stdin).
    /// Cannot be combined with -f/-F. Note: null-valued fields in the JSON
    /// object are omitted from the request (the form encoder skips nulls).
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with_all = ["raw_fields", "fields"]
    )]
    pub input: Option<String>,
}

/// Run the api command
pub async fn run(
    cmd: &ApiCmd,
    plain: bool,
    workspace: Option<&str>,
    token_override: Option<&str>,
) -> crate::error::Result<()> {
    use crate::api::SlackClient;
    use crate::error::SlackError;
    use crate::output::write_json;

    if plain {
        return Err(SlackError::Usage(
            "'slack api' does not support --plain; the full JSON response is always printed".into(),
        ));
    }

    let method = parse_method(&cmd.method).map_err(SlackError::Usage)?;
    let http_method = if method == "GET" {
        reqwest::Method::GET
    } else {
        reqwest::Method::POST
    };

    let params = if let Some(input) = &cmd.input {
        read_input(input)?
    } else {
        build_params(&cmd.raw_fields, &cmd.fields)?
    };

    let token = crate::auth::resolve_token(workspace, token_override)?;
    let client = SlackClient::new(token)?;

    let response = client
        .api_request(&cmd.endpoint, http_method, &params)
        .await?;

    write_json(&response)?;
    Ok(())
}

/// Validate and normalize the HTTP method flag (GET/POST only)
fn parse_method(s: &str) -> Result<String, String> {
    let upper = s.to_ascii_uppercase();
    match upper.as_str() {
        "GET" | "POST" => Ok(upper),
        _ => Err(format!(
            "unsupported HTTP method '{s}': only GET and POST are supported"
        )),
    }
}

/// Split a KEY=VALUE argument into its parts
fn split_field(arg: &str) -> crate::error::Result<(&str, &str)> {
    use crate::error::SlackError;

    let (key, value) = arg.split_once('=').ok_or_else(|| {
        SlackError::Usage(format!("invalid field '{arg}': expected KEY=VALUE format"))
    })?;
    if key.is_empty() {
        return Err(SlackError::Usage(format!(
            "invalid field '{arg}': key must not be empty"
        )));
    }
    Ok((key, value))
}

/// Parse a typed (-F/--field) value: JSON booleans/numbers/arrays/objects are
/// parsed, null is rejected, anything else stays a string.
fn parse_typed_value(key: &str, raw: &str) -> crate::error::Result<serde_json::Value> {
    use crate::error::SlackError;

    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::Null) => Err(SlackError::Usage(format!(
            "field '{key}' has value null, which cannot be sent (form encoding omits nulls); \
             omit the field, or use -f {key}=null to send the string \"null\""
        ))),
        Ok(value) => Ok(value),
        Err(_) => Ok(serde_json::Value::String(raw.to_string())),
    }
}

/// Build the request parameter object from -f/--raw-field and -F/--field args
fn build_params(
    raw_fields: &[String],
    fields: &[String],
) -> crate::error::Result<serde_json::Value> {
    use crate::error::SlackError;

    let mut map = serde_json::Map::new();

    for arg in raw_fields {
        let (key, value) = split_field(arg)?;
        if map
            .insert(
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            )
            .is_some()
        {
            return Err(SlackError::Usage(format!(
                "duplicate field key '{key}': each key may only be specified once"
            )));
        }
    }

    for arg in fields {
        let (key, value) = split_field(arg)?;
        let parsed = parse_typed_value(key, value)?;
        if map.insert(key.to_string(), parsed).is_some() {
            return Err(SlackError::Usage(format!(
                "duplicate field key '{key}': each key may only be specified once"
            )));
        }
    }

    Ok(serde_json::Value::Object(map))
}

/// Read request parameters from a file (or stdin when "-"), requiring a JSON
/// object at the top level.
fn read_input(path: &str) -> crate::error::Result<serde_json::Value> {
    use crate::error::SlackError;
    use std::io::Read;

    let contents = if path == "-" {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        buf
    } else {
        std::fs::read_to_string(path)?
    };

    let value: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|e| SlackError::Usage(format!("--input is not valid JSON: {e}")))?;

    if !value.is_object() {
        return Err(SlackError::Usage(
            "--input must contain a JSON object of parameters (e.g. {\"channel\": \"C123\"})"
                .into(),
        ));
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Commands};
    use crate::error::SlackError;
    use clap::{CommandFactory, Parser};
    use serde_json::json;

    #[test]
    fn test_api_cmd_valid() {
        Cli::command().debug_assert();
    }

    // --- clap parsing ---

    fn parse_api(args: &[&str]) -> ApiCmd {
        let mut full = vec!["slack", "api"];
        full.extend_from_slice(args);
        let cli = Cli::try_parse_from(full).unwrap();
        match cli.command {
            Commands::Api(cmd) => cmd,
            _ => panic!("Expected Api command"),
        }
    }

    #[test]
    fn test_parse_defaults_to_post() {
        let cmd = parse_api(&["chat.postMessage"]);
        assert_eq!(cmd.endpoint, "chat.postMessage");
        assert_eq!(cmd.method, "POST");
        assert!(cmd.raw_fields.is_empty());
        assert!(cmd.fields.is_empty());
        assert!(cmd.input.is_none());
    }

    #[test]
    fn test_parse_get_method_case_insensitive() {
        let cmd = parse_api(&["conversations.list", "-X", "get"]);
        assert_eq!(cmd.method, "GET");
        let cmd = parse_api(&["conversations.list", "--method", "GET"]);
        assert_eq!(cmd.method, "GET");
    }

    #[test]
    fn test_parse_rejects_other_methods() {
        for method in ["PUT", "DELETE", "PATCH", "HEAD", "bogus"] {
            let result = Cli::try_parse_from(["slack", "api", "users.list", "-X", method]);
            assert!(result.is_err(), "method {method} should be rejected");
        }
    }

    #[test]
    fn test_parse_fields_repeatable() {
        let cmd = parse_api(&[
            "chat.postMessage",
            "-f",
            "channel=C123",
            "--raw-field",
            "text=hello",
            "-F",
            "limit=10",
            "--field",
            "extra=true",
        ]);
        assert_eq!(cmd.raw_fields, vec!["channel=C123", "text=hello"]);
        assert_eq!(cmd.fields, vec!["limit=10", "extra=true"]);
    }

    #[test]
    fn test_parse_rejects_input_mixed_with_fields() {
        let result = Cli::try_parse_from([
            "slack",
            "api",
            "chat.postMessage",
            "--input",
            "params.json",
            "-f",
            "text=hi",
        ]);
        assert!(result.is_err());

        let result = Cli::try_parse_from([
            "slack",
            "api",
            "chat.postMessage",
            "--input",
            "-",
            "-F",
            "limit=10",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_requires_endpoint() {
        let result = Cli::try_parse_from(["slack", "api"]);
        assert!(result.is_err());
    }

    // --- split_field ---

    #[test]
    fn test_split_field_basic() {
        assert_eq!(split_field("key=value").unwrap(), ("key", "value"));
        // Only the first '=' splits; values may contain '='
        assert_eq!(split_field("k=a=b").unwrap(), ("k", "a=b"));
        // Empty values are allowed
        assert_eq!(split_field("k=").unwrap(), ("k", ""));
    }

    #[test]
    fn test_split_field_rejects_missing_separator() {
        let err = split_field("noequals").unwrap_err();
        assert!(matches!(err, SlackError::Usage(_)));
    }

    #[test]
    fn test_split_field_rejects_empty_key() {
        let err = split_field("=value").unwrap_err();
        assert!(matches!(err, SlackError::Usage(_)));
    }

    // --- parse_typed_value ---

    #[test]
    fn test_typed_value_booleans_and_numbers() {
        assert_eq!(parse_typed_value("k", "true").unwrap(), json!(true));
        assert_eq!(parse_typed_value("k", "false").unwrap(), json!(false));
        assert_eq!(parse_typed_value("k", "42").unwrap(), json!(42));
        assert_eq!(parse_typed_value("k", "-1.5").unwrap(), json!(-1.5));
    }

    #[test]
    fn test_typed_value_arrays_and_objects() {
        assert_eq!(parse_typed_value("k", "[1,2,3]").unwrap(), json!([1, 2, 3]));
        assert_eq!(
            parse_typed_value("k", r#"{"a":{"b":true}}"#).unwrap(),
            json!({"a": {"b": true}})
        );
    }

    #[test]
    fn test_typed_value_falls_back_to_string() {
        assert_eq!(parse_typed_value("k", "hello").unwrap(), json!("hello"));
        // Invalid JSON stays a string, predictably
        assert_eq!(parse_typed_value("k", "007").unwrap(), json!("007"));
        assert_eq!(parse_typed_value("k", "[1,2").unwrap(), json!("[1,2"));
        // Quoted JSON strings parse to the unquoted string
        assert_eq!(parse_typed_value("k", "\"null\"").unwrap(), json!("null"));
    }

    #[test]
    fn test_typed_value_rejects_null() {
        let err = parse_typed_value("k", "null").unwrap_err();
        match err {
            SlackError::Usage(msg) => {
                assert!(msg.contains("null"), "message should mention null: {msg}");
            }
            other => panic!("Expected Usage error, got {other:?}"),
        }
    }

    // --- build_params ---

    #[test]
    fn test_build_params_combines_raw_and_typed() {
        let params = build_params(
            &["channel=C123".into(), "text=hi there".into()],
            &["limit=10".into(), "unfurl=false".into()],
        )
        .unwrap();
        assert_eq!(
            params,
            json!({
                "channel": "C123",
                "text": "hi there",
                "limit": 10,
                "unfurl": false,
            })
        );
    }

    #[test]
    fn test_build_params_raw_field_never_parses_json() {
        let params = build_params(&["count=10".into(), "flag=true".into()], &[]).unwrap();
        assert_eq!(params, json!({"count": "10", "flag": "true"}));
    }

    #[test]
    fn test_build_params_empty() {
        let params = build_params(&[], &[]).unwrap();
        assert_eq!(params, json!({}));
    }

    #[test]
    fn test_build_params_rejects_duplicate_keys() {
        // Duplicate within raw fields
        let err = build_params(&["a=1".into(), "a=2".into()], &[]).unwrap_err();
        assert!(matches!(err, SlackError::Usage(_)));

        // Duplicate within typed fields
        let err = build_params(&[], &["a=1".into(), "a=2".into()]).unwrap_err();
        assert!(matches!(err, SlackError::Usage(_)));

        // Duplicate across raw and typed fields
        let err = build_params(&["a=1".into()], &["a=2".into()]).unwrap_err();
        assert!(matches!(err, SlackError::Usage(_)));
    }

    // --- read_input ---

    #[test]
    fn test_read_input_valid_object() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("slack_cli_api_test_{}.json", std::process::id()));
        std::fs::write(&path, r#"{"channel": "C123", "text": "hi"}"#).unwrap();
        let params = read_input(path.to_str().unwrap()).unwrap();
        assert_eq!(params, json!({"channel": "C123", "text": "hi"}));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_input_rejects_non_object() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "slack_cli_api_test_arr_{}.json",
            std::process::id()
        ));
        std::fs::write(&path, r#"[1, 2, 3]"#).unwrap();
        let err = read_input(path.to_str().unwrap()).unwrap_err();
        assert!(matches!(err, SlackError::Usage(_)));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_input_rejects_invalid_json() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "slack_cli_api_test_bad_{}.json",
            std::process::id()
        ));
        std::fs::write(&path, "not json").unwrap();
        let err = read_input(path.to_str().unwrap()).unwrap_err();
        assert!(matches!(err, SlackError::Usage(_)));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_read_input_missing_file_is_io_error() {
        let err = read_input("/nonexistent/definitely-missing.json").unwrap_err();
        assert!(matches!(err, SlackError::Io(_)));
    }

    // --- run guards ---

    #[tokio::test]
    async fn test_run_rejects_plain() {
        let cmd = parse_api(&["auth.test"]);
        let err = run(&cmd, true, None, None).await.unwrap_err();
        match err {
            SlackError::Usage(msg) => assert!(msg.contains("--plain")),
            other => panic!("Expected Usage error, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_method_validator() {
        assert_eq!(parse_method("get").unwrap(), "GET");
        assert_eq!(parse_method("Post").unwrap(), "POST");
        assert!(parse_method("PUT").is_err());
        assert!(parse_method("").is_err());
    }
}
