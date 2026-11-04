import type { Destination, DestKind, DestType, Job, Notifier, NotifierKind, NotifierType, Source, SourceType } from "./types";
import { DEFAULT_EVENTS } from "./types";

export function newId(prefix: string): string {
  const b = new Uint8Array(4);
  crypto.getRandomValues(b);
  return `${prefix}_${[...b].map((x) => x.toString(16).padStart(2, "0")).join("")}`;
}

/** 32 characters from an alphabet without look-alikes: strong, and readable when written down. */
export function randomPassword(n = 32): string {
  const abc = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
  const b = new Uint8Array(n);
  crypto.getRandomValues(b);
  return [...b].map((x) => abc[x % abc.length]).join("");
}

export function newJob(p: Partial<Job> = {}): Job {
  return {
    id: newId("j"),
    name: "",
    enabled: true,
    sources: [],
    destination: "",
    copies: [],
    schedule: { cron: "0 3 * * *", catch_up: true },
    retention: { last: 3, hourly: 0, daily: 7, weekly: 4, monthly: 12, yearly: 2, prune_every_days: 7 },
    verify: { every_days: 7, read_data_percent: 5 },
    hooks: { before: [], after: [] },
    options: { compression: "auto", exclude_caches: true, exclude_larger_than_mib: 0, low_priority: true },
    healthcheck_url: "",
    ...p,
  };
}

const conn = { host: "", port: 0, user: "", password: "", container: "", os_user: "" };

export function newSource(type: SourceType): Source {
  const id = newId("src");
  switch (type) {
    case "paths":
      return { type, id, paths: [], excludes: [...DEFAULT_EXCLUDES], one_file_system: false };
    case "postgres":
      return { type, id, databases: [], globals: true, ...conn, os_user: "postgres" };
    case "mysql":
      return { type, id, databases: [], ...conn, user: "root" };
    case "mongo":
      return { type, id, uri: "", container: "" };
    case "redis":
      return { type, id, ...conn };
    case "sqlite":
      return { type, id, path: "" };
    case "clickhouse":
      return { type, id, databases: [], ...conn };
    default:
      return { type: "command", id, command: "", filename: "output.txt" };
  }
}

export const SOURCE_TYPES: SourceType[] = ["paths", "postgres", "mysql", "mongo", "redis", "sqlite", "clickhouse", "command"];

export function newDestKind(type: DestType): DestKind {
  switch (type) {
    case "local":
      return { type, path: "/var/backups/orluma" };
    case "sftp":
      return { type, host: "", port: 22, user: "root", path: "/var/backups/orluma" };
    case "s3":
      return { type, provider: "aws", endpoint: "", bucket: "", path: "", region: "", access_key: "", secret_key: "" };
    case "b2":
      return { type, bucket: "", path: "", account_id: "", account_key: "" };
    case "azure":
      return { type, account_name: "", account_key: "", container: "", path: "" };
    case "gcs":
      return { type, project_id: "", bucket: "", path: "", credentials_json: "" };
    case "rest":
      return { type, url: "https://", username: "", password: "" };
    default:
      return { type: "rclone", remote: "", path: "orluma" };
  }
}

export const DEST_TYPES: DestType[] = ["local", "sftp", "s3", "b2", "azure", "gcs", "rest", "rclone"];

/** S3-compatible providers: the endpoint (with {region}/{account} filled in by hand) and region rules. */
export const S3_PROVIDERS: { id: string; name: string; endpoint: string; region: string }[] = [
  { id: "aws", name: "Amazon S3", endpoint: "s3.amazonaws.com", region: "us-east-1" },
  { id: "r2", name: "Cloudflare R2", endpoint: "<account-id>.r2.cloudflarestorage.com", region: "" },
  { id: "b2s3", name: "Backblaze B2 (S3)", endpoint: "s3.us-west-004.backblazeb2.com", region: "us-west-004" },
  { id: "wasabi", name: "Wasabi", endpoint: "s3.eu-central-1.wasabisys.com", region: "eu-central-1" },
  { id: "hetzner", name: "Hetzner Object Storage", endpoint: "fsn1.your-objectstorage.com", region: "fsn1" },
  { id: "do", name: "DigitalOcean Spaces", endpoint: "fra1.digitaloceanspaces.com", region: "fra1" },
  { id: "scaleway", name: "Scaleway", endpoint: "s3.fr-par.scw.cloud", region: "fr-par" },
  { id: "idrive", name: "iDrive e2", endpoint: "<region>.idrivee2.com", region: "" },
  { id: "minio", name: "MinIO / other", endpoint: "minio.example.net:9000", region: "" },
];

export function newDestination(type: DestType, name: string): Destination {
  return { id: newId("d"), name, kind: newDestKind(type), password: randomPassword(), limit_upload_kib: 0, limit_download_kib: 0 };
}

export function newNotifierKind(type: NotifierType): NotifierKind {
  switch (type) {
    case "webhook":
      return { type, url: "https://", secret: "" };
    case "discord":
    case "slack":
    case "teams":
      return { type, webhook_url: "" };
    case "telegram":
      return { type, bot_token: "", chat_id: "" };
    case "ntfy":
      return { type, server: "https://ntfy.sh", topic: "", token: "" };
    case "gotify":
      return { type, server: "https://", token: "" };
    case "pushover":
      return { type, user_key: "", app_token: "" };
    default:
      return { type: "email", smtp_host: "", smtp_port: 587, security: "starttls", username: "", password: "", from: "", to: [] };
  }
}

export const NOTIFIER_TYPES: NotifierType[] = ["discord", "telegram", "slack", "teams", "email", "ntfy", "gotify", "pushover", "webhook"];

export function newNotifier(type: NotifierType, name: string): Notifier {
  return { id: newId("n"), name, enabled: true, kind: newNotifierKind(type), events: [...DEFAULT_EVENTS], jobs: [] };
}

/** "where it goes" in one line, for lists. */
export function destSummary(k: DestKind): string {
  switch (k.type) {
    case "local":
      return k.path;
    case "sftp":
      return `${k.user}@${k.host}${k.port !== 22 ? `:${k.port}` : ""}:${k.path}`;
    case "s3":
      return `${k.bucket}${k.path ? `/${k.path}` : ""} · ${k.endpoint || "s3.amazonaws.com"}`;
    case "b2":
      return `b2:${k.bucket}/${k.path}`;
    case "azure":
      return `azure:${k.container}/${k.path}`;
    case "gcs":
      return `gs:${k.bucket}/${k.path}`;
    case "rest":
      return k.url;
    default:
      return `${k.remote}:${k.path}`;
  }
}

/** What a file backup never needs: things a tool rebuilds (node_modules, build output, caches, language runtimes and
 *  package stores), junk and system folders. restic patterns, added to every new file backup and editable in the job.
 *  A name matches at any depth, "a/b" matches wherever a/b occurs, a leading / pins it to the root. The agent also
 *  skips virtualenvs of any name (pyvenv.cfg), cargo's target (CACHEDIR.TAG) and folders with a .nobackup file. */
export const DEFAULT_EXCLUDES = [
  "node_modules", ".npm", ".pnpm-store", ".local/share/pnpm", ".yarn/cache", ".yarn/unplugged", "bower_components", ".bun/install/cache", ".node-gyp", ".electron",
  "__pycache__", "*.pyc", ".venv", "venv", ".virtualenvs", ".local/share/virtualenvs", ".tox", ".pytest_cache", ".mypy_cache", ".ruff_cache", ".nyc_output",
  ".cache", ".ccache", ".gradle", ".m2/repository", ".cargo/registry", ".cargo/git", ".rustup", "go/pkg/mod", ".nuget/packages", ".composer/cache", ".pub-cache", ".stack-work",
  ".nvm/versions", ".pyenv/versions", ".rbenv/versions", ".sdkman", "miniconda3", "anaconda3", ".conda",
  ".next", ".nuxt", ".svelte-kit", ".turbo", ".vite", ".astro", ".expo", ".parcel-cache", ".angular/cache", ".sass-cache", ".eslintcache", ".dart_tool", ".terraform",
  "target/debug", "target/release",
  ".vscode-server", ".cursor-server", ".Trash", ".local/share/Trash",
  "*.tmp", "*.swp", ".DS_Store", "Thumbs.db", "lost+found",
  "/var/lib/docker/overlay2", "/var/lib/containerd", "/proc", "/sys", "/dev", "/run", "/tmp", "/var/tmp", "/var/cache",
];

/** True for entries the default excludes already skip, so the folder picker can show them as such. Same rules as
 *  restic: a plain name at any depth, a pattern with a slash at the end of the path, a leading / from the root. */
export function isSkipped(path: string, name: string): boolean {
  return DEFAULT_EXCLUDES.some((e) => {
    if (e.startsWith("/")) return path === e;
    if (e.startsWith("*")) return name.endsWith(e.slice(1));
    return e.includes("/") ? path.endsWith(`/${e}`) : name === e;
  });
}
