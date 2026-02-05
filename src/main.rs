//! Slack CLI - Command-line interface for Slack workspaces
//!
//! A command-line tool optimized for AI agents and scripts.

use clap::Parser;
use tracing_subscriber::EnvFilter;

use slack_cli::cli::{
    auth, channels, files, generate_completions, messages, reactions, reminders, status, users,
    Cli, Commands,
};
use slack_cli::error::SlackError;
use slack_cli::output::OutputMode;

#[tokio::main]
async fn main() {
    let exit_code = run().await;
    std::process::exit(exit_code);
}

async fn run() -> i32 {
    // Parse CLI arguments
    let cli = Cli::parse();

    // Initialize tracing based on verbose flag
    init_tracing(cli.verbose);

    // Determine output mode
    let output_mode = if cli.plain {
        OutputMode::Plain
    } else {
        OutputMode::from_env()
    };

    // Run the command
    let result = run_command(&cli).await;

    // Handle errors
    match result {
        Ok(()) => 0,
        Err(e) => {
            output_mode.write_error(&e);
            e.exit_code()
        }
    }
}

/// Initialize tracing subscriber
fn init_tracing(verbose: bool) {
    let filter = if verbose {
        EnvFilter::new("slack_cli=debug")
    } else {
        EnvFilter::new("slack_cli=warn")
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}

/// Route command to appropriate handler
async fn run_command(cli: &Cli) -> Result<(), SlackError> {
    match &cli.command {
        Commands::Auth(cmd) => {
            auth::run(
                cmd,
                cli.plain,
                cli.workspace.as_deref(),
                cli.token.as_deref(),
            )
            .await
        }
        Commands::Channels(cmd) => {
            channels::run(
                cmd,
                cli.plain,
                cli.workspace.as_deref(),
                cli.token.as_deref(),
            )
            .await
        }
        Commands::Messages(cmd) => {
            messages::run(
                cmd,
                cli.plain,
                cli.workspace.as_deref(),
                cli.token.as_deref(),
            )
            .await
        }
        Commands::Users(cmd) => {
            users::run(
                cmd,
                cli.plain,
                cli.workspace.as_deref(),
                cli.token.as_deref(),
            )
            .await
        }
        Commands::Files(cmd) => {
            files::run(
                cmd,
                cli.plain,
                cli.workspace.as_deref(),
                cli.token.as_deref(),
            )
            .await
        }
        Commands::Reactions(cmd) => {
            reactions::run(
                cmd,
                cli.plain,
                cli.workspace.as_deref(),
                cli.token.as_deref(),
            )
            .await
        }
        Commands::Status(cmd) => {
            status::run(
                cmd,
                cli.plain,
                cli.workspace.as_deref(),
                cli.token.as_deref(),
            )
            .await
        }
        Commands::Reminders(cmd) => {
            reminders::run(
                cmd,
                cli.plain,
                cli.workspace.as_deref(),
                cli.token.as_deref(),
            )
            .await
        }
        Commands::Completions(args) => {
            generate_completions(args.shell);
            Ok(())
        }
    }
}
