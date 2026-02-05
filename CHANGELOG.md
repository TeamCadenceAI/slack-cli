# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/user/slack-cli/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/user/slack-cli/releases/tag/v0.1.0
