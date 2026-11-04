# Changelog

## 0.1.9

- **Google Drive, OneDrive and Dropbox in one click.** Storage → Add destination → pick one and press Connect: the sign-in page opens in your browser, and that is all. Orluma installs rclone on the server (a pinned, checksum-verified download, like restic), keeps the sign-in there in a root-only file, and never brings the token to this PC. Backups are encrypted before they reach the account.
- **Restore a database wherever it is now.** "Restore to database" lists the databases running on the server (services and Docker containers, with their logins) and lets you pick where the backup goes, so a backup moved to a new server loads into that server's database. Before, it could only go back to the database it came from, and MongoDB in Docker could not be restored at all.
- **MongoDB in Docker is backed up with its login.** "What should I back up?" left the user and password out for MongoDB containers, so the backup failed with "requires authentication". The URI now goes to `mongodump`/`mongorestore` through a config file, never on the command line.
- **Database passwords are no longer visible in the process list.** For databases in Docker the password used to be part of the `docker exec` command, which every user on the server can read with `ps`. It now travels only in the environment.
- **Failed dumps say why.** The dump tool's own message ("requires authentication", "access denied") is shown instead of restic's "context canceled", with any password in it masked; run logs and errors are masked the same way before they are stored or sent as alerts.
- New file backups no longer skip folders named `.Trash`: in a mail server's Maildir that is the mailbox's Trash folder, which is real mail. Only the desktop trash (`.local/share/Trash`) is skipped. Jobs made with 0.1.5 to 0.1.8 have `.Trash` in "Leave out"; remove that line to back those folders up.
- **Redis backups work again with Redis 7.** `redis-cli --rdb /dev/stdout` fails there ("Fail to fsync"); Redis 7 now streams with `--rdb -`, older versions through a temporary file that is removed after.
- **ClickHouse's password is no longer visible in the process list** either: it goes to `clickhouse-client` through the environment.
- **Cancelling a backup no longer leaves the repository locked.** The next run's cleanup used to stop at "the unlock command can be used to remove stale locks"; locks left by a killed run (cancel, reboot, power cut) are now removed before retention, and only those.
- A failed backup says why in its summary (Backups page, alerts, webhooks): "1 of the job's sources could not be backed up (…). paths: not found on the server: /srv/app" instead of only the count.
- An SFTP destination whose server turns the agent's key away says so, instead of "server unexpectedly closed connection".
- The recovery kit explains how to reach a Google Drive, OneDrive or Dropbox destination without Orluma.
- Releases: the server agent is published even when a macOS build cannot get a GitHub runner, so servers can always install and update to the new version.

Tested end to end against a real agent (117 checks): every source kind (folders, PostgreSQL with roles and every database, MariaDB with views and procedures, MongoDB, Redis, SQLite, ClickHouse, command output) into a local disk, S3-compatible storage, a restic REST server and SFTP; browsing, single-file and zip downloads, restores (owner, mode, times, symlinks, empty folders, non-ASCII names; refusing a non-empty folder), restores into every database kind (new database, refusal without "replace", replace), copies to a second destination and to another server, retention, prune, unlock, verify (and a deliberately damaged repository being caught, with its alert), hooks and `ORLUMA_STATUS`, health-check pings, signed webhooks, cancel, two jobs at once, cron and catch-up after downtime, disk alerts, and no password in logs, run history or the process list. The PC's own repository server was tested with restic itself.

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
