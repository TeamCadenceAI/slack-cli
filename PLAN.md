# Slack CLI - Rust Implementation Plan

> Converting [slack-mcp-server](https://github.com/korotovsky/slack-mcp-server) to a Rust CLI modeled after [gogcli](https://github.com/steipete/gogcli)

---

## Table of Contents

1. [Project Overview](#1-project-overview)
2. [Architecture](#2-architecture)
3. [Project Structure](#3-project-structure)
4. [Dependencies](#4-dependencies)
5. [Authentication System](#5-authentication-system)
6. [CLI Commands](#6-cli-commands)
7. [API Integration](#7-api-integration)
8. [Output Formatting](#8-output-formatting)
9. [Name Resolution](#9-name-resolution-no-persistent-cache)
10. [Configuration Management](#10-configuration-management)
11. [Error Handling](#11-error-handling)
12. [Implementation Phases](#12-implementation-phases)
13. [Testing Strategy](#13-testing-strategy)
14. [Future Enhancements](#14-future-enhancements)

---

## 1. Project Overview

### 1.1 Goals

Create a Rust CLI tool named `slack` that provides comprehensive Slack workspace access with:

- **Multiple authentication methods**: OAuth flow, browser tokens (XOXC/XOXD), bot tokens
- **Full workspace access**: Channels, messages, threads, search, files, reactions
- **Agent-first design**: Output optimized for AI agents and scripts, not humans
- **Minimal footprint**: No config files, no cache directories - tokens stored in system keyring
- **JSON by default**: Machine-readable output as the primary mode

### 1.2 Feature Parity with slack-mcp-server

| MCP Tool | CLI Equivalent |
|----------|----------------|
| `conversations_history` | `slack messages list` |
| `conversations_replies` | `slack messages thread` |
| `conversations_add_message` | `slack messages send` |
| `conversations_search_messages` | `slack messages search` |
| `channels_list` | `slack channels list` |
| `reactions_add` | `slack reactions add` |
| `reactions_remove` | `slack reactions remove` |
| `attachment_get_data` | `slack files get` |
| `slack://{workspace}/channels` | `slack channels export` |
| `slack://{workspace}/users` | `slack users list` |

### 1.3 Design Principles

- **Command structure**: `slack <noun> <verb>` (e.g., `slack messages search`, `slack channels list`)
- **JSON by default**: All output is JSON unless `--plain` is specified
- **No config files**: All settings stored in system keyring
- **No cache directories**: Just-in-time API lookups for name→ID resolution
- **IDs preferred**: Agents should use IDs (C1234567890) but names (#general) are supported via API lookup
- **Stdin support**: Commands accept input from stdin where appropriate
- **JSON errors**: Errors are JSON-formatted when in JSON output mode

### 1.4 Naming

- **Binary name**: `slack`
- **Package name**: `slack-cli`
- **Keyring service**: `slack-cli` (tokens and config stored here)

---

## 2. Architecture

### 2.1 High-Level Design

```
┌─────────────────────────────────────────────────────────────────┐
│                         CLI Layer (clap)                        │
│   slack auth | slack channels | slack messages | slack files    │
└─────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Command Handlers                            │
│   Parse args → Resolve auth → Call API → Format output          │
└─────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                     API Client Layer                             │
│   SlackClient { web_api, edge_api, rate_limiter }               │
└─────────────────────────────────────────────────────────────────┘
                                │
              ┌─────────────────┴─────────────────┐
              ▼                                   ▼
┌─────────────────────────┐          ┌─────────────────────────┐
│   Web API               │          │   Edge API              │
│   (xoxp/xoxb tokens)    │          │   (xoxc/xoxd tokens)    │
└─────────────────────────┘          └─────────────────────────┘
```

### 2.2 Core Components

| Component | Responsibility |
|-----------|----------------|
| `cli` | Clap-based command parsing and dispatch |
| `auth` | Token storage in keyring, OAuth flow, browser auth |
| `api` | Slack Web API and Edge API clients |
| `output` | JSON (default) and plain TSV formatting |
| `error` | Custom error types, JSON-formatted in JSON mode |
| `resolve` | Just-in-time name→ID resolution via API |

---

## 3. Project Structure

```
slack-cli/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── PLAN.md
├── LICENSE
├── .github/
│   └── workflows/
│       ├── ci.yml
│       └── release.yml
├── src/
│   ├── main.rs                 # Entry point, clap setup
│   ├── lib.rs                  # Library exports
│   │
│   ├── cli/
│   │   ├── mod.rs              # CLI struct definitions
│   │   ├── root.rs             # Root CLI and global flags
│   │   ├── auth.rs             # slack auth subcommands
│   │   ├── channels.rs         # slack channels subcommands
│   │   ├── messages.rs         # slack messages subcommands (includes search)
│   │   ├── users.rs            # slack users subcommands
│   │   ├── files.rs            # slack files subcommands
│   │   ├── reactions.rs        # slack reactions subcommands
│   │   ├── status.rs           # slack status subcommands
│   │   └── reminders.rs        # slack reminders subcommands
│   │
│   ├── auth/
│   │   ├── mod.rs
│   │   ├── oauth.rs            # OAuth2 browser flow
│   │   ├── tokens.rs           # Token types and validation
│   │   ├── storage.rs          # Keyring token storage
│   │   └── browser.rs          # XOXC/XOXD extraction helper
│   │
│   ├── api/
│   │   ├── mod.rs
│   │   ├── client.rs           # Main SlackClient
│   │   ├── web.rs              # Web API methods
│   │   ├── edge.rs             # Edge API methods (stealth mode)
│   │   ├── resolve.rs          # Name→ID resolution via API
│   │   └── rate_limiter.rs     # Rate limiting (Tier 2)
│   │
│   ├── models/
│   │   ├── mod.rs
│   │   ├── channel.rs          # Channel types
│   │   ├── message.rs          # Message types
│   │   ├── user.rs             # User types
│   │   ├── file.rs             # File/attachment types
│   │   └── reaction.rs         # Reaction types
│   │
│   ├── output/
│   │   ├── mod.rs
│   │   ├── json.rs             # JSON output (default)
│   │   └── plain.rs            # Plain TSV output
│   │
│   └── error/
│       ├── mod.rs
│       └── types.rs            # Custom error types (JSON-aware)
│
└── tests/
    ├── integration/
    │   ├── auth_test.rs
    │   ├── channels_test.rs
    │   └── messages_test.rs
    └── fixtures/
        └── ...
```

---

## 4. Dependencies

### 4.1 Core Dependencies

```toml
[package]
name = "slack-cli"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"

[[bin]]
name = "slack"
path = "src/main.rs"

[dependencies]
# CLI parsing
clap = { version = "4.5", features = ["derive", "env", "wrap_help"] }

# Async runtime
tokio = { version = "1.40", features = ["rt-multi-thread", "macros", "io-std"] }

# HTTP client
reqwest = { version = "0.12", features = ["json", "cookies", "rustls-tls"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# OAuth2
oauth2 = "4.4"

# Keyring for token storage (cross-platform)
keyring = "3"

# Time handling
chrono = { version = "0.4", features = ["serde"] }

# Error handling
thiserror = "1.0"
anyhow = "1.0"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# URL handling
url = "2.5"

# Base64 encoding
base64 = "0.22"

# Rate limiting
governor = "0.6"

# Open browser
open = "5"

# Local HTTP server for OAuth callback
tiny_http = "0.12"

[dev-dependencies]
mockito = "1.4"
tokio-test = "0.4"
assert_cmd = "2.0"
predicates = "3.1"
tempfile = "3.10"
```

### 4.2 Optional Dependencies

```toml
[features]
default = ["native-tls"]
native-tls = ["reqwest/native-tls"]
rustls = ["reqwest/rustls-tls"]

# For browser token extraction helper
browser-helper = ["headless_chrome"]

[dependencies.headless_chrome]
version = "1.0"
optional = true
```

---

## 5. Authentication System

### 5.1 Supported Authentication Methods

| Method | Tokens | Use Case | Search Support |
|--------|--------|----------|----------------|
| **User OAuth** | `xoxp-*` | Full access with user permissions | ✅ |
| **Bot OAuth** | `xoxb-*` | App-level access to invited channels | ❌ |
| **Browser Tokens** | `xoxc-*` + `xoxd-*` | Stealth mode, no app required | ✅ |

### 5.2 Token Priority (highest to lowest)

1. Command-line flags: `--token` or `--xoxc/--xoxd`
2. Environment variables: `SLACK_TOKEN`, `SLACK_XOXC_TOKEN`, `SLACK_XOXD_TOKEN`
3. Stored tokens in keyring (from `slack auth add`)

### 5.3 OAuth Flow Implementation

```rust
// src/auth/oauth.rs

pub struct OAuthFlow {
    client_id: String,
    client_secret: String,
    scopes: Vec<String>,
    redirect_port: u16,
}

impl OAuthFlow {
    /// Perform browser-based OAuth flow
    pub async fn authorize(&self) -> Result<TokenSet, AuthError> {
        // 1. Generate state token (32 bytes, base64)
        let state = generate_state();

        // 2. Build authorization URL
        let auth_url = format!(
            "https://slack.com/oauth/v2/authorize?\
             client_id={}&\
             scope={}&\
             redirect_uri=http://localhost:{}/callback&\
             state={}",
            self.client_id,
            self.scopes.join(","),
            self.redirect_port,
            state
        );

        // 3. Start local HTTP server for callback
        let server = tiny_http::Server::http(
            format!("127.0.0.1:{}", self.redirect_port)
        )?;

        // 4. Open browser
        open::that(&auth_url)?;

        // 5. Wait for callback (with timeout)
        let code = wait_for_callback(&server, &state, Duration::from_secs(120))?;

        // 6. Exchange code for tokens
        let tokens = exchange_code(&self.client_id, &self.client_secret, &code).await?;

        Ok(tokens)
    }

    /// Manual flow for headless environments
    pub async fn authorize_manual(&self) -> Result<TokenSet, AuthError> {
        // Print URL, ask user to paste redirect URL
        // Extract code from pasted URL
        // Exchange for tokens
    }
}
```

### 5.4 Browser Token Auth

```rust
// src/auth/browser.rs

pub struct BrowserTokens {
    pub xoxc: String,  // Session token from localStorage
    pub xoxd: String,  // Cookie value
}

impl BrowserTokens {
    /// Validate token format
    pub fn validate(&self) -> Result<(), AuthError> {
        if !self.xoxc.starts_with("xoxc-") {
            return Err(AuthError::InvalidToken("xoxc token must start with 'xoxc-'"));
        }
        if self.xoxd.is_empty() {
            return Err(AuthError::InvalidToken("xoxd cookie is required"));
        }
        Ok(())
    }
}

/// Instructions for extracting browser tokens
pub fn print_extraction_instructions() {
    println!(r#"
To extract browser tokens:

1. Open Slack in your browser (not the desktop app)
2. Open Developer Tools (F12)
3. Go to Application → Local Storage → https://app.slack.com
4. Find 'localConfig_v2' and copy the 'token' value (starts with xoxc-)
5. Go to Application → Cookies → https://app.slack.com
6. Find 'd' cookie and copy its value (starts with xoxd-)

Then run:
  slack auth add --xoxc <TOKEN> --xoxd <COOKIE>
"#);
}
```

### 5.5 Token Storage

```rust
// src/auth/storage.rs

use keyring::Entry;

const SERVICE_NAME: &str = "slack-cli";

pub struct TokenStore {
    workspace: String,
}

impl TokenStore {
    /// Store token in system keyring
    pub fn store(&self, token: &TokenSet) -> Result<(), StorageError> {
        let entry = Entry::new(SERVICE_NAME, &self.workspace)?;
        let serialized = serde_json::to_string(token)?;
        entry.set_password(&serialized)?;
        Ok(())
    }

    /// Retrieve token from keyring
    pub fn load(&self) -> Result<Option<TokenSet>, StorageError> {
        let entry = Entry::new(SERVICE_NAME, &self.workspace)?;
        match entry.get_password() {
            Ok(data) => Ok(Some(serde_json::from_str(&data)?)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List all stored workspaces
    pub fn list_workspaces() -> Result<Vec<String>, StorageError> {
        // Platform-specific enumeration
    }

    /// Delete stored token
    pub fn delete(&self) -> Result<(), StorageError> {
        let entry = Entry::new(SERVICE_NAME, &self.workspace)?;
        entry.delete_credential()?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct TokenSet {
    pub token_type: TokenType,
    pub access_token: String,
    pub xoxd_cookie: Option<String>,  // For browser tokens
    pub team_id: String,
    pub team_name: String,
    pub user_id: String,
    pub created_at: DateTime<Utc>,
    pub scopes: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub enum TokenType {
    UserOAuth,   // xoxp-*
    BotOAuth,    // xoxb-*
    Browser,     // xoxc-* + xoxd-*
}
```

### 5.6 Auth Commands

```
slack auth
├── add [--oauth | --xoxc <TOKEN> --xoxd <COOKIE>]  # Add/authorize workspace
├── list                                              # List authorized workspaces
├── remove <WORKSPACE>                                # Remove authorization
├── status                                            # Show current auth status
├── switch <WORKSPACE>                                # Set default workspace
└── token                                             # Print current token (debug)
```

---

## 6. CLI Commands

### 6.1 Global Flags

```rust
// src/cli/root.rs

#[derive(Parser)]
#[command(name = "slack", version, about = "Slack CLI for agents")]
pub struct Cli {
    /// Plain TSV output instead of JSON
    #[arg(long, global = true)]
    pub plain: bool,

    /// Workspace to use (defaults to first authorized)
    #[arg(short = 'w', long, global = true, env = "SLACK_WORKSPACE")]
    pub workspace: Option<String>,

    /// Override token (skip keyring)
    #[arg(long, global = true, env = "SLACK_TOKEN", hide = true)]
    pub token: Option<String>,

    /// Verbose logging (to stderr)
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Authentication management
    Auth(AuthCmd),
    /// Channel operations
    Channels(ChannelsCmd),
    /// Message operations (list, send, search, thread)
    Messages(MessagesCmd),
    /// User operations
    Users(UsersCmd),
    /// File operations
    Files(FilesCmd),
    /// Reaction operations
    Reactions(ReactionsCmd),
    /// User status/presence
    Status(StatusCmd),
    /// Reminder operations
    Reminders(RemindersCmd),
}
```

**Note**: JSON is the default output format. Use `--plain` for TSV output suitable for shell scripting.

### 6.2 Auth Commands

```rust
// src/cli/auth.rs

#[derive(Args)]
pub struct AuthCmd {
    #[command(subcommand)]
    pub command: AuthCommands,
}

#[derive(Subcommand)]
pub enum AuthCommands {
    /// Authorize a Slack workspace
    Add {
        /// Use browser OAuth flow (default)
        #[arg(long, conflicts_with_all = ["xoxc", "xoxd", "token"])]
        oauth: bool,

        /// Browser session token (xoxc-*)
        #[arg(long, requires = "xoxd")]
        xoxc: Option<String>,

        /// Browser cookie (xoxd-*)
        #[arg(long, requires = "xoxc")]
        xoxd: Option<String>,

        /// Direct token (xoxp-* or xoxb-*)
        #[arg(long)]
        token: Option<String>,

        /// Manual OAuth flow (no browser)
        #[arg(long)]
        manual: bool,

        /// OAuth scopes to request
        #[arg(long, value_delimiter = ',', default_value = "channels:read,channels:history,users:read,search:read")]
        scopes: Vec<String>,
    },

    /// List authorized workspaces
    List,

    /// Remove workspace authorization
    Remove {
        /// Workspace name or ID
        workspace: String,
    },

    /// Show current authentication status
    Status,

    /// Set default workspace
    Switch {
        /// Workspace name or ID
        workspace: String,
    },

    /// Print instructions for extracting browser tokens
    Help,
}
```

### 6.3 Channel Commands

```rust
// src/cli/channels.rs

#[derive(Args)]
pub struct ChannelsCmd {
    #[command(subcommand)]
    pub command: ChannelsCommands,
}

#[derive(Subcommand)]
pub enum ChannelsCommands {
    /// List channels
    List {
        /// Channel types to include
        #[arg(short, long, value_delimiter = ',',
              default_value = "public_channel,private_channel")]
        types: Vec<ChannelType>,

        /// Sort by popularity
        #[arg(long)]
        sort_popularity: bool,

        /// Maximum results
        #[arg(short = 'n', long, default_value = "100")]
        limit: u32,

        /// Pagination cursor
        #[arg(long)]
        cursor: Option<String>,
    },

    /// Show channel info
    Info {
        /// Channel name or ID (#general or C1234567890)
        channel: String,
    },

    /// List direct messages
    Dms {
        /// Include multi-person DMs
        #[arg(long)]
        include_mpim: bool,

        /// Maximum results
        #[arg(short = 'n', long, default_value = "50")]
        limit: u32,
    },

    /// Export channel list as CSV
    Export {
        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Clone, ValueEnum)]
pub enum ChannelType {
    PublicChannel,
    PrivateChannel,
    Im,
    Mpim,
}
```

### 6.4 Message Commands

```rust
// src/cli/messages.rs

#[derive(Args)]
pub struct MessagesCmd {
    #[command(subcommand)]
    pub command: MessagesCommands,
}

#[derive(Subcommand)]
pub enum MessagesCommands {
    /// List messages in a channel
    List {
        /// Channel name or ID (prefer ID: C1234567890)
        channel: String,

        /// Time limit (1d, 7d, 1m, 90d) or message count
        #[arg(short, long, default_value = "1d")]
        limit: String,

        /// Include join/leave messages
        #[arg(long)]
        include_activity: bool,

        /// Pagination cursor
        #[arg(long)]
        cursor: Option<String>,
    },

    /// Show thread replies
    Thread {
        /// Channel name or ID
        channel: String,

        /// Thread timestamp
        thread_ts: String,

        /// Time limit or message count
        #[arg(short, long, default_value = "1d")]
        limit: String,

        /// Include activity messages
        #[arg(long)]
        include_activity: bool,
    },

    /// Send a message
    Send {
        /// Channel name or ID
        channel: String,

        /// Message text (if not using --stdin)
        text: Option<String>,

        /// Read message from stdin
        #[arg(long)]
        stdin: bool,

        /// Reply to thread
        #[arg(long)]
        thread_ts: Option<String>,

        /// Content type
        #[arg(long, default_value = "markdown")]
        format: MessageFormat,

        /// Mark as read after sending
        #[arg(long)]
        mark_read: bool,
    },

    /// Search messages
    Search {
        /// Search query
        query: String,

        /// Filter to specific channel
        #[arg(long)]
        in_channel: Option<String>,

        /// Filter to DMs
        #[arg(long)]
        in_dm: bool,

        /// Filter by sender
        #[arg(long)]
        from: Option<String>,

        /// Filter by participant in thread
        #[arg(long)]
        with: Option<String>,

        /// Date before (YYYY-MM-DD)
        #[arg(long)]
        before: Option<String>,

        /// Date after (YYYY-MM-DD)
        #[arg(long)]
        after: Option<String>,

        /// Only thread replies
        #[arg(long)]
        threads_only: bool,

        /// Maximum results
        #[arg(short = 'n', long, default_value = "20")]
        limit: u32,

        /// Pagination cursor
        #[arg(long)]
        cursor: Option<String>,
    },

    /// Get a single message by URL or timestamp
    Get {
        /// Message URL or channel:timestamp
        message: String,
    },
}

#[derive(Clone, ValueEnum)]
pub enum MessageFormat {
    Markdown,
    Plain,
}
```

**Stdin example**:
```bash
echo "Hello from stdin" | slack messages send C1234567890 --stdin
cat message.md | slack messages send C1234567890 --stdin --format markdown
```

### 6.5 User Commands

```rust
// src/cli/users.rs

#[derive(Args)]
pub struct UsersCmd {
    #[command(subcommand)]
    pub command: UsersCommands,
}

#[derive(Subcommand)]
pub enum UsersCommands {
    /// List all users
    List {
        /// Maximum results
        #[arg(short = 'n', long)]
        limit: Option<u32>,

        /// Include deactivated users
        #[arg(long)]
        include_deactivated: bool,
    },

    /// Show user info
    Info {
        /// Username or user ID (@john or U1234567890)
        user: String,
    },

    /// Show current user
    Me,

    /// Export user list as CSV
    Export {
        /// Output file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}
```

### 6.6 File Commands

```rust
// src/cli/files.rs

#[derive(Args)]
pub struct FilesCmd {
    #[command(subcommand)]
    pub command: FilesCommands,
}

#[derive(Subcommand)]
pub enum FilesCommands {
    /// Download a file
    Get {
        /// File ID
        file_id: String,

        /// Output file (stdout if not specified)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Output as base64 (for binary files)
        #[arg(long)]
        base64: bool,
    },

    /// Show file info
    Info {
        /// File ID
        file_id: String,
    },

    /// List files in channel
    List {
        /// Channel name or ID
        #[arg(long)]
        channel: Option<String>,

        /// Filter by user
        #[arg(long)]
        user: Option<String>,

        /// Maximum results
        #[arg(short = 'n', long, default_value = "20")]
        limit: u32,
    },
}
```

### 6.7 Reaction Commands

```rust
// src/cli/reactions.rs

#[derive(Args)]
pub struct ReactionsCmd {
    #[command(subcommand)]
    pub command: ReactionsCommands,
}

#[derive(Subcommand)]
pub enum ReactionsCommands {
    /// Add reaction to a message
    Add {
        /// Channel name or ID
        channel: String,

        /// Message timestamp
        timestamp: String,

        /// Emoji name (without colons)
        emoji: String,
    },

    /// Remove reaction from a message
    Remove {
        /// Channel name or ID
        channel: String,

        /// Message timestamp
        timestamp: String,

        /// Emoji name
        emoji: String,
    },

    /// List reactions on a message
    List {
        /// Channel name or ID
        channel: String,

        /// Message timestamp
        timestamp: String,
    },
}
```

### 6.8 Status Commands (Personal presence/status)

```rust
// src/cli/status.rs

#[derive(Args)]
pub struct StatusCmd {
    #[command(subcommand)]
    pub command: StatusCommands,
}

#[derive(Subcommand)]
pub enum StatusCommands {
    /// Show current status
    Get,

    /// Set status
    Set {
        /// Status text
        text: String,

        /// Status emoji
        #[arg(long)]
        emoji: Option<String>,

        /// Expiration (30m, 1h, 4h, today, tomorrow)
        #[arg(long)]
        expires: Option<String>,
    },

    /// Clear status
    Clear,

    /// Set presence
    Presence {
        /// Presence: auto or away
        #[arg(value_enum)]
        status: PresenceStatus,
    },
}

#[derive(Clone, ValueEnum)]
pub enum PresenceStatus {
    Auto,
    Away,
}
```

### 6.9 Reminders Commands

```rust
// src/cli/reminders.rs

#[derive(Args)]
pub struct RemindersCmd {
    #[command(subcommand)]
    pub command: RemindersCommands,
}

#[derive(Subcommand)]
pub enum RemindersCommands {
    /// List reminders
    List,

    /// Add a reminder
    Add {
        /// Reminder text
        text: String,

        /// When (in 30m, tomorrow at 9am, etc.)
        #[arg(long)]
        when: String,
    },

    /// Complete a reminder
    Complete {
        /// Reminder ID
        id: String,
    },

    /// Delete a reminder
    Delete {
        /// Reminder ID
        id: String,
    },
}
```

---

## 7. API Integration

### 7.1 Client Architecture

```rust
// src/api/client.rs

pub struct SlackClient {
    http: reqwest::Client,
    token: TokenSet,
    rate_limiter: RateLimiter,
    api_base: Url,
    edge_base: Option<Url>,
}

impl SlackClient {
    pub fn new(token: TokenSet) -> Result<Self, ApiError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()?;

        let rate_limiter = RateLimiter::new(
            Duration::from_secs(3),  // Tier 2: 3 second interval
            3,                        // Burst of 3
        );

        let edge_base = if token.token_type == TokenType::Browser {
            Some(Url::parse(&format!(
                "https://edgeapi.slack.com/cache/{}/",
                token.team_id
            ))?)
        } else {
            None
        };

        Ok(Self {
            http,
            token,
            rate_limiter,
            api_base: Url::parse("https://slack.com/api/")?,
            edge_base,
        })
    }

    /// Make authenticated API request
    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        endpoint: &str,
        params: Option<&[(&str, &str)]>,
    ) -> Result<T, ApiError> {
        self.rate_limiter.acquire().await;

        let url = self.api_base.join(endpoint)?;
        let mut req = self.http.request(method, url);

        // Add authentication
        req = match &self.token.token_type {
            TokenType::Browser => {
                req.header("Authorization", format!("Bearer {}", self.token.access_token))
                   .header("Cookie", format!("d={}", self.token.xoxd_cookie.as_ref().unwrap()))
            }
            _ => req.bearer_auth(&self.token.access_token),
        };

        if let Some(params) = params {
            req = req.query(params);
        }

        let resp = req.send().await?;

        // Handle rate limiting
        if resp.status() == 429 {
            if let Some(retry_after) = resp.headers().get("Retry-After") {
                let secs: u64 = retry_after.to_str()?.parse()?;
                tokio::time::sleep(Duration::from_secs(secs)).await;
                return self.request(method, endpoint, params).await;
            }
        }

        let body: SlackResponse<T> = resp.json().await?;

        if body.ok {
            Ok(body.data.unwrap())
        } else {
            Err(ApiError::Slack {
                error: body.error.unwrap_or_default(),
                detail: body.detail,
            })
        }
    }
}
```

### 7.2 Web API Methods

```rust
// src/api/web.rs

impl SlackClient {
    /// Test authentication
    pub async fn auth_test(&self) -> Result<AuthInfo, ApiError> {
        self.request(Method::GET, "auth.test", None).await
    }

    /// List conversations/channels
    pub async fn conversations_list(
        &self,
        types: &[ChannelType],
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<ConversationsListResponse, ApiError> {
        let types_str = types.iter()
            .map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join(",");

        let mut params = vec![
            ("types", types_str.as_str()),
            ("limit", &limit.to_string()),
        ];

        if let Some(c) = cursor {
            params.push(("cursor", c));
        }

        self.request(Method::GET, "conversations.list", Some(&params)).await
    }

    /// Get conversation history
    pub async fn conversations_history(
        &self,
        channel: &str,
        limit: u32,
        oldest: Option<&str>,
        cursor: Option<&str>,
    ) -> Result<ConversationsHistoryResponse, ApiError> {
        let mut params = vec![
            ("channel", channel),
            ("limit", &limit.to_string()),
        ];

        if let Some(o) = oldest {
            params.push(("oldest", o));
        }
        if let Some(c) = cursor {
            params.push(("cursor", c));
        }

        self.request(Method::GET, "conversations.history", Some(&params)).await
    }

    /// Get thread replies
    pub async fn conversations_replies(
        &self,
        channel: &str,
        ts: &str,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<ConversationsRepliesResponse, ApiError> {
        // Similar implementation
    }

    /// Search messages
    pub async fn search_messages(
        &self,
        query: &str,
        sort: &str,
        count: u32,
        page: u32,
    ) -> Result<SearchResponse, ApiError> {
        // Note: Not available for bot tokens
        if self.token.token_type == TokenType::BotOAuth {
            return Err(ApiError::NotSupported("Search not available for bot tokens"));
        }
        // Implementation
    }

    /// Post message
    pub async fn chat_post_message(
        &self,
        channel: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> Result<PostMessageResponse, ApiError> {
        // Implementation using POST with JSON body
    }

    /// Add reaction
    pub async fn reactions_add(
        &self,
        channel: &str,
        timestamp: &str,
        emoji: &str,
    ) -> Result<(), ApiError> {
        // Implementation
    }

    /// Remove reaction
    pub async fn reactions_remove(
        &self,
        channel: &str,
        timestamp: &str,
        emoji: &str,
    ) -> Result<(), ApiError> {
        // Implementation
    }

    /// Get file info
    pub async fn files_info(&self, file_id: &str) -> Result<FileInfo, ApiError> {
        // Implementation
    }

    /// Download file
    pub async fn files_download(&self, url: &str) -> Result<Bytes, ApiError> {
        // Implementation with size limit (5MB)
    }

    /// List users
    pub async fn users_list(
        &self,
        cursor: Option<&str>,
    ) -> Result<UsersListResponse, ApiError> {
        // Implementation
    }

    /// Get user info
    pub async fn users_info(&self, user_id: &str) -> Result<User, ApiError> {
        // Implementation
    }
}
```

### 7.3 Edge API (Stealth Mode)

```rust
// src/api/edge.rs

impl SlackClient {
    /// Client boot - get initial workspace data
    async fn edge_client_boot(&self) -> Result<ClientBootResponse, ApiError> {
        let url = self.edge_base.as_ref()
            .ok_or(ApiError::EdgeNotAvailable)?
            .join("client.boot")?;

        let resp = self.http.post(url)
            .header("Authorization", format!("Bearer {}", self.token.access_token))
            .header("Cookie", format!("d={}", self.token.xoxd_cookie.as_ref().unwrap()))
            .json(&json!({
                "token": self.token.access_token,
                "team_id": self.token.team_id,
            }))
            .send()
            .await?;

        resp.json().await.map_err(Into::into)
    }

    /// Get conversation via Edge API
    async fn edge_conversations_view(
        &self,
        channel: &str,
    ) -> Result<ConversationView, ApiError> {
        // Implementation for stealth mode channel access
    }

    /// Search channels via Edge API
    async fn edge_search_channels(
        &self,
        query: &str,
    ) -> Result<Vec<Channel>, ApiError> {
        // Implementation
    }
}
```

### 7.4 Rate Limiter

```rust
// src/api/rate_limiter.rs

use governor::{Quota, RateLimiter as GovernorLimiter};
use std::num::NonZeroU32;

pub struct RateLimiter {
    limiter: GovernorLimiter</* ... */>,
}

impl RateLimiter {
    pub fn new(interval: Duration, burst: u32) -> Self {
        let quota = Quota::with_period(interval)
            .unwrap()
            .allow_burst(NonZeroU32::new(burst).unwrap());

        Self {
            limiter: GovernorLimiter::direct(quota),
        }
    }

    pub async fn acquire(&self) {
        self.limiter.until_ready().await;
    }
}
```

---

## 8. Output Formatting

### 8.1 Output Mode Detection

```rust
// src/output/mod.rs

#[derive(Clone, Copy, PartialEq, Default)]
pub enum OutputMode {
    #[default]
    Json,    // Default: machine-readable JSON
    Plain,   // TSV for shell scripting
}

impl OutputMode {
    pub fn from_flags(plain: bool) -> Self {
        if plain { OutputMode::Plain }
        else { OutputMode::Json }
    }

    pub fn from_env() -> Self {
        if std::env::var("SLACK_PLAIN").is_ok() { OutputMode::Plain }
        else { OutputMode::Json }
    }
}
```

### 8.2 JSON Output (Default)

```rust
// src/output/json.rs

use serde::Serialize;

/// Write value as pretty-printed JSON to stdout
pub fn write_json<T: Serialize>(value: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(value)?;
    println!("{}", json);
    Ok(())
}

/// Write JSON error to stdout (for agent consumption)
pub fn write_json_error(error: &SlackError) -> Result<()> {
    let err_json = serde_json::json!({
        "error": true,
        "code": error.code(),
        "message": error.to_string(),
        "detail": error.detail(),
    });
    println!("{}", serde_json::to_string_pretty(&err_json)?);
    Ok(())
}
```

### 8.3 Plain TSV Output

```rust
// src/output/plain.rs

/// Write messages as TSV
pub fn write_messages_plain(messages: &[Message]) -> Result<()> {
    for msg in messages {
        // Escape newlines and tabs in message text
        let text = msg.text
            .replace('\t', "\\t")
            .replace('\n', "\\n");
        println!("{}\t{}\t{}\t{}",
            msg.timestamp,
            msg.user_id,
            msg.channel,
            text
        );
    }
    Ok(())
}

/// Write channels as TSV
pub fn write_channels_plain(channels: &[Channel]) -> Result<()> {
    for ch in channels {
        println!("{}\t{}\t{}\t{}",
            ch.id,
            ch.name.as_deref().unwrap_or(""),
            ch.num_members.unwrap_or(0),
            if ch.is_private { "private" } else { "public" }
        );
    }
    Ok(())
}
```

### 8.4 Error Handling in JSON Mode

When output is JSON (default), errors are also output as JSON to stdout:

```json
{
  "error": true,
  "code": "channel_not_found",
  "message": "Channel not found: #nonexistent",
  "detail": null
}
```

Exit codes are still set appropriately (1 for errors, 2 for usage errors).

---

## 9. Name Resolution (No Persistent Cache)

Instead of persistent caching, we use just-in-time API lookups for name→ID resolution. IDs are preferred for agent use.

### 9.1 Resolution Strategy

```rust
// src/api/resolve.rs

impl SlackClient {
    /// Resolve channel identifier to ID
    ///
    /// Accepts:
    /// - Direct IDs: C1234567890, D1234567890, G1234567890 (returned as-is)
    /// - Channel names: #general, general (looked up via API)
    /// - User DMs: @username (looked up via API)
    pub async fn resolve_channel(&self, identifier: &str) -> Result<String, SlackError> {
        // Direct ID - return immediately
        if identifier.starts_with('C') || identifier.starts_with('D') || identifier.starts_with('G') {
            if identifier.len() >= 9 && identifier.chars().skip(1).all(|c| c.is_alphanumeric()) {
                return Ok(identifier.to_string());
            }
        }

        // Name lookup required
        let name = identifier.trim_start_matches('#');

        // Search through conversations to find by name
        let mut cursor: Option<String> = None;
        loop {
            let resp = self.conversations_list(
                &[ChannelType::PublicChannel, ChannelType::PrivateChannel],
                200,
                cursor.as_deref(),
            ).await?;

            for channel in &resp.channels {
                if channel.name.as_deref() == Some(name) {
                    return Ok(channel.id.clone());
                }
            }

            cursor = resp.response_metadata.and_then(|m| m.next_cursor);
            if cursor.is_none() || cursor.as_ref().map(|c| c.is_empty()).unwrap_or(true) {
                break;
            }
        }

        Err(SlackError::ChannelNotFound(identifier.to_string()))
    }

    /// Resolve user identifier to ID
    ///
    /// Accepts:
    /// - Direct IDs: U1234567890 (returned as-is)
    /// - Usernames: @john, john (looked up via API)
    pub async fn resolve_user(&self, identifier: &str) -> Result<String, SlackError> {
        // Direct ID - return immediately
        if identifier.starts_with('U') && identifier.len() >= 9 {
            return Ok(identifier.to_string());
        }

        let name = identifier.trim_start_matches('@');

        // Search through users to find by name
        let mut cursor: Option<String> = None;
        loop {
            let resp = self.users_list(cursor.as_deref()).await?;

            for user in &resp.members {
                if user.name == name || user.profile.display_name.as_deref() == Some(name) {
                    return Ok(user.id.clone());
                }
            }

            cursor = resp.response_metadata.and_then(|m| m.next_cursor);
            if cursor.is_none() || cursor.as_ref().map(|c| c.is_empty()).unwrap_or(true) {
                break;
            }
        }

        Err(SlackError::UserNotFound(identifier.to_string()))
    }
}
```

### 9.2 Recommendations for Agents

For best performance, agents should:

1. **Use IDs directly** when available (C1234567890, U1234567890)
2. **Cache IDs** on the agent side after first lookup
3. **Avoid name lookups** in tight loops

Name resolution requires paginating through the full user/channel list which can be slow for large workspaces.

---

## 10. Configuration Management

All configuration is stored in the system keyring. No config files are used.

### 10.1 Keyring Storage

```rust
// src/auth/storage.rs

use keyring::Entry;

const SERVICE_NAME: &str = "slack-cli";

/// Keyring entries:
/// - "slack-cli:token:<team_id>" -> TokenSet JSON
/// - "slack-cli:default" -> default team_id
/// - "slack-cli:config" -> Config JSON (optional settings)

pub struct KeyringStore;

impl KeyringStore {
    /// Store token for a workspace
    pub fn store_token(team_id: &str, token: &TokenSet) -> Result<(), StorageError> {
        let entry = Entry::new(SERVICE_NAME, &format!("token:{}", team_id))?;
        let json = serde_json::to_string(token)?;
        entry.set_password(&json)?;
        Ok(())
    }

    /// Get token for a workspace
    pub fn get_token(team_id: &str) -> Result<Option<TokenSet>, StorageError> {
        let entry = Entry::new(SERVICE_NAME, &format!("token:{}", team_id))?;
        match entry.get_password() {
            Ok(json) => Ok(Some(serde_json::from_str(&json)?)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Set default workspace
    pub fn set_default(team_id: &str) -> Result<(), StorageError> {
        let entry = Entry::new(SERVICE_NAME, "default")?;
        entry.set_password(team_id)?;
        Ok(())
    }

    /// Get default workspace
    pub fn get_default() -> Result<Option<String>, StorageError> {
        let entry = Entry::new(SERVICE_NAME, "default")?;
        match entry.get_password() {
            Ok(id) => Ok(Some(id)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List all stored workspaces
    /// Note: Platform-specific; may not be supported on all backends
    pub fn list_workspaces() -> Result<Vec<WorkspaceInfo>, StorageError> {
        // Implementation varies by platform
        // On macOS: query Keychain for items with service = "slack-cli"
        // On Linux: query Secret Service
        // Fallback: store workspace list in a separate keyring entry
    }

    /// Delete token for a workspace
    pub fn delete_token(team_id: &str) -> Result<(), StorageError> {
        let entry = Entry::new(SERVICE_NAME, &format!("token:{}", team_id))?;
        entry.delete_credential()?;
        Ok(())
    }
}
```

### 10.2 Environment Variables

| Variable | Purpose |
|----------|---------|
| `SLACK_WORKSPACE` | Default workspace (overrides keyring) |
| `SLACK_TOKEN` | Override token (skip keyring) |
| `SLACK_XOXC_TOKEN` | Browser session token |
| `SLACK_XOXD_TOKEN` | Browser cookie (requires XOXC) |
| `SLACK_PLAIN` | Force plain TSV output |
| `SLACK_API_BASE` | Custom API URL (enterprise/gov) |
| `SLACK_LOG_LEVEL` | Logging verbosity (debug, info, warn, error) |

### 10.3 Token Resolution Order

1. `--token` flag (highest priority)
2. `SLACK_TOKEN` environment variable
3. `--xoxc` + `--xoxd` flags
4. `SLACK_XOXC_TOKEN` + `SLACK_XOXD_TOKEN` env vars
5. Keyring stored token for specified workspace
6. Keyring stored token for default workspace

---

## 11. Error Handling

### 11.1 Error Types

```rust
// src/error/types.rs

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SlackError {
    #[error("Authentication required. Run: slack auth add")]
    AuthRequired,

    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Workspace not found: {0}")]
    WorkspaceNotFound(String),

    #[error("Channel not found: {0}")]
    ChannelNotFound(String),

    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("Slack API error: {error}")]
    Api { error: String, detail: Option<String> },

    #[error("Rate limited. Retry after {0} seconds")]
    RateLimited(u64),

    #[error("Search not available for bot tokens")]
    SearchNotAvailable,

    #[error("File too large (max 5MB)")]
    FileTooLarge,

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Keyring error: {0}")]
    Keyring(#[from] keyring::Error),

    #[error("{0}")]
    Usage(String),
}

impl SlackError {
    pub fn exit_code(&self) -> i32 {
        match self {
            SlackError::Usage(_) => 2,
            _ => 1,
        }
    }
}
```

### 11.2 Error Formatting

```rust
// src/error/mod.rs

pub fn format_error(err: &SlackError) -> String {
    match err {
        SlackError::AuthRequired => {
            format!(
                "{}\n\n{}\n  {}",
                "Authentication required.".red(),
                "To authorize a workspace, run:",
                "slack auth add".cyan()
            )
        }
        SlackError::Api { error, detail } => {
            let mut msg = format!("Slack API error: {}", error.red());
            if let Some(d) = detail {
                msg.push_str(&format!("\n{}", d));
            }
            msg
        }
        _ => err.to_string(),
    }
}
```

---

## 12. Implementation Phases

### Phase 1: Core Foundation

**Goal**: Basic CLI structure with authentication

- [ ] Project setup (Cargo.toml, directory structure)
- [ ] CLI parsing with clap (global flags, subcommand skeleton)
- [ ] Token types and validation
- [ ] Keyring token storage
- [ ] `slack auth add --token <TOKEN>` (direct token)
- [ ] `slack auth add --xoxc <TOKEN> --xoxd <COOKIE>` (browser tokens)
- [ ] `slack auth list` / `slack auth remove`
- [ ] `slack auth status`
- [ ] Basic error types (JSON-formatted errors)
- [ ] Output mode (JSON default, --plain for TSV)

**Deliverable**: User can authenticate with tokens and see auth status

### Phase 2: Channel Operations

**Goal**: Channel listing and info

- [ ] API client foundation (HTTP, auth headers)
- [ ] Rate limiter implementation
- [ ] `slack channels list`
- [ ] `slack channels info <CHANNEL>`
- [ ] `slack channels dms`
- [ ] Just-in-time channel name resolution (#general → C1234567890)
- [ ] JSON/plain output for channels

**Deliverable**: User can list and query channels

### Phase 3: Message Operations

**Goal**: Read and send messages

- [ ] `slack messages list <CHANNEL>`
- [ ] Time-based limit parsing (1d, 7d, 1m)
- [ ] `slack messages thread <CHANNEL> <THREAD_TS>`
- [ ] `slack messages send <CHANNEL> <TEXT>`
- [ ] `slack messages send --stdin` (read from stdin)
- [ ] Thread reply support (`--thread-ts`)
- [ ] `slack messages search <QUERY>` with filters

**Deliverable**: User can read, send, and search messages

### Phase 4: Users & Remaining Commands

**Goal**: User operations and remaining commands

- [ ] `slack users list`
- [ ] `slack users info <USER>`
- [ ] `slack users me`
- [ ] Just-in-time user name resolution (@john → U1234567890)
- [ ] `slack files get/info/list`
- [ ] `slack reactions add/remove/list`
- [ ] `slack status get/set/clear`
- [ ] `slack reminders list/add/complete/delete`

**Deliverable**: Full command coverage

### Phase 5: OAuth & Edge API

**Goal**: Browser-based OAuth and stealth mode

- [ ] Local HTTP server for OAuth callback
- [ ] Browser opening with `open` crate
- [ ] OAuth state token generation
- [ ] Token exchange implementation
- [ ] `slack auth add` (defaults to OAuth flow)
- [ ] `slack auth add --manual` (no browser)
- [ ] Edge API implementation for browser tokens (xoxc/xoxd)

**Deliverable**: Full authentication support

### Phase 6: Polish & Release

**Goal**: Production-ready CLI

- [ ] Enterprise/GovSlack support
- [ ] Shell completions (bash, zsh, fish)
- [ ] Comprehensive error messages
- [ ] Unit tests for all modules
- [ ] Integration tests with mock server
- [ ] README.md with examples
- [ ] CI/CD pipeline (build, test, release)
- [ ] Homebrew formula

**Deliverable**: Release v1.0.0

---

## 13. Testing Strategy

### 13.1 Unit Tests

```rust
// tests/unit/auth_test.rs

#[test]
fn test_token_validation() {
    assert!(TokenSet::validate_xoxp("xoxp-12345").is_ok());
    assert!(TokenSet::validate_xoxb("xoxb-12345").is_ok());
    assert!(TokenSet::validate_xoxc("xoxc-12345").is_ok());
    assert!(TokenSet::validate_xoxp("invalid").is_err());
}

#[test]
fn test_channel_resolution() {
    let mut cache = ChannelsCache::new();
    cache.insert(Channel { id: "C123".into(), name: Some("general".into()), .. });

    assert_eq!(cache.resolve("#general"), Some("C123"));
    assert_eq!(cache.resolve("C123"), Some("C123"));
    assert_eq!(cache.resolve("general"), Some("C123"));
    assert_eq!(cache.resolve("#nonexistent"), None);
}
```

### 13.2 Integration Tests

```rust
// tests/integration/channels_test.rs

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_channels_list_json() {
    Command::cargo_bin("slack")
        .unwrap()
        .args(["channels", "list", "--json"])
        .env("SLACK_TOKEN", "xoxp-test-token")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"channels\""));
}

#[test]
fn test_auth_required() {
    Command::cargo_bin("slack")
        .unwrap()
        .args(["channels", "list"])
        .env_remove("SLACK_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Authentication required"));
}
```

### 13.3 Mock Server

```rust
// tests/helpers/mock_server.rs

use mockito::{Server, Mock};

pub fn mock_auth_test(server: &mut Server) -> Mock {
    server.mock("GET", "/auth.test")
        .with_status(200)
        .with_body(r#"{
            "ok": true,
            "team_id": "T12345",
            "team": "Test Workspace",
            "user_id": "U12345",
            "user": "testuser"
        }"#)
        .create()
}

pub fn mock_conversations_list(server: &mut Server, channels: &[Channel]) -> Mock {
    server.mock("GET", "/conversations.list")
        .with_status(200)
        .with_body(serde_json::to_string(&json!({
            "ok": true,
            "channels": channels
        })).unwrap())
        .create()
}
```

---

## 14. Future Enhancements

### 14.1 Phase 2 Features (v1.1+)

- **Persistent caching**: Optional disk cache for user/channel name→ID mappings
- **Real-time**: WebSocket connection for live message updates
- **Bulk operations**: Export/import channel history
- **Workflows**: Trigger workflow runs
- **Apps**: Manage Slack apps

### 14.2 Integration Ideas

- **MCP mode**: Run as MCP server for AI agents (reverse the original direction)
- **Slack Connect**: Cross-workspace channel support
- **Enterprise Grid**: Multi-workspace management
- **Natural language**: Optional LLM-based query parsing

### 14.3 Performance Improvements

- **Parallel requests**: Concurrent API calls where possible
- **Connection pooling**: Reuse HTTP connections
- **Optional caching**: Opt-in name→ID cache to reduce API calls

---

## Appendix A: Slack API Reference

### Key Endpoints Used

| Endpoint | Method | Description |
|----------|--------|-------------|
| `auth.test` | GET | Validate token |
| `conversations.list` | GET | List channels |
| `conversations.history` | GET | Get messages |
| `conversations.replies` | GET | Get thread |
| `chat.postMessage` | POST | Send message |
| `reactions.add` | POST | Add reaction |
| `reactions.remove` | POST | Remove reaction |
| `search.messages` | GET | Search (user tokens only) |
| `users.list` | GET | List users |
| `users.info` | GET | Get user |
| `files.info` | GET | File metadata |
| `files.download` | GET | Download file |

### Rate Limits

| Tier | Requests/min | Burst |
|------|-------------|-------|
| Tier 1 | 1 | 1 |
| Tier 2 | 20 | 3 |
| Tier 3 | 50 | 10 |
| Tier 4 | 100 | 20 |

Most endpoints are Tier 2. See [Slack Rate Limits](https://api.slack.com/docs/rate-limits).

---

## Appendix B: Token Scopes

### Recommended OAuth Scopes

```
channels:read        - View basic channel info
channels:history     - View messages in public channels
groups:read          - View private channels
groups:history       - View messages in private channels
im:read              - View DM info
im:history           - View DM messages
mpim:read            - View group DM info
mpim:history         - View group DM messages
users:read           - View users
users:read.email     - View user emails
search:read          - Search messages
files:read           - View files
reactions:read       - View reactions
reactions:write      - Add/remove reactions
chat:write           - Send messages
```

### Minimal Read-Only Scopes

```
channels:read
channels:history
users:read
```

---

## Appendix C: Example Usage

```bash
# Authentication
slack auth add                              # OAuth flow in browser
slack auth add --xoxc TOKEN --xoxd COOKIE   # Browser tokens
slack auth add --token xoxp-...             # Direct token
slack auth list                             # Show workspaces
slack auth status                           # Current auth info

# Channels (JSON output by default)
slack channels list                         # List all channels
slack channels list --types im              # List DMs only
slack channels info C1234567890             # Channel details (prefer IDs)
slack channels info "#general"              # Also works with names

# Messages
slack messages list C1234567890             # Last day of messages
slack messages list C1234567890 -l 7d       # Last week
slack messages list C1234567890 -l 50       # Last 50 messages
slack messages thread C1234567890 1234.5678 # Thread replies
slack messages send C1234567890 "Hello!"    # Send message
slack messages send C1234567890 --stdin     # Read from stdin
echo "Hello" | slack messages send C1234567890 --stdin

# Search (under messages)
slack messages search "quarterly report"
slack messages search "bug" --from U1234567890 --after 2024-01-01
slack messages search "urgent" --in-channel C1234567890

# Users
slack users list                            # All users
slack users info U1234567890                # User details (prefer IDs)
slack users me                              # Current user

# Files
slack files get F12345678 -o report.pdf     # Download file
slack files info F12345678                  # File metadata
slack files list --channel C1234567890      # Files in channel

# Reactions
slack reactions add C1234567890 1234.5678 thumbsup
slack reactions remove C1234567890 1234.5678 thumbsup
slack reactions list C1234567890 1234.5678

# Status
slack status get                            # Show status
slack status set "In a meeting" --emoji calendar --expires 1h
slack status clear

# Output formats (JSON is default)
slack channels list                         # JSON output (default)
slack channels list --plain                 # TSV output for scripting
SLACK_PLAIN=1 slack channels list           # TSV via env
```

### JSON Output Examples

**Successful response:**
```json
{
  "channels": [
    {
      "id": "C1234567890",
      "name": "general",
      "is_private": false,
      "num_members": 42
    }
  ],
  "next_cursor": "dGVhbTpDMDY..."
}
```

**Error response:**
```json
{
  "error": true,
  "code": "channel_not_found",
  "message": "Channel not found: #nonexistent",
  "detail": null
}
```

---

*Document Version: 1.1*
*Last Updated: 2025-02-04*
