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
    ///
    /// With a workspace argument (e.g. `myteam` or `myteam.slack.com`) and no
    /// other method flag, credentials are extracted from a locally logged-in
    /// Slack (the desktop app or a browser). Use `slack auth discover` to see
    /// what is available.
    Add {
        /// Workspace subdomain or URL to import from local apps (e.g. myteam or myteam.slack.com)
        #[arg(conflicts_with_all = ["oauth", "token", "xoxc", "xoxd", "manual"])]
        workspace: Option<String>,

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

        /// Auto-import creds from a locally logged-in Slack (browser / desktop app)
        #[arg(long, conflicts_with_all = ["oauth", "token", "xoxc", "xoxd", "manual"])]
        from_browser: bool,

        /// Restrict local import to a workspace URL (alias of the positional argument)
        #[arg(long)]
        url: Option<String>,

        /// Restrict local import to a specific browser/app (e.g. slack, chrome, brave)
        #[arg(long)]
        browser: Option<String>,

        /// OAuth scopes to request
        #[arg(
            long,
            value_delimiter = ',',
            default_value = "channels:read,channels:history,users:read,search:read"
        )]
        scopes: Vec<String>,
    },

    /// List authorized workspaces
    List {
        /// Validate each stored token via auth.test and report live/expired status
        #[arg(long)]
        check: bool,
    },

    /// Discover Slack workspaces signed into local apps (desktop app / browsers)
    ///
    /// Reads only local storage; performs no Keychain access or network calls
    /// unless `--check` is given.
    Discover {
        /// Restrict discovery to a specific browser/app (e.g. slack, chrome, brave)
        #[arg(long)]
        browser: Option<String>,

        /// Validate each discovered workspace by importing-and-testing its token
        #[arg(long)]
        check: bool,
    },

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
            workspace: ws_arg,
            oauth: _, // Default behavior is OAuth, so this flag is now only used for documentation
            xoxc,
            xoxd,
            token,
            manual,
            from_browser,
            url,
            browser,
            scopes,
        } => {
            // A positional workspace argument (or --url) selects the local
            // extraction path; the positional form takes precedence.
            let local_target = ws_arg.clone().or_else(|| url.clone());

            // Determine which auth method to use
            if *from_browser || local_target.is_some() {
                // Auto-import from a locally logged-in Slack (browser / desktop app)
                add_from_browser(local_target, browser.clone(), output_mode).await?;
            } else if let Some(token_str) = token {
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

        AuthCommands::List { check } => {
            let workspaces = store.get_workspace_info()?;

            // Optionally validate each stored token via auth.test.
            let mut live: std::collections::HashMap<String, bool> =
                std::collections::HashMap::new();
            if *check {
                for ws in &workspaces {
                    let ok = match store.get_token(&ws.team_id) {
                        Ok(Some(token)) => match SlackClient::new(token) {
                            Ok(client) => client.auth_test().await.is_ok(),
                            Err(_) => false,
                        },
                        _ => false,
                    };
                    live.insert(ws.team_id.clone(), ok);
                }
            }

            if plain {
                // Columns: team_id, domain, name, token_type, default-marker.
                // team_id and domain are the stable selectors accepted by -w.
                for ws in &workspaces {
                    let default_marker = if ws.is_default { "*" } else { "" };
                    let domain = ws.team_domain.as_deref().unwrap_or("");
                    if *check {
                        let status = if live.get(&ws.team_id).copied().unwrap_or(false) {
                            "live"
                        } else {
                            "expired"
                        };
                        println!(
                            "{}\t{}\t{}\t{}\t{}\t{}",
                            ws.team_id, domain, ws.team_name, ws.token_type, default_marker, status
                        );
                    } else {
                        println!(
                            "{}\t{}\t{}\t{}\t{}",
                            ws.team_id, domain, ws.team_name, ws.token_type, default_marker
                        );
                    }
                }
            } else if *check {
                let enriched: Vec<serde_json::Value> = workspaces
                    .iter()
                    .map(|ws| {
                        serde_json::json!({
                            "team_id": ws.team_id,
                            "team_domain": ws.team_domain,
                            "team_name": ws.team_name,
                            "token_type": ws.token_type,
                            "is_default": ws.is_default,
                            "live": live.get(&ws.team_id).copied().unwrap_or(false),
                        })
                    })
                    .collect();
                write_json(&enriched)?;
            } else {
                write_json(&workspaces)?;
            }
        }

        AuthCommands::Discover { browser, check } => {
            run_discover(browser.clone(), *check, output_mode).await?;
        }

        AuthCommands::Remove { workspace, yes } => {
            // Find the workspace by name or ID
            let workspaces = store.get_workspace_info()?;
            let ws = workspaces
                .iter()
                .find(|w| {
                    crate::auth::workspace_matches(workspace, &w.team_id, w.team_domain.as_deref())
                })
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
                    .find(|w| {
                        crate::auth::workspace_matches(
                            ws_name,
                            &w.team_id,
                            w.team_domain.as_deref(),
                        )
                    })
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
                .find(|w| {
                    crate::auth::workspace_matches(workspace, &w.team_id, w.team_domain.as_deref())
                })
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
    )?
    .with_domain(&auth_info.url);

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

/// Validate a pair of browser tokens against the API and persist them.
///
/// This is the shared "auth_test -> store" path used both by the manual
/// `--xoxc/--xoxd` flow and the `--from-browser` importer. It performs no
/// output of its own so callers can format results however they like.
///
/// Returns the validated `AuthInfo` (team/user identity) on success.
async fn store_browser_tokens(
    xoxc_str: &str,
    xoxd_str: &str,
) -> crate::error::Result<crate::api::AuthTestResponse> {
    use crate::api::SlackClient;
    use crate::auth::{get_token_store, TokenSet};

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
    )?
    .with_domain(&auth_info.url);

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

    Ok(auth_info)
}

/// Add browser tokens (xoxc-* with xoxd-*)
async fn add_browser_tokens(
    xoxc_str: &str,
    xoxd_str: &str,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use crate::output::write_json;

    let auth_info = store_browser_tokens(xoxc_str, xoxd_str).await?;

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

/// List Slack workspaces signed into local apps (desktop app / browsers).
///
/// Backs `slack auth discover`. By default this only reads local storage — no
/// Keychain access, no cookie decryption, no network. With `check = true`, each
/// discovered workspace is validated by extracting and `auth_test`-ing its
/// token (which does touch the Keychain and network).
async fn run_discover(
    browser: Option<String>,
    check: bool,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use crate::auth::{discover_workspaces, extract_workspaces, ExtractOptions};
    use crate::output::write_json;

    let discovered = discover_workspaces(browser.as_deref());

    // When --check is requested, extract full credentials once and auth_test
    // each, keyed by team domain/id so we can annotate the discovery list with
    // both liveness and the *authoritative* team name from auth.test (which
    // supersedes the best-effort name recovered from local storage).
    let mut live: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
    let mut auth_name: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if check {
        let extracted = extract_workspaces(&ExtractOptions {
            url: None,
            browser: browser.clone(),
        })
        .unwrap_or_default();
        for ws in &extracted {
            let result = store_browser_tokens(&ws.tokens.xoxc, &ws.tokens.xoxd).await;
            let keys: Vec<String> = ws
                .team_id
                .iter()
                .chain(ws.team_domain.iter())
                .cloned()
                .collect();
            match result {
                Ok(info) => {
                    for key in &keys {
                        live.insert(key.clone(), true);
                        auth_name.insert(key.clone(), info.team.clone());
                    }
                }
                Err(_) => {
                    for key in &keys {
                        live.entry(key.clone()).or_insert(false);
                    }
                }
            }
        }
    }

    let live_for = |team_id: &Option<String>, domain: &Option<String>| -> Option<bool> {
        if !check {
            return None;
        }
        team_id
            .as_ref()
            .and_then(|id| live.get(id).copied())
            .or_else(|| domain.as_ref().and_then(|d| live.get(d).copied()))
            .or(Some(false))
    };

    // Prefer the authoritative auth.test name (when checked), else the
    // locally-recovered name.
    let name_for = |team_id: &Option<String>,
                    domain: &Option<String>,
                    local: &Option<String>|
     -> Option<String> {
        team_id
            .as_ref()
            .and_then(|id| auth_name.get(id).cloned())
            .or_else(|| domain.as_ref().and_then(|d| auth_name.get(d).cloned()))
            .or_else(|| local.clone())
    };

    if output_mode == crate::output::OutputMode::Plain {
        for ws in &discovered {
            let name = name_for(&ws.team_id, &ws.team_domain, &ws.team_name);
            let base = format!(
                "{}\t{}\t{}\t{}",
                ws.team_domain.as_deref().unwrap_or(""),
                ws.team_id.as_deref().unwrap_or(""),
                name.as_deref().unwrap_or(""),
                ws.source,
            );
            match live_for(&ws.team_id, &ws.team_domain) {
                Some(ok) => println!("{base}\t{}", if ok { "live" } else { "expired" }),
                None => println!("{base}"),
            }
        }
    } else {
        let rows: Vec<serde_json::Value> = discovered
            .iter()
            .map(|ws| {
                let name = name_for(&ws.team_id, &ws.team_domain, &ws.team_name);
                let mut obj = serde_json::json!({
                    "team_id": ws.team_id,
                    "team_domain": ws.team_domain,
                    "team_name": name,
                    "source": ws.source,
                });
                if let Some(ok) = live_for(&ws.team_id, &ws.team_domain) {
                    obj["live"] = serde_json::Value::Bool(ok);
                }
                obj
            })
            .collect();
        write_json(&serde_json::json!({
            "workspaces": rows,
            "count": discovered.len(),
        }))?;
    }

    Ok(())
}

/// Auto-import creds from a locally logged-in Slack (browser / desktop app).
///
/// Discovers workspaces via [`crate::auth::extract::extract_workspaces`], then
/// reuses the shared `auth_test -> store` path ([`store_browser_tokens`]) for
/// each. Every workspace is attempted independently so one bad/expired session
/// does not abort the import; results are summarized as JSON (or TSV in
/// `--plain` mode).
async fn add_from_browser(
    url: Option<String>,
    browser: Option<String>,
    output_mode: crate::output::OutputMode,
) -> crate::error::Result<()> {
    use crate::auth::{extract_workspaces, ExtractOptions};
    use crate::output::write_json;

    let opts = ExtractOptions { url, browser };
    let requested_url = opts.url.clone();
    let workspaces = extract_workspaces(&opts)?;

    if workspaces.is_empty() {
        let msg = match requested_url {
            Some(u) => format!(
                "No locally logged-in Slack workspace matching '{u}' was found. It may not have an \
                 active session stored locally - open it in a supported browser and sign in, or run \
                 without --url to see all detected workspaces."
            ),
            None => "No locally logged-in Slack workspaces were found. Make sure you are signed in to \
                 Slack in a supported browser or the Slack desktop app."
                .to_string(),
        };
        return Err(crate::error::SlackError::Other(msg));
    }

    // Accumulate outcomes so a single failing workspace doesn't abort the rest.
    let mut added: Vec<serde_json::Value> = Vec::new();
    let mut errored: Vec<serde_json::Value> = Vec::new();

    for ws in &workspaces {
        match store_browser_tokens(&ws.tokens.xoxc, &ws.tokens.xoxd).await {
            Ok(auth_info) => added.push(serde_json::json!({
                "team_id": auth_info.team_id,
                "team": auth_info.team,
                "user_id": auth_info.user_id,
                "user": auth_info.user,
                "source": ws.source,
            })),
            Err(e) => errored.push(serde_json::json!({
                "team_id": ws.team_id,
                "team": ws.team_name,
                "domain": ws.team_domain,
                "source": ws.source,
                "error": e.to_string(),
            })),
        }
    }

    if output_mode == crate::output::OutputMode::Plain {
        for a in &added {
            println!(
                "Added\t{}\t{}\t{}\t{}",
                a["team_id"].as_str().unwrap_or(""),
                a["team"].as_str().unwrap_or(""),
                a["user_id"].as_str().unwrap_or(""),
                a["user"].as_str().unwrap_or("")
            );
        }
        for e in &errored {
            eprintln!(
                "Error\t{}\t{}\t{}",
                e["team"].as_str().unwrap_or(""),
                e["source"].as_str().unwrap_or(""),
                e["error"].as_str().unwrap_or("")
            );
        }
    } else {
        write_json(&serde_json::json!({
            "added": added,
            "errored": errored,
            "added_count": added.len(),
            "errored_count": errored.len(),
        }))?;
    }

    // If nothing was added but we had candidates, surface a non-zero exit.
    if added.is_empty() {
        return Err(crate::error::SlackError::Other(
            "Found local Slack sessions but none could be validated (all tokens failed auth_test)."
                .into(),
        ));
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
            assert!(matches!(auth_cmd.command, AuthCommands::List { .. }));
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
    fn test_parse_auth_add_positional_workspace() {
        let cli = Cli::try_parse_from(["slack", "auth", "add", "onlinegeniuses"]).unwrap();
        if let crate::cli::Commands::Auth(auth_cmd) = cli.command {
            if let AuthCommands::Add { workspace, .. } = auth_cmd.command {
                assert_eq!(workspace.as_deref(), Some("onlinegeniuses"));
            } else {
                panic!("Expected Add command");
            }
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_parse_auth_add_positional_conflicts_with_token() {
        // A positional workspace and an explicit --token are mutually exclusive.
        let result = Cli::try_parse_from(["slack", "auth", "add", "myteam", "--token", "xoxp-123"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_auth_list_check() {
        let cli = Cli::try_parse_from(["slack", "auth", "list", "--check"]).unwrap();
        if let crate::cli::Commands::Auth(auth_cmd) = cli.command {
            if let AuthCommands::List { check } = auth_cmd.command {
                assert!(check);
            } else {
                panic!("Expected List command");
            }
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_parse_auth_discover() {
        let cli = Cli::try_parse_from(["slack", "auth", "discover", "--browser", "slack"]).unwrap();
        if let crate::cli::Commands::Auth(auth_cmd) = cli.command {
            if let AuthCommands::Discover { browser, check } = auth_cmd.command {
                assert_eq!(browser.as_deref(), Some("slack"));
                assert!(!check);
            } else {
                panic!("Expected Discover command");
            }
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
                from_browser,
                ..
            } = auth_cmd.command
            {
                // All method flags should be false/None
                assert!(token.is_none());
                assert!(xoxc.is_none());
                assert!(xoxd.is_none());
                assert!(!oauth); // --oauth flag not explicitly set
                assert!(!manual);
                assert!(!from_browser);
                // But the behavior will default to OAuth in the run() function
            } else {
                panic!("Expected Add command");
            }
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_parse_auth_add_from_browser() {
        let cli = Cli::try_parse_from(["slack", "auth", "add", "--from-browser"]).unwrap();
        if let crate::cli::Commands::Auth(auth_cmd) = cli.command {
            if let AuthCommands::Add {
                from_browser,
                url,
                browser,
                ..
            } = auth_cmd.command
            {
                assert!(from_browser);
                assert!(url.is_none());
                assert!(browser.is_none());
            } else {
                panic!("Expected Add command");
            }
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_parse_auth_add_from_browser_with_url_and_browser() {
        let cli = Cli::try_parse_from([
            "slack",
            "auth",
            "add",
            "--from-browser",
            "--url",
            "myteam.slack.com",
            "--browser",
            "chrome",
        ])
        .unwrap();
        if let crate::cli::Commands::Auth(auth_cmd) = cli.command {
            if let AuthCommands::Add {
                from_browser,
                url,
                browser,
                ..
            } = auth_cmd.command
            {
                assert!(from_browser);
                assert_eq!(url, Some("myteam.slack.com".to_string()));
                assert_eq!(browser, Some("chrome".to_string()));
            } else {
                panic!("Expected Add command");
            }
        } else {
            panic!("Expected Auth command");
        }
    }

    #[test]
    fn test_parse_auth_add_from_browser_conflicts_with_token() {
        let result = Cli::try_parse_from([
            "slack",
            "auth",
            "add",
            "--from-browser",
            "--token",
            "xoxp-123",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_auth_add_from_browser_conflicts_with_xoxc() {
        let result = Cli::try_parse_from([
            "slack",
            "auth",
            "add",
            "--from-browser",
            "--xoxc",
            "xoxc-1",
            "--xoxd",
            "xoxd-2",
        ]);
        assert!(result.is_err());
    }
}
