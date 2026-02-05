//! Files CLI commands for Slack CLI
//!
//! Handles file operations: get, info, list.

use clap::{Args, Subcommand};

/// File operations commands
#[derive(Args, Debug)]
pub struct FilesCmd {
    #[command(subcommand)]
    pub command: FilesCommands,
}

/// File subcommands
#[derive(Subcommand, Debug)]
pub enum FilesCommands {
    /// Download a file by ID
    Get {
        /// File ID
        file_id: String,

        /// Output file (stdout if not specified)
        #[arg(long, short = 'o')]
        output: Option<String>,

        /// Output as base64 (useful for binary files)
        #[arg(long)]
        base64: bool,
    },

    /// Show file info/metadata
    Info {
        /// File ID
        file_id: String,
    },

    /// List files
    List {
        /// Filter by channel
        #[arg(long)]
        channel: Option<String>,

        /// Filter by user
        #[arg(long)]
        user: Option<String>,

        /// Maximum number of files to return
        #[arg(long)]
        limit: Option<u32>,

        /// Pagination cursor
        #[arg(long)]
        cursor: Option<String>,
    },
}

/// Run the files command
pub async fn run(
    cmd: &FilesCmd,
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
        FilesCommands::Get {
            file_id,
            output,
            base64,
        } => {
            get_file(&client, file_id, output.as_deref(), *base64).await?;
        }

        FilesCommands::Info { file_id } => {
            info_file(&client, file_id, output_mode).await?;
        }

        FilesCommands::List {
            channel,
            user,
            limit,
            cursor,
        } => {
            list_files(
                &client,
                channel.as_deref(),
                user.as_deref(),
                *limit,
                cursor.as_deref(),
                output_mode,
            )
            .await?;
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

/// Download a file
async fn get_file(
    client: &crate::api::SlackClient,
    file_id: &str,
    output_path: Option<&str>,
    use_base64: bool,
) -> crate::error::Result<()> {
    use base64::Engine;
    use std::io::Write;

    // Get file info first to check size
    let file = client.files_info(file_id).await?;

    // Check size limit (5MB)
    if !file.is_within_download_limit() {
        return Err(crate::error::SlackError::FileTooLarge);
    }

    // Download the file
    let data = client.files_download(&file).await?;

    // Output
    if let Some(path) = output_path {
        if use_base64 {
            let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
            std::fs::write(path, encoded)?;
        } else {
            std::fs::write(path, &data)?;
        }
        eprintln!(
            "Downloaded {} ({}) to {}",
            file.name.as_deref().unwrap_or(file_id),
            file.human_size(),
            path
        );
    } else {
        // Write to stdout
        let stdout = std::io::stdout();
        let mut handle = stdout.lock();

        if use_base64 {
            let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
            handle.write_all(encoded.as_bytes())?;
            // Add newline for base64 output
            writeln!(handle)?;
        } else {
            handle.write_all(&data)?;
        }
    }

    Ok(())
}

/// Show file info
async fn info_file(
    client: &crate::api::SlackClient,
    file_id: &str,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use crate::output::write_json;

    let file = client.files_info(file_id).await?;

    if output_mode == crate::output::OutputMode::Plain {
        println!("id\t{}", file.id);
        if let Some(name) = &file.name {
            println!("name\t{}", name);
        }
        if let Some(title) = &file.title {
            println!("title\t{}", title);
        }
        if let Some(filetype) = &file.filetype {
            println!("filetype\t{}", filetype);
        }
        if let Some(mimetype) = &file.mimetype {
            println!("mimetype\t{}", mimetype);
        }
        if let Some(size) = file.size {
            println!("size\t{}", size);
            println!("size_human\t{}", file.human_size());
        }
        if let Some(user) = &file.user {
            println!("user\t{}", user);
        }
        if let Some(created) = file.created {
            println!("created\t{}", created);
        }
        if let Some(permalink) = &file.permalink {
            println!("permalink\t{}", permalink);
        }
        println!("is_public\t{}", file.is_public);
        println!("downloadable\t{}", file.is_within_download_limit());
    } else {
        write_json(&file)?;
    }

    Ok(())
}

/// List files
async fn list_files(
    client: &crate::api::SlackClient,
    channel: Option<&str>,
    user: Option<&str>,
    limit: Option<u32>,
    cursor: Option<&str>,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use crate::output::{write_files_plain, write_json, FilePlain};

    let response = client.files_list(channel, user, limit, cursor).await?;

    if output_mode == crate::output::OutputMode::Plain {
        let plain_files: Vec<FilePlain> = response
            .files
            .iter()
            .map(|f| FilePlain {
                id: &f.id,
                name: f.name.as_deref().unwrap_or(""),
                filetype: f.filetype.as_deref().unwrap_or(""),
                size: f.size.unwrap_or(0),
            })
            .collect();
        write_files_plain(&plain_files)?;
    } else {
        write_json(&serde_json::json!({
            "files": response.files,
            "paging": response.paging,
        }))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    use crate::cli::Cli;

    #[test]
    fn test_files_cmd_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn test_parse_files_get() {
        let cli = Cli::try_parse_from(["slack", "files", "get", "F123456789"]).unwrap();
        if let crate::cli::Commands::Files(files_cmd) = cli.command {
            if let FilesCommands::Get {
                file_id,
                output,
                base64,
            } = files_cmd.command
            {
                assert_eq!(file_id, "F123456789");
                assert!(output.is_none());
                assert!(!base64);
            } else {
                panic!("Expected Get command");
            }
        } else {
            panic!("Expected Files command");
        }
    }

    #[test]
    fn test_parse_files_get_with_options() {
        let cli = Cli::try_parse_from([
            "slack",
            "files",
            "get",
            "F123456789",
            "-o",
            "output.bin",
            "--base64",
        ])
        .unwrap();
        if let crate::cli::Commands::Files(files_cmd) = cli.command {
            if let FilesCommands::Get {
                file_id,
                output,
                base64,
            } = files_cmd.command
            {
                assert_eq!(file_id, "F123456789");
                assert_eq!(output, Some("output.bin".to_string()));
                assert!(base64);
            } else {
                panic!("Expected Get command");
            }
        } else {
            panic!("Expected Files command");
        }
    }

    #[test]
    fn test_parse_files_info() {
        let cli = Cli::try_parse_from(["slack", "files", "info", "F123456789"]).unwrap();
        if let crate::cli::Commands::Files(files_cmd) = cli.command {
            if let FilesCommands::Info { file_id } = files_cmd.command {
                assert_eq!(file_id, "F123456789");
            } else {
                panic!("Expected Info command");
            }
        } else {
            panic!("Expected Files command");
        }
    }

    #[test]
    fn test_parse_files_list() {
        let cli = Cli::try_parse_from(["slack", "files", "list"]).unwrap();
        if let crate::cli::Commands::Files(files_cmd) = cli.command {
            if let FilesCommands::List {
                channel,
                user,
                limit,
                cursor,
            } = files_cmd.command
            {
                assert!(channel.is_none());
                assert!(user.is_none());
                assert!(limit.is_none());
                assert!(cursor.is_none());
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Files command");
        }
    }

    #[test]
    fn test_parse_files_list_with_filters() {
        let cli = Cli::try_parse_from([
            "slack",
            "files",
            "list",
            "--channel",
            "C123",
            "--user",
            "U456",
            "--limit",
            "10",
        ])
        .unwrap();
        if let crate::cli::Commands::Files(files_cmd) = cli.command {
            if let FilesCommands::List {
                channel,
                user,
                limit,
                cursor,
            } = files_cmd.command
            {
                assert_eq!(channel, Some("C123".to_string()));
                assert_eq!(user, Some("U456".to_string()));
                assert_eq!(limit, Some(10));
                assert!(cursor.is_none());
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Files command");
        }
    }

    #[test]
    fn test_parse_files_alias() {
        let cli = Cli::try_parse_from(["slack", "f", "list"]).unwrap();
        if let crate::cli::Commands::Files(_) = cli.command {
            // Success
        } else {
            panic!("Expected Files command");
        }
    }
}
