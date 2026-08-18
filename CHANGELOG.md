# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.2] - 2026-08-18

### Fixed

- **Web API requests now use form encoding**: all Web API calls were sent as
  JSON bodies, which Slack silently ignores on several endpoints when using
  browser (xoxc) tokens — `messages thread` and `messages search` failed with
  `invalid_arguments: missing required field`. Requests are now sent as
  `application/x-www-form-urlencoded` (the canonical encoding for the Slack
  Web API); nested values such as `blocks`/`attachments` are encoded as JSON
  strings within form fields. The Edge API is unchanged (it expects JSON).
- **Payload deserialization failures are no longer masked as `missing_data`**:
  responses are parsed as raw JSON first (checking `ok`/`error`), and payload
  parse failures now return a `parse_error` with the underlying serde message
  (the offending JSON is logged with `--verbose`) instead of the misleading
  `missing_data: Response was ok but contained no data`.
- **One unparseable message no longer blanks an entire page**: message lists
  (`conversations.history`, `conversations.replies`, `search.messages`) are
  deserialized element-by-element; elements that fail to parse are skipped
  with a warning on stderr while the rest are returned. stdout stays
  machine-readable JSON.
- **Huddle/system messages parse correctly**: `Message.channel` now accepts
  both the `{id, name}` object form (search results) and the bare channel ID
  string form (e.g. `slack_system.huddle.started` messages), which previously
  failed deserialization and caused `messages list` to fail on affected pages.

### Changed

- CLI tests are now hermetic: they point `SLACK_TOKEN_STORE_PATH` at a
  nonexistent file and clear `SLACK_TOKEN`/`SLACK_WORKSPACE`, so they no
  longer read the developer's real keychain.

## [0.1.1] - 2026-06-25

### Fixed

- **Token storage on macOS (and all platforms)**: `keyring` v3 compiles no
  credential backend unless a platform feature is enabled. The dependency was
  declared without any features, so the CLI silently fell back to keyring's
  non-persistent in-memory mock store — tokens appeared to save but vanished
  between invocations. Enabled the native backends so tokens persist:
  - macOS/iOS: `apple-native` (Keychain via `security-framework`)
  - Windows: `windows-native` (Credential Manager)
  - Linux: `sync-secret-service` (Secret Service / gnome-keyring / KWallet)
- **Graceful degradation when the keyring is unavailable**: token *read*
  operations now treat an unreachable/inaccessible platform store (e.g.
  headless Linux with no Secret Service daemon, or a locked backend) as "no
  credentials stored", so commands cleanly report `auth_required` instead of a
  raw platform error. Writes still fail hard so a token is never silently
  dropped. Set `SLACK_TOKEN_STORE_PATH` to use file-based storage in headless
  environments.

### Changed

- Keyring backend features are now gated per-platform via `[target.*]`
  dependency tables so each release target only pulls the credential store it
  can use. The Linux backend uses the `vendored` feature to build libdbus from
  source, so builds (including the static musl target) don't require
  `libdbus-1-dev` on the host.
- The `--version` integration test now asserts against `CARGO_PKG_VERSION`
  instead of a hard-coded version string.

## [0.1.0] - 2024-02-05

### Added

- **Authentication**
  - OAuth token support (xoxp- user tokens, xoxb- bot tokens)
  - Browser token support (xoxc- with xoxd cookie)
  - Secure token storage in system keyring
  - Multi-workspace support with workspace switching
  - `auth add`, `auth list`, `auth status`, `auth switch`, `auth remove` commands
  - `auth browser-help` for extracting browser tokens

- **Channels**
  - List channels with filtering by type (public, private, mpim, im)
  - Get channel info by ID or name
  - List direct messages
  - Export channels to CSV
  - Sort by popularity, exclude archived

- **Messages**
  - List messages in channels with count or time-based limits
  - Send messages to channels or threads
  - Read message text from stdin
  - View thread replies
  - Search messages with advanced query syntax
  - Get specific messages by ID or permalink
  - Mark channels as read

- **Users**
  - List workspace users with active-only filter
  - Get current user info (`users me`)
  - Get user info by ID or username
  - Export users to CSV

- **Files**
  - List files with filters (channel, user, types)
  - Get file info
  - Download files with size limits

- **Reactions**
  - Add reactions to messages
  - Remove reactions from messages
  - List reactions on messages

- **Status**
  - Get current status and presence
  - Set status with emoji, text, and expiration
  - Clear status
  - Set presence (away/auto)

- **Reminders**
  - List reminders
  - Create reminders with natural language time
  - Complete and delete reminders

- **Output**
  - JSON output by default (agent-friendly)
  - Plain TSV output with `--plain` flag
  - Structured error responses with error codes

- **Shell Completions**
  - Bash completions
  - Zsh completions
  - Fish completions
  - PowerShell completions

- **API Features**
  - Rate limiting with automatic retry
  - Configurable API base URL for testing
  - Channel and user name resolution
  - Edge API support for browser tokens

### Technical

- Built with Rust for performance and safety
- Comprehensive test suite (unit + integration)
- CI/CD workflows for testing and releases
- No external config files required

[Unreleased]: https://github.com/TeamCadenceAI/slack-cli/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/TeamCadenceAI/slack-cli/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/TeamCadenceAI/slack-cli/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/TeamCadenceAI/slack-cli/releases/tag/v0.1.0
