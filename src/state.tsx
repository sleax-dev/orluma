import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { api, errText, isTauri, type ServerEvent, type ServerState } from "./lib/api";
import type { AgentConfig, AgentStatus, AppInfo, PcSync, RunProgress, Server, Settings, UpdateStatus } from "./lib/types";
import type { Page } from "./nav";

export interface Live {
  status: AgentStatus | null;
  online: boolean | null; // null = not asked yet
  error: string | null;
  loading: boolean;
}

export interface Toast {
  id: number;
  kind: "success" | "error" | "info";
  text: string;
}

interface Ctx {
  page: Page;
  setPage: (p: Page, params?: Record<string, string>) => void;
  params: Record<string, string>;
  settings: Settings;
  saveSettings: (s: Settings) => Promise<void>;
  info: AppInfo | null;
  servers: Server[];
  refreshServers: () => Promise<void>;
  server: string | null;
  setServer: (id: string) => void;
  live: Record<string, Live>;
  refreshStatus: (id: string) => Promise<void>;
  configs: Record<string, AgentConfig>;
  loadConfig: (id: string, force?: boolean) => Promise<AgentConfig | null>;
  saveConfig: (id: string, cfg: AgentConfig) => Promise<boolean>;
  running: RunProgress[];
  toasts: Toast[];
  toast: (kind: Toast["kind"], text: string) => void;
  dismiss: (id: number) => void;
  update: UpdateStatus | null;
  setUpdate: (u: UpdateStatus) => void;
  pcSyncs: PcSync[];
}

const AppCtx = createContext<Ctx | null>(null);

const NO_LIVE: Live = { status: null, online: null, error: null, loading: false };

export function useApp() {
  const c = useContext(AppCtx);
  if (!c) throw new Error("useApp outside AppProvider");
  return c;
}

const initialPage = (): Page => (new URLSearchParams(location.search).get("page") as Page) || "overview";

export function AppProvider({ initial, children }: { initial: Settings; children: ReactNode }) {
  const [page, setPageRaw] = useState<Page>(isTauri ? "overview" : initialPage());
  const [params, setParams] = useState<Record<string, string>>({});
  const [settings, setSettings] = useState(initial);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [servers, setServers] = useState<Server[]>([]);
  const [server, setServerRaw] = useState<string | null>(null);
  const [live, setLive] = useState<Record<string, Live>>({});
  const [configs, setConfigs] = useState<Record<string, AgentConfig>>({});
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [update, setUpdate] = useState<UpdateStatus | null>(null);
  const seq = useRef(0);

  const toast = useCallback((kind: Toast["kind"], text: string) => {
    const id = ++seq.current;
    setToasts((t) => [...t.slice(-3), { id, kind, text }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), kind === "error" ? 9000 : 4500);
  }, []);
  const dismiss = useCallback((id: number) => setToasts((t) => t.filter((x) => x.id !== id)), []);

  const setPage = useCallback((p: Page, prm: Record<string, string> = {}) => {
    setParams(prm);
    if (prm.server) setServerRaw(prm.server);
    setPageRaw(p);
  }, []);

  const setServer = useCallback((id: string) => setServerRaw(id), []);

  const saveSettings = useCallback(async (s: Settings) => {
    setSettings(s);
    await api.setSettings(s);
  }, []);

  const patchLive = useCallback((id: string, p: Partial<Live>) => {
    setLive((l) => ({ ...l, [id]: { ...NO_LIVE, ...l[id], ...p } }));
  }, []);

  const refreshStatus = useCallback(
    async (id: string) => {
      patchLive(id, { loading: true });
      try {
        const status = await api.status(id);
        patchLive(id, { status, online: true, error: null, loading: false });
      } catch (e) {
        patchLive(id, { online: false, error: errText(e), loading: false });
      }
    },
    [patchLive],
  );

  const refreshServers = useCallback(async () => {
    const list = await api.listServers();
    setServers(list);
    setServerRaw((cur) => (cur && list.some((s) => s.id === cur) ? cur : (list[0]?.id ?? null)));
    list.forEach((s) => void refreshStatus(s.id));
  }, [refreshStatus]);

  const loadConfig = useCallback(async (id: string, force = false) => {
    if (!force && configs[id]) return configs[id];
    try {
      const cfg = await api.getConfig(id);
      setConfigs((c) => ({ ...c, [id]: cfg }));
      return cfg;
    } catch (e) {
      toast("error", errText(e));
      return null;
    }
  }, [configs, toast]);

  const saveConfig = useCallback(async (id: string, cfg: AgentConfig) => {
    try {
      await api.setConfig(id, cfg);
      setConfigs((c) => ({ ...c, [id]: cfg }));
      void refreshStatus(id);
      return true;
    } catch (e) {
      toast("error", errText(e));
      return false;
    }
  }, [refreshStatus, toast]);

  useEffect(() => {
    void refreshServers();
    void api.appInfo().then(setInfo).catch(() => {});
    void api.updateStatus().then(setUpdate).catch(() => {});
  }, [refreshServers]);

  // Live events: progress and finished runs update the status in place; a reconnect re-reads it.
  useEffect(() => {
    const offs: Promise<() => void>[] = [];
    offs.push(
      api.listen<ServerEvent>("agent-event", ({ server: id, event }) => {
        setLive((l) => {
          const cur = l[id];
          if (!cur?.status) return l;
          const s = { ...cur.status };
          if (event.event === "progress") {
            const { event: _e, ...p } = event;
            s.running = [...s.running.filter((r) => r.run_id !== p.run_id), p];
          } else if (event.event === "finished") {
            const { event: _e, ...r } = event;
            s.running = s.running.filter((x) => x.run_id !== r.id);
            s.jobs = s.jobs.map((j) => (j.id === r.job_id && r.kind === "backup" ? { ...j, last_run: r, last_success: r.status === "success" || r.status === "warning" ? r.started : j.last_success } : j));
          }
          return { ...l, [id]: { ...cur, status: s, online: true } };
        });
        if (event.event === "config_changed") {
          void api.getConfig(id).then((cfg) => setConfigs((c) => ({ ...c, [id]: cfg }))).catch(() => {});
        }
      }),
    );
    offs.push(
      api.listen<ServerState>("server-state", ({ server: id, online, error }) => {
        if (online) void refreshStatus(id);
        else patchLive(id, { online: false, error });
      }),
    );
    offs.push(api.listen<UpdateStatus>("update-status", setUpdate));
    offs.push(api.listen<PcSync[]>("pc-sync", (pc_syncs) => setSettings((s) => ({ ...s, pc_syncs }))));
    return () => offs.forEach((p) => void p.then((off) => off()));
  }, [patchLive, refreshStatus]);

  // A slow poll on top of the events: disks, next runs and anything an event missed.
  useEffect(() => {
    const t = setInterval(() => servers.forEach((s) => void refreshStatus(s.id)), 30_000);
    return () => clearInterval(t);
  }, [servers, refreshStatus]);

  const running = useMemo(() => Object.values(live).flatMap((l) => l.status?.running ?? []), [live]);

  const value: Ctx = {
    page, setPage, params, settings, saveSettings, info, servers, refreshServers, server, setServer, live, refreshStatus,
    configs, loadConfig, saveConfig, running, toasts, toast, dismiss, update, setUpdate, pcSyncs: settings.pc_syncs,
  };
  return <AppCtx.Provider value={value}>{children}</AppCtx.Provider>;
}
