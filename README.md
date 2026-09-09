# Slack CLI

A comprehensive Rust CLI tool for Slack, designed for AI agents and automation.

## Features

- **Multiple authentication methods**: OAuth, browser tokens (xoxc+xoxd), direct tokens (xoxp/xoxb)
- **Full workspace access**: Channels, messages, threads, search, files, reactions, reminders, status
- **Generic API escape hatch**: `slack api` calls any Slack Web API method with your stored auth
- **Agent-first design**: JSON output by default, optimized for AI consumption
- **Minimal footprint**: No config files, tokens stored in system keyring
- **Fast and reliable**: Built with Rust for performance and safety
- **Shell completions**: Bash, Zsh, Fish, PowerShell

## Agent Skill

If you're using an AI coding agent (Claude, Codex, Cursor, etc.), install the Slack skill so your agent knows how to use this CLI:

```bash
npx skills add TeamCadenceAI/slack-cli
```

The skill lives in [`skills/slack/`](skills/slack/) and covers all commands with usage examples.

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

# Import from a workspace you're already signed into locally (Slack desktop app
# or a browser) - no manual token copying. Give just the subdomain or full URL:
slack auth add onlinegeniuses
slack auth add onlinegeniuses.slack.com
slack auth add myteam --browser slack   # narrow the source app/browser

# Discover which workspaces are signed into local apps (reads local storage
# only - no Keychain access or network calls unless you pass --check)
slack auth discover
slack auth discover --browser slack
slack auth discover --check            # also validate each token is live

# List configured workspaces (add --check to verify each token via auth.test)
slack auth list
slack auth list --check

# Show current auth status
slack auth status

# Switch default workspace
slack auth switch T1234567890

# Remove a workspace
slack auth remove T1234567890

# Get help extracting browser tokens
slack auth browser-help
```

#### Selecting a workspace

With multiple workspaces configured, target one per command with `-w` /
`--workspace` (or the `SLACK_WORKSPACE` env var). The value is matched against
each workspace's **team ID** or **domain** — team *names* are not matched
(they're user-editable and too volatile to be a stable selector):

```bash
slack -w T04U8BDD0KC channels list        # by team ID
slack -w cadence-app channels list         # by domain (subdomain)
slack -w cadence-app.slack.com channels list   # full URL also accepted

export SLACK_WORKSPACE=cadence-app         # session default
slack channels list
```

Run `slack auth list` to see the selectable values. Its columns are
`team_id`, `domain`, `name`, `token_type`, and a `*` default marker (JSON
includes a `team_domain` field):

```
T04U8BDD0KC   cadence-app   Cadence   Browser   *
```

The stored default (set via `slack auth switch <team_id|domain>`) is used when
no `-w` / `SLACK_WORKSPACE` is given.

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

# Send a message (text is Markdown by default and converted to Slack mrkdwn)
slack messages send "#general" "Hello, **world**! See [docs](https://example.com)"

# Send verbatim, no Markdown conversion / mrkdwn parsing
slack messages send "#general" "literal *text*" --format plain

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

#### Message formatting

`messages send` treats input as **standard Markdown** by default (`--format
markdown`) and converts it to Slack **mrkdwn** before sending, so agents and
scripts can emit ordinary Markdown:

| Markdown | Sent as (mrkdwn) | Renders as |
| --- | --- | --- |
| `**bold**`, `__bold__` | `*bold*` | **bold** |
| `*italic*`, `_italic_` | `_italic_` | _italic_ |
| `~~strike~~` | `~strike~` | ~~strike~~ |
| `# Heading` | `*Heading*` | bold line |
| `[text](url)` | `<url\|text>` | linked text |
| `![alt](url)` | `<url\|alt>` | link |
| `- item` / `* item` / `+ item` | `• item` | bullet |
| `1. item` / `1) item` | `1. item` | numbered |

Inline code `` `…` ``, fenced code blocks ```` ``` ````, and existing mrkdwn
spans (`<@U…>` mentions, `<url|text>` links) are passed through untouched. Use
`--format plain` to send text verbatim with mrkdwn parsing disabled.

### Users (`slack users` or `slack u`)

```bash
# List all users
slack users list

# List only active users
slack users list --active-only

# Get current user info
slack users me

# Get user info by ID, name, or email
slack users info U123456789
slack users info @username
slack users info alice@example.com

# Send a direct message (opens or reuses the IM, then sends normally)
slack messages send @username "Hello directly"

# List user groups and group members
slack users groups list
slack users groups members @engineering
slack users groups members S123456789 --resolve

# Export users to CSV
slack users export --output users.csv
```

Email lookup requires `users:read.email`, and user-group commands require
`usergroups:read`. Direct-message opening uses `conversations.open` and normally
requires `im:write` (or the applicable conversation-write scope for the Slack
token type). Slack API missing-scope errors are returned unchanged.

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

### Pins (`slack pins`)

```bash
# Pin or unpin a message
slack pins add "#general" 1234567890.123456
slack pins remove C123456789 1234567890.123456

# List all pinned items in a channel
slack pins list "#general"
```

Adding and removing pins requires `pins:write`; listing requires `pins:read`.
Pin list JSON preserves message, file, and other item records returned by Slack.

### Emoji (`slack emoji`)

```bash
# List workspace custom emoji
slack emoji list
slack --plain emoji list
```

`emoji list` requires `emoji:read` and returns custom workspace emoji only,
including unchanged image URLs and `alias:<name>` values. It does not include
Slack's built-in Unicode emoji.

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

### Generic API (`slack api`)

Escape hatch for any Slack Web API method that doesn't have a dedicated
command. Reuses your stored credentials (including browser xoxc token +
xoxd cookie), the `-w`/`--workspace` selector, and `--token`/`SLACK_TOKEN`
overrides — your token's scopes still apply, so a method can fail with
`missing_scope` just as it would with curl.

```bash
# Call a method (POST is the default HTTP method)
slack api conversations.create -f name=my-new-channel

# GET with query parameters
slack api conversations.info -X GET -f channel=C123456789

# Typed fields: -F parses JSON booleans, numbers, arrays and objects
slack api conversations.list -X GET -F limit=200 -F exclude_archived=true
slack api chat.postMessage -f channel=C123 -f text=hi -F unfurl_links=false

# Load the parameter object from a JSON file or stdin
slack api chat.postMessage --input params.json
echo '{"channel":"C123","text":"hi"}' | slack api chat.postMessage --input -

# Full Slack URLs are accepted and normalized to the method name
slack api https://slack.com/api/team.info -X GET
```

**Flags**

| Flag | Description |
|------|-------------|
| `-X, --method <GET\|POST>` | HTTP method. Defaults to `POST` (unlike `gh api`, which defaults to GET). |
| `-f, --raw-field key=value` | Add a parameter as a plain string. Repeatable. |
| `-F, --field key=value` | Add a typed parameter: `true`/`false`, numbers, and JSON arrays/objects are parsed as JSON; anything else is a string. `-F key=null` is rejected — omit the field instead. Repeatable. |
| `--input FILE` | Read the parameter object from a JSON file, or from stdin with `--input -`. Cannot be combined with `-f`/`-F`. Fields whose value is `null` are omitted, matching the CLI's form encoder. |

Duplicate parameter names (across `-f`/`-F`) are rejected.

**Request conventions**

- Only `GET` and `POST` are supported. `GET` sends parameters as URL query
  parameters; `POST` sends them form-encoded
  (`application/x-www-form-urlencoded`), which is what the Slack Web API
  expects.
- There is no raw JSON request body: `--input` loads a JSON *parameter
  object* which is then form-encoded like `-f`/`-F` fields. Nested arrays
  and objects (e.g. `blocks`, `attachments`) are serialized as JSON strings,
  per Slack convention.
- Endpoints are Web API method names (`conversations.info`) or full
  `https://slack.com/api/<method>` URLs, which are normalized to the method
  name. URLs with other hosts, embedded credentials, query strings,
  fragments, or extra path segments are rejected. (`SLACK_API_BASE_URL`
  still overrides the base URL for testing/mocks.)
- HTTP redirects are never followed, so your token and cookies can't leak
  to another host.
- No automatic pagination — pass `cursor`/`limit` yourself and follow
  `response_metadata.next_cursor`.
- Not supported: custom headers, file uploads, name→ID resolution, jq
  filtering, or Edge API endpoints.

**Output** is the full JSON response from Slack on success. `--plain` is not
supported for `slack api`. Slack `ok: false` responses, rate limits, and
network failures exit with status 1 and structured JSON error codes;
invalid arguments exit with status 2. Rate limits are retried at most five
times; a server-requested delay over 60 seconds is returned immediately
as a rate-limit error rather than blocking or retrying too early.

## Output Modes

By default, output is JSON (optimized for AI agents). Use `--plain` for human-readable TSV output:

```bash
# JSON output (default)
slack channels list

# Plain text output (TSV format)
slack --plain channels list
slack channels list --plain
```

`slack api` is JSON-only and rejects `--plain`.

## Global Options

```bash
--plain            # Plain TSV output instead of JSON
-w, --workspace    # Select workspace by team ID (T…) or domain (myteam / myteam.slack.com)
--token            # Override token (skip keyring)
-v, --verbose      # Enable verbose logging to stderr
--help             # Show help
--version          # Show version
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `SLACK_TOKEN` | Default token (overrides keyring) |
| `SLACK_WORKSPACE` | Default workspace (team ID or domain, same as `-w`) |
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
