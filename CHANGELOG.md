# Changelog

## 0.1.8

- Fixed backups failing at once with "wrong password or no key found" after "What should I back up?" was used twice. Each run made a new destination with a new password in the same folder, so the repository the first run created could not be opened by the second. A second run now reuses the destination that already has that folder, and if the folder holds a backup this app has no password for, it says so before creating anything.
- Two destinations can no longer point at the same place: the destination editor names the one already there and does not save a second.
- A backup that already failed this way shows "Use <destination>" under the error: it checks that destination's password really opens the repository, then points the backup at it. No password is copied or shown.
- The agent's error now says what "wrong password" usually means here (another destination using the same folder).
- The window has a fixed size, like Zanrei: no maximize button, no resizing by the edges or corners. It still shrinks to fit small screens when it opens.

## 0.1.7

- Sizes in "What should I back up?" no longer count what the backup skips: a project folder is measured without its node_modules, caches and so on, so "30 GB" is what would really be stored. Measuring is also much faster in trees full of node_modules, and the free-space warning is accurate.
- More is recognised as rebuildable and skipped: Python virtualenvs of any name (found by their pyvenv.cfg, not by the name `venv`), folders holding a `.nobackup` file, Node/Bun/pnpm/Yarn stores and caches, nvm/pyenv/rbenv/sdkman/conda installs, `.terraform`, `.dart_tool`, Vite/Astro/Expo caches, ccache, composer and pub caches. `node_modules` was already skipped at any depth. The folder tree marks all of these as skipped, with the same rules the backup uses.
- Jobs made before this version can add the recommended skips with one button under "Leave out".

## 0.1.6

- "What should I back up?" is a page of its own with three steps (databases, folders, where to) instead of a cramped dialog. The folder tree uses the whole page, Back/Next stay in reach, and the step is skipped when the server has no database.
- The window can be resized again by its edges and corners on Windows and Linux (it has no frame of its own, so it had no handles).

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
