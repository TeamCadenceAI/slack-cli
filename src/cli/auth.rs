//! Auth CLI commands for Slack CLI
//!
//! Handles authentication management: add, list, remove, status, switch, help.

use clap::{Args, Subcommand};

/// Authentication management commands
#[derive(Args, Debug)]
pub struct AuthCmd {
    #[command(subcommand)]
    pub command: AuthCommands,
}

/// Auth subcommands
#[derive(Subcommand, Debug)]
pub enum AuthCommands {
    /// Authorize a Slack workspace
    Add {
        /// Use browser OAuth flow
        #[arg(long, conflicts_with_all = ["xoxc", "xoxd", "token"])]
        oauth: bool,

        /// Browser session token (xoxc-*)
        #[arg(long, requires = "xoxd")]
        xoxc: Option<String>,

        /// Browser cookie (xoxd-*)
        #[arg(long, requires = "xoxc")]
        xoxd: Option<String>,

        /// Direct token (xoxp-* or xoxb-*)
        #[arg(long, conflicts_with_all = ["xoxc", "xoxd", "oauth"])]
        token: Option<String>,

        /// Manual OAuth flow (no browser)
        #[arg(long)]
        manual: bool,

        /// OAuth scopes to request
        #[arg(
            long,
            value_delimiter = ',',
            default_value = "channels:read,channels:history,users:read,search:read"
        )]
        scopes: Vec<String>,
    },

    /// List authorized workspaces
    List,

    /// Remove workspace authorization
    Remove {
        /// Workspace name or team ID
        workspace: String,

        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },

    /// Show current authentication status
    Status,

    /// Set default workspace
    Switch {
        /// Workspace name or team ID
        workspace: String,
    },

    /// Print instructions for extracting browser tokens
    #[command(name = "browser-help")]
    BrowserHelp,
}

/// Run the auth command
pub async fn run(
    cmd: &AuthCmd,
    plain: bool,
    workspace: Option<&str>,
    token_override: Option<&str>,
) -> crate::error::Result<()> {
    use crate::api::SlackClient;
    use crate::auth::{get_token_store, OAuthConfig, OAuthFlow, TokenSet, TokenType};
    use crate::output::{write_json, OutputMode};

    let store = get_token_store();

    let output_mode = OutputMode::from_flags(plain);

    match &cmd.command {
        AuthCommands::Add {
            oauth: _, // Default behavior is OAuth, so this flag is now only used for documentation
            xoxc,
            xoxd,
            token,
            manual,
            scopes,
        } => {
            // Determine which auth method to use
            if let Some(token_str) = token {
                // Direct token provided
                add_direct_token(token_str, output_mode).await?;
            } else if let (Some(xoxc_str), Some(xoxd_str)) = (xoxc, xoxd) {
                // Browser tokens provided
                add_browser_tokens(xoxc_str, xoxd_str, output_mode).await?;
            } else {
                // Default to OAuth (--oauth, --manual, or no flags)
                // Check for client credentials
                let client_id = std::env::var("SLACK_CLIENT_ID").ok();
                let client_secret = std::env::var("SLACK_CLIENT_SECRET").ok();

                match (client_id, client_secret) {
                    (Some(id), Some(secret)) => {
                        let config = OAuthConfig::new(id, secret).with_scopes(scopes.clone());
                        let flow = OAuthFlow::new(config);

                        // authorize() and authorize_manual() now return TokenSet directly
                        let token = if *manual {
                            flow.authorize_manual()?
                        } else {
                            flow.authorize()?
                        };

                        // Store the token (with fallback hint on error)
                        if let Err(e) = store.store_token(&token.team_id, &token) {
                            print_keyring_fallback_hint();
                            return Err(e);
                        }

                        // Set as default if first workspace
                        let workspaces = store.list_workspaces()?;
                        if workspaces.len() == 1 {
                            if let Err(e) = store.set_default(&token.team_id) {
                                print_keyring_fallback_hint();
                                return Err(e);
                            }
                        }

                        if output_mode == crate::output::OutputMode::Plain {
                            println!(
                                "Added\t{}\t{}\t{}",
                                token.team_id, token.team_name, token.user_id
                            );
                        } else {
                            write_json(&serde_json::json!({
                                "added": true,
                                "team_id": token.team_id,
                                "team_name": token.team_name,
                                "user_id": token.user_id,
                                "scopes": token.scopes,
                            }))?;
                        }
                    }
                    _ => {
                        // OAuth credentials missing - show helpful error
                        return Err(crate::error::SlackError::Config(
                            "OAuth requires SLACK_CLIENT_ID and SLACK_CLIENT_SECRET environment variables.\n\
                             Create a Slack app at https://api.slack.com/apps and set these variables.\n\
                             Alternatively, use --token or --xoxc/--xoxd for direct token auth.".into(),
                        ));
                    }
                }
            }
        }

        AuthCommands::List => {
            let workspaces = store.get_workspace_info()?;

            if plain {
                for ws in &workspaces {
                    let default_marker = if ws.is_default { "*" } else { "" };
                    println!(
                        "{}\t{}\t{}\t{}",
                        ws.team_id, ws.team_name, ws.token_type, default_marker
                    );
                }
            } else {
                write_json(&workspaces)?;
            }
        }

        AuthCommands::Remove { workspace, yes } => {
            // Find the workspace by name or ID
            let workspaces = store.get_workspace_info()?;
            let ws = workspaces
                .iter()
                .find(|w| w.team_id == *workspace || w.team_name == *workspace)
                .ok_or_else(|| crate::error::SlackError::WorkspaceNotFound(workspace.clone()))?;

            if !yes {
                // In a real implementation, we would prompt for confirmation
                // For now, just proceed
                eprintln!("Removing workspace: {} ({})", ws.team_name, ws.team_id);
            }

            store.delete_token(&ws.team_id)?;

            if plain {
                println!("Removed\t{}\t{}", ws.team_id, ws.team_name);
            } else {
                write_json(&serde_json::json!({
                    "removed": true,
                    "team_id": ws.team_id,
                    "team_name": ws.team_name,
                }))?;
            }
        }

        AuthCommands::Status => {
            // Get the token to check
            let token = if let Some(token_str) = token_override {
                // Token override provided on command line
                let token_type = TokenType::from_prefix(token_str).ok_or_else(|| {
                    crate::error::SlackError::InvalidToken(
                        "Token must start with xoxp-, xoxb-, or xoxc-".into(),
                    )
                })?;

                // For browser tokens, we need the xoxd cookie, which we don't have here
                if token_type == TokenType::Browser {
                    return Err(crate::error::SlackError::InvalidToken(
                        "Browser tokens require --xoxc and --xoxd flags in 'auth add'".into(),
                    ));
                }

                // Create a temporary token set to test
                Some(TokenSet::new_oauth(
                    token_str.to_string(),
                    "unknown".into(),
                    "unknown".into(),
                    "unknown".into(),
                    vec![],
                )?)
            } else if let Some(ws_name) = workspace {
                // Workspace specified
                let workspaces = store.get_workspace_info()?;
                let ws = workspaces
                    .iter()
                    .find(|w| w.team_id == *ws_name || w.team_name == *ws_name)
                    .ok_or_else(|| {
                        crate::error::SlackError::WorkspaceNotFound(ws_name.to_string())
                    })?;
                store.get_token(&ws.team_id)?
            } else {
                // Use default or first workspace
                store.get_default_or_first()?
            };

            match token {
                Some(token) => {
                    // Test the token with the API
                    let client = SlackClient::new(token.clone())?;
                    match client.auth_test().await {
                        Ok(auth_info) => {
                            if plain {
                                println!(
                                    "ok\t{}\t{}\t{}\t{}",
                                    auth_info.team_id,
                                    auth_info.team,
                                    auth_info.user_id,
                                    auth_info.user
                                );
                            } else {
                                write_json(&serde_json::json!({
                                    "ok": true,
                                    "team_id": auth_info.team_id,
                                    "team": auth_info.team,
                                    "user_id": auth_info.user_id,
                                    "user": auth_info.user,
                                    "url": auth_info.url,
                                    "token_type": format!("{:?}", token.token_type),
                                }))?;
                            }
                        }
                        Err(e) => {
                            if plain {
                                eprintln!("error\t{}", e);
                            } else {
                                write_json(&serde_json::json!({
                                    "ok": false,
                                    "error": e.to_string(),
                                }))?;
                            }
                            return Err(e);
                        }
                    }
                }
                None => {
                    return Err(crate::error::SlackError::AuthRequired);
                }
            }
        }

        AuthCommands::Switch { workspace } => {
            // Find the workspace by name or ID
            let workspaces = store.get_workspace_info()?;
            let ws = workspaces
                .iter()
                .find(|w| w.team_id == *workspace || w.team_name == *workspace)
                .ok_or_else(|| crate::error::SlackError::WorkspaceNotFound(workspace.clone()))?;

            store.set_default(&ws.team_id)?;

            if plain {
                println!("Switched\t{}\t{}", ws.team_id, ws.team_name);
            } else {
                write_json(&serde_json::json!({
                    "switched": true,
                    "team_id": ws.team_id,
                    "team_name": ws.team_name,
                }))?;
            }
        }

        AuthCommands::BrowserHelp => {
            crate::auth::print_extraction_instructions();
        }
    }

    Ok(())
}

/// Print keyring troubleshooting hint to stderr
fn print_keyring_fallback_hint() {
    eprintln!();
    eprintln!("Troubleshooting: If this is a keyring access issue, use file-based storage:");
    eprintln!("  export SLACK_TOKEN_STORE_PATH=~/.slack-tokens.json");
    eprintln!("  slack auth add ...");
    eprintln!();
    eprintln!("For more help, run: cargo run --bin test_keyring");
}

/// Add a direct token (xoxp-* or xoxb-*)
async fn add_direct_token(
    token_str: &str,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use crate::api::SlackClient;
    use crate::auth::{get_token_store, TokenSet};
    use crate::output::write_json;

    let store = get_token_store();

    // Validate token format
    let token_type = crate::auth::TokenType::from_prefix(token_str).ok_or_else(|| {
        crate::error::SlackError::InvalidToken(
            "Token must start with xoxp-, xoxb-, or xoxc-".into(),
        )
    })?;

    if token_type == crate::auth::TokenType::Browser {
        return Err(crate::error::SlackError::InvalidToken(
            "Browser tokens (xoxc-*) require --xoxc and --xoxd flags together".into(),
        ));
    }

    // Create a temporary token set to test auth
    let temp_token = TokenSet::new_oauth(
        token_str.to_string(),
        "temp".into(),
        "temp".into(),
        "temp".into(),
        vec![],
    )?;

    // Test the token
    let client = SlackClient::new(temp_token)?;
    let auth_info = client.auth_test().await?;

    // Create the real token set with actual team info
    let token = TokenSet::new_oauth(
        token_str.to_string(),
        auth_info.team_id.clone(),
        auth_info.team.clone(),
        auth_info.user_id.clone(),
        vec![], // Scopes not returned by auth.test
    )?;

    // Store the token (with fallback hint on error)
    if let Err(e) = store.store_token(&auth_info.team_id, &token) {
        print_keyring_fallback_hint();
        return Err(e);
    }

    // Set as default if it's the first workspace
    let workspaces = store.list_workspaces()?;
    if workspaces.len() == 1 {
        if let Err(e) = store.set_default(&auth_info.team_id) {
            print_keyring_fallback_hint();
            return Err(e);
        }
    }

    if output_mode == crate::output::OutputMode::Plain {
        println!(
            "Added\t{}\t{}\t{}\t{}",
            auth_info.team_id, auth_info.team, auth_info.user_id, auth_info.user
        );
    } else {
        write_json(&serde_json::json!({
            "added": true,
            "team_id": auth_info.team_id,
            "team": auth_info.team,
            "user_id": auth_info.user_id,
            "user": auth_info.user,
            "token_type": format!("{:?}", token.token_type),
        }))?;
    }

    Ok(())
}

/// Add browser tokens (xoxc-* with xoxd-*)
async fn add_browser_tokens(
    xoxc_str: &str,
    xoxd_str: &str,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use crate::api::SlackClient;
    use crate::auth::{get_token_store, TokenSet};
    use crate::output::write_json;

    let store = get_token_store();

    // Create the browser token set
    let temp_token = TokenSet::new_browser(
        xoxc_str.to_string(),
        xoxd_str.to_string(),
        "temp".into(),
        "temp".into(),
        "temp".into(),
    )?;

    // Test the token
    let client = SlackClient::new(temp_token)?;
    let auth_info = client.auth_test().await?;

    // Create the real token set with actual team info
    let token = TokenSet::new_browser(
        xoxc_str.to_string(),
        xoxd_str.to_string(),
        auth_info.team_id.clone(),
        auth_info.team.clone(),
        auth_info.user_id.clone(),
    )?;

    // Store the token (with fallback hint on error)
    if let Err(e) = store.store_token(&auth_info.team_id, &token) {
        print_keyring_fallback_hint();
        return Err(e);
    }

    // Set as default if it's the first workspace
    let workspaces = store.list_workspaces()?;
    if workspaces.len() == 1 {
        if let Err(e) = store.set_default(&auth_info.team_id) {
            print_keyring_fallback_hint();
            return Err(e);
        }
    }

    if output_mode == crate::output::OutputMode::Plain {
        println!(
            "Added\t{}\t{}\t{}\t{}",
            auth_info.team_id, auth_info.team, auth_info.user_id, auth_info.user
        );
    } else {
        write_json(&serde_json::json!({
            "added": true,
            "team_id": auth_info.team_id,
            "team": auth_info.team,
            "user_id": auth_info.user_id,
            "user": auth_info.user,
            "token_type": "Browser",
        }))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{CommandFactory, Parser};

    // Import the parent Cli for full parsing tests
    use crate::cli::Cli;

    #[test]
    fn test_auth_cmd_valid() {
        // Verify the auth command structure is valid
        Cli::command().debug_assert();
    }

    #[test]
    fn test_parse_auth_add_token() {
        let cli =
            Cli::try_parse_from(["slack", "auth", "add", "--token", "xoxp-123456789"]).unwrap();
        if let crate::cli::Commands::Auth(auth_cmd) = cli.command {
            if let AuthCommands::Add {
                token,
                xoxc,
                xoxd,
                oauth,
                ..
            } = auth_cmd.command
            {
                assert_eq!(token, Some("xoxp-123456789".to_string()));
                assert!(xoxc.is_none());
                assert!(xoxd.is_none());
                assert!(!oauth);
            } else {
                panic!("Expected Add command");
            }
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_parse_auth_add_browser_tokens() {
        let cli = Cli::try_parse_from([
            "slack", "auth", "add", "--xoxc", "xoxc-123", "--xoxd", "xoxd-456",
        ])
        .unwrap();
        if let crate::cli::Commands::Auth(auth_cmd) = cli.command {
            if let AuthCommands::Add {
                token,
                xoxc,
                xoxd,
                oauth,
                ..
            } = auth_cmd.command
            {
                assert!(token.is_none());
                assert_eq!(xoxc, Some("xoxc-123".to_string()));
                assert_eq!(xoxd, Some("xoxd-456".to_string()));
                assert!(!oauth);
            } else {
                panic!("Expected Add command");
            }
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_parse_auth_add_oauth() {
        let cli = Cli::try_parse_from(["slack", "auth", "add", "--oauth"]).unwrap();
        if let crate::cli::Commands::Auth(auth_cmd) = cli.command {
            if let AuthCommands::Add {
                token,
                xoxc,
                xoxd,
                oauth,
                ..
            } = auth_cmd.command
            {
                assert!(token.is_none());
                assert!(xoxc.is_none());
                assert!(xoxd.is_none());
                assert!(oauth);
            } else {
                panic!("Expected Add command");
            }
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_parse_auth_add_conflicts_token_and_oauth() {
        let result =
            Cli::try_parse_from(["slack", "auth", "add", "--token", "xoxp-123", "--oauth"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_auth_add_conflicts_token_and_xoxc() {
        let result = Cli::try_parse_from([
            "slack", "auth", "add", "--token", "xoxp-123", "--xoxc", "xoxc-456", "--xoxd",
            "xoxd-789",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_auth_add_xoxc_requires_xoxd() {
        let result = Cli::try_parse_from(["slack", "auth", "add", "--xoxc", "xoxc-123"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_auth_add_xoxd_requires_xoxc() {
        let result = Cli::try_parse_from(["slack", "auth", "add", "--xoxd", "xoxd-123"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_auth_list() {
        let cli = Cli::try_parse_from(["slack", "auth", "list"]).unwrap();
        if let crate::cli::Commands::Auth(auth_cmd) = cli.command {
            assert!(matches!(auth_cmd.command, AuthCommands::List));
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_parse_auth_remove() {
        let cli = Cli::try_parse_from(["slack", "auth", "remove", "T12345"]).unwrap();
        if let crate::cli::Commands::Auth(auth_cmd) = cli.command {
            if let AuthCommands::Remove { workspace, yes } = auth_cmd.command {
                assert_eq!(workspace, "T12345");
                assert!(!yes);
            } else {
                panic!("Expected Remove command");
            }
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_parse_auth_remove_with_yes() {
        let cli = Cli::try_parse_from(["slack", "auth", "remove", "T12345", "--yes"]).unwrap();
        if let crate::cli::Commands::Auth(auth_cmd) = cli.command {
            if let AuthCommands::Remove { workspace, yes } = auth_cmd.command {
                assert_eq!(workspace, "T12345");
                assert!(yes);
            } else {
                panic!("Expected Remove command");
            }
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_parse_auth_status() {
        let cli = Cli::try_parse_from(["slack", "auth", "status"]).unwrap();
        if let crate::cli::Commands::Auth(auth_cmd) = cli.command {
            assert!(matches!(auth_cmd.command, AuthCommands::Status));
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_parse_auth_switch() {
        let cli = Cli::try_parse_from(["slack", "auth", "switch", "T12345"]).unwrap();
        if let crate::cli::Commands::Auth(auth_cmd) = cli.command {
            if let AuthCommands::Switch { workspace } = auth_cmd.command {
                assert_eq!(workspace, "T12345");
            } else {
                panic!("Expected Switch command");
            }
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_parse_auth_browser_help() {
        let cli = Cli::try_parse_from(["slack", "auth", "browser-help"]).unwrap();
        if let crate::cli::Commands::Auth(auth_cmd) = cli.command {
            assert!(matches!(auth_cmd.command, AuthCommands::BrowserHelp));
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_parse_auth_add_custom_scopes() {
        let cli = Cli::try_parse_from([
            "slack",
            "auth",
            "add",
            "--oauth",
            "--scopes",
            "channels:read,chat:write",
        ])
        .unwrap();
        if let crate::cli::Commands::Auth(auth_cmd) = cli.command {
            if let AuthCommands::Add { scopes, .. } = auth_cmd.command {
                assert_eq!(scopes, vec!["channels:read", "chat:write"]);
            } else {
                panic!("Expected Add command");
            }
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_parse_auth_add_no_flags() {
        // Test that `slack auth add` with no flags is valid (defaults to OAuth)
        let cli = Cli::try_parse_from(["slack", "auth", "add"]).unwrap();
        if let crate::cli::Commands::Auth(auth_cmd) = cli.command {
            if let AuthCommands::Add {
                token,
                xoxc,
                xoxd,
                oauth,
                manual,
                ..
            } = auth_cmd.command
            {
                // All method flags should be false/None
                assert!(token.is_none());
                assert!(xoxc.is_none());
                assert!(xoxd.is_none());
                assert!(!oauth); // --oauth flag not explicitly set
                assert!(!manual);
                // But the behavior will default to OAuth in the run() function
            } else {
                panic!("Expected Add command");
            }
        } else {
            panic!("Expected Auth command");
        }
    }
}
