# slack pins

Manage pinned items in a Slack conversation. Channel names (with or without `#`)
and channel, group, or DM IDs are accepted.

## Add a message pin

```bash
slack pins add "#general" 1234567890.123456
slack pins add C123456789 1234567890.123456
```

Requires `pins:write`. Success JSON contains `ok`, the resolved `channel`, and
`ts`; `--plain` prints only the timestamp. Slack errors such as
`already_pinned` are returned unchanged.

## Remove a message pin

```bash
slack pins remove "#general" 1234567890.123456
```

Requires `pins:write`. Slack errors such as `not_pinned` are returned unchanged.

## List pins

```bash
slack pins list "#general"
slack --plain pins list C123456789
```

Requires `pins:read`. JSON preserves every item returned by Slack, including
message, file, and file-comment records and their optional metadata, in API
order. Plain output is TSV:

```text
type<TAB>message-ts-or-file-id<TAB>author-id<TAB>text-or-title
```

Missing fields are empty, and an empty list produces no plain rows.
