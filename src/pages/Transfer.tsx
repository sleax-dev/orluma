import { Button, Checkbox, Dropdown, Field, Option, ProgressBar, Spinner } from "@fluentui/react-components";
import { ArrowSwap20Regular, ArrowUpload16Regular, DesktopArrowRight16Regular, Dismiss16Regular, Send16Regular, ShareScreenStart16Regular } from "@fluentui/react-icons";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { PageHead, TextField, useAction } from "../components/ui";
import { api, errText } from "../lib/api";
import { bytes } from "../lib/format";
import type { Destination } from "../lib/types";
import { useApp } from "../state";

/** A server, then one of its destinations. */
function DestPick({ label, server, dest, onChange, hideLocalOf, avoid }: { label: string; server: string; dest: string; onChange: (server: string, dest: string) => void; hideLocalOf?: string; avoid?: string }) {
  const { t } = useTranslation();
  const { servers, configs, loadConfig } = useApp();
  useEffect(() => {
    if (server) void loadConfig(server);
  }, [server, loadConfig]);
  const dests = (configs[server]?.destinations ?? []).filter((d) => !(hideLocalOf && hideLocalOf !== server && d.kind.type === "local"));
  const cur = dests.find((d) => d.id === dest);
  const touched = useRef(false);
  // start on the first destination: most servers have only one
  useEffect(() => {
    const pick = dests.find((d) => d.id !== avoid) ?? dests[0];
    if ((!cur || (avoid && cur.id === avoid && pick && pick.id !== avoid && !touched.current)) && pick) onChange(server, pick.id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [server, dests.length, cur]);
  return (
    <div className="stack">
      <Field label={`${label} · ${t("transfer.server")}`}>
        <Dropdown value={servers.find((s) => s.id === server)?.name ?? ""} selectedOptions={[server]} onOptionSelect={(_, o) => onChange(o.optionValue ?? "", "")}>
          {servers.map((s) => <Option key={s.id} value={s.id}>{s.name}</Option>)}
        </Dropdown>
      </Field>
      <Field label={`${label} · ${t("transfer.destination")}`}>
        <Dropdown value={cur?.name ?? ""} selectedOptions={[dest]} onOptionSelect={(_, o) => { touched.current = true; onChange(server, o.optionValue ?? ""); }}>
          {dests.map((d) => <Option key={d.id} value={d.id}>{d.name}</Option>)}
        </Dropdown>
      </Field>
    </div>
  );
}

function ServerToServer() {
  const { t } = useTranslation();
  const { servers, configs, toast, setPage } = useApp();
  const first = servers[0]?.id ?? "";
  const [from, setFrom] = useState({ server: first, dest: "", job: "" });
  const [to, setTo] = useState({ server: first, dest: "" });
  const { busy, run } = useAction();
  const toDest: Destination | undefined = configs[to.server]?.destinations.find((d) => d.id === to.dest);
  const same = from.server === to.server;
  const ok = !!from.dest && !!toDest && !(same && from.dest === to.dest);
  const jobs = configs[from.server]?.jobs ?? [];

  const start = async () => {
    if (!toDest) return;
    const r = await run("s2s", () =>
      api.transfer(from.server, { from: from.dest, job_id: from.job, init: true, ...(same ? { to: to.dest } : { to_destination: toDest }) }),
    );
    if (r) {
      toast("success", t("transfer.started"));
      setPage("activity", { server: from.server });
    }
  };

  return (
    <section className="block">
      <div className="sect-head"><h2>{t("transfer.s1.title")}</h2></div>
      <div className="panel">
        <p className="small muted" style={{ margin: 0 }}>{t("transfer.s1.text")}</p>
        <div className="split-2">
          <div className="stack">
            <DestPick label={t("transfer.from")} server={from.server} dest={from.dest} onChange={(s, d) => setFrom({ server: s, dest: d, job: s === from.server ? from.job : "" })} />
            <Field label={t("transfer.job")}>
              <Dropdown value={from.job ? jobs.find((j) => j.id === from.job)?.name ?? "" : t("transfer.allJobs")} selectedOptions={[from.job]} onOptionSelect={(_, o) => setFrom({ ...from, job: o.optionValue ?? "" })}>
                <Option value="">{t("transfer.allJobs")}</Option>
                {jobs.map((j) => <Option key={j.id} value={j.id}>{j.name}</Option>)}
              </Dropdown>
            </Field>
          </div>
          <div className="stack">
            <DestPick label={t("transfer.to")} server={to.server} dest={to.dest} onChange={(s, d) => setTo({ server: s, dest: d })} hideLocalOf={from.server} avoid={same ? from.dest : ""} />
            {!same && <p className="small muted" style={{ margin: 0 }}>{t("transfer.localOther")}</p>}
          </div>
        </div>
        <div><Button appearance="primary" icon={busy === "s2s" ? <Spinner size="extra-tiny" /> : <ArrowSwap20Regular />} disabled={!ok || busy === "s2s"} onClick={() => void start()}>{t("transfer.start")}</Button></div>
      </div>
    </section>
  );
}

function PcToServer() {
  const { t } = useTranslation();
  const { servers, toast, setPage } = useApp();
  const [folders, setFolders] = useState<[string, number][]>([]);
  const [folder, setFolder] = useState("");
  const [to, setTo] = useState({ server: servers[0]?.id ?? "", dest: "" });
  const { busy, run } = useAction();
  useEffect(() => {
    void api.pcUsage().then((u) => { setFolders(u.repos); setFolder(u.repos[0]?.[0] ?? ""); }).catch(() => {});
  }, []);
  const label = (f: string) => servers.find((s) => s.id === f)?.name ?? f;
  return (
    <section className="block">
      <div className="sect-head"><h2>{t("transfer.s2.title")}</h2></div>
      <div className="panel">
        <p className="small muted" style={{ margin: 0 }}>{t("transfer.s2.text")}</p>
        {folders.length === 0 ? (
          <p className="small">{t("transfer.noFolders")}</p>
        ) : (
          <>
            <Field label={t("transfer.pcFolder")}>
              <Dropdown value={folder ? `${label(folder)} · ${bytes(folders.find(([f]) => f === folder)?.[1] ?? 0)}` : ""} selectedOptions={[folder]} onOptionSelect={(_, o) => setFolder(o.optionValue ?? "")}>
                {folders.map(([f, n]) => <Option key={f} value={f} text={`${label(f)} · ${bytes(n)}`}>{label(f)} · {bytes(n)}</Option>)}
              </Dropdown>
            </Field>
            <DestPick label={t("transfer.to")} server={to.server} dest={to.dest} onChange={(s, d) => setTo({ server: s, dest: d })} />
            <div>
              <Button
                appearance="primary"
                icon={busy === "p2s" ? <Spinner size="extra-tiny" /> : <ArrowUpload16Regular />}
                disabled={!folder || !to.dest || busy === "p2s"}
                onClick={async () => {
                  const r = await run("p2s", () => api.pcPush(to.server, folder, to.dest));
                  if (r) {
                    toast("success", t("transfer.started"));
                    setPage("activity", { server: to.server });
                  }
                }}
              >
                {t("transfer.start")}
              </Button>
            </div>
          </>
        )}
      </div>
    </section>
  );
}

interface LanProgress { phase: string; bytes_done: number; bytes_total: number; files_done: number; files_total: number }

function useLan() {
  const [p, setP] = useState<LanProgress | null>(null);
  useEffect(() => {
    let off = () => {};
    void api.listen<LanProgress>("lan-progress", setP).then((u) => (off = u));
    return () => off();
  }, []);
  return [p, setP] as const;
}

function Meter({ p }: { p: LanProgress | null }) {
  const { t } = useTranslation();
  if (!p || p.bytes_total === 0 || p.phase === "waiting") return null;
  return (
    <div className="stack" style={{ gap: 4 }}>
      <ProgressBar value={p.bytes_done / p.bytes_total} />
      <span className="small muted">{t(p.phase === "done" ? "transfer.done" : "transfer.progress", { done: bytes(p.bytes_done), total: bytes(p.bytes_total) })}</span>
    </div>
  );
}

function Send() {
  const { t } = useTranslation();
  const { servers } = useApp();
  const [folders, setFolders] = useState<[string, number][]>([]);
  const [pick, setPick] = useState<string[]>([]);
  const [info, setInfo] = useState<{ code: string; port: number; ip: string | null; name: string } | null>(null);
  const [p, setP] = useLan();
  const { busy, run } = useAction();
  useEffect(() => {
    void api.pcUsage().then((u) => { setFolders(u.repos); setPick(u.repos.map(([f]) => f)); }).catch(() => {});
  }, []);
  const label = (f: string) => servers.find((s) => s.id === f)?.name ?? f;
  const stop = async () => {
    await api.lanShareStop().catch(() => {});
    setInfo(null);
    setP(null);
  };
  return (
    <div className="panel" style={{ flex: 1, minWidth: 280 }}>
      <div className="gap-s"><Send16Regular /><b>{t("transfer.send")}</b></div>
      {info ? (
        <>
          <p className="small muted" style={{ margin: 0 }}>{t("transfer.codeHint")}</p>
          <div className="code-box selectable" aria-label={t("transfer.code")}>{info.code.replace(/(\d{4})(\d{4})/, "$1 $2")}</div>
          <p className="small muted" style={{ margin: 0 }}>{t("transfer.address")}: <span className="mono selectable">{info.ip ?? "?"}:{info.port}</span></p>
          {!p || p.phase === "waiting" ? <p className="small"><Spinner size="extra-tiny" /> {t("transfer.waiting")}</p> : <Meter p={p} />}
          <div><Button icon={<Dismiss16Regular />} onClick={() => void stop()}>{t("transfer.stop")}</Button></div>
        </>
      ) : folders.length === 0 ? (
        <p className="small">{t("transfer.noFolders")}</p>
      ) : (
        <>
          <p className="small muted" style={{ margin: 0 }}>{t("transfer.sendText")}</p>
          <div className="stack" style={{ gap: 2 }}>
            {folders.map(([f, n]) => (
              <Checkbox key={f} checked={pick.includes(f)} onChange={(_, d) => setPick(d.checked ? [...pick, f] : pick.filter((x) => x !== f))} label={`${label(f)} · ${bytes(n)}`} />
            ))}
          </div>
          <div>
            <Button appearance="primary" icon={busy === "share" ? <Spinner size="extra-tiny" /> : <ShareScreenStart16Regular />} disabled={pick.length === 0 || busy === "share"} onClick={async () => setInfo((await run("share", () => api.lanShareStart(pick))) ?? null)}>
              {t("transfer.sendStart")}
            </Button>
          </div>
        </>
      )}
    </div>
  );
}

function Receive() {
  const { t } = useTranslation();
  const { toast } = useApp();
  const [peers, setPeers] = useState<{ name: string; addr: string }[] | null>(null);
  const [addr, setAddr] = useState("");
  const [code, setCode] = useState("");
  const [got, setGot] = useState(false);
  const [p] = useLan();
  const { busy, run } = useAction();
  const scan = () => {
    setPeers(null);
    void api.lanScan().then(setPeers).catch(() => setPeers([]));
  };
  useEffect(scan, []);
  const go = async () => {
    setGot(false);
    try {
      await api.lanReceive(addr.trim(), code.replace(/\s/g, ""));
      setGot(true);
    } catch (e) {
      const m = errText(e);
      toast("error", m === "WRONG_CODE" ? t("transfer.wrongCode") : m);
    }
  };
  return (
    <div className="panel" style={{ flex: 1, minWidth: 280 }}>
      <div className="gap-s"><DesktopArrowRight16Regular /><b>{t("transfer.receive")}</b></div>
      <p className="small muted" style={{ margin: 0 }}>{t("transfer.receiveText")}</p>
      <div className="stack" style={{ gap: 4 }}>
        {peers === null && <p className="small"><Spinner size="extra-tiny" /> {t("transfer.scanning")}</p>}
        {peers?.length === 0 && <p className="small muted">{t("transfer.noPeers")}</p>}
        {peers?.map((x) => (
          <button key={x.addr} className={`peer${addr === x.addr ? " on" : ""}`} onClick={() => setAddr(x.addr)}>
            <span>{x.name}</span><span className="mono muted">{x.addr}</span>
          </button>
        ))}
        <div><Button size="small" onClick={scan} disabled={peers === null}>{t("transfer.scan")}</Button></div>
      </div>
      <TextField label={t("transfer.manual")} value={addr} onChange={setAddr} placeholder="192.168.1.20:41234" className="mono" />
      <TextField label={t("transfer.enterCode")} value={code} onChange={(v) => setCode(v.replace(/[^\d ]/g, ""))} placeholder="0000 0000" className="mono" />
      <Meter p={p} />
      {got && <p className="small" style={{ margin: 0, color: "var(--ok, inherit)" }}>{t("transfer.received")}</p>}
      <div>
        <Button appearance="primary" icon={busy === "recv" ? <Spinner size="extra-tiny" /> : <DesktopArrowRight16Regular />} disabled={!addr.trim() || code.replace(/\s/g, "").length < 8 || busy === "recv"} onClick={() => void run("recv", go)}>
          {t("transfer.receiveStart")}
        </Button>
      </div>
    </div>
  );
}

export function Transfer() {
  const { t } = useTranslation();
  const { servers } = useApp();
  return (
    <div className="page">
      <PageHead title={t("nav.transfer")} desc={t("transfer.desc")} />
      {servers.length > 0 && <ServerToServer />}
      {servers.length > 0 && <PcToServer />}
      <section className="block">
        <div className="sect-head"><h2>{t("transfer.s3.title")}</h2></div>
        <p className="small muted" style={{ margin: "0 0 10px" }}>{t("transfer.s3.text")}</p>
        <div style={{ display: "flex", gap: 14, flexWrap: "wrap" }}>
          <Send />
          <Receive />
        </div>
      </section>
    </div>
  );
}
