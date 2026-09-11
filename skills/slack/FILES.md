# slack files

Work with Slack files. JSON is the default output; pass the global `--plain`
flag before `files` for script-friendly output.

## Upload a file

```bash
slack files upload <path> [--channel <channel>] [--title <title>] \
  [--comment <text>] [--thread-ts <ts>] [--filename <filename>]
```

Examples:

```bash
# Upload without sharing it to a channel
slack files upload ./report.pdf

# Resolve a channel name, share the file, and set its display title
slack files upload ./report.pdf --channel general --title "Quarterly report"

# Share in a thread with an initial comment
slack files upload ./notes.txt --channel C123456789 \
  --comment "Meeting notes" --thread-ts 1234567890.123456
```

Uploads require Slack's `files:write` scope and use the supported external
upload flow (`files.getUploadURLExternal`, raw byte upload, then
`files.completeUploadExternal`). The path must name a regular, readable file;
stdin and directories are not accepted. Uploads are not subject to the CLI's
5 MiB download limit.

The uploaded filename defaults to the path's UTF-8 basename. Use `--filename`
to override it or when the path has no usable UTF-8 basename. The override must
be a standalone, non-empty filename without path separators or control
characters. `--comment` and `--thread-ts` are valid only with `--channel`.

JSON output has the form:

```json
{"ok": true, "files": [{"id": "F123456789"}]}
```

Plain output writes one returned file ID per line.

## Search files

```bash
slack files search <query> [--count <n>] [--page <n>]
```

`--count` defaults to 20 and accepts 1 through 100. `--page` defaults to 1 and
must be positive.

```bash
slack files search "quarterly report"
slack files search "from:alice has:pdf" --count 50 --page 2
```

Search uses `search.files`, requires `search:read`, and is available only with
a user OAuth token or a stored browser token. Slack does not support search
with bot tokens. JSON output preserves Slack's `total`, `pagination`, and file
matches. Plain output is tab-separated:

```text
id<TAB>title-or-name<TAB>permalink
```

Missing titles, names, or permalinks are emitted as empty fields. Tabs and line
breaks inside fields are escaped.

## Existing file commands

```bash
slack files list [--channel <channel>] [--user <user>] [--limit <n>] [--cursor <page>]
slack files info <file-id>
slack files get <file-id> [--output <path>] [--base64]
```

The 5 MiB safety limit applies to downloads performed by `files get`, not to
`files upload`.
