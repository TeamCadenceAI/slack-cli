# slack status

Read or update the authenticated user's Slack status and presence.

## Commands

```bash
slack status get
slack status set "In a meeting" --emoji meeting
slack status set "Out of office" --emoji calendar --expires tomorrow
slack status clear
slack status presence away
slack status presence auto
```

`--expires` accepts durations such as `30m`, `1h`, and `4h`, plus `today` or
`tomorrow`. Emoji names are supplied without surrounding colons. Output is JSON
by default; add the global `--plain` flag for TSV.
