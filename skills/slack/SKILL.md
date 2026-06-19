---
name: slack
description: Send and read Slack messages, search conversations, manage channels, users, files, reactions, status, and reminders. Use when the user wants to interact with Slack — post a message, check recent messages, search for something, or manage their workspace.
license: MIT
compatibility: Requires the slack CLI. If not installed, direct the user to https://github.com/TeamCadenceAI/slack-cli
---

# slack

> Install this skill: `npx skills add TeamCadenceAI/slack-cli`

Command-line interface for Slack workspaces, optimized for AI agents and automation.

## Prerequisite

Before using any `slack` command, verify the CLI is installed:

```bash
command -v slack >/dev/null 2>&1 || echo "NOT INSTALLED"
```

If not installed, tell the user to install the Slack CLI from:
https://github.com/TeamCadenceAI/slack-cli

## Output

Output is **JSON by default** — ideal for parsing and automation. Use `--plain` for TSV output.

## Global flags

```
--plain              TSV output instead of JSON
-w, --workspace ID   Target a specific workspace (or set SLACK_WORKSPACE env var)
--token TOKEN        Use a token directly, bypassing the keyring
-v, --verbose        Verbose logging to stderr
```

## Authentication

```bash
# Add a token
slack auth add --token xoxp-your-token

# Add browser tokens (xoxc + xoxd cookie)
slack auth add --xoxc xoxc-... --xoxd xoxd-...

# List authorized workspaces
slack auth list

# Check current auth
slack auth status

# Switch default workspace
slack auth switch T1234567890
```

See [AUTH.md](AUTH.md) for full authentication reference.

## Common usage

### Send a message
```bash
slack messages send "#general" "Hello from the CLI"

# Reply to a thread
slack messages send "#general" "Reply" --thread-ts 1234567890.123456

# Pipe text in
echo "Automated report ready" | slack messages send "#ops" --stdin
```

### Read messages
```bash
# Last 50 messages in a channel
slack messages list "#general"

# Last 7 days
slack messages list "#general" --limit 7d

# Search
slack messages search "deploy failed" --in-channel "#ops"
slack messages search "from:@alice budget"
```

### List channels
```bash
slack channels list
slack channels list --types public_channel,private_channel,im,mpim
slack channels list --sort-popularity --exclude-archived
```

### Look up users
```bash
slack users me          # current authenticated user
slack users list        # all workspace users
slack users info @alice
```

### Set status
```bash
slack status set "In a meeting" --emoji meeting --expires 1h
slack status clear
```

## Command reference files

| File | Commands |
|------|----------|
| [AUTH.md](AUTH.md) | `auth add/list/remove/status/switch/browser-help` |
| [CHANNELS.md](CHANNELS.md) | `channels list/info/dms/export` |
| [MESSAGES.md](MESSAGES.md) | `messages list/send/search/thread/get` |
| [USERS.md](USERS.md) | `users list/info/me/export` |
| [FILES.md](FILES.md) | `files list/info/get` |
| [REACTIONS.md](REACTIONS.md) | `reactions add/remove/list` |
| [STATUS.md](STATUS.md) | `status get/set/clear/presence` |
| [REMINDERS.md](REMINDERS.md) | `reminders list/add/complete/delete` |
