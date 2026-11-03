# Changelog

## 0.1.1

- The macOS app bundle is built and published, so Macs can update themselves.
- The progress bar animates smoothly.
- Restore a database backup straight into a running database (PostgreSQL, MySQL/MariaDB, SQLite, MongoDB, ClickHouse). It goes into a new database by default; replacing an existing one needs a tick, and a replaced SQLite file is kept next to the new one.
- Transfer page: copy snapshots between destinations and servers, load a backup kept on this PC onto a server, and send backups to another PC on the same network (pairing code, SPAKE2, encrypted and verified).
- MariaDB 11 containers work again (their dump tools only have the new names).
- Clearer icons: one tinted mark per database engine, a bell for alerts, a server icon that no longer looks like a phone.

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
