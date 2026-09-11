# slack reminders

List and manage reminders for the authenticated Slack user.

## Commands

```bash
slack reminders list
slack reminders add "Review pull requests" --when "in 2 hours"
slack reminders add "Team meeting" --when "tomorrow at 10am"
slack reminders complete Rm123456789
slack reminders delete Rm123456789
```

Reminder times accept supported natural expressions and explicit date/time
forms. Output is JSON by default; add the global `--plain` flag for TSV.
