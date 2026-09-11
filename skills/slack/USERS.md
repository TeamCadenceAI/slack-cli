# slack users

Inspect workspace identities and user groups. Output is JSON by default; add
the global `--plain` flag for TSV intended for scripts.

## List and inspect users

```bash
slack users list
slack users list --include-deactivated --limit 200
slack users info @alice
slack users info U123456789
slack users me
```

Names and display names are resolved through `users.list`. Valid U-IDs are used
directly. `users info` then fetches the complete user record.

## Look up by email

```bash
slack users info alice@example.com
slack users info alice+alerts@example.com --plain
```

A trimmed value with nonempty text on both sides of `@` is sent directly to
`users.lookupByEmail`; the returned user is printed without a redundant
`users.info` request. The token needs `users:read.email`. Lookup failures,
including missing scopes, are returned unchanged and do not fall back to a
workspace-wide username search.

## Send a direct message

```bash
slack messages send @alice "Hello"
slack messages send U123456789 "Hello"
```

In message channel position, a leading `@` or a valid U-ID resolves the user,
calls `conversations.open`, and sends to the returned IM channel. This can
create/open an IM and is therefore a write-side effect even before the message
is posted. Bare `alice` remains a channel name; existing C/D/G channel IDs are
never reinterpreted as users. Direct IM opening generally requires `im:write`
(or the applicable conversation-write scope for the Slack token type).

## User groups

```bash
# Enabled groups with member counts
slack users groups list

# Resolve an exact handle (the @ is optional)
slack users groups members @engineering
slack users groups members engineering --plain

# An S-ID avoids the handle lookup
slack users groups members S123456789

# Resolve member IDs with one paginated users.list traversal
slack users groups members S123456789 --resolve
```

User-group operations require `usergroups:read`. `groups list` JSON is
`{"usergroups":[...]}`; plain output columns are `id`, `handle`, `name`, and
`user_count`. `groups members` emits `{"usergroup":"S…","members":[...]}` or
one ID per line. With `--resolve`, members are `{id,user_name}` objects, and
plain output is `id<TAB>user_name`. Resolution prefers the Slack username,
then the display name, and finally the ID. It does not make one `users.info`
request per member.

## Export

```bash
slack users export --output users.csv
slack users export --include-deactivated
```
