# slack messages

Read, send, and search Slack messages. Alias: `slack m`, `slack msg`

## List messages in a channel

```bash
# Last 50 messages (default)
slack messages list "#general"
slack messages list C1234567890      # by channel ID

# By count
slack messages list "#general" --limit 100

# By time period
slack messages list "#general" --limit 7d
slack messages list "#general" --limit 1m    # 1 month
slack messages list "#general" --limit 90d

# Include join/leave/topic activity messages (filtered out by default)
slack messages list "#general" --include-activity

# Paginate
slack messages list "#general" --cursor <cursor_from_response>
```

## Get a single message

The most reliable way to fetch a specific message — accepts a Slack permalink URL
or `channel:timestamp` format:

```bash
# By permalink URL (copy from Slack: right-click message → Copy link)
slack messages get "https://workspace.slack.com/archives/C1234567890/p1234567890123456"

# By channel:timestamp
slack messages get "C1234567890:1234567890.123456"
slack messages get "#general:1234567890.123456"
```

The permalink URL format `p1234567890123456` is automatically converted to the
`1234567890.123456` timestamp format the API expects.

## Read a thread

```bash
slack messages thread CHANNEL THREAD_TS

# Examples
slack messages thread C1234567890 1234567890.123456
slack messages thread "#general" 1234567890.123456

# With limit
slack messages thread C1234567890 1234567890.123456 --limit 50
```

**THREAD_TS format:** use the dot-separated timestamp (`1234567890.123456`), not the
URL `p` format. To get the thread_ts from a permalink URL, strip the `p` prefix and
insert a `.` after the 10th digit:
`p1781832228649249` → `1781832228.649249`

## Send a message

```bash
# Basic send
slack messages send "#general" "Hello from the CLI"

# Reply to a thread
slack messages send "#general" "Reply text" --thread-ts 1234567890.123456

# Pipe text from stdin
echo "Automated report ready" | slack messages send "#ops" --stdin
cat report.txt | slack messages send "#ops" --stdin

# Plain text (disable Slack markdown)
slack messages send "#general" "Hello" --format plain

# Mark channel as read after sending
slack messages send "#general" "Hello" --mark-read
```

## Search messages

Requires a **user token** (`xoxp-`) — search is not available with bot or browser tokens.

```bash
# Basic search
slack messages search "deploy failed"

# In a specific channel
slack messages search "budget" --in-channel "#finance"

# From a specific user
slack messages search "standup" --from "@alice"

# Mentioning a user
slack messages search "review" --with @bob

# Date range (YYYY-MM-DD)
slack messages search "incident" --after 2026-01-01 --before 2026-06-01

# In a DM
slack messages search "meeting" --in-dm "@alice"

# Threads only
slack messages search "decision" --threads-only

# Pagination
slack messages search "query" --count 50 --page 2
```

## Output

All commands output JSON by default. Use `--plain` (global flag) for TSV:

```bash
slack --plain messages list "#general"
slack --plain messages search "hello"
```

## Resolving user IDs in output

Message JSON includes `user` as a Slack user ID (e.g. `U090BKEQXMH`). Resolve it
to a display name with the CLI:

```bash
slack users info U090BKEQXMH               # full user record (JSON)
slack --plain users info U090BKEQXMH       # TSV
```

Note: with browser tokens, `real_name` may be empty — prefer the profile's
`display_name`, falling back to `real_name`.
