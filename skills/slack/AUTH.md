# slack auth

Authentication and workspace management.

## Add a workspace

```bash
# Direct token (user or bot)
slack auth add --token xoxp-your-token
slack auth add --token xoxb-your-bot-token

# Browser tokens (full workspace access without creating a Slack app)
slack auth add --xoxc xoxc-... --xoxd xoxd-...

# OAuth flow (opens browser)
slack auth add --oauth

# Manual OAuth (no browser — prints URL for you to visit)
slack auth add --oauth --manual
```

Run `slack auth browser-help` for step-by-step instructions on extracting browser tokens.

## List & inspect

```bash
slack auth list       # all authorized workspaces
slack auth status     # current workspace auth details
```

## Switch & remove

```bash
slack auth switch T1234567890   # set default workspace by team ID
slack auth remove T1234567890   # remove a workspace
```

## Token types

| Prefix | Type | Notes |
|--------|------|-------|
| `xoxp-` | User OAuth | Full user access including search |
| `xoxb-` | Bot | Limited — no search, no DMs |
| `xoxc-` | Browser session | Requires `xoxd` cookie, full access |

## Environment variables

| Variable | Purpose |
|----------|---------|
| `SLACK_TOKEN` | Override token for all commands |
| `SLACK_WORKSPACE` | Default workspace team ID |
| `SLACK_TOKEN_STORE_PATH` | Use a JSON file instead of system keyring |
