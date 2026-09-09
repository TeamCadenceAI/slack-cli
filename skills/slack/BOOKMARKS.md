# slack bookmarks

Manage link bookmarks in a Slack channel. Channel names such as `#general` are
resolved to channel IDs before the bookmark API is called.

## List bookmarks

```bash
slack bookmarks list <channel>
slack bookmarks list "#general"
slack --plain bookmarks list C123456789
```

JSON output is `{ "channel": "<id>", "bookmarks": [...] }` and retains optional
bookmark metadata returned by Slack. Plain output has one bookmark per line as
`id<TAB>title<TAB>link<TAB>emoji`, in API order. Missing emoji produce an empty
final column. This command requires `bookmarks:read`.

## Add a bookmark

```bash
slack bookmarks add <channel> <title> <link> [--emoji <emoji>]
slack bookmarks add "#general" "Runbook" https://example.com/runbook
slack bookmarks add C123456789 "Team docs" https://example.com/docs --emoji books
slack bookmarks add C123456789 "Team docs" https://example.com/docs --emoji :books:
```

Links must be absolute HTTP(S) URLs without URL user information. Emoji names
may have surrounding colons; both `books` and `:books:` are sent to Slack as
`:books:`. JSON output is
`{ "ok": true, "channel": "<id>", "bookmark": {...} }`; `--plain` prints the
returned bookmark ID. This command requires `bookmarks:write`.

## Remove a bookmark

```bash
slack bookmarks remove <channel> <bookmark_id>
slack bookmarks remove "#general" Bk123456789
```

JSON output is
`{ "ok": true, "channel": "<id>", "bookmark_id": "<bookmark_id>" }`;
`--plain` prints the removed bookmark ID. This command requires
`bookmarks:write`. Missing bookmarks and permission errors are returned as
Slack API errors.
