// Mirrors crates/core (model.rs, api.rs). The agent's JSON is the source of truth; keep these in step.

export type DestKind =
  | { type: "local"; path: string }
  | { type: "sftp"; host: string; port: number; user: string; path: string }
  | { type: "s3"; provider: string; endpoint: string; bucket: string; path: string; region: string; access_key: string; secret_key: string }
  | { type: "b2"; bucket: string; path: string; account_id: string; account_key: string }
  | { type: "azure"; account_name: string; account_key: string; container: string; path: string }
  | { type: "gcs"; project_id: string; bucket: string; path: string; credentials_json: string }
  | { type: "rest"; url: string; username: string; password: string }
  | { type: "rclone"; remote: string; path: string };

export type DestType = DestKind["type"];

export interface Destination {
  id: string;
  name: string;
  kind: DestKind;
  password: string;
  limit_upload_kib: number;
  limit_download_kib: number;
}

export interface DbConn {
  host: string;
  port: number;
  user: string;
  password: string;
  container: string;
  os_user: string;
}

export type Source =
  | { type: "paths"; id: string; paths: string[]; excludes: string[]; one_file_system: boolean }
  | ({ type: "postgres"; id: string; databases: string[]; globals: boolean } & DbConn)
  | ({ type: "mysql"; id: string; databases: string[] } & DbConn)
  | { type: "mongo"; id: string; uri: string; container: string }
  | ({ type: "redis"; id: string } & DbConn)
  | { type: "sqlite"; id: string; path: string }
  | ({ type: "clickhouse"; id: string; databases: string[] } & DbConn)
  | { type: "command"; id: string; command: string; filename: string };

export type SourceType = Source["type"];

export interface Retention {
  last: number;
  hourly: number;
  daily: number;
  weekly: number;
  monthly: number;
  yearly: number;
  prune_every_days: number;
}

export interface Hook {
  command: string;
  timeout_secs: number;
  continue_on_error: boolean;
}

export interface Job {
  id: string;
  name: string;
  enabled: boolean;
  sources: Source[];
  destination: string;
  copies: string[];
  schedule: { cron: string; catch_up: boolean };
  retention: Retention;
  verify: { every_days: number; read_data_percent: number };
  hooks: { before: Hook[]; after: Hook[] };
  options: { compression: string; exclude_caches: boolean; exclude_larger_than_mib: number; low_priority: boolean };
  healthcheck_url: string;
}

export type AlertEvent = "run_succeeded" | "run_warning" | "run_failed" | "verify_failed" | "disk_low" | "agent_updated";
export const ALERT_EVENTS: AlertEvent[] = ["run_failed", "run_warning", "verify_failed", "disk_low", "run_succeeded", "agent_updated"];
export const DEFAULT_EVENTS: AlertEvent[] = ["run_warning", "run_failed", "verify_failed", "disk_low"];

export type NotifierKind =
  | { type: "webhook"; url: string; secret: string }
  | { type: "discord"; webhook_url: string }
  | { type: "slack"; webhook_url: string }
  | { type: "teams"; webhook_url: string }
  | { type: "telegram"; bot_token: string; chat_id: string }
  | { type: "ntfy"; server: string; topic: string; token: string }
  | { type: "gotify"; server: string; token: string }
  | { type: "pushover"; user_key: string; app_token: string }
  | { type: "email"; smtp_host: string; smtp_port: number; security: string; username: string; password: string; from: string; to: string[] };

export type NotifierType = NotifierKind["type"];

export interface Notifier {
  id: string;
  name: string;
  enabled: boolean;
  kind: NotifierKind;
  events: AlertEvent[];
  jobs: string[];
}

export interface AgentConfig {
  version: number;
  agent: { name: string; max_parallel: number; auto_update: boolean; disk_warn_percent: number; timezone: string };
  destinations: Destination[];
  jobs: Job[];
  notifiers: Notifier[];
}

export type RunKind = "backup" | "verify" | "prune" | "copy" | "restore";
export type RunStatus = "running" | "success" | "warning" | "failed" | "cancelled";

export interface RunSummary {
  id: string;
  job_id: string;
  kind: RunKind;
  status: RunStatus;
  trigger: "schedule" | "manual" | "catch_up";
  started: string;
  finished: string | null;
  bytes_added: number;
  bytes_total: number;
  files_new: number;
  files_changed: number;
  snapshots: string[];
  error: string | null;
}

export interface LogLine {
  at: string;
  level: "info" | "warn" | "error";
  text: string;
}

export interface RunRecord extends RunSummary {
  log: LogLine[];
}

export interface RunProgress {
  run_id: string;
  job_id: string;
  kind: RunKind;
  step: string;
  step_index: number;
  step_count: number;
  percent: number;
  bytes_done: number;
  bytes_total: number;
  files_done: number;
  files_total: number;
  seconds_remaining: number;
  started: string;
}

export interface JobState {
  id: string;
  next_run: string | null;
  last_run: RunSummary | null;
  last_success: string | null;
}

export interface Disk {
  mount: string;
  total: number;
  available: number;
}

export interface AgentStatus {
  version: string;
  api: number;
  hostname: string;
  name: string;
  os: string;
  arch: string;
  uptime_secs: number;
  timezone: string;
  restic: string | null;
  ssh_public_key: string;
  disks: Disk[];
  running: RunProgress[];
  jobs: JobState[];
  update: string | null;
}

export interface Snapshot {
  id: string;
  short_id: string;
  time: string;
  hostname: string;
  paths: string[];
  tags: string[];
  job: string | null;
  source: string | null;
  bytes: number;
  files: number;
}

export interface FsNode {
  name: string;
  path: string;
  kind: "file" | "dir" | "symlink" | string;
  size: number;
  mtime: string | null;
}

export interface RepoStats {
  snapshots: number;
  stored_bytes: number;
  restore_bytes: number;
  compression_ratio: number;
}

export interface TestResult {
  ok: boolean;
  message: string;
}

export interface FoundDatabase {
  kind: "postgres" | "mysql" | "mongo" | "redis" | "clickhouse";
  label: string;
  container: string;
  host: string;
  port: number;
  user: string;
  password: string;
  os_user: string;
  databases: string[];
}

export interface Discovery {
  databases: FoundDatabase[];
  paths: { path: string; bytes: number; hint: string }[];
  tools: { name: string; version: string }[];
}

export type AgentEvent =
  | ({ event: "progress" } & RunProgress)
  | ({ event: "finished" } & RunSummary)
  | { event: "config_changed" };

// ---- the desktop side

export interface Server {
  id: string;
  name: string;
  host: string;
  port: number;
  user: string;
  host_key: string;
  added: string | null;
}

export interface PcSync {
  id: string;
  server_id: string;
  destination_id: string;
  job_id: string;
  every_hours: number;
  enabled: boolean;
  apply_retention: boolean;
  last_run: string | null;
  last_status: string | null;
  last_error: string | null;
}

export interface Settings {
  language: string | null;
  setup_done: boolean;
  run_in_background: boolean;
  start_with_os: boolean;
  auto_update: boolean;
  notify_desktop: boolean;
  notify_success: boolean;
  pc_storage_dir: string;
  pc_syncs: PcSync[];
}

export interface AppInfo {
  version: string;
  public_key: string;
  data_dir: string;
  pc_root: string;
  os: string;
}

export interface UpdateStatus {
  current: string;
  available: { version: string; notes: string | null; date: string | null } | null;
  checking: boolean;
  installing: boolean;
  downloaded: number;
  total: number | null;
  error: string | null;
  last_checked: number | null;
}

export interface PcUsage {
  root: string;
  free: number;
  repos: [string, number][];
}
