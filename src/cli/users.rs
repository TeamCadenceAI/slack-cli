//! Users CLI commands for Slack CLI
//!
//! Handles user operations: list, info, me, export.

use clap::{Args, Subcommand};

/// User operations commands
#[derive(Args, Debug)]
pub struct UsersCmd {
    #[command(subcommand)]
    pub command: UsersCommands,
}

/// User subcommands
#[derive(Subcommand, Debug)]
pub enum UsersCommands {
    /// List all workspace users
    List {
        /// Include deactivated users
        #[arg(long)]
        include_deactivated: bool,

        /// Maximum number of users to return
        #[arg(long)]
        limit: Option<u32>,

        /// Pagination cursor for next page
        #[arg(long)]
        cursor: Option<String>,
    },

    /// Show user info
    Info {
        /// Username or user ID
        user: String,
    },

    /// Show current authenticated user
    Me,

    /// Export all users to CSV
    Export {
        /// Output file (stdout if not specified)
        #[arg(long, short = 'o')]
        output: Option<String>,

        /// Include deactivated users
        #[arg(long)]
        include_deactivated: bool,
    },
}

/// Run the users command
pub async fn run(
    cmd: &UsersCmd,
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
        UsersCommands::List {
            include_deactivated,
            limit,
            cursor,
        } => {
            list_users(
                &client,
                *include_deactivated,
                *limit,
                cursor.as_deref(),
                output_mode,
            )
            .await?;
        }

        UsersCommands::Info { user } => {
            info_user(&client, user, output_mode).await?;
        }

        UsersCommands::Me => {
            me(&client, output_mode).await?;
        }

        UsersCommands::Export {
            output,
            include_deactivated,
        } => {
            export_users(&client, output.as_deref(), *include_deactivated).await?;
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
        // Token override provided on command line
        let token_type = TokenType::from_prefix(token_str).ok_or_else(|| {
            SlackError::InvalidToken("Token must start with xoxp-, xoxb-, or xoxc-".into())
        })?;

        if token_type == TokenType::Browser {
            return Err(SlackError::InvalidToken(
                "Browser tokens require --xoxc and --xoxd flags in 'auth add'".into(),
            ));
        }

        // Create a temporary token set
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
            // Workspace specified
            let workspaces = store.get_workspace_info()?;
            let ws = workspaces
                .iter()
                .find(|w| w.team_id == *ws_name || w.team_name == *ws_name)
                .ok_or_else(|| SlackError::WorkspaceNotFound(ws_name.to_string()))?;
            store
                .get_token(&ws.team_id)?
                .ok_or(SlackError::AuthRequired)
        } else {
            // Use default or first workspace
            store
                .get_default_or_first()?
                .ok_or(SlackError::AuthRequired)
        }
    }
}

/// List users
async fn list_users(
    client: &crate::api::SlackClient,
    include_deactivated: bool,
    limit: Option<u32>,
    cursor: Option<&str>,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use crate::api::types::PaginationParams;
    use crate::output::{write_json, write_users_plain, UserPlain};

    let mut params = PaginationParams::new();

    if let Some(l) = limit {
        params = params.with_limit(l);
    }

    if let Some(c) = cursor {
        params = params.with_cursor(c);
    }

    let response = client.users_list(params).await?;

    // Filter out deactivated users if not requested
    let users: Vec<_> = if include_deactivated {
        response.members
    } else {
        response
            .members
            .into_iter()
            .filter(|u| !u.deleted)
            .collect()
    };

    if output_mode == crate::output::OutputMode::Plain {
        let plain_users: Vec<UserPlain> = users
            .iter()
            .map(|u| UserPlain {
                id: &u.id,
                name: u.name.as_deref().unwrap_or(""),
                real_name: u.real_name.as_deref(),
                email: u.profile.as_ref().and_then(|p| p.email.as_deref()),
            })
            .collect();
        write_users_plain(&plain_users)?;
    } else {
        // For JSON output, include the full response with pagination metadata
        write_json(&serde_json::json!({
            "members": users,
            "response_metadata": response.response_metadata,
        }))?;
    }

    Ok(())
}

/// Show user info
async fn info_user(
    client: &crate::api::SlackClient,
    user_identifier: &str,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use crate::output::write_json;

    // Resolve user name to ID if needed, then get full info
    let user = client.resolve_user_info(user_identifier).await?;

    if output_mode == crate::output::OutputMode::Plain {
        // For plain output, print key fields on separate lines
        println!("id\t{}", user.id);
        if let Some(name) = &user.name {
            println!("name\t{}", name);
        }
        if let Some(real_name) = &user.real_name {
            println!("real_name\t{}", real_name);
        }
        if let Some(profile) = &user.profile {
            if let Some(email) = &profile.email {
                println!("email\t{}", email);
            }
            if let Some(title) = &profile.title {
                if !title.is_empty() {
                    println!("title\t{}", title);
                }
            }
            if let Some(phone) = &profile.phone {
                if !phone.is_empty() {
                    println!("phone\t{}", phone);
                }
            }
            if let Some(status_text) = &profile.status_text {
                if !status_text.is_empty() {
                    println!("status_text\t{}", status_text);
                }
            }
            if let Some(status_emoji) = &profile.status_emoji {
                if !status_emoji.is_empty() {
                    println!("status_emoji\t{}", status_emoji);
                }
            }
        }
        println!("is_admin\t{}", user.is_admin);
        println!("is_owner\t{}", user.is_owner);
        println!("is_bot\t{}", user.is_bot);
        println!("deleted\t{}", user.deleted);
        if let Some(tz) = &user.tz {
            println!("tz\t{}", tz);
        }
    } else {
        write_json(&user)?;
    }

    Ok(())
}

/// Show current authenticated user
async fn me(
    client: &crate::api::SlackClient,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use crate::output::write_json;

    // Get auth.test to get current user info
    let auth_info = client.auth_test().await?;

    // Then get full user info
    let user = client.users_info(&auth_info.user_id).await?;

    if output_mode == crate::output::OutputMode::Plain {
        println!("id\t{}", user.id);
        if let Some(name) = &user.name {
            println!("name\t{}", name);
        }
        if let Some(real_name) = &user.real_name {
            println!("real_name\t{}", real_name);
        }
        println!("team_id\t{}", auth_info.team_id);
        println!("team\t{}", auth_info.team);
        if let Some(profile) = &user.profile {
            if let Some(email) = &profile.email {
                println!("email\t{}", email);
            }
        }
    } else {
        write_json(&serde_json::json!({
            "user": user,
            "auth": {
                "team_id": auth_info.team_id,
                "team": auth_info.team,
                "url": auth_info.url,
            }
        }))?;
    }

    Ok(())
}

/// Export users to CSV
async fn export_users(
    client: &crate::api::SlackClient,
    output_path: Option<&str>,
    include_deactivated: bool,
) -> crate::error::Result<()> {
    // Get all users (paginated)
    let all_users = client.users_list_all().await?;

    // Filter out deactivated users if not requested
    let users: Vec<_> = if include_deactivated {
        all_users
    } else {
        all_users.into_iter().filter(|u| !u.deleted).collect()
    };

    // Prepare CSV content
    let mut csv_content = String::from("id,name,real_name,email,is_admin\n");

    for user in &users {
        let name = user.name.as_deref().unwrap_or("");
        let real_name = user.real_name.as_deref().unwrap_or("");
        let email = user
            .profile
            .as_ref()
            .and_then(|p| p.email.as_deref())
            .unwrap_or("");
        let is_admin = user.is_admin;

        // Escape fields if they contain comma or quotes
        let escaped_name = escape_csv_field(name);
        let escaped_real_name = escape_csv_field(real_name);
        let escaped_email = escape_csv_field(email);

        csv_content.push_str(&format!(
            "{},{},{},{},{}\n",
            user.id, escaped_name, escaped_real_name, escaped_email, is_admin
        ));
    }

    // Write to file or stdout
    if let Some(path) = output_path {
        std::fs::write(path, &csv_content)?;
        eprintln!("Exported {} users to {}", users.len(), path);
    } else {
        print!("{}", csv_content);
    }

    Ok(())
}

/// Escape a field for CSV output
fn escape_csv_field(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    // Import the parent Cli for full parsing tests
    use crate::cli::Cli;

    #[test]
    fn test_users_cmd_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn test_parse_users_list_default() {
        let cli = Cli::try_parse_from(["slack", "users", "list"]).unwrap();
        if let crate::cli::Commands::Users(users_cmd) = cli.command {
            if let UsersCommands::List {
                include_deactivated,
                limit,
                cursor,
            } = users_cmd.command
            {
                assert!(!include_deactivated);
                assert!(limit.is_none());
                assert!(cursor.is_none());
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Users command");
        }
    }

    #[test]
    fn test_parse_users_list_with_flags() {
        let cli = Cli::try_parse_from([
            "slack",
            "users",
            "list",
            "--include-deactivated",
            "--limit",
            "50",
        ])
        .unwrap();
        if let crate::cli::Commands::Users(users_cmd) = cli.command {
            if let UsersCommands::List {
                include_deactivated,
                limit,
                cursor,
            } = users_cmd.command
            {
                assert!(include_deactivated);
                assert_eq!(limit, Some(50));
                assert!(cursor.is_none());
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Users command");
        }
    }

    #[test]
    fn test_parse_users_info() {
        let cli = Cli::try_parse_from(["slack", "users", "info", "U123456789"]).unwrap();
        if let crate::cli::Commands::Users(users_cmd) = cli.command {
            if let UsersCommands::Info { user } = users_cmd.command {
                assert_eq!(user, "U123456789");
            } else {
                panic!("Expected Info command");
            }
        } else {
            panic!("Expected Users command");
        }
    }

    #[test]
    fn test_parse_users_info_by_name() {
        let cli = Cli::try_parse_from(["slack", "users", "info", "@johndoe"]).unwrap();
        if let crate::cli::Commands::Users(users_cmd) = cli.command {
            if let UsersCommands::Info { user } = users_cmd.command {
                assert_eq!(user, "@johndoe");
            } else {
                panic!("Expected Info command");
            }
        } else {
            panic!("Expected Users command");
        }
    }

    #[test]
    fn test_parse_users_me() {
        let cli = Cli::try_parse_from(["slack", "users", "me"]).unwrap();
        if let crate::cli::Commands::Users(users_cmd) = cli.command {
            if let UsersCommands::Me = users_cmd.command {
                // Success
            } else {
                panic!("Expected Me command");
            }
        } else {
            panic!("Expected Users command");
        }
    }

    #[test]
    fn test_parse_users_export_default() {
        let cli = Cli::try_parse_from(["slack", "users", "export"]).unwrap();
        if let crate::cli::Commands::Users(users_cmd) = cli.command {
            if let UsersCommands::Export {
                output,
                include_deactivated,
            } = users_cmd.command
            {
                assert!(output.is_none());
                assert!(!include_deactivated);
            } else {
                panic!("Expected Export command");
            }
        } else {
            panic!("Expected Users command");
        }
    }

    #[test]
    fn test_parse_users_export_with_output() {
        let cli = Cli::try_parse_from(["slack", "users", "export", "-o", "users.csv"]).unwrap();
        if let crate::cli::Commands::Users(users_cmd) = cli.command {
            if let UsersCommands::Export {
                output,
                include_deactivated,
            } = users_cmd.command
            {
                assert_eq!(output, Some("users.csv".to_string()));
                assert!(!include_deactivated);
            } else {
                panic!("Expected Export command");
            }
        } else {
            panic!("Expected Users command");
        }
    }

    #[test]
    fn test_parse_users_alias() {
        let cli = Cli::try_parse_from(["slack", "u", "list"]).unwrap();
        if let crate::cli::Commands::Users(_) = cli.command {
            // Success
        } else {
            panic!("Expected Users command");
        }
    }

    #[test]
    fn test_escape_csv_field_no_special() {
        assert_eq!(escape_csv_field("simple"), "simple");
    }

    #[test]
    fn test_escape_csv_field_with_comma() {
        assert_eq!(escape_csv_field("a,b"), "\"a,b\"");
    }

    #[test]
    fn test_escape_csv_field_with_quote() {
        assert_eq!(escape_csv_field("say \"hello\""), "\"say \"\"hello\"\"\"");
    }

    #[test]
    fn test_escape_csv_field_with_newline() {
        assert_eq!(escape_csv_field("line1\nline2"), "\"line1\nline2\"");
    }
}
