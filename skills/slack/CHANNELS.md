# slack channels

Channel listing, info, and export. Alias: `slack c`

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

## Export

```bash
slack channels export --output channels.csv
```
