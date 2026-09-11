//! Nested `slack users groups` commands.

use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

use clap::{Args, Subcommand};
use serde::Serialize;

use crate::api::identity_ops::{UsergroupsListParams, UsergroupsUsersListParams};
use crate::api::{PaginationParams, SlackClient};
use crate::error::{Result, SlackError};
use crate::models::User;
use crate::output::{write_json, OutputMode};

/// User-group operations.
#[derive(Args, Debug)]
pub struct UsergroupsCmd {
    /// User-group command to run.
    #[command(subcommand)]
    pub command: UsergroupsCommands,
}

/// User-group subcommands.
#[derive(Subcommand, Debug)]
pub enum UsergroupsCommands {
    /// List enabled workspace user groups.
    List,

    /// List members of a user group.
    Members {
        /// User-group handle (optionally prefixed by @) or S-ID.
        usergroup: String,

        /// Resolve member IDs to user names.
        #[arg(long)]
        resolve: bool,
    },
}

#[derive(Serialize)]
struct ResolvedMember {
    id: String,
    user_name: String,
}

/// Run a nested user-group command with an authenticated client.
pub async fn run(cmd: &UsergroupsCmd, client: &SlackClient, output_mode: OutputMode) -> Result<()> {
    match &cmd.command {
        UsergroupsCommands::List => list(client, output_mode).await,
        UsergroupsCommands::Members { usergroup, resolve } => {
            members(client, usergroup, *resolve, output_mode).await
        }
    }
}

async fn list(client: &SlackClient, output_mode: OutputMode) -> Result<()> {
    let response = client
        .usergroups_list(UsergroupsListParams {
            include_disabled: false,
            include_count: true,
            include_users: false,
        })
        .await?;

    if output_mode == OutputMode::Plain {
        let stdout = io::stdout();
        let mut output = stdout.lock();
        for group in &response.usergroups {
            writeln!(
                output,
                "{}\t{}\t{}\t{}",
                escape_tsv(&group.id),
                escape_tsv(&group.handle),
                escape_tsv(&group.name),
                group.user_count
            )?;
        }
    } else {
        write_json(&serde_json::json!({ "usergroups": response.usergroups }))?;
    }

    Ok(())
}

async fn members(
    client: &SlackClient,
    identifier: &str,
    resolve: bool,
    output_mode: OutputMode,
) -> Result<()> {
    let usergroup = resolve_usergroup(client, identifier).await?;
    let response = client
        .usergroups_users_list(UsergroupsUsersListParams {
            usergroup: &usergroup,
            include_disabled: false,
        })
        .await?;

    if resolve {
        let users = users_for_resolution(client).await?;
        let names: HashMap<String, String> = users
            .into_iter()
            .map(|user| {
                let display_name = user.display_name();
                let name = user
                    .name
                    .as_deref()
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
                    .or_else(|| (!display_name.is_empty()).then_some(display_name))
                    .unwrap_or_else(|| user.id.clone());
                (user.id, name)
            })
            .collect();
        let members: Vec<ResolvedMember> = response
            .users
            .into_iter()
            .map(|id| ResolvedMember {
                user_name: names.get(&id).cloned().unwrap_or_else(|| id.clone()),
                id,
            })
            .collect();

        if output_mode == OutputMode::Plain {
            let stdout = io::stdout();
            let mut output = stdout.lock();
            for member in &members {
                writeln!(
                    output,
                    "{}\t{}",
                    escape_tsv(&member.id),
                    escape_tsv(&member.user_name)
                )?;
            }
        } else {
            write_json(&serde_json::json!({
                "usergroup": usergroup,
                "members": members,
            }))?;
        }
    } else if output_mode == OutputMode::Plain {
        let stdout = io::stdout();
        let mut output = stdout.lock();
        for id in &response.users {
            writeln!(output, "{}", escape_tsv(id))?;
        }
    } else {
        write_json(&serde_json::json!({
            "usergroup": usergroup,
            "members": response.users,
        }))?;
    }

    Ok(())
}

async fn users_for_resolution(client: &SlackClient) -> Result<Vec<User>> {
    let mut users = Vec::new();
    let mut cursor: Option<String> = None;
    let mut seen_cursors = HashSet::new();

    loop {
        let mut params = PaginationParams::new().with_limit(200);
        if let Some(value) = cursor {
            params = params.with_cursor(value);
        }

        let response = client.users_list(params).await?;
        users.extend(response.members);
        cursor = response
            .response_metadata
            .and_then(|metadata| metadata.next_cursor)
            .filter(|value| !value.is_empty());

        match cursor.as_ref() {
            Some(value) if !seen_cursors.insert(value.clone()) => {
                return Err(SlackError::Api {
                    error: "repeated_cursor".to_string(),
                    detail: Some("users.list returned a repeated pagination cursor".to_string()),
                });
            }
            Some(_) => {}
            None => break,
        }
    }

    Ok(users)
}

async fn resolve_usergroup(client: &SlackClient, identifier: &str) -> Result<String> {
    if identifier.len() >= 9 && identifier.starts_with('S') {
        return Ok(identifier.to_string());
    }

    let handle = identifier.strip_prefix('@').unwrap_or(identifier);
    let response = client
        .usergroups_list(UsergroupsListParams {
            include_disabled: false,
            include_count: true,
            include_users: false,
        })
        .await?;

    response
        .usergroups
        .into_iter()
        .find(|group| group.handle == handle)
        .map(|group| group.id)
        .ok_or_else(|| SlackError::Api {
            error: "usergroup_not_found".to_string(),
            detail: Some(format!("No user group has handle {}", identifier)),
        })
}

fn escape_tsv(value: &str) -> String {
    value
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::cli::{Cli, Commands};

    #[test]
    fn parse_groups_list() {
        let cli = Cli::try_parse_from(["slack", "users", "groups", "list"]).unwrap();
        match cli.command {
            Commands::Users(users) => {
                assert!(matches!(
                    users.command,
                    crate::cli::users::UsersCommands::Groups(UsergroupsCmd {
                        command: UsergroupsCommands::List
                    })
                ));
            }
            _ => panic!("expected users command"),
        }
    }

    #[test]
    fn parse_groups_members_resolve() {
        let cli = Cli::try_parse_from([
            "slack",
            "users",
            "groups",
            "members",
            "@engineering",
            "--resolve",
        ])
        .unwrap();
        match cli.command {
            Commands::Users(users) => match users.command {
                crate::cli::users::UsersCommands::Groups(UsergroupsCmd {
                    command: UsergroupsCommands::Members { usergroup, resolve },
                }) => {
                    assert_eq!(usergroup, "@engineering");
                    assert!(resolve);
                }
                _ => panic!("expected groups members command"),
            },
            _ => panic!("expected users command"),
        }
    }

    #[test]
    fn parse_groups_members_defaults_to_ids() {
        let cli =
            Cli::try_parse_from(["slack", "users", "groups", "members", "S12345678"]).unwrap();
        match cli.command {
            Commands::Users(users) => match users.command {
                crate::cli::users::UsersCommands::Groups(UsergroupsCmd {
                    command: UsergroupsCommands::Members { resolve, .. },
                }) => assert!(!resolve),
                _ => panic!("expected groups members command"),
            },
            _ => panic!("expected users command"),
        }
    }
}
