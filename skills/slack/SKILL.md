---
name: slack
description: Send and read Slack messages; DM @users; edit, delete, schedule, and search messages; manage channels, user groups, file uploads, file search, pins, bookmarks, custom emoji, reactions, status, and reminders across workspaces. Use when the user wants to interact with Slack or connect a locally signed-in workspace.
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

## Token store setup (required if not using system keyring)

By default the CLI stores tokens in the system keyring. If the keyring is unavailable
or `slack auth list` returns `[]` despite tokens being present, the CLI is using the
keyring backend and can't find anything. Switch to file-based storage:

```bash
export SLACK_TOKEN_STORE_PATH=~/.slack/tokens.json
```

Set this **before every `slack` command** in the session, or add it to `~/.zshrc` /
`~/.bashrc`. Without it, auth commands will appear to succeed but list nothing.
See [AUTH.md](AUTH.md) for full details.

## Output

Output is **JSON by default** — ideal for parsing and automation. Use `--plain` for TSV output.

## Global flags

```
--plain              TSV output instead of JSON
-w, --workspace VAL  Select workspace by team ID (T…) or domain (myteam /
                     myteam.slack.com). Team names are NOT accepted. Or set
                     the SLACK_WORKSPACE env var.
--token TOKEN        Use a token directly, bypassing the keyring
-v, --verbose        Verbose logging to stderr
```

**Selecting a workspace:** `-w` matches a workspace's **team ID** or **domain**
only (names are too volatile). Get the values from `slack auth list` (columns:
`team_id  domain  name  token_type  default`). Examples:
`slack -w T04U8BDD0KC …`, `slack -w cadence-app …`, `slack -w cadence-app.slack.com …`.

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

# Optional OAuth through a configured Slack app
export SLACK_CLIENT_ID="your-slack-app-client-id"
export SLACK_CLIENT_SECRET="your-slack-app-client-secret"
slack auth add             # default browser OAuth route
slack auth add --oauth     # the same route, selected explicitly
slack auth add --manual    # print URL; paste the full redirect URL

# Check current auth / switch default workspace
slack auth status
slack auth switch T1234567890
```

For OAuth, configure the Slack app redirect URL exactly as
`http://localhost:8765/callback`. Both browser and manual modes use it; manual
mode may show an unreachable localhost page before its URL is pasted into the
CLI. Missing app credentials produce a configuration error. The credentials
above identify the Slack app and are not an access token; resulting tokens are
saved in the same keyring/file store as other methods.

OAuth currently defaults to `channels:read,channels:history,users:read,search:read`.
Use `--scopes scope1,scope2` to replace (not extend) that list; scopes for other
commands are not added automatically. A positional workspace or `--url` uses
local token extraction rather than OAuth. See [AUTH.md](AUTH.md) for the full
authentication reference.

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

**Formatting:** message text is **standard Markdown by default** and is
converted to Slack mrkdwn before sending. Just write normal Markdown —
`**bold**`, `*italic*`, `[text](url)`, `# heading`, and `-`/`1.` lists all work.
Inline/fenced code and existing `<@U…>`/`<url|text>` spans are left untouched.
Use `--format plain` to send text verbatim (no conversion, mrkdwn parsing off).

```bash
slack messages send "#general" "Deploy **failed** on [prod](https://ci/123) — see logs"
```

**Advanced sending and mutations:**

```bash
slack messages send "#general" "Fallback" --blocks blocks.json
slack messages send "#general" "Reply" --thread-ts 1234567890.123456 --broadcast
slack messages send "#general" "Tomorrow" --schedule "tomorrow at 9am"
slack messages edit "#general:1234567890.123456" "Corrected text"
slack messages delete "#general:1234567890.123456"
slack messages permalink "#general:1234567890.123456"
slack messages mark "#general" 1234567890.123456
slack messages scheduled list
slack messages scheduled delete "#general" Q123456789
```

Natural schedule expressions use local time and must be future times within
120 days. `--schedule` conflicts with `--mark-read` and `--broadcast`.
`--blocks -` reads the JSON array from stdin and cannot be combined with
`--stdin`. Immediate JSON send/get permalink enrichment is best-effort and may
produce `permalink: null`; plain output stays stable and skips enrichment. See
[MESSAGES.md](MESSAGES.md).

### Read messages
```bash
# Last 50 messages in a channel
slack messages list "#general"

# Last 7 days
slack messages list "#general" --limit 7d

# Fetch a single message by permalink URL (most reliable)
slack messages get "https://workspace.slack.com/archives/C123/p1234567890123456"

# Read a thread
slack messages thread C1234567890 1234567890.123456

# Search
slack messages search "deploy failed" --in-channel "#ops"
slack messages search "from:@alice budget"
```

Use exclusive UTC `--since`/`--until` bounds on `messages list`; each accepts a
`YYYY-MM-DD` date, RFC3339 instant, or Slack decimal timestamp. `--all` fetches
all pages in response order, conflicts with `--cursor`, and ignores a numeric
`--limit` (its requests use 200-message pages). Add `--resolve-users` to list,
thread, or search to load the complete paginated user directory once and show
resolved authors and mentions. Search ordering is selectable with `--sort
score|timestamp` and `--sort-dir asc|desc`.

### List and manage channels
```bash
slack channels list
slack channels list --types public_channel,private_channel,im,mpim
slack channels list --sort-popularity --exclude-archived
slack channels members "#general" --resolve
slack channels create project-room --private
slack channels invite "#project-room" @alice U123456789
slack channels set-topic "#project-room" "Launch coordination"
slack channels archive "#old-project"
slack channels unread
```

Channel and invite operands resolve names to IDs; invitations fail without
mutating if any user cannot be resolved. Management requires the applicable
Slack scopes and permissions, and archive has no confirmation. Unread counts
are Web-API-only and capability dependent: inspect `unavailable_channels` and
do not treat the overview as complete when Slack omits count fields. See
[CHANNELS.md](CHANNELS.md).

### Look up users
```bash
slack users me          # current authenticated user
slack users list        # all workspace users
slack users info @alice
slack users info alice@example.com
```

### Identity resolution and user groups
```bash
# A leading @ opens or reuses an IM before sending
slack messages send @alice "Can you review this?"

slack users groups list
slack users groups members @engineering
slack users groups members S123456789 --resolve
```

Bare names in message channel position remain channel names; use `@name` or a
U-ID to select a user. Email lookup requires `users:read.email`, groups require
`usergroups:read`, and opening an IM through `conversations.open` generally
requires `im:write` (or the applicable conversation-write scope for the token
type). Missing scopes are reported as Slack API errors. See [USERS.md](USERS.md).

### Manage pins
```bash
slack pins add "#general" 1234567890.123456
slack pins remove "#general" 1234567890.123456
slack pins list "#general"
```

Adding/removing requires `pins:write`; listing requires `pins:read`. See
[PINS.md](PINS.md).

### List custom emoji
```bash
slack emoji list
slack --plain emoji list
```

This lists custom workspace emoji only and requires `emoji:read`. See
[EMOJI.md](EMOJI.md).

### Manage channel bookmarks
```bash
slack bookmarks list "#general"
slack bookmarks add "#general" "Runbook" https://example.com/runbook --emoji :books:
slack bookmarks remove "#general" Bk123456789
```

Listing requires `bookmarks:read`; adding and removing require
`bookmarks:write`. See [BOOKMARKS.md](BOOKMARKS.md).

### Set status
```bash
slack status set "In a meeting" --emoji meeting --expires 1h
slack status clear
```

### Upload and search files
```bash
# Upload; the path basename is used unless --filename is supplied
slack files upload ./report.pdf --channel general --title "Quarterly report"

# --comment and --thread-ts require --channel
slack files upload ./notes.txt --channel C123456789 \
  --comment "Meeting notes" --thread-ts 1234567890.123456

# Search requires a user OAuth or stored browser token (not a bot token)
slack files search "quarterly report" --count 20 --page 1
```

Uploads require `files:write`; searches require `search:read`. See
[FILES.md](FILES.md) for validation, output, and authentication details.

## Command reference files

| File | Commands |
|------|----------|
| [AUTH.md](AUTH.md) | `auth add/discover/list/remove/status/switch/browser-help` |
| [CHANNELS.md](CHANNELS.md) | `channels list/info/dms/members/create/join/leave/archive/unarchive/invite/set-topic/set-purpose/rename/unread/export` |
| [MESSAGES.md](MESSAGES.md) | `messages list/thread/send/edit/delete/permalink/mark/scheduled/search/get` |
| [USERS.md](USERS.md) | `users list/info/me/groups/export`; direct messages by user |
| [FILES.md](FILES.md) | `files list/info/get/upload/search` |
| [REACTIONS.md](REACTIONS.md) | `reactions add/remove/list` |
| [PINS.md](PINS.md) | `pins add/remove/list` |
| [EMOJI.md](EMOJI.md) | `emoji list` |
| [BOOKMARKS.md](BOOKMARKS.md) | `bookmarks list/add/remove` |
| [STATUS.md](STATUS.md) | `status get/set/clear/presence` |
| [REMINDERS.md](REMINDERS.md) | `reminders list/add/complete/delete` |
