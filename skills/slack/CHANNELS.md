# slack channels

Channel listing, membership, lifecycle management, unread overview, and export. Alias: `slack c`

## List channels

```bash
slack channels list

# Filter by type (default: public_channel,private_channel)
slack channels list --types public_channel,private_channel,im,mpim

# Sort by member count
slack channels list --sort-popularity

# Exclude archived
slack channels list --exclude-archived

# Limit results
slack channels list --limit 50
```

## Get channel info

```bash
slack channels info "#general"
slack channels info C1234567890    # by channel ID
```

## Direct messages

```bash
slack channels dms    # list all DM conversations
```

## Channel members

```bash
slack channels members "#general"             # one user ID per line with --plain
slack channels members "#general" --resolve   # resolve to id + user_name
```

The channel operand may be a name or ID. Member pages are fetched
automatically and duplicate IDs are removed without changing order.
`--resolve` loads `users.list` once; it prefers username, then display name,
then the ID for unresolved or unnamed users. The token needs access to the
conversation and its applicable read scope; resolution also needs
`users:read`.

## Channel lifecycle

```bash
slack channels create project-room
slack channels create leadership --private
slack channels join "#project-room"
slack channels invite "#project-room" @alice U123456789
slack channels set-topic "#project-room" "Quarterly launch"
slack channels set-purpose "#project-room" "Launch coordination"
slack channels rename "#project-room" launch-room
slack channels leave "#launch-room"
slack channels archive "#launch-room"
slack channels unarchive "#launch-room"
```

All channel operands resolve names or IDs, including archived names. Invite
operands resolve to user IDs and are deduplicated in argument order before a
single API call. Resolution is all-or-nothing: one unknown user prevents the
invite entirely. Slack enforces the applicable management/join/invite scopes,
conversation membership, admin permissions, naming rules, and text limits.
Archiving interrupts normal channel use and has no confirmation prompt.

An empty topic or purpose clears it:

```bash
slack channels set-topic "#project-room" ""
slack channels set-purpose "#project-room" ""
```

Mutation JSON is `{"ok":true,"channel":...}`. With `--plain`, every mutation
prints only its channel ID.

## Unread overview

```bash
slack channels unread
slack --plain channels unread
```

Unread overview uses only Web API calls. It checks joined public/private
channels and all listed DMs/group DMs, preferring `unread_count_display` over
`unread_count`, and shows only positive counts. Slack does not expose these
fields to every workspace or token. The result is therefore capability
dependent, not guaranteed complete; JSON reports missing-count IDs in
`unavailable_channels`, while plain mode emits one warning. If no eligible
conversation exposes count information, the command returns
`unread_unavailable`. Applicable conversation read scopes are required; there
is no browser/Edge fallback.

## Export

```bash
slack channels export --output channels.csv
```
