import { Button } from "@fluentui/react-components";
import { Add16Regular, CheckmarkCircle16Regular, CheckmarkCircle20Regular, ErrorCircle16Regular, ServerMultiple20Regular, ShieldCheckmark16Regular, Warning16Regular } from "@fluentui/react-icons";
import { useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { GetStarted } from "../components/guide";
import { LiveRun, Skeleton, Status } from "../components/ui";
import { api } from "../lib/api";
import { bytes, relative } from "../lib/format";
import type { RunSummary } from "../lib/types";
import { useApp } from "../state";

const DAY = 86_400_000;

export function Overview() {
  const { t } = useTranslation();
  const { servers, live, configs, loadConfig, setPage, running } = useApp();

  useEffect(() => {
    servers.forEach((s) => void loadConfig(s.id));
    // configs are read once per visit; their changes come in as events
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [servers]);

  const jobName = (server: string, id: string) => configs[server]?.jobs.find((j) => j.id === id)?.name ?? id;

  const facts = useMemo(() => {
    const online = servers.filter((s) => live[s.id]?.online).length;
    const states = servers.flatMap((s) => (live[s.id]?.status?.jobs ?? []).map((j) => ({ server: s.id, ...j })));
    const last = states.map((j) => j.last_run).filter((r): r is RunSummary => !!r);
    const recent = last.filter((r) => Date.now() - Date.parse(r.started) < DAY);
    const protectedBytes = last.filter((r) => r.status === "success" || r.status === "warning").reduce((a, r) => a + r.bytes_total, 0);
    const next = states.map((j) => j.next_run).filter((x): x is string => !!x).sort()[0] ?? null;
    const problems: { server: string; kind: "offline" | "failed" | "warning" | "disk"; text: string }[] = [];
    for (const s of servers) {
      const l = live[s.id];
      if (l?.online === false) problems.push({ server: s.id, kind: "offline", text: t("overview.problem.offline", { server: s.name, error: l.error ?? "" }) });
      for (const j of l?.status?.jobs ?? []) {
        if (j.last_run?.status === "failed") problems.push({ server: s.id, kind: "failed", text: t("overview.problem.failed", { job: jobName(s.id, j.id), server: s.name, error: j.last_run.error ?? "" }) });
        else if (j.last_run?.status === "warning") problems.push({ server: s.id, kind: "warning", text: t("overview.problem.warning", { job: jobName(s.id, j.id), server: s.name }) });
      }
      for (const d of l?.status?.disks ?? []) {
        const used = 1 - d.available / d.total;
        if (used >= 0.9) problems.push({ server: s.id, kind: "disk", text: t("overview.problem.disk", { mount: d.mount, server: s.name, percent: Math.round(used * 100) }) });
      }
    }
    return {
      online,
      jobs: states.length,
      ran: last.length > 0,
      ok24: recent.filter((r) => r.status === "success" || r.status === "warning").length,
      bad24: recent.filter((r) => r.status === "failed").length,
      protectedBytes,
      next,
      problems,
    };
    // jobName reads configs, which is in the list
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [servers, live, configs, t]);

  if (servers.length === 0) {
    return (
      <div className="page">
        <div className="lead">
          <h1>{t("overview.welcome")}</h1>
          <p>{t("overview.welcomeText")}</p>
        </div>
        <GetStarted />
      </div>
    );
  }

  const loading = servers.some((s) => live[s.id] == null || live[s.id].online == null);

  return (
    <div className="page">
      <div className="lead">
        <h1>{!facts.ran ? t("overview.welcome") : facts.problems.length === 0 ? t("overview.allGood") : t("overview.attention", { count: facts.problems.length })}</h1>
        <p>{!facts.ran ? t("overview.welcomeText") : facts.next ? t("overview.nextBackup", { when: relative(facts.next) }) : t("overview.noSchedule")}</p>
      </div>

      <GetStarted compact />

      {facts.ran && <div className="facts">
        <span className={`fact${facts.online < servers.length ? " bad" : " ok"}`}>
          {loading ? <Skeleton h={16} w={90} /> : <><CheckmarkCircle16Regular /><span><b>{facts.online}/{servers.length}</b> {t("overview.fact.online")}</span></>}
        </span>
        <span className="fact ok"><ShieldCheckmark16Regular /><span><b>{facts.ok24}</b> {t("overview.fact.ok")}</span></span>
        {facts.bad24 > 0 && <span className="fact bad"><ErrorCircle16Regular /><span><b>{facts.bad24}</b> {t("overview.fact.failed")}</span></span>}
        <span className="fact"><span><b>{bytes(facts.protectedBytes)}</b> {t("overview.fact.protected")}</span></span>
      </div>}

      {running.length > 0 && (
        <section className="block">
          <div className="sect-head">
            <h2>{t("overview.runningNow")}</h2>
          </div>
          {servers.flatMap((s) =>
            (live[s.id]?.status?.running ?? []).map((p) => (
              <LiveRun key={p.run_id} p={p} title={`${jobName(s.id, p.job_id)} · ${s.name}`} onCancel={() => void api.cancelRun(s.id, p.run_id)} />
            )),
          )}
        </section>
      )}

      {facts.problems.length > 0 && (
        <section className="block">
          <div className="sect-head">
            <h2>{t("overview.needsAttention")}</h2>
          </div>
          <div className="rows">
            {facts.problems.map((p, i) => (
              <div
                key={i}
                className="row clickable"
                role="button"
                tabIndex={0}
                onClick={() => setPage(p.kind === "offline" || p.kind === "disk" ? "servers" : "activity", { server: p.server })}
              >
                <span className="icon" style={{ color: p.kind === "warning" || p.kind === "disk" ? "var(--warn)" : "var(--bad)" }}>
                  {p.kind === "warning" || p.kind === "disk" ? <Warning16Regular /> : <ErrorCircle16Regular />}
                </span>
                <div className="info">
                  <div className="desc selectable" style={{ color: "var(--text)", marginTop: 0 }}>{p.text}</div>
                </div>
              </div>
            ))}
          </div>
        </section>
      )}

      <section className="block">
        <div className="sect-head">
          <h2>{t("overview.servers")}</h2>
          <Button size="small" icon={<Add16Regular />} onClick={() => setPage("servers", { add: "1" })}>
            {t("servers.add")}
          </Button>
        </div>
        <div className="cards">
          {servers.map((s) => {
            const l = live[s.id];
            const st = l?.status;
            const jobs = st?.jobs ?? [];
            const lastOk = jobs.map((j) => j.last_success).filter(Boolean).sort().reverse()[0] ?? null;
            const nextRun = jobs.map((j) => j.next_run).filter(Boolean).sort()[0] ?? null;
            const root = st?.disks.find((d) => d.mount === "/") ?? st?.disks[0];
            const used = root ? 1 - root.available / root.total : 0;
            return (
              <button key={s.id} className="card" onClick={() => setPage("jobs", { server: s.id })}>
                <div className="card-head">
                  <span className="avatar">
                    <ServerMultiple20Regular />
                  </span>
                  <div className="ellipsis">
                    <div className="name ellipsis">{s.name}</div>
                    <div className="host ellipsis">{st?.os ?? s.host}</div>
                  </div>
                  <Status status={l?.online === true ? "online" : l?.online === false ? "offline" : "unknown"} />
                </div>
                {st ? (
                  <dl className="kv">
                    <dt>{t("overview.card.jobs")}</dt>
                    <dd>{jobs.length}</dd>
                    <dt>{t("overview.card.lastOk")}</dt>
                    <dd>{relative(lastOk)}</dd>
                    <dt>{t("overview.card.next")}</dt>
                    <dd>{relative(nextRun)}</dd>
                  </dl>
                ) : l?.online === false ? (
                  <p className="small bad-text ellipsis">{l.error}</p>
                ) : (
                  <Skeleton h={58} />
                )}
                {root && (
                  <div className="disk-line">
                    <span>{root.mount}</span>
                    <div className={`meter${used > 0.9 ? " full" : used > 0.8 ? " warn" : ""}`}>
                      <i style={{ transform: `scaleX(${used})` }} />
                    </div>
                    <span className="num">{t("overview.card.free", { size: bytes(root.available) })}</span>
                  </div>
                )}
              </button>
            );
          })}
        </div>
      </section>
      {facts.problems.length === 0 && !loading && (
        <p className="ok-line small">
          <CheckmarkCircle20Regular /> {t("overview.allGoodText")}
        </p>
      )}
    </div>
  );
}
