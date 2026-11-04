# Contributing

Thanks for looking. The short version: keep it small, keep it tested, and say what you changed and why.

## Setup

You need Rust (stable), Node.js (LTS), and on Linux the Tauri dependencies (`libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libssl-dev`). restic is only needed to run the agent for real.

```sh
npm install
npm run tauri dev            # desktop app with hot reload
npm run dev                  # the UI alone in a browser, with demo data (src/lib/mock.ts)
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
npm run check:i18n
```

## Layout

| Path | What |
|---|---|
| `crates/core` | What the app and the agent share: config, API types, cron schedules, logging with secrets masked |
| `crates/agent` | The server agent: restic, sources, discovery, scheduler, alerts, logs, self-update |
| `src-tauri` | The desktop side: SSH, installing the agent, the PC repository, PC-to-PC transfer, the app log, tray, updates |
| `src` | The interface (React, Fluent UI); `src/i18n/locales` has one file per language |
| `docs/ARCHITECTURE.md` | How the pieces talk to each other |

## Trying the agent without touching a server

```sh
cargo build -p orluma-agent
ORLUMA_ROOT=/tmp/orluma ./target/debug/orluma-agent serve &
curl -s --unix-socket /tmp/orluma/run/orluma/agent.sock -X POST http://agent/v1/status -d '{}'
```

`ORLUMA_ROOT` moves `/etc`, `/var` and `/run` under a directory. To test the desktop app against an agent you built, set `ORLUMA_AGENT_BINARY` to the file and "Add server" uploads it instead of downloading a release.

## When something goes wrong

Settings → Diagnostics in the app shows both logs and copies them as one report; attach it to an issue. On a server, `journalctl -u orluma-agent` or `/var/lib/orluma/agent.log` has the same lines as the agent's part of it.

## Translations

`src/i18n/locales/en.ts` is the source. Another language is one file typed against it, plus one line in `src/i18n/index.ts`; `npm run check:i18n` lists every missing string and plural form (the plural forms a language needs come from `Intl.PluralRules`).

## Changing the config or the API

The agent's JSON is the contract with the app: `crates/core/src/{model,api}.rs` and `src/lib/types.ts` mirror each other, and a config written by an older version has to keep loading, so new fields get a default.

## Pull requests

Describe the behaviour change, not just the diff. A change that touches how backups are written, restored or verified needs a test that fails without it.
