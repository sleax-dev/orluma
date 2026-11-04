# Changelog

## 0.1.5

- "What should I back up?" starts with nothing ticked: nothing is chosen for you.
- Folders are a tree. Open any folder (/root, /var/www, ...) to see what is inside and tick single folders or files at any level; ticking a folder takes everything in it. Sizes fill in as you open things.
- New file backups skip what never needs backing up: node_modules, build output and caches (.cache, .next, target/debug, .gradle, __pycache__, virtualenvs, ...), editor servers, trash, temp files and system folders (/proc, /sys, /tmp, /var/cache, Docker overlay layers). The list is shown in the dialog and stays editable in the job.

## 0.1.4

- "What should I back up?" is fast: the list of databases and folders appears at once and folder sizes fill in afterwards (measuring a big folder such as /root took 25 seconds on a cold disk, and the whole window waited for it). Folders too big to measure say so.
- The window no longer repeats its look-up. Reading the settings used to restart it, and each round started more `du` commands on the server, which could push its load very high. The agent now runs one look-up at a time, measures at low priority, and stops commands that pass their time limit.
- Safer defaults: only settings and websites are ticked. Home folders and app data are often caches and can be huge, so they are opt-in, and the window warns when what is ticked will not fit in the free space of the backup disk.
- The lists in the window showed only the first rows when the window was short (they were squeezed instead of scrolling). They keep their full height now, here and in the destination, job and alert editors.
- Diagnostics (Settings): a log of this app and of each server's agent, with warnings and errors filtered, and "Copy report" / "Save report" that put both logs, server states and last runs into one text to send. Passwords, keys, tokens and webhook secrets are removed before anything is written, shown or saved. The app logs every server call with its time (slow ones as warnings), failures, panics and interface errors; the agent logs requests, runs, alerts and updates to `/var/lib/orluma/agent.log` (rotated) and journald.
- Storage statistics are kept for five minutes instead of re-reading every snapshot each time the page opens.

## 0.1.3

- Fixed "What should I back up?" looking at the server over and over: reading the settings restarted the look-up, and every round started more `du` commands on the server, which could push its load very high. The window now looks once, the agent runs one look-up at a time, and a command that runs past its time limit is stopped.

## 0.1.2

- First run is guided: the Overview opens with a short "Set up your backups" list (connect a server, choose what to back up, run the first backup, optionally keep a copy on this PC) with one button on the current step. Pages that need a server say so and point back to step 1 instead of showing nothing.
- Explanations moved behind small (i) tooltips; page descriptions are one short line.
- Transfer page: tabs (servers and clouds, PC to server, another PC) with a from-to layout; Send and Receive panels no longer stretch into empty space.
- Server rows show the system and free space; agent, restic, uptime and host key sit behind a tooltip.
- Removed the light glow at the top of the window. Icons inside buttons on empty screens are no longer drawn in the accent colour.

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
