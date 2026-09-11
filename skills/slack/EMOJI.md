# slack emoji

List custom emoji configured for a Slack workspace.

## List custom emoji

```bash
slack emoji list
slack --plain emoji list
slack -w cadence-app emoji list
```

This command requires `emoji:read`. JSON output maps each custom emoji name to
its Slack image URL or unchanged `alias:<name>` value. Plain output is sorted by
name and uses `name<TAB>value` rows.

The command does not download images or expand aliases. Slack's built-in
Unicode emoji are not included; `emoji list` reports workspace custom emoji
only.
