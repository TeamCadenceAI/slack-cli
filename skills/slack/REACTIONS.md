# slack reactions

Add, remove, and inspect emoji reactions on Slack messages. Channel names and
IDs are accepted; emoji names are passed without surrounding colons.

## Commands

```bash
slack reactions add "#general" 1234567890.123456 thumbsup
slack reactions remove "#general" 1234567890.123456 thumbsup
slack reactions list "#general" 1234567890.123456
```

Output is JSON by default. Add the global `--plain` flag for TSV output. Slack
enforces access to the conversation and the token's reaction scopes.
