// Demo data for running the UI in a browser (vite dev, README screenshots). Never used inside the app.
import type { AgentConfig, AgentStatus, Discovery, FsNode, RunRecord, RunSummary, Server, Settings, Snapshot } from "./types";

const H = 3600_000;
const now = Date.now();
const iso = (msAgo: number) => new Date(now - msAgo).toISOString();
const ahead = (ms: number) => new Date(now + ms).toISOString();
const GB = 1e9;

// ?empty=1: nothing added yet. ?empty=2: one server, no backups or storage yet. Screenshots of the first-run screens use these.
const EMPTY = new URLSearchParams(location.search).get("empty");
const allServers: Server[] = [
  { id: "s_web", name: "web-01", host: "web-01.example.net", port: 22, user: "root", host_key: "SHA256:q2V4Wm0rQk3ZcY7oF1t9xLr8UuKe0n5aPjB6dHsT1wM", added: iso(40 * 24 * H) },
  { id: "s_db", name: "db-main", host: "10.20.0.4", port: 22, user: "root", host_key: "SHA256:Zx3pL0aY6mK2rT8qW1eV9uN5cB7dF4gH0jS2kQ6oP3", added: iso(32 * 24 * H) },
  { id: "s_mail", name: "mail", host: "mail.example.net", port: 2222, user: "root", host_key: "SHA256:Hk7Q0r1T3yU5iO8pA2sD4fG6hJ9kL1zX3cV5bN7mQ0", added: iso(9 * 24 * H) },
];
const servers: Server[] = EMPTY === "1" ? [] : EMPTY === "2" ? allServers.slice(0, 1) : allServers;

const retention = { last: 3, hourly: 0, daily: 7, weekly: 4, monthly: 12, yearly: 2, prune_every_days: 7 };
const job = (p: Partial<AgentConfig["jobs"][number]>): AgentConfig["jobs"][number] => ({
  id: "",
  name: "",
  enabled: true,
  sources: [],
  destination: "",
  copies: [],
  schedule: { cron: "0 3 * * *", catch_up: true },
  retention,
  verify: { every_days: 7, read_data_percent: 5 },
  hooks: { before: [], after: [] },
  options: { compression: "auto", exclude_caches: true, exclude_larger_than_mib: 0, low_priority: true },
  healthcheck_url: "",
  ...p,
});

const configs: Record<string, AgentConfig> = {
  s_web: {
    version: 1,
    agent: { name: "", max_parallel: 1, auto_update: true, disk_warn_percent: 90, timezone: "Europe/Istanbul" },
    destinations: [
      { id: "d_local", name: "Second disk", kind: { type: "local", path: "/mnt/backup/orluma" }, password: "demo-password-1", limit_upload_kib: 0, limit_download_kib: 0 },
      { id: "d_r2", name: "Cloudflare R2", kind: { type: "s3", provider: "r2", endpoint: "4f2c1a.r2.cloudflarestorage.com", bucket: "orluma-backups", path: "web-01", region: "", access_key: "a1b2c3", secret_key: "••••" }, password: "demo-password-2", limit_upload_kib: 20480, limit_download_kib: 0 },
      { id: "d_box", name: "backup-box (SFTP)", kind: { type: "sftp", host: "backup.example.net", port: 22, user: "orluma", path: "/srv/backups/web-01" }, password: "demo-password-3", limit_upload_kib: 0, limit_download_kib: 0 },
    ],
    jobs: [
      job({ id: "j_site", name: "Websites", destination: "d_local", copies: ["d_r2"], sources: [{ type: "paths", id: "p1", paths: ["/var/www", "/etc/nginx", "/etc/letsencrypt"], excludes: ["*/cache/*", "*.log"], one_file_system: false }], schedule: { cron: "0 */6 * * *", catch_up: true }, healthcheck_url: "https://hc-ping.com/7f1e..." }),
      job({ id: "j_pg", name: "PostgreSQL", destination: "d_local", copies: ["d_r2", "d_box"], sources: [{ type: "postgres", id: "pg", host: "", port: 0, user: "", password: "", container: "", os_user: "postgres", databases: [], globals: true }], schedule: { cron: "30 2 * * *", catch_up: true } }),
      job({ id: "j_docker", name: "Docker volumes", destination: "d_box", sources: [{ type: "paths", id: "v1", paths: ["/var/lib/docker/volumes/uploads/_data", "/var/lib/docker/volumes/redis-data/_data"], excludes: [], one_file_system: true }, { type: "redis", id: "rd", host: "", port: 0, user: "", password: "", container: "app-redis-1", os_user: "" }], schedule: { cron: "0 4 * * 0", catch_up: true } }),
    ],
    notifiers: [
      { id: "n_dc", name: "Discord #ops", enabled: true, kind: { type: "discord", webhook_url: "https://discord.com/api/webhooks/…" }, events: ["run_failed", "run_warning", "verify_failed", "disk_low"], jobs: [] },
      { id: "n_tg", name: "Telegram", enabled: true, kind: { type: "telegram", bot_token: "123:abc", chat_id: "-100200300" }, events: ["run_failed", "verify_failed"], jobs: [] },
      { id: "n_mail", name: "On-call email", enabled: false, kind: { type: "email", smtp_host: "smtp.example.net", smtp_port: 587, security: "starttls", username: "alerts", password: "", from: "Orluma <alerts@example.net>", to: ["ops@example.net"] }, events: ["run_failed"], jobs: [] },
    ],
  },
  s_db: {
    version: 1,
    agent: { name: "", max_parallel: 1, auto_update: true, disk_warn_percent: 90, timezone: "Europe/Istanbul" },
    destinations: [
      { id: "d_b2", name: "Backblaze B2", kind: { type: "b2", bucket: "acme-db", path: "db-main", account_id: "005abc", account_key: "••••" }, password: "demo-password-4", limit_upload_kib: 0, limit_download_kib: 0 },
    ],
    jobs: [
      job({ id: "j_ch", name: "ClickHouse", destination: "d_b2", sources: [{ type: "clickhouse", id: "ch", host: "", port: 9000, user: "default", password: "", container: "", os_user: "", databases: ["analytics"] }], schedule: { cron: "15 1 * * *", catch_up: true } }),
      job({ id: "j_my", name: "MySQL + configs", destination: "d_b2", sources: [{ type: "mysql", id: "my", host: "", port: 0, user: "root", password: "", container: "mysql-1", os_user: "", databases: [] }, { type: "paths", id: "etc", paths: ["/etc"], excludes: [], one_file_system: true }], schedule: { cron: "0 3 * * *", catch_up: true } }),
    ],
    notifiers: [{ id: "n_ntfy", name: "ntfy", enabled: true, kind: { type: "ntfy", server: "https://ntfy.sh", topic: "acme-backups", token: "" }, events: [], jobs: [] }],
  },
  s_mail: {
    version: 1,
    agent: { name: "", max_parallel: 1, auto_update: true, disk_warn_percent: 90, timezone: "UTC" },
    destinations: [{ id: "d_local", name: "Local disk", kind: { type: "local", path: "/var/backups/orluma" }, password: "demo-password-5", limit_upload_kib: 0, limit_download_kib: 0 }],
    jobs: [job({ id: "j_mail", name: "Mailboxes", destination: "d_local", sources: [{ type: "paths", id: "m", paths: ["/var/vmail", "/etc/postfix", "/etc/dovecot"], excludes: [], one_file_system: false }, { type: "sqlite", id: "sq", path: "/etc/mailserver/mailserver.db" }], schedule: { cron: "0 */4 * * *", catch_up: true } })],
    notifiers: [],
  },
};

function run(id: string, job_id: string, status: RunSummary["status"], hoursAgo: number, mins: number, added: number, total: number, error: string | null = null, kind: RunSummary["kind"] = "backup"): RunSummary {
  return {
    id, job_id, kind, status, trigger: "schedule", started: iso(hoursAgo * H), finished: iso(hoursAgo * H - mins * 60_000),
    bytes_added: added, bytes_total: total, files_new: Math.round(added / 40_000), files_changed: 12, snapshots: ["a1", "b2"], error,
  };
}

if (EMPTY === "2") {
  configs.s_web.jobs = [];
  configs.s_web.destinations = [];
  configs.s_web.notifiers = [];
}

const runs: Record<string, RunSummary[]> = {
  s_web: [
    run("r1", "j_site", "success", 2, 4, 182e6, 18.4 * GB),
    run("r2", "j_pg", "success", 9.5, 7, 410e6, 6.2 * GB),
    run("r3", "j_site", "success", 8, 5, 96e6, 18.3 * GB),
    run("r4", "j_docker", "warning", 30, 11, 1.2 * GB, 9.7 * GB, null),
    run("r5", "j_site", "success", 14, 4, 120e6, 18.3 * GB),
    run("r6", "j_pg", "success", 33.5, 6, 380e6, 6.1 * GB),
    run("r7", "j_site", "success", 20, 5, 88e6, 18.2 * GB),
  ],
  s_db: [run("r8", "j_ch", "success", 12, 23, 2.4 * GB, 41 * GB), run("r9", "j_my", "failed", 11, 1, 0, 0, "mysql: Access denied for user 'root'@'localhost'"), run("r10", "j_ch", "success", 36, 25, 2.1 * GB, 40.2 * GB)],
  s_mail: [run("r11", "j_mail", "success", 1, 2, 64e6, 3.1 * GB), run("r12", "j_mail", "success", 5, 2, 51e6, 3.1 * GB)],
};

const running = {
  run_id: "r_live", job_id: "j_ch", kind: "backup" as const, step: "clickhouse analytics.events", step_index: 3, step_count: 7,
  percent: 0.64, bytes_done: 9.8 * GB, bytes_total: 15.3 * GB, files_done: 1, files_total: 1, seconds_remaining: 312, started: iso(0.2 * H),
};

if (EMPTY === "2") runs.s_web = [];

function status(id: string): AgentStatus {
  const cfg = configs[id];
  const host = servers.find((s) => s.id === id)!.name;
  return {
    version: "0.1.0", api: 1, hostname: host, name: host,
    os: id === "s_db" ? "Debian GNU/Linux 12 (bookworm)" : "Ubuntu 24.04.1 LTS", arch: "x86_64", uptime_secs: 86400 * (id === "s_mail" ? 9 : 41),
    timezone: cfg.agent.timezone, restic: "0.19.1", ssh_public_key: `ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHk1… orluma@${host}`,
    disks: id === "s_db" ? [{ mount: "/", total: 160 * GB, available: 38 * GB }, { mount: "/data", total: 500 * GB, available: 61 * GB }] : [{ mount: "/", total: 80 * GB, available: (id === "s_mail" ? 51 : 23) * GB }],
    running: id === "s_db" ? [running] : [],
    jobs: cfg.jobs.map((j) => {
      const last = runs[id].find((r) => r.job_id === j.id) ?? null;
      return { id: j.id, next_run: ahead((j.id.length % 5 + 1) * H), last_run: last, last_success: last?.status === "failed" ? iso(35 * H) : last?.started ?? null };
    }),
    update: null,
  };
}

const snapshots = (dest: string): Snapshot[] => {
  const out: Snapshot[] = [];
  for (let i = 0; i < 9; i++) {
    out.push({ id: `${dest}site${i}`.padEnd(64, "0"), short_id: `a${i}3f9c${i}e`, time: iso((2 + i * 6) * H), hostname: "web-01", paths: ["/var/www", "/etc/nginx", "/etc/letsencrypt"], tags: ["orluma", "job:j_site", "source:p1", "kind:paths"], job: "j_site", source: "p1", bytes: 18.4 * GB - i * 0.05 * GB, files: 182_400 - i * 120 });
    if (i % 3 === 0) out.push({ id: `${dest}pg${i}`.padEnd(64, "0"), short_id: `7c${i}d01b${i}`, time: iso((9.5 + i * 8) * H), hostname: "web-01", paths: ["/postgres/shop.dump"], tags: ["orluma", "job:j_pg", "source:pg", "kind:postgres"], job: "j_pg", source: "pg", bytes: 4.9 * GB, files: 1 });
  }
  return out.sort((a, b) => b.time.localeCompare(a.time));
};

const tree: Record<string, FsNode[]> = {
  "/": [{ name: "var", path: "/var", kind: "dir", size: 0, mtime: iso(3 * H) }, { name: "etc", path: "/etc", kind: "dir", size: 0, mtime: iso(3 * H) }],
  "/var": [{ name: "www", path: "/var/www", kind: "dir", size: 0, mtime: iso(3 * H) }],
  "/var/www": [
    { name: "shop.example.net", path: "/var/www/shop.example.net", kind: "dir", size: 0, mtime: iso(5 * H) },
    { name: "blog.example.net", path: "/var/www/blog.example.net", kind: "dir", size: 0, mtime: iso(30 * H) },
    { name: "status", path: "/var/www/status", kind: "dir", size: 0, mtime: iso(90 * H) },
    { name: "index.html", path: "/var/www/index.html", kind: "file", size: 2_410, mtime: iso(400 * H) },
  ],
  "/var/www/shop.example.net": [
    { name: "public", path: "/var/www/shop.example.net/public", kind: "dir", size: 0, mtime: iso(5 * H) },
    { name: "storage", path: "/var/www/shop.example.net/storage", kind: "dir", size: 0, mtime: iso(5 * H) },
    { name: ".env", path: "/var/www/shop.example.net/.env", kind: "file", size: 1_284, mtime: iso(200 * H) },
    { name: "composer.json", path: "/var/www/shop.example.net/composer.json", kind: "file", size: 3_918, mtime: iso(300 * H) },
  ],
};

const discovery: Discovery = {
  databases: [
    { kind: "postgres", label: "postgresql@16-main.service", container: "", host: "", port: 0, user: "", password: "", os_user: "postgres", databases: [] },
    { kind: "redis", label: "app-redis-1 (redis:7-alpine)", container: "app-redis-1", host: "", port: 0, user: "", password: "", os_user: "", databases: [] },
    { kind: "mysql", label: "wordpress-db-1 (mariadb:11)", container: "wordpress-db-1", host: "", port: 0, user: "root", password: "••••", os_user: "", databases: [] },
  ],
  paths: [
    { path: "/etc", bytes: 18e6, hint: "config" },
    { path: "/root", bytes: 420e6, hint: "home" },
    { path: "/home", bytes: 2.1 * GB, hint: "home" },
    { path: "/var/www", bytes: 14.2 * GB, hint: "web" },
    { path: "/opt", bytes: 1.4 * GB, hint: "data" },
    { path: "/var/lib/docker/volumes/uploads/_data", bytes: 3.3 * GB, hint: "docker-volume" },
  ],
  tools: [{ name: "restic", version: "restic 0.19.1" }, { name: "docker", version: "Docker version 28.4.0" }, { name: "pg_dump", version: "pg_dump (PostgreSQL) 16.9" }],
};

let settings: Settings = {
  language: new URLSearchParams(location.search).get("lang"),
  setup_done: !new URLSearchParams(location.search).has("setup"),
  run_in_background: true,
  start_with_os: true,
  auto_update: true,
  notify_desktop: true,
  notify_success: false,
  pc_storage_dir: "D:\\Backups\\Orluma",
  pc_syncs: EMPTY ? [] : [
    { id: "pc1", server_id: "s_web", destination_id: "d_local", job_id: "j_pg", every_hours: 24, enabled: true, apply_retention: true, last_run: iso(6 * H), last_status: "success", last_error: null },
    { id: "pc2", server_id: "s_mail", destination_id: "d_local", job_id: "", every_hours: 12, enabled: true, apply_retention: false, last_run: iso(2 * H), last_status: "success", last_error: null },
  ],
};

const wait = (ms = 120) => new Promise((r) => setTimeout(r, ms));

function agentCall(server: string, path: string, body: Record<string, unknown>): unknown {
  switch (path) {
    case "status":
      return status(server);
    case "config/get":
      return structuredClone(configs[server]);
    case "config/set":
      configs[server] = body as unknown as AgentConfig;
      return null;
    case "runs/list":
      return runs[server].filter((r) => !body.job_id || r.job_id === body.job_id);
    case "runs/get": {
      const r = Object.values(runs).flat().find((x) => x.id === body.id) ?? runs.s_web[0];
      const log: RunRecord["log"] = [
        { at: r.started, level: "info", text: "Backup 'Websites' started" },
        { at: r.started, level: "info", text: "Backing up /var/www, /etc/nginx, /etc/letsencrypt" },
        { at: r.started, level: "info", text: "  snapshot a03f9c0e: 18.4 GB processed, 182.0 MB new" },
        { at: r.started, level: "info", text: "Retention on 'Second disk': 1 old snapshot(s) removed" },
        { at: r.started, level: "info", text: "Copying the new snapshots to 'Cloudflare R2'" },
        { at: r.started, level: "info", text: "  1 snapshot(s) copied" },
        ...(r.status === "failed" ? [{ at: r.started, level: "error" as const, text: r.error ?? "failed" }] : []),
        { at: r.finished ?? r.started, level: "info", text: "Done in 4m 12s: 182.0 MB new, 18.4 GB in total" },
      ];
      return { ...r, log };
    }
    case "snapshots/list":
      return snapshots(String(body.destination));
    case "snapshots/ls":
      return tree[String(body.path || "/")] ?? [];
    case "destinations/stats":
      return { snapshots: 47, stored_bytes: 21.7 * GB, restore_bytes: 612 * GB, compression_ratio: 1.42 };
    case "destinations/test":
      return { ok: true, message: "A repository is there and the password opens it" };
    case "discover":
      return discovery;
    case "notifiers/test":
      return { ok: true, message: "Sent" };
    case "agent/update-check":
      return "0.1.0";
    default:
      return { run_id: "r_demo" };
  }
}

export async function mock(cmd: string, args: Record<string, unknown> = {}): Promise<unknown> {
  await wait();
  switch (cmd) {
    case "get_settings":
      return structuredClone(settings);
    case "set_settings":
      settings = structuredClone(args.settings as Settings);
      return null;
    case "app_info":
      return { version: "0.1.0", public_key: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDemoKey orluma@DESKTOP", data_dir: "C:\\Users\\you\\AppData\\Roaming\\app.orluma.desktop", pc_root: settings.pc_storage_dir, os: "windows" };
    case "list_servers":
      return structuredClone(servers);
    case "probe_server":
      return "SHA256:N3wS3rv3rK3yF1ng3rpr1ntD3m0x7Q2w9e8r6t5y4u1";
    case "agent":
      return agentCall(String(args.server), String(args.path), (args.body ?? {}) as Record<string, unknown>);
    case "pc_usage":
      return { root: settings.pc_storage_dir, free: 812 * GB, repos: EMPTY ? [] : [["s_web", 6.3 * GB], ["s_mail", 2.8 * GB]] };
    case "lan_scan":
      return EMPTY ? [] : [{ name: "LAPTOP-OFFICE", addr: "192.168.1.24:41873" }];
    case "lan_share_start":
      return { code: "48201937", port: 41873, ip: "192.168.1.20", name: "DESKTOP" };
    case "update_status":
    case "check_update":
      return { current: "0.1.0", available: null, checking: false, installing: false, downloaded: 0, total: null, error: null, last_checked: Math.floor(now / 1000) - 3600 };
    case "add_server":
      await wait(1500);
      return { ...servers[0], id: "s_new", name: "new-server" };
    default:
      return null;
  }
}
