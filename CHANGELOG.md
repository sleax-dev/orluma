# Changelog

## 0.1.0

First release.

- Desktop app for Windows, macOS and Linux; agent for Linux servers (x86-64, ARM64) with systemd.
- Adding a server over SSH with a pinned host key; the password is used once to authorize the app's own key.
- Sources: folders, PostgreSQL, MySQL/MariaDB, MongoDB, Redis, SQLite, ClickHouse, command output. Discovery of running databases and common folders.
- Destinations: server disk, SFTP, S3-compatible, Backblaze B2, Azure, Google Cloud Storage, restic REST, rclone.
- Schedules (presets and cron, time zones, catch-up), retention, scheduled verification and prune, hooks, health-check pings, copies to further destinations.
- A repository on the PC that servers copy into through a reverse SSH tunnel.
- Restore: browse snapshots, download files or folders (zip), restore to the server.
- Alerts: Discord, Telegram, Slack, Microsoft Teams, email, ntfy, Gotify, Pushover, signed webhooks; desktop notifications.
- Recovery kit, signed self-updates for the app and the agents, ten languages.
