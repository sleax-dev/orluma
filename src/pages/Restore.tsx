import { Button, Checkbox, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Dropdown, Option, Spinner } from "@fluentui/react-components";
import {
  ArrowCounterclockwise16Regular,
  ArrowDownload16Regular,
  ArrowUp16Regular,
  Delete16Regular,
  Document16Regular,
  Folder16Filled,
  History20Regular,
  Link16Regular,
} from "@fluentui/react-icons";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Confirm, Empty, PageHead, ServerPicker, Skeleton, TextField, useAction } from "../components/ui";
import { api, errText } from "../lib/api";
import { newId } from "../lib/defaults";
import { bytes, dateTime, num, relative } from "../lib/format";
import type { FsNode, Snapshot } from "../lib/types";
import { useApp } from "../state";

function parentOf(p: string) {
  const i = p.replace(/\/$/, "").lastIndexOf("/");
  return i <= 0 ? "/" : p.slice(0, i);
}

export function RestorePage() {
  const { t } = useTranslation();
  const { server, configs, loadConfig, params, toast } = useApp();
  const [dest, setDest] = useState(params.destination ?? "");
  const [snaps, setSnaps] = useState<Snapshot[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sel, setSel] = useState<Snapshot | null>(null);
  const [path, setPath] = useState("/");
  const [nodes, setNodes] = useState<FsNode[] | null>(null);
  const [filter, setFilter] = useState("");
  const [restoring, setRestoring] = useState<{ include: string[]; target: string; overwrite: boolean } | null>(null);
  const [forgetting, setForgetting] = useState<Snapshot | null>(null);
  const { busy, run } = useAction();

  useEffect(() => {
    if (server) void loadConfig(server);
  }, [server, loadConfig]);
  const cfg = server ? configs[server] : undefined;
  useEffect(() => {
    if (cfg && !cfg.destinations.some((d) => d.id === dest)) setDest(cfg.destinations[0]?.id ?? "");
  }, [cfg, dest]);

  const load = () => {
    if (!server || !dest) return;
    setSnaps(null);
    setError(null);
    setSel(null);
    api
      .snapshots(server, dest)
      .then((s) => {
        setSnaps(s);
        if (s[0]) open(s[0]);
      })
      .catch((e) => setError(errText(e)));
  };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(load, [server, dest]);

  const open = (s: Snapshot, at?: string) => {
    setSel(s);
    // a database dump is one file at the top; folders start where the snapshot's first path does
    const start = at ?? (s.tags.some((x) => x.startsWith("kind:") && x !== "kind:paths") ? parentOf(s.paths[0] ?? "/") : (s.paths[0] ?? "/"));
    setPath(start);
    setNodes(null);
    if (!server) return;
    api
      .ls(server, dest, s.id, start)
      .then(setNodes)
      .catch((e) => {
        setNodes([]);
        toast("error", errText(e));
      });
  };

  const jobName = (id: string | null) => cfg?.jobs.find((j) => j.id === id)?.name ?? id ?? t("restore.manual");
  const sourceType = (s: Snapshot) => s.tags.find((x) => x.startsWith("kind:"))?.slice(5) ?? "paths";
  const shown = useMemo(() => (snaps ?? []).filter((s) => !filter || s.job === filter), [snaps, filter]);
  const jobs = useMemo(() => [...new Set((snaps ?? []).map((s) => s.job ?? ""))], [snaps]);

  const download = async (n: FsNode | null) => {
    if (!server || !sel) return;
    const target = n ?? { name: path.split("/").pop() || "snapshot", path, kind: "dir", size: 0, mtime: null };
    const zip = target.kind === "dir";
    const name = `${target.name || "snapshot"}${zip ? ".zip" : ""}`;
    const save = await api.pickSave(name);
    if (!save) return;
    await run(`dl${target.path}`, async () => {
      const n = await api.download({ id: newId("dl"), server, destination: dest, snapshot: sel.id, path: target.path, zip, target: save });
      toast("success", t("restore.downloaded", { size: bytes(n), file: save }));
    });
  };

  const crumbs = path.split("/").filter(Boolean);

  return (
    <div className="page" style={{ maxWidth: 1180 }}>
      <PageHead
        title={t("nav.restore")}
        desc={t("restore.desc")}
        actions={
          cfg && cfg.destinations.length > 0 ? (
            <Dropdown value={cfg.destinations.find((d) => d.id === dest)?.name ?? ""} selectedOptions={[dest]} onOptionSelect={(_, o) => setDest(o.optionValue ?? "")}>
              {cfg.destinations.map((d) => <Option key={d.id} value={d.id}>{d.name}</Option>)}
            </Dropdown>
          ) : undefined
        }
      />
      <ServerPicker />
      {cfg && cfg.destinations.length === 0 && <Empty icon={<History20Regular />} title={t("storage.none")} text={t("storage.noneText")} />}
      {error && <p className="bad-text selectable">{error}</p>}
      {cfg && cfg.destinations.length > 0 && !error && (
        <div className="split">
          <div className="rows">
            <div className="row" style={{ padding: "10px 14px" }}>
              <Dropdown size="small" style={{ minWidth: 0, width: "100%" }} value={filter ? jobName(filter) : t("restore.allJobs")} selectedOptions={[filter]} onOptionSelect={(_, o) => setFilter(o.optionValue ?? "")}>
                <Option value="">{t("restore.allJobs")}</Option>
                {jobs.map((j) => <Option key={j} value={j}>{jobName(j || null)}</Option>)}
              </Dropdown>
            </div>
            <div className="snap-list">
              {snaps === null && <div style={{ padding: 14 }} className="stack"><Skeleton h={36} /><Skeleton h={36} /><Skeleton h={36} /></div>}
              {snaps?.length === 0 && <p className="muted small" style={{ padding: 14, margin: 0 }}>{t("restore.empty")}</p>}
              {shown.map((s) => (
                <div key={s.id} className={`snap${sel?.id === s.id ? " on" : ""}`} role="button" tabIndex={0} onClick={() => open(s)} onKeyDown={(e) => e.key === "Enter" && open(s)}>
                  <span className="when">{dateTime(s.time)}</span>
                  <span className="what ellipsis">
                    {jobName(s.job)} · {t(`source.type.${sourceType(s)}`)} · {bytes(s.bytes)}
                  </span>
                  <span className="what mono">{s.short_id} · {relative(s.time)}</span>
                </div>
              ))}
            </div>
          </div>
          <div className="stack">
            {sel ? (
              <>
                <div className="panel" style={{ gap: 8 }}>
                  <div className="sect-head">
                    <div className="crumbs">
                      <button onClick={() => open(sel, "/")}>/</button>
                      {crumbs.map((c, i) => (
                        <button key={i} onClick={() => open(sel, "/" + crumbs.slice(0, i + 1).join("/"))}>
                          {c}
                        </button>
                      ))}
                    </div>
                    <div className="gap-s">
                      <Button size="small" icon={<ArrowUp16Regular />} disabled={path === "/"} onClick={() => open(sel, parentOf(path))} aria-label={t("restore.up")} />
                      <Button size="small" icon={busy === `dl${path}` ? <Spinner size="extra-tiny" /> : <ArrowDownload16Regular />} onClick={() => void download(null)}>
                        {t("restore.downloadFolder")}
                      </Button>
                      <Button size="small" appearance="primary" icon={<ArrowCounterclockwise16Regular />} onClick={() => setRestoring({ include: path === "/" ? [] : [path], target: `/var/tmp/orluma-restore/${sel.short_id}`, overwrite: false })}>
                        {t("restore.toServer")}
                      </Button>
                    </div>
                  </div>
                  <div className="small muted">
                    {t("restore.snapshotInfo", { id: sel.short_id, host: sel.hostname, files: num(sel.files), size: bytes(sel.bytes) })}
                  </div>
                </div>
                <div className="rows files">
                  {nodes === null && <div style={{ padding: 14 }} className="stack"><Skeleton h={18} /><Skeleton h={18} /><Skeleton h={18} /></div>}
                  {nodes?.length === 0 && <p className="muted small" style={{ padding: 14, margin: 0 }}>{t("restore.emptyFolder")}</p>}
                  {nodes?.map((n) => (
                    <div key={n.path} className={`file${n.kind === "dir" ? " dir" : ""}`}>
                      {n.kind === "dir" ? <Folder16Filled /> : n.kind === "symlink" ? <Link16Regular /> : <Document16Regular />}
                      {n.kind === "dir" ? (
                        <button className="n" onClick={() => open(sel, n.path)}>{n.name}</button>
                      ) : (
                        <span className="n selectable">{n.name}</span>
                      )}
                      <span className="s">{n.kind === "dir" ? "" : bytes(n.size)}</span>
                      <span className="m">{dateTime(n.mtime)}</span>
                      <Button size="small" appearance="subtle" icon={busy === `dl${n.path}` ? <Spinner size="extra-tiny" /> : <ArrowDownload16Regular />} aria-label={t("restore.download")} title={t("restore.download")} onClick={() => void download(n)} />
                    </div>
                  ))}
                </div>
                <Button size="small" appearance="subtle" icon={<Delete16Regular />} style={{ alignSelf: "flex-end" }} onClick={() => setForgetting(sel)}>
                  {t("restore.forget")}
                </Button>
              </>
            ) : (
              snaps !== null && <Empty icon={<History20Regular />} title={t("restore.pick")} />
            )}
          </div>
        </div>
      )}

      {restoring && sel && (
        <Dialog open onOpenChange={(_, d) => !d.open && setRestoring(null)}>
          <DialogSurface className="dialog-wide">
            <DialogBody>
              <DialogTitle>{t("restore.toServerTitle")}</DialogTitle>
              <DialogContent>
                <div className="stack">
                  <p className="muted" style={{ margin: 0 }}>
                    {restoring.include.length ? t("restore.toServerText", { path: restoring.include[0], id: sel.short_id }) : t("restore.toServerAll", { id: sel.short_id })}
                  </p>
                  <TextField label={t("restore.target")} hint={t("restore.targetHint")} value={restoring.target} onChange={(v) => setRestoring({ ...restoring, target: v })} className="mono" />
                  <Checkbox checked={restoring.overwrite} onChange={(_, d) => setRestoring({ ...restoring, overwrite: !!d.checked })} label={t("restore.overwrite")} />
                  {restoring.overwrite && <p className="small" style={{ margin: 0, color: "var(--warn)" }}>{t("restore.overwriteWarn")}</p>}
                </div>
              </DialogContent>
              <DialogActions>
                <Button onClick={() => setRestoring(null)}>{t("common.cancel")}</Button>
                <Button
                  appearance="primary"
                  disabled={!restoring.target.startsWith("/") || restoring.target.trim() === "/"}
                  onClick={async () => {
                    const ok = await run("restore", () => api.restore(server!, { destination: dest, snapshot: sel.id, include: restoring.include, target: restoring.target.trim(), overwrite: restoring.overwrite }), t("restore.started"));
                    if (ok) setRestoring(null);
                  }}
                >
                  {t("restore.start")}
                </Button>
              </DialogActions>
            </DialogBody>
          </DialogSurface>
        </Dialog>
      )}

      <Confirm
        open={!!forgetting}
        title={t("restore.forgetTitle")}
        text={t("restore.forgetText", { id: forgetting?.short_id, when: dateTime(forgetting?.time) })}
        confirm={t("restore.forget")}
        danger
        onClose={() => setForgetting(null)}
        onConfirm={async () => {
          if (forgetting && server) await run("forget", () => api.forget(server, dest, [forgetting.id]), t("restore.forgotten"));
          setForgetting(null);
          load();
        }}
      />
    </div>
  );
}
