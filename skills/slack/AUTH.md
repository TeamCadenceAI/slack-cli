# slack auth

Authentication and workspace management.

## Add a workspace

```bash
# Import from a locally logged-in Slack (desktop app or browser) - RECOMMENDED.
# Give just the subdomain or the full URL. Extracts the xoxc token + xoxd cookie
# for that workspace and authorizes it; no manual token copying.
slack auth add onlinegeniuses
slack auth add onlinegeniuses.slack.com
slack auth add myteam --browser slack   # narrow the source app/browser

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

`slack auth add <subdomain>` currently supports macOS (Chromium-family browsers
+ the Slack desktop app). It reads the workspace's locally-stored session, so
you must already be signed into that workspace in one of those apps.

## Discover local workspaces

List the Slack workspaces signed into local apps (desktop app / browsers)
**without** connecting them. This reads local storage only — no Keychain access,
no cookie decryption, no network — unless you pass `--check`.

```bash
slack auth discover                  # all discoverable workspaces
slack auth discover --browser slack  # only the Slack desktop app
slack auth discover --check          # also validate each token is live (network)
slack auth discover --plain          # TSV: team_domain  team_id  team_name  source
```

JSON output: `{ "workspaces": [ { team_id, team_domain, team_name, source[, live] } ], "count": N }`.

Use this to resolve a workspace the user named but that is not connected yet,
then `slack auth add <team_domain>` to connect it (ask the user first).

## List & inspect

```bash
slack auth list           # all authorized workspaces
slack auth list --check   # + validate each token via auth.test (live/expired)
slack auth status         # current workspace auth details
```

`auth list` columns (`--plain`): `team_id  domain  name  token_type  default`.
The **team_id** and **domain** are the values accepted by `-w` / `SLACK_WORKSPACE`
and `auth switch`/`remove` (team names are not matched).

`auth list --check` validates each stored token via `auth.test` and adds a
`live` boolean (JSON) / trailing `live|expired` column (`--plain`) so you can
tell which tokens still work.

## Selecting a workspace

```bash
slack -w T04U8BDD0KC channels list          # by team ID
slack -w cadence-app channels list          # by domain (subdomain)
slack -w cadence-app.slack.com channels list  # full URL accepted too
export SLACK_WORKSPACE=cadence-app           # session default
```

## Switch & remove

```bash
slack auth switch cadence-app     # set default workspace (team ID or domain)
slack auth remove T1234567890     # remove a workspace (team ID or domain)
```

## Token types

| Prefix | Type | Notes |
|--------|------|-------|
| `xoxp-` | User OAuth | Full user access including search |
| `xoxb-` | Bot | Limited — no search, no DMs |
| `xoxc-` | Browser session | Requires `xoxd` cookie, full access. Auto-extracted by `auth add <subdomain>` / `auth discover`. |

## Environment variables

| Variable | Purpose |
|----------|---------|
| `SLACK_TOKEN` | Override token for all commands |
| `SLACK_WORKSPACE` | Default workspace (team ID or domain, same as `-w`) |
| `SLACK_TOKEN_STORE_PATH` | Use a JSON file instead of system keyring |
