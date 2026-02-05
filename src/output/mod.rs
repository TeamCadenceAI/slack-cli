mod json;
mod plain;

pub use json::{write_json, write_json_compact};
pub use plain::{
    write_channels_plain, write_files_plain, write_list_plain, write_messages_plain,
    write_users_plain, ChannelPlain, FilePlain, MessagePlain, UserPlain,
};

use crate::error::{format_error_json, SlackError};

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum OutputMode {
    #[default]
    Json,
    Plain,
}

impl OutputMode {
    pub fn from_flags(plain: bool) -> Self {
        if plain {
            OutputMode::Plain
        } else {
            OutputMode::Json
        }
    }

    pub fn from_env() -> Self {
        if std::env::var("SLACK_PLAIN").is_ok() {
            OutputMode::Plain
        } else {
            OutputMode::Json
        }
    }

    /// Write error in the appropriate format
    pub fn write_error(&self, err: &SlackError) {
        match self {
            OutputMode::Json => {
                let json = format_error_json(err);
                if let Ok(s) = serde_json::to_string_pretty(&json) {
                    println!("{}", s);
                }
            }
            OutputMode::Plain => {
                eprintln!("Error: {}", err);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_mode_from_flags() {
        assert_eq!(OutputMode::from_flags(false), OutputMode::Json);
        assert_eq!(OutputMode::from_flags(true), OutputMode::Plain);
    }

    #[test]
    fn test_output_mode_default() {
        let mode = OutputMode::default();
        assert_eq!(mode, OutputMode::Json);
    }

    #[test]
    fn test_output_mode_from_env() {
        // Without SLACK_PLAIN set (remove it if present)
        std::env::remove_var("SLACK_PLAIN");
        assert_eq!(OutputMode::from_env(), OutputMode::Json);

        // With SLACK_PLAIN set
        std::env::set_var("SLACK_PLAIN", "1");
        assert_eq!(OutputMode::from_env(), OutputMode::Plain);

        // Cleanup
        std::env::remove_var("SLACK_PLAIN");
    }
}
