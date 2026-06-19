# Slack CLI

A comprehensive Rust CLI tool for Slack, designed for AI agents and automation.

## Features

- **Multiple authentication methods**: OAuth, browser tokens (xoxc+xoxd), direct tokens (xoxp/xoxb)
- **Full workspace access**: Channels, messages, threads, search, files, reactions, reminders, status
- **Agent-first design**: JSON output by default, optimized for AI consumption
- **Minimal footprint**: No config files, tokens stored in system keyring
- **Fast and reliable**: Built with Rust for performance and safety
- **Shell completions**: Bash, Zsh, Fish, PowerShell

## Installation

### Quick Install

```bash
curl -fsSL https://raw.githubusercontent.com/TeamCadenceAI/slack-cli/main/install.sh | sh
```

Installs to `~/.slack/bin/slack` and symlinks into `~/.local/bin/slack`. On macOS the quarantine attribute is removed automatically so Gatekeeper won't block the binary.

### From Source

```bash
git clone https://github.com/TeamCadenceAI/slack-cli
cd slack-cli
cargo build --release
# binary at target/release/slack
```

Or via `cargo install`:

```bash
cargo install --git https://github.com/TeamCadenceAI/slack-cli
```

### Shell Completions

```bash
# Bash
slack completions bash > ~/.local/share/bash-completion/completions/slack

# Zsh
slack completions zsh > ~/.zsh/completions/_slack

# Fish
slack completions fish > ~/.config/fish/completions/slack.fish

# PowerShell
slack completions powershell >> $PROFILE
```

## Quick Start

```bash
# Authenticate with a token
slack auth add --token xoxp-your-token-here

# Check auth status
slack auth status

# List channels
slack channels list

# Send a message
slack messages send "#general" "Hello from CLI!"

# Search messages
slack messages search "important updates"
```

## Commands

### Authentication (`slack auth`)

```bash
# Add a user or bot token
slack auth add --token xoxp-...
slack auth add --token xoxb-...

# Add browser token (xoxc + xoxd cookie)
slack auth add --xoxc xoxc-... --xoxd xoxd-...

# List configured workspaces
slack auth list

# Show current auth status
slack auth status

# Switch default workspace
slack auth switch T1234567890

# Remove a workspace
slack auth remove T1234567890

# Get help extracting browser tokens
slack auth browser-help
```

### Channels (`slack channels` or `slack c`)

```bash
# List channels (public and private by default)
slack channels list

# List with specific types
slack channels list --types public_channel,private_channel,mpim,im

# Sort by popularity (member count)
slack channels list --sort-popularity

# Exclude archived channels
slack channels list --exclude-archived

# Get channel info by ID or name
slack channels info C123456789
slack channels info #general

# List direct messages
slack channels dms

# Export channels to CSV
slack channels export --output channels.csv
```

### Messages (`slack messages` or `slack m`)

```bash
# List messages in a channel (default: last 50)
slack messages list "#general"
slack messages list C123456789 --limit 100

# List messages from last 7 days
slack messages list "#general" --limit 7d

# Send a message
slack messages send "#general" "Hello, world!"

# Reply to a thread
slack messages send "#general" "Reply text" --thread-ts 1234567890.123456

# Read message from stdin
echo "Message from pipe" | slack messages send "#general" --stdin

# View thread replies
slack messages thread "#general" 1234567890.123456

# Search messages
slack messages search "important updates"
slack messages search "from:@username budget"
slack messages search "in:#general project" --count 50

# Get a specific message
slack messages get "C123456789:1234567890.123456"
```

### Users (`slack users` or `slack u`)

```bash
# List all users
slack users list

# List only active users
slack users list --active-only

# Get current user info
slack users me

# Get user info by ID or name
slack users info U123456789
slack users info @username

# Export users to CSV
slack users export --output users.csv
```

### Files (`slack files` or `slack f`)

```bash
# List recent files
slack files list

# List files in a channel
slack files list --channel "#general"

# List files by type
slack files list --types images,documents

# Get file info
slack files info F123456789

# Download a file
slack files download F123456789 --output ./downloads/
```

### Reactions (`slack reactions` or `slack r`)

```bash
# Add a reaction
slack reactions add C123456789 1234567890.123456 thumbsup

# Remove a reaction
slack reactions remove C123456789 1234567890.123456 thumbsup

# List reactions on a message
slack reactions list C123456789 1234567890.123456
```

### Status (`slack status` or `slack s`)

```bash
# Get current status
slack status get

# Set status with emoji and text
slack status set ":coffee:" "Taking a break"

# Set status with expiration
slack status set ":meeting:" "In a meeting" --expires 1h
slack status set ":calendar:" "Out of office" --expires today
slack status set ":palm_tree:" "On vacation" --expires tomorrow

# Clear status
slack status clear

# Set presence
slack status presence away
slack status presence auto
```

### Reminders (`slack reminders`)

```bash
# List reminders
slack reminders list

# Create a reminder
slack reminders add "Review PRs" --time "in 2 hours"
slack reminders add "Team meeting" --time "tomorrow at 10am"

# Complete a reminder
slack reminders complete Rm123456789

# Delete a reminder
slack reminders delete Rm123456789
```

## Output Modes

By default, output is JSON (optimized for AI agents). Use `--plain` for human-readable TSV output:

```bash
# JSON output (default)
slack channels list

# Plain text output (TSV format)
slack --plain channels list
slack channels list --plain
```

## Global Options

```bash
--plain            # Plain TSV output instead of JSON
-w, --workspace    # Specify workspace (team ID or name)
--token            # Override token (skip keyring)
-v, --verbose      # Enable verbose logging to stderr
--help             # Show help
--version          # Show version
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `SLACK_TOKEN` | Default token (overrides keyring) |
| `SLACK_WORKSPACE` | Default workspace |
| `SLACK_TOKEN_STORE_PATH` | Use file-based storage instead of keyring |
| `SLACK_API_BASE_URL` | Override API base URL (for testing) |

## Token Types

| Prefix | Type | Use Case |
|--------|------|----------|
| `xoxp-` | User OAuth | Full user access |
| `xoxb-` | Bot OAuth | Bot access (no search) |
| `xoxc-` | Browser | Requires xoxd cookie |

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Authentication required |
| 3 | Invalid arguments |
| 4 | API error |
| 5 | Rate limited |
| 6 | Network error |

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Run with verbose output
cargo run -- -v channels list

# Format code
cargo fmt

# Lint
cargo clippy
```

## FAQ

**Q: Why do I get "auth_required" errors?**

A: You need to authenticate first with `slack auth add --token xoxp-...`. Make sure your token is valid and has the required scopes.

**Q: How do I use browser tokens?**

A: Run `slack auth browser-help` for detailed instructions on extracting xoxc and xoxd tokens from your browser.

**Q: Can I use this with a bot token?**

A: Yes, but bot tokens (xoxb-*) have limited access. Notably, search is not available with bot tokens.

**Q: How do I switch between multiple workspaces?**

A: Use `slack auth switch <workspace>` to set the default workspace. You can also use `--workspace` or `-w` flag to specify a workspace for a single command.

**Q: Where are my tokens stored?**

A: Tokens are stored in your system keyring (macOS Keychain, Windows Credential Manager, or Linux Secret Service). Set `SLACK_TOKEN_STORE_PATH` to use file-based storage instead (useful for testing).

**Q: Authentication isn't persisting - what should I do?**

A: See the Troubleshooting section below for keyring debugging steps and file-based fallback options.

**Q: How do I get JSON output for scripting?**

A: JSON is the default output format. Use `--plain` if you want human-readable TSV output.

## Troubleshooting

### Authentication Not Persisting

If `slack auth add` succeeds but `slack auth list` shows no workspaces or you get "auth_required" errors:

**1. Test keyring access:**

```bash
# Run the keyring test utility
cargo run --bin test_keyring
```

If all tests pass but auth still fails, enable verbose logging to see more details:

```bash
slack -v auth add --token xoxp-...
```

**2. Use file-based storage as fallback:**

If keyring access is problematic (CI environments, sandboxed apps, etc.), use file-based storage:

```bash
export SLACK_TOKEN_STORE_PATH=~/.slack-tokens.json
slack auth add --token xoxp-...
```

**3. Platform-specific checks:**

**macOS:**
- Open Keychain Access and search for "slack-cli"
- Verify the app has permission to access the keychain
- Run: `security find-generic-password -s slack-cli`

**Linux:**
- Ensure a Secret Service daemon is running (gnome-keyring, KWallet)
- Check if you're in a headless/SSH environment (may need D-Bus session)

**Windows:**
- Check Credential Manager for "slack-cli" entries

### Rate Limiting

If you receive rate limit errors, the CLI will automatically retry with backoff. For bulk operations, consider:

- Adding delays between requests
- Using search instead of listing all messages
- Batching operations appropriately

### Token Errors

**"invalid_auth" error:**
- Verify your token is valid and not expired
- Check that the token has the required scopes
- Try re-authenticating with `slack auth add`

**"missing_scope" error:**
- Your token doesn't have permission for this operation
- Use a token with more scopes, or try a different token type

## Full Specification

For complete implementation details, see [PLAN.md](PLAN.md).

## License

MIT License - see [LICENSE](LICENSE) for details.

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for release history.
