---
name: slack
description: Send and read Slack messages, search conversations, manage channels, users, files, reactions, status, and reminders across multiple workspaces. Use when the user wants to interact with Slack — post a message, check recent messages, search for something, or work with a specific workspace/team by name. Can discover and connect workspaces the user is already signed into locally (desktop app or browser).
license: MIT
compatibility: Requires the slack CLI. If not installed, direct the user to https://github.com/TeamCadenceAI/slack-cli
allowed-tools: Bash(slack:*) Bash(jq:*)
disable-model-invocation: true
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
# Import a workspace you're already signed into locally (Slack desktop app or a
# browser) - no manual token copying. Give just the subdomain or full URL:
slack auth add onlinegeniuses
slack auth add onlinegeniuses.slack.com

# Discover which workspaces are signed into local apps (local read only)
slack auth discover

# List authorized workspaces (add --check to verify each token is live)
slack auth list
slack auth list --check

# Add a token / browser tokens directly
slack auth add --token xoxp-your-token
slack auth add --xoxc xoxc-... --xoxd xoxd-...

# Check current auth / switch default workspace
slack auth status
slack auth switch T1234567890
```

See [AUTH.md](AUTH.md) for full authentication reference.

## Resolving a workspace by name (IMPORTANT for agents)

When the user refers to a workspace by name ("check the latest posts from **Online
Geniuses**"), do **not** assume it is connected. Resolve it in this order:

1. **Check already-authorized workspaces.** `slack auth list` returns
   `team_id`, `team_name`, and `token_type`. Match the user's name against
   `team_name` (case-insensitive) or the team ID. If found, use its `team_id`
   with `-w <team_id>` and proceed.
2. **If not connected, discover local sessions.** `slack auth discover` lists
   workspaces signed into the desktop app / browsers (metadata only, no network,
   no Keychain). Match the user's name against `team_name` or `team_domain`.
3. **If discoverable but not connected, ASK before connecting.** Tell the user:
   *"'Online Geniuses' is signed into your Slack desktop app but not connected to
   the CLI yet. Connect it?"* On yes, run `slack auth add <team_domain>`, then use
   the returned `team_id`.
4. **If not found anywhere,** say so and point them to `slack auth add` /
   `slack auth discover`.

```bash
# Example: "check the 5 latest posts from Online Geniuses"

# 1. Is it already connected? (match name -> team_id)
slack auth list --plain | grep -i "online geniuses"

# 2. Not connected -> is it available locally?
slack auth discover --plain | grep -i "online geniuses"
#   onlinegeniuses  T02LMATJK  Online Geniuses  Slack/Default

# 3. With the user's OK, connect it (returns team_id T02LMATJK):
slack auth add onlinegeniuses

# 4. Now read from it via its team_id:
slack -w T02LMATJK channels list --plain
slack -w T02LMATJK messages list "#general" --limit 5
```

`auth discover` columns (`--plain`): `team_domain  team_id  team_name  source`
(plus a `live` column when `--check` is passed). In JSON, each entry has
`team_id`, `team_domain`, `team_name`, `source`.

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
| [AUTH.md](AUTH.md) | `auth add/discover/list/remove/status/switch/browser-help` |
| [CHANNELS.md](CHANNELS.md) | `channels list/info/dms/export` |
| [MESSAGES.md](MESSAGES.md) | `messages list/send/search/thread/get` |
| [USERS.md](USERS.md) | `users list/info/me/export` |
| [FILES.md](FILES.md) | `files list/info/get` |
| [REACTIONS.md](REACTIONS.md) | `reactions add/remove/list` |
| [STATUS.md](STATUS.md) | `status get/set/clear/presence` |
| [REMINDERS.md](REMINDERS.md) | `reminders list/add/complete/delete` |
