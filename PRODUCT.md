# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Stack
Existing: Tauri 2 desktop app, React 19, Fluent UI v9, Vite, i18next (10 languages). Rust agent on servers.

## Users
Two audiences in one app: people with a site or server who are not backup experts (no SSH/restic/cron vocabulary), and developers or small teams running a few VPSs who want a dense, fast control room. Both install the desktop app on Windows, macOS or Linux and add servers over SSH.

## Product Purpose
Orluma runs scheduled, encrypted, verified backups of servers (folders and databases) from one desktop app, copies them where the user wants (another server, cloud, this PC) and restores them without a terminal. Success: someone with no backup experience adds a server, gets a first backup running, and later restores something, without reading documentation.

## Positioning
A small agent on each server plus a desktop control room: no open ports (everything through SSH), restic underneath, one-click restore into files or live databases, and moving backups between servers, clouds and PCs (including PC to PC on the same Wi-Fi).

## Operating Context
First run usually has zero servers, zero destinations, zero backups: empty states are the most-seen screens. Users alternate between short setup sessions and rare, stressful restores.

## Capabilities and Constraints
Servers, Backups (jobs), Storage (destinations + copies to this PC), Restore (browse, download, restore to folder or into a database), Transfer, Activity, Alerts, Settings. Ten UI languages (en, tr, es, de, fr, pt, ru, zh, ja, ar incl. RTL): every string goes through i18n and every key must exist in all languages. No new dependencies for what CSS/Fluent already does.

## Brand Commitments
Name Orluma, existing logo (docs/brand/logo.svg), dark theme with the neutral silver accent, Inter. User chose: keep the current look, rebuild every page's structure and flow.

## Evidence on Hand
Demo data in src/lib/mock.ts and screenshots script (scripts/screenshots.mjs). No user testimonials or usage data; do not invent any.

## Product Principles
1. Every screen answers "what do I do next?" before anything else, especially when empty.
2. Explain in one short line; put the rest behind an info icon or tooltip.
3. Safe by default: new database, empty folder, nothing deleted without a clear choice.
4. Progressive disclosure: expert settings exist, but out of the first path.
5. Show state with icons and color, not paragraphs.

## Accessibility & Inclusion
Keyboard operable, visible focus, tooltips reachable by focus, RTL layout for Arabic, contrast on dark surfaces.
