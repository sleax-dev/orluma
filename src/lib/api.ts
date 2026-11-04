import type { UnlistenFn } from "@tauri-apps/api/event";
import { mock } from "./mock";
import type {
  AgentConfig,
  AgentEvent,
  AgentStatus,
  AppInfo,
  DbConn,
  Destination,
  Discovery,
  FsNode,
  Notifier,
  PcSync,
  PcUsage,
  RepoStats,
  RunRecord,
  RunSummary,
  Server,
  Settings,
  Snapshot,
  TestResult,
  UpdateStatus,
} from "./types";

/** In a browser (vite dev, screenshots) there is no Rust side: everything is answered by the demo data in mock.ts. */
export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Writes a line to the app's log file (the one the Diagnostics page shows). Never throws. */
export function logEvent(level: "info" | "warn" | "error", area: string, msg: string) {
  if (!isTauri) return;
  void import("@tauri-apps/api/core").then(({ invoke }) => invoke("log_event", { level, area, msg }).catch(() => {})).catch(() => {});
}

const QUIET = new Set(["log_event", "log_tail", "log_redact", "log_export", "get_settings", "list_servers", "app_info", "update_status"]);

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri) return mock(cmd, args) as Promise<T>;
  const { invoke } = await import("@tauri-apps/api/core");
  const t = performance.now();
  try {
    const r = await invoke<T>(cmd, args);
    const ms = Math.round(performance.now() - t);
    if (ms >= 3000 && !QUIET.has(cmd)) logEvent("warn", "ui", `${cmd}${args?.path ? " " + String(args.path) : ""} took ${ms}ms`);
    return r;
  } catch (e) {
    if (!QUIET.has(cmd)) logEvent("error", "ui", `${cmd}${args?.path ? " " + String(args.path) : ""} failed after ${Math.round(performance.now() - t)}ms: ${errText(e)}`);
    throw e;
  }
}

/** Error text from a failed command, without the "Error: " decoration. */
export function errText(e: unknown): string {
  const s = typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
  return s.replace(/^Error:\s*/, "");
}

export const DEFAULT_SETTINGS: Settings = {
  language: null,
  setup_done: false,
  run_in_background: true,
  start_with_os: false,
  auto_update: true,
  notify_desktop: true,
  notify_success: false,
  pc_storage_dir: "",
  pc_syncs: [],
};

const agent = <T>(server: string, path: string, body?: unknown) => call<T>("agent", { server, path, body: body ?? {} });

export interface NewServer {
  name: string;
  host: string;
  port: number;
  user: string;
  password: string;
  fingerprint: string;
}

export interface DownloadReq {
  id: string;
  server: string;
  destination: string;
  snapshot: string;
  path: string;
  zip: boolean;
  target: string;
}

export const api = {
  getSettings: () => call<Settings>("get_settings"),
  setSettings: (settings: Settings) => call<void>("set_settings", { settings }),
  appInfo: () => call<AppInfo>("app_info"),
  minimizeWindow: () => call<void>("minimize_window"),
  closeWindow: () => call<void>("close_window"),

  listServers: () => call<Server[]>("list_servers"),
  probeServer: (host: string, port: number) => call<string>("probe_server", { host, port }),
  addServer: (server: NewServer) => call<Server>("add_server", { server }),
  updateServer: (server: Server) => call<void>("update_server", { server }),
  reinstallAgent: (id: string) => call<void>("reinstall_agent", { id }),
  removeServer: (id: string, uninstall: boolean, purge: boolean) => call<void>("remove_server", { id, uninstall, purge }),
  authorizeOn: (server: string, publicKey: string) => call<void>("authorize_on_server", { server, publicKey }),

  status: (s: string) => agent<AgentStatus>(s, "status"),
  getConfig: (s: string) => agent<AgentConfig>(s, "config/get"),
  setConfig: (s: string, cfg: AgentConfig) => agent<null>(s, "config/set", cfg),
  runJob: (s: string, id: string) => agent<{ run_id: string }>(s, "jobs/run", { id }),
  cancelRun: (s: string, id: string) => agent<boolean>(s, "runs/cancel", { id }),
  runs: (s: string, job_id?: string, limit = 100) => agent<RunSummary[]>(s, "runs/list", { job_id: job_id ?? null, limit }),
  run: (s: string, id: string) => agent<RunRecord>(s, "runs/get", { id }),
  testDestination: (s: string, d: Destination) => agent<TestResult>(s, "destinations/test", d),
  initDestination: (s: string, id: string) => agent<TestResult>(s, "destinations/init", { id }),
  repoStats: (s: string, id: string) => agent<RepoStats>(s, "destinations/stats", { id }),
  unlock: (s: string, id: string) => agent<null>(s, "destinations/unlock", { id }),
  verify: (s: string, id: string, read_data_percent: number) => agent<{ run_id: string }>(s, "destinations/verify", { id, read_data_percent }),
  prune: (s: string, id: string) => agent<{ run_id: string }>(s, "destinations/prune", { id }),
  snapshots: (s: string, destination: string) => agent<Snapshot[]>(s, "snapshots/list", { destination }),
  ls: (s: string, destination: string, snapshot: string, path: string) => agent<FsNode[]>(s, "snapshots/ls", { destination, snapshot, path }),
  forget: (s: string, destination: string, snapshots: string[]) => agent<null>(s, "snapshots/forget", { destination, snapshots }),
  restore: (s: string, req: { destination: string; snapshot: string; include: string[]; target: string; overwrite: boolean }) =>
    agent<{ run_id: string }>(s, "restore", req),
  restoreDb: (s: string, req: { destination: string; snapshot: string; kind: string; file: string; conn: DbConn; uri: string; database: string; replace: boolean }) =>
    agent<{ run_id: string }>(s, "restore-db", req),
  transfer: (s: string, req: { from: string; job_id: string; init: boolean; to?: string; to_destination?: Destination }) =>
    agent<{ run_id: string }>(s, "copy", req),
  pcPush: (server: string, folder: string, destination: string) => call<string>("pc_push", { server, folder, destination }),
  lanShareStart: (folders: string[]) => call<{ code: string; port: number; ip: string | null; name: string }>("lan_share_start", { folders }),
  lanShareStop: () => call<void>("lan_share_stop"),
  lanScan: () => call<{ name: string; addr: string }[]>("lan_scan"),
  lanReceive: (addr: string, code: string) => call<string[]>("lan_receive", { addr, code }),
  discover: (s: string) => agent<Discovery>(s, "discover"),
  listDir: (s: string, path: string) => agent<FsNode[]>(s, "discover/ls", { path }),
  pathSize: (s: string, path: string, excludes: string[] = []) => agent<{ bytes: number; complete: boolean }>(s, "discover/size", { path, excludes }),
  agentLogs: (s: string, lines: number) => agent<string[]>(s, "logs", { lines }),
  appLogs: (lines: number) => call<string[]>("log_tail", { lines }),
  redact: (text: string) => call<string>("log_redact", { text }),
  exportReport: (text: string) => call<string>("log_export", { text }),
  cloudConnect: (server: string, provider: string) => call<string>("cloud_connect", { server, provider }),
  cloudCancel: (server: string) => call<void>("cloud_cancel", { server }),
  testNotifier: (s: string, n: Notifier) => agent<TestResult>(s, "notifiers/test", n),
  agentUpdateCheck: (s: string) => agent<string>(s, "agent/update-check"),
  agentUpdate: (s: string) => agent<null>(s, "agent/update", { version: "" }),

  download: (req: DownloadReq) => call<number>("download", { req }),
  pcSyncNow: (id: string) => call<string>("pc_sync_now", { id }),
  pcUsage: () => call<PcUsage>("pc_usage"),
  recoveryKit: (target: string) => call<number>("recovery_kit", { target }),

  updateStatus: () => call<UpdateStatus>("update_status"),
  checkUpdate: () => call<UpdateStatus>("check_update"),
  installUpdate: () => call<void>("install_update"),

  async listen<T>(event: string, cb: (payload: T) => void): Promise<UnlistenFn> {
    if (!isTauri) return () => {};
    const { listen } = await import("@tauri-apps/api/event");
    return listen<T>(event, (e) => cb(e.payload));
  },

  async openUrl(url: string) {
    if (!isTauri) return void window.open(url, "_blank");
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  },
  async reveal(path: string) {
    if (!isTauri) return;
    const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
    await revealItemInDir(path);
  },
  async pickFolder(): Promise<string | null> {
    if (!isTauri) return null;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const r = await open({ directory: true });
    return typeof r === "string" ? r : null;
  },
  async pickSave(defaultPath: string): Promise<string | null> {
    if (!isTauri) return null;
    const { save } = await import("@tauri-apps/plugin-dialog");
    return (await save({ defaultPath })) ?? null;
  },
  async copy(text: string) {
    await navigator.clipboard?.writeText(text).catch(() => {});
  },
};

export type ServerEvent = { server: string; event: AgentEvent };
export type ServerState = { server: string; online: boolean; error: string | null };
export type PcSyncList = PcSync[];
