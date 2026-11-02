# Security

## Reporting a vulnerability

Please do not open a public issue for a security problem. Use GitHub's private reporting: **Security → Report a vulnerability** on [sleax-dev/orluma](https://github.com/sleax-dev/orluma/security/advisories/new). You will get an answer within a few days.

## Model, and what it does not cover

- **Who can control a server's backups:** whoever can log in to that server as root over SSH. The agent listens only on a root-only Unix socket (`/run/orluma/agent.sock`); there is no network listener to attack.
- **Secrets at rest.** Repository passwords, cloud keys and SMTP passwords are stored in `/etc/orluma/agent.json` (mode 600, root) on each server, and the PC repository password in the app's data folder. They are not encrypted at rest: anyone with root on the server, or your user account on the PC, can read them. That is inherent in unattended backups, where the agent must be able to open the repository.
- **A compromised server can delete its own backups** unless the destination prevents it. Use credentials scoped to one bucket, object lock (S3/B2), or an append-only restic REST server, and keep a copy the server cannot reach (the copy on your PC is one).
- **Host keys are pinned** on first contact, after you confirm the fingerprint. If you skip that comparison, the first connection is only as trustworthy as your network.
- **Updates are verified** with a minisign signature before anything is installed. The release key must be kept private; losing it means no more updates, leaking it means anyone could ship one.
- **The PC repository** is a local REST server bound to 127.0.0.1 on a random port with random credentials, reachable by a server only through the SSH tunnel Orluma opens.
- **Recovery kits contain every password in plain text.** Keep them offline.

## Supported versions

Only the latest release receives fixes.
