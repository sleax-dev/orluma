# How Orluma works

```
 your PC                                   each server
┌──────────────────────────┐   SSH        ┌─────────────────────────────┐
│ Orluma (Tauri)           │─────────────▶│ sshd                        │
│  React UI                │  one         │   │ direct-streamlocal      │
│  Rust: ssh, PC repo,     │  connection  │   ▼                         │
│  tray, updates           │              │ orluma-agent (systemd)      │
│                          │◀─────────────│  /run/orluma/agent.sock     │
│  restic REST server      │  reverse     │  scheduler · runner · alerts│
│  127.0.0.1:random        │  tunnel      │   │                         │
└──────────────────────────┘              │   ▼                         │
                                          │ restic ──▶ disk / SFTP /    │
                                          │            S3 / B2 / …      │
                                          └─────────────────────────────┘
```

## The agent

`orluma-agent` is one static-ish binary run by systemd (`orluma-agent install` writes the unit). It owns everything that has to happen whether or not a PC is connected.

- **State.** `/etc/orluma/agent.json` (config, mode 600), `/var/lib/orluma/history.db` (SQLite: run history, last prune and verify dates), `/var/cache/orluma` (restic's cache).
- **API.** `POST /v1/<name>` with JSON on `/run/orluma/agent.sock`, plus `GET /v1/events` (server-sent events) and `GET /v1/dump` (streams a file or a zip out of a snapshot). Types are in `crates/core/src/api.rs`.
- **A run** (`runner.rs`): before-hooks, one snapshot per source (`sources.rs` turns databases into dumps piped into `restic backup --stdin-from-command`, so a failing dump fails the snapshot), retention (`forget`), prune and verify when due, copies to the job's other destinations, after-hooks, a health-check ping, and the alert. Everything goes into the run's log.
- **Scheduler** (`scheduler.rs`): cron in the configured zone, catch-up of one missed run after downtime, disk alerts, and update checks every six hours.
- **restic** (`restic.rs`): used from the system if it is 0.17 or newer, otherwise a pinned version is downloaded and checked against a SHA-256 in the source.
- **Self-update** (`update.rs`): a release's agent binary and its `.sig` are fetched, the minisign signature is checked against the key compiled into the agent, and only then is the file replaced; systemd restarts it. Nothing is replaced while a run is going.

## The desktop app

- **SSH** (`ssh.rs`, russh): one connection per server, kept alive. It carries the agent's API (a `direct-streamlocal` channel to the socket, spoken to with HTTP/1.1), commands during setup, and the reverse tunnel.
- **Adding a server** (`install.rs`): read the host key, show the fingerprint, log in with the password, append the app's public key to `authorized_keys`, upload the signed agent over the SSH channel, run `orluma-agent install`, and wait for the socket to answer. The password never touches the disk.
- **This PC as a destination** (`pcrepo.rs`): a small restic REST server (`axum`) on `127.0.0.1:<random>` with random basic-auth credentials. The server's agent runs `restic copy` into `rest:http://…@127.0.0.1:<port>/<server>/`, where the port is the far end of a reverse tunnel back to this server. Objects are named by their SHA-256 and checked on upload.
- **Events.** Each server's event stream is followed for as long as the server is in the list, reconnecting with backoff. Finished runs become desktop notifications.

## Why restic

Chunking, deduplication, compression and authenticated encryption are the parts that must not be wrong, and restic has years of use behind them. Orluma adds what a single restic repository does not do: scheduling across servers, databases as first-class sources, alerts, a place to see everything, and the copy to a second location. Every repository stays readable with plain `restic`.

## Releases

A `v*` tag builds the desktop installers (Tauri) and the agent for x86-64 and ARM64 Linux, signs every artifact with the same minisign key, and publishes `latest.json` for the app's updater. The agent a new server gets is always the one matching the app's version.
