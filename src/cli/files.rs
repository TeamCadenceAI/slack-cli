//! Files CLI commands for Slack CLI
//!
//! Handles file operations: get, info, list, upload, and search.

use std::path::{Path, PathBuf};

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

    /// Upload a file using Slack's external-upload flow
    Upload {
        /// Path to a regular readable file
        path: PathBuf,

        /// Channel name or ID to share the file to
        #[arg(long)]
        channel: Option<String>,

        /// File title displayed in Slack
        #[arg(long)]
        title: Option<String>,

        /// Comment to post with the shared file
        #[arg(long, requires = "channel")]
        comment: Option<String>,

        /// Thread timestamp to share the file into
        #[arg(long, requires = "channel")]
        thread_ts: Option<String>,

        /// Override the uploaded filename
        #[arg(long)]
        filename: Option<String>,
    },

    /// Search files (requires a user or browser token)
    Search {
        /// Slack file-search query
        query: String,

        /// Number of results per page (1-100)
        #[arg(long, default_value_t = 20, value_parser = parse_search_count)]
        count: u32,

        /// One-based page number
        #[arg(long, default_value_t = 1, value_parser = parse_search_page)]
        page: u32,
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

    let token = crate::auth::resolve_token(workspace, token_override)?;
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

        FilesCommands::Upload {
            path,
            channel,
            title,
            comment,
            thread_ts,
            filename,
        } => {
            upload_file(
                &client,
                path,
                channel.as_deref(),
                title.as_deref(),
                comment.as_deref(),
                thread_ts.as_deref(),
                filename.as_deref(),
                output_mode,
            )
            .await?;
        }

        FilesCommands::Search { query, count, page } => {
            search_files(&client, query, *count, *page, output_mode).await?;
        }
    }

    Ok(())
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

/// Upload a regular file through Slack's external-upload flow.
#[allow(clippy::too_many_arguments)]
async fn upload_file(
    client: &crate::api::SlackClient,
    path: &Path,
    channel: Option<&str>,
    title: Option<&str>,
    comment: Option<&str>,
    thread_ts: Option<&str>,
    filename_override: Option<&str>,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use std::io::Read;

    use crate::error::SlackError;
    use crate::output::write_json;

    if (comment.is_some() || thread_ts.is_some()) && channel.is_none() {
        return Err(SlackError::Usage(
            "--comment and --thread-ts require --channel".to_string(),
        ));
    }
    if path == Path::new("-") {
        return Err(SlackError::Usage(
            "file uploads do not accept stdin; provide a file path".to_string(),
        ));
    }

    let filename = upload_filename(path, filename_override)?;
    // Check the path before opening it: on Windows, opening a directory returns
    // an access-denied error instead of a file handle whose metadata we can
    // inspect.
    if !std::fs::metadata(path)?.is_file() {
        return Err(SlackError::Usage(format!(
            "upload path is not a regular file: {}",
            path.display()
        )));
    }
    let mut file = std::fs::File::open(path)?;

    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;

    // Resolution intentionally happens before the upload URL is requested.
    let channel_id = match channel {
        Some(identifier) => Some(client.resolve_channel(identifier).await?),
        None => None,
    };

    let response = client
        .files_upload_external(
            &filename,
            bytes,
            title,
            channel_id.as_deref(),
            comment,
            thread_ts,
        )
        .await?;

    if output_mode == crate::output::OutputMode::Plain {
        for file in &response.files {
            println!("{}", file.id);
        }
    } else {
        write_json(&serde_json::json!({
            "ok": true,
            "files": response.files,
        }))?;
    }

    Ok(())
}

fn upload_filename(path: &Path, filename_override: Option<&str>) -> crate::error::Result<String> {
    use crate::error::SlackError;

    let filename = match filename_override {
        Some(filename) => filename,
        None => path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                SlackError::Usage(
                    "upload path has no UTF-8 filename; supply --filename".to_string(),
                )
            })?,
    };

    if filename.trim().is_empty()
        || filename == "."
        || filename == ".."
        || filename.contains('/')
        || filename.contains('\\')
        || filename.chars().any(char::is_control)
    {
        return Err(SlackError::Usage(
            "--filename must be a non-empty filename without path separators or control characters"
                .to_string(),
        ));
    }

    Ok(filename.to_string())
}

/// Search files and render either structured JSON or escaped TSV.
async fn search_files(
    client: &crate::api::SlackClient,
    query: &str,
    count: u32,
    page: u32,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use crate::api::file_ops::SearchFilesParams;
    use crate::error::SlackError;
    use crate::output::write_json;

    if !client.supports_search() {
        return Err(SlackError::SearchNotAvailable);
    }

    let response = client
        .search_files(SearchFilesParams {
            query: query.to_string(),
            count,
            page,
        })
        .await?;

    if output_mode == crate::output::OutputMode::Plain {
        for file in &response.files.matches {
            println!(
                "{}\t{}\t{}",
                escape_tsv(&file.id),
                escape_tsv(
                    file.title
                        .as_deref()
                        .filter(|title| !title.is_empty())
                        .or(file.name.as_deref())
                        .unwrap_or(""),
                ),
                escape_tsv(file.permalink.as_deref().unwrap_or("")),
            );
        }
    } else {
        write_json(&serde_json::json!({
            "total": response.files.total,
            "pagination": response.files.pagination,
            "files": response.files.matches,
        }))?;
    }

    Ok(())
}

fn escape_tsv(value: &str) -> String {
    value
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn parse_search_count(value: &str) -> std::result::Result<u32, String> {
    let count = value
        .parse::<u32>()
        .map_err(|_| "count must be an integer from 1 to 100".to_string())?;
    if (1..=100).contains(&count) {
        Ok(count)
    } else {
        Err("count must be from 1 to 100".to_string())
    }
}

fn parse_search_page(value: &str) -> std::result::Result<u32, String> {
    let page = value
        .parse::<u32>()
        .map_err(|_| "page must be a positive integer".to_string())?;
    if page > 0 {
        Ok(page)
    } else {
        Err("page must be positive".to_string())
    }
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
    fn test_parse_files_upload() {
        let cli = Cli::try_parse_from([
            "slack",
            "files",
            "upload",
            "report.bin",
            "--channel",
            "general",
            "--title",
            "Quarterly report",
            "--comment",
            "Please review",
            "--thread-ts",
            "123.456",
            "--filename",
            "report-final.bin",
        ])
        .unwrap();

        match cli.command {
            crate::cli::Commands::Files(FilesCmd {
                command:
                    FilesCommands::Upload {
                        path,
                        channel,
                        title,
                        comment,
                        thread_ts,
                        filename,
                    },
            }) => {
                assert_eq!(path, PathBuf::from("report.bin"));
                assert_eq!(channel.as_deref(), Some("general"));
                assert_eq!(title.as_deref(), Some("Quarterly report"));
                assert_eq!(comment.as_deref(), Some("Please review"));
                assert_eq!(thread_ts.as_deref(), Some("123.456"));
                assert_eq!(filename.as_deref(), Some("report-final.bin"));
            }
            _ => panic!("Expected Upload command"),
        }
    }

    #[test]
    fn test_parse_files_upload_comment_requires_channel() {
        assert!(Cli::try_parse_from([
            "slack",
            "files",
            "upload",
            "report.bin",
            "--comment",
            "hello",
        ])
        .is_err());
        assert!(Cli::try_parse_from([
            "slack",
            "files",
            "upload",
            "report.bin",
            "--thread-ts",
            "123.456",
        ])
        .is_err());
    }

    #[test]
    fn test_parse_files_search_defaults_and_options() {
        let defaults = Cli::try_parse_from(["slack", "files", "search", "budget"]).unwrap();
        match defaults.command {
            crate::cli::Commands::Files(FilesCmd {
                command: FilesCommands::Search { query, count, page },
            }) => {
                assert_eq!(query, "budget");
                assert_eq!(count, 20);
                assert_eq!(page, 1);
            }
            _ => panic!("Expected Search command"),
        }

        let custom = Cli::try_parse_from([
            "slack", "files", "search", "budget", "--count", "50", "--page", "3",
        ])
        .unwrap();
        match custom.command {
            crate::cli::Commands::Files(FilesCmd {
                command: FilesCommands::Search { count, page, .. },
            }) => {
                assert_eq!(count, 50);
                assert_eq!(page, 3);
            }
            _ => panic!("Expected Search command"),
        }
    }

    #[test]
    fn test_parse_files_search_rejects_ranges() {
        assert!(
            Cli::try_parse_from(["slack", "files", "search", "budget", "--count", "0"]).is_err()
        );
        assert!(
            Cli::try_parse_from(["slack", "files", "search", "budget", "--count", "101"]).is_err()
        );
        assert!(
            Cli::try_parse_from(["slack", "files", "search", "budget", "--page", "0"]).is_err()
        );
    }

    #[test]
    fn test_upload_filename_rejects_empty_and_invalid_overrides() {
        let path = PathBuf::from("report.txt");
        assert!(upload_filename(&path, Some("")).is_err());
        assert!(upload_filename(&path, Some("..")).is_err());
        assert!(upload_filename(&path, Some("folder/file.txt")).is_err());
        assert!(upload_filename(&path, Some("bad\nname")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn test_upload_filename_requires_utf8_basename_unless_overridden() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let invalid = PathBuf::from(OsString::from_vec(vec![b'f', 0xff]));
        assert!(upload_filename(&invalid, None).is_err());
        assert_eq!(
            upload_filename(&invalid, Some("fallback.bin")).unwrap(),
            "fallback.bin"
        );
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
