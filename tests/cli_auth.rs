//! Unit tests for auth CLI parsing
//!
//! Tests for auth subcommand parsing, flag conflicts, and requirements.

use clap::Parser;
use slack_cli::cli::auth::AuthCommands;
use slack_cli::cli::{Cli, Commands};

// ============================================================================
// Auth Add Command Tests
// ============================================================================

#[test]
fn test_parse_auth_add_token() {
    let cli = Cli::try_parse_from(["slack", "auth", "add", "--token", "xoxp-123456789"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        if let AuthCommands::Add {
            token,
            xoxc,
            xoxd,
            oauth,
            manual,
            scopes,
        } = auth_cmd.command
        {
            assert_eq!(token, Some("xoxp-123456789".to_string()));
            assert!(xoxc.is_none());
            assert!(xoxd.is_none());
            assert!(!oauth);
            assert!(!manual);
            // Check default scopes
            assert!(scopes.contains(&"channels:read".to_string()));
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
    if let Commands::Auth(auth_cmd) = cli.command {
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
    if let Commands::Auth(auth_cmd) = cli.command {
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
fn test_parse_auth_add_manual() {
    let cli = Cli::try_parse_from(["slack", "auth", "add", "--manual"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        if let AuthCommands::Add { manual, .. } = auth_cmd.command {
            assert!(manual);
        } else {
            panic!("Expected Add command");
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
        "channels:read,chat:write,users:read",
    ])
    .unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        if let AuthCommands::Add { scopes, .. } = auth_cmd.command {
            assert_eq!(scopes, vec!["channels:read", "chat:write", "users:read"]);
        } else {
            panic!("Expected Add command");
        }
    } else {
        panic!("Expected Auth command");
    }
}

// ============================================================================
// Auth Add Flag Conflict Tests
// ============================================================================

#[test]
fn test_auth_add_conflicts_token_and_oauth() {
    let result = Cli::try_parse_from(["slack", "auth", "add", "--token", "xoxp-123", "--oauth"]);
    assert!(result.is_err());
}

#[test]
fn test_auth_add_conflicts_token_and_xoxc() {
    let result = Cli::try_parse_from([
        "slack", "auth", "add", "--token", "xoxp-123", "--xoxc", "xoxc-456", "--xoxd", "xoxd-789",
    ]);
    assert!(result.is_err());
}

#[test]
fn test_auth_add_conflicts_oauth_and_xoxc() {
    let result = Cli::try_parse_from([
        "slack", "auth", "add", "--oauth", "--xoxc", "xoxc-123", "--xoxd", "xoxd-456",
    ]);
    assert!(result.is_err());
}

// ============================================================================
// Auth Add Flag Requirement Tests
// ============================================================================

#[test]
fn test_auth_add_xoxc_requires_xoxd() {
    let result = Cli::try_parse_from(["slack", "auth", "add", "--xoxc", "xoxc-123"]);
    assert!(result.is_err());
}

#[test]
fn test_auth_add_xoxd_requires_xoxc() {
    let result = Cli::try_parse_from(["slack", "auth", "add", "--xoxd", "xoxd-123"]);
    assert!(result.is_err());
}

// ============================================================================
// Auth List Command Tests
// ============================================================================

#[test]
fn test_parse_auth_list() {
    let cli = Cli::try_parse_from(["slack", "auth", "list"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        assert!(matches!(auth_cmd.command, AuthCommands::List));
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_parse_auth_list_with_plain() {
    let cli = Cli::try_parse_from(["slack", "auth", "list", "--plain"]).unwrap();
    assert!(cli.plain);
    if let Commands::Auth(auth_cmd) = cli.command {
        assert!(matches!(auth_cmd.command, AuthCommands::List));
    } else {
        panic!("Expected Auth command");
    }
}

// ============================================================================
// Auth Remove Command Tests
// ============================================================================

#[test]
fn test_parse_auth_remove() {
    let cli = Cli::try_parse_from(["slack", "auth", "remove", "T12345"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
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
    if let Commands::Auth(auth_cmd) = cli.command {
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
fn test_parse_auth_remove_with_y_short() {
    let cli = Cli::try_parse_from(["slack", "auth", "remove", "T12345", "-y"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        if let AuthCommands::Remove { yes, .. } = auth_cmd.command {
            assert!(yes);
        } else {
            panic!("Expected Remove command");
        }
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_parse_auth_remove_by_name() {
    let cli = Cli::try_parse_from(["slack", "auth", "remove", "My Workspace"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        if let AuthCommands::Remove { workspace, .. } = auth_cmd.command {
            assert_eq!(workspace, "My Workspace");
        } else {
            panic!("Expected Remove command");
        }
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_auth_remove_requires_workspace() {
    let result = Cli::try_parse_from(["slack", "auth", "remove"]);
    assert!(result.is_err());
}

// ============================================================================
// Auth Status Command Tests
// ============================================================================

#[test]
fn test_parse_auth_status() {
    let cli = Cli::try_parse_from(["slack", "auth", "status"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        assert!(matches!(auth_cmd.command, AuthCommands::Status));
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_parse_auth_status_with_workspace() {
    let cli = Cli::try_parse_from(["slack", "-w", "T12345", "auth", "status"]).unwrap();
    assert_eq!(cli.workspace, Some("T12345".to_string()));
    if let Commands::Auth(auth_cmd) = cli.command {
        assert!(matches!(auth_cmd.command, AuthCommands::Status));
    } else {
        panic!("Expected Auth command");
    }
}

// ============================================================================
// Auth Switch Command Tests
// ============================================================================

#[test]
fn test_parse_auth_switch() {
    let cli = Cli::try_parse_from(["slack", "auth", "switch", "T12345"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
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
fn test_parse_auth_switch_by_name() {
    let cli = Cli::try_parse_from(["slack", "auth", "switch", "My Workspace"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        if let AuthCommands::Switch { workspace } = auth_cmd.command {
            assert_eq!(workspace, "My Workspace");
        } else {
            panic!("Expected Switch command");
        }
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_auth_switch_requires_workspace() {
    let result = Cli::try_parse_from(["slack", "auth", "switch"]);
    assert!(result.is_err());
}

// ============================================================================
// Auth Help Command Tests
// ============================================================================

#[test]
fn test_parse_auth_browser_help() {
    let cli = Cli::try_parse_from(["slack", "auth", "browser-help"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        assert!(matches!(auth_cmd.command, AuthCommands::BrowserHelp));
    } else {
        panic!("Expected Auth command");
    }
}

// ============================================================================
// Auth Alias Tests
// ============================================================================

#[test]
fn test_auth_alias_list() {
    let cli = Cli::try_parse_from(["slack", "a", "list"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        assert!(matches!(auth_cmd.command, AuthCommands::List));
    } else {
        panic!("Expected Auth command");
    }
}

#[test]
fn test_auth_alias_status() {
    let cli = Cli::try_parse_from(["slack", "a", "status"]).unwrap();
    if let Commands::Auth(auth_cmd) = cli.command {
        assert!(matches!(auth_cmd.command, AuthCommands::Status));
    } else {
        panic!("Expected Auth command");
    }
}
