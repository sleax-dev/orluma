import { Button, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Dropdown, Option } from "@fluentui/react-components";
import { Pulse20Regular } from "@fluentui/react-icons";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { NoServer } from "../components/guide";
import { Empty, LiveRun, PageHead, RefreshButton, Skeleton, Status } from "../components/ui";
import { api } from "../lib/api";
import { between, bytes, dateTime, num, relative, time } from "../lib/format";
import type { RunRecord, RunStatus, RunSummary } from "../lib/types";
import { useApp } from "../state";

type Row = RunSummary & { server: string };

export function Activity() {
  const { t } = useTranslation();
  const { servers, live, configs, loadConfig, params } = useApp();
  const [rows, setRows] = useState<Row[] | null>(null);
  const [srv, setSrv] = useState(params.server ?? "");
  const [job, setJob] = useState(params.job ?? "");
  const [status, setStatus] = useState<"" | RunStatus>("");
  const [detail, setDetail] = useState<(RunRecord & { server: string }) | null>(null);

  const load = useCallback(() => {
    setRows(null);
    const list = srv ? servers.filter((s) => s.id === srv) : servers;
    list.forEach((s) => void loadConfig(s.id));
    void Promise.all(list.map((s) => api.runs(s.id, job || undefined, 150).then((r) => r.map((x) => ({ ...x, server: s.id }))).catch(() => [] as Row[]))).then((all) =>
      setRows(all.flat().sort((a, b) => b.started.localeCompare(a.started))),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [servers, srv, job]);
  useEffect(load, [load]);

  // Reload when a run finishes anywhere, so the list matches the live cards.
  const finished = useMemo(() => Object.values(live).map((l) => l.status?.running.length ?? 0).join(","), [live]);
  useEffect(() => {
    const id = setTimeout(load, 1500);
    return () => clearTimeout(id);
  }, [finished, load]);

  const name = (s: string) => servers.find((x) => x.id === s)?.name ?? s;
  const jobName = (s: string, id: string) => {
    if (id.startsWith("dest:")) return configs[s]?.destinations.find((d) => d.id === id.slice(5))?.name ?? id;
    if (id === "restore") return t("activity.restore");
    return configs[s]?.jobs.find((j) => j.id === id)?.name ?? id;
  };
  const shown = (rows ?? []).filter((r) => !status || r.status === status);
  const runningNow = servers.flatMap((s) => (live[s.id]?.status?.running ?? []).filter(() => !srv || s.id === srv).map((p) => ({ s: s.id, p })));
  const allJobs = srv ? configs[srv]?.jobs ?? [] : [];

  return (
    <div className="page">
      <PageHead title={t("nav.activity")} desc={t("activity.desc")} info={t("activity.info")} actions={<RefreshButton onClick={load} />} />
      <div className="logbar">
        <div className="gap-s">
          <Dropdown size="small" value={srv ? name(srv) : t("activity.allServers")} selectedOptions={[srv]} onOptionSelect={(_, o) => { setSrv(o.optionValue ?? ""); setJob(""); }}>
            <Option value="">{t("activity.allServers")}</Option>
            {servers.map((s) => <Option key={s.id} value={s.id}>{s.name}</Option>)}
          </Dropdown>
          {srv && (
            <Dropdown size="small" value={job ? jobName(srv, job) : t("restore.allJobs")} selectedOptions={[job]} onOptionSelect={(_, o) => setJob(o.optionValue ?? "")}>
              <Option value="">{t("restore.allJobs")}</Option>
              {allJobs.map((j) => <Option key={j.id} value={j.id}>{j.name}</Option>)}
            </Dropdown>
          )}
        </div>
        <div className="seg" role="radiogroup">
          {(["", "success", "warning", "failed"] as const).map((s) => (
            <button key={s} className={status === s ? "on" : ""} role="radio" aria-checked={status === s} onClick={() => setStatus(s)}>
              {s ? t(`status.${s}`) : t("activity.all")}
              <span className="n">{(rows ?? []).filter((r) => !s || r.status === s).length}</span>
            </button>
          ))}
        </div>
      </div>

      {runningNow.map(({ s, p }) => (
        <LiveRun key={p.run_id} p={p} title={`${jobName(s, p.job_id)} · ${name(s)} · ${t(`activity.kind.${p.kind}`)}`} onCancel={() => void api.cancelRun(s, p.run_id)} />
      ))}

      {servers.length === 0 ? (
        <NoServer page="activity" />
      ) : rows === null ? (
        <div className="stack">
          <Skeleton h={52} r={12} />
          <Skeleton h={52} r={12} />
          <Skeleton h={52} r={12} />
        </div>
      ) : shown.length === 0 ? (
        <Empty icon={<Pulse20Regular />} title={t("activity.none")} text={t("activity.noneText")} />
      ) : (
        <div className="rows">
          {shown.map((r) => (
            <div key={`${r.server}${r.id}`} className="row clickable" role="button" tabIndex={0} onClick={() => void api.run(r.server, r.id).then((d) => setDetail({ ...d, server: r.server }))}>
              <div className="info">
                <div className="gap-s">
                  <span className="name">{jobName(r.server, r.job_id)}</span>
                  <span className="tag">{t(`activity.kind.${r.kind}`)}</span>
                  {!srv && <span className="small muted">{name(r.server)}</span>}
                </div>
                <div className="meta">
                  <span>{dateTime(r.started)}</span>
                  <span>{between(r.started, r.finished)}</span>
                  {r.kind === "backup" && r.status !== "failed" && (
                    <>
                      <span>{t("activity.added", { size: bytes(r.bytes_added) })}</span>
                      <span>{t("activity.total", { size: bytes(r.bytes_total) })}</span>
                    </>
                  )}
                  <span>{t(`activity.trigger.${r.trigger}`)}</span>
                </div>
                {r.error && <div className="desc bad-text small ellipsis">{r.error}</div>}
              </div>
              <div className="side">
                <Status status={r.status} />
                <span>{relative(r.started)}</span>
              </div>
            </div>
          ))}
        </div>
      )}

      {detail && (
        <Dialog open onOpenChange={(_, d) => !d.open && setDetail(null)}>
          <DialogSurface className="dialog-xl">
            <DialogBody>
              <DialogTitle>
                {jobName(detail.server, detail.job_id)} · {name(detail.server)}
              </DialogTitle>
              <DialogContent>
                <div className="stack">
                  <div className="gap-s">
                    <Status status={detail.status} />
                    <span className="tag">{t(`activity.kind.${detail.kind}`)}</span>
                    <span className="small muted">{dateTime(detail.started)} · {between(detail.started, detail.finished)}</span>
                  </div>
                  {detail.kind === "backup" && (
                    <dl className="kv" style={{ maxWidth: 420 }}>
                      <dt>{t("activity.added", { size: "" }).trim()}</dt>
                      <dd>{bytes(detail.bytes_added)}</dd>
                      <dt>{t("activity.total", { size: "" }).trim()}</dt>
                      <dd>{bytes(detail.bytes_total)}</dd>
                      <dt>{t("activity.files")}</dt>
                      <dd>{t("activity.filesValue", { new: num(detail.files_new), changed: num(detail.files_changed) })}</dd>
                      <dt>{t("activity.snapshots")}</dt>
                      <dd className="mono">{detail.snapshots.map((s) => s.slice(0, 8)).join(" ") || "–"}</dd>
                    </dl>
                  )}
                  <pre className="logview">
                    {detail.log.map((l, i) => (
                      <div key={i} className={l.level === "warn" ? "w" : l.level === "error" ? "e" : undefined}>
                        <span className="t">{time(l.at)}</span>
                        {l.text}
                      </div>
                    ))}
                  </pre>
                </div>
              </DialogContent>
              <DialogActions>
                <Button onClick={() => void api.copy(detail.log.map((l) => `${l.at} ${l.level} ${l.text}`).join("\n"))}>{t("activity.copyLog")}</Button>
                <Button appearance="primary" onClick={() => setDetail(null)}>{t("common.close")}</Button>
              </DialogActions>
            </DialogBody>
          </DialogSurface>
        </Dialog>
      )}
    </div>
  );
}
