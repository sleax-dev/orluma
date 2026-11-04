import { Button, Menu, MenuItem, MenuList, MenuPopover, MenuTrigger, Spinner, Switch } from "@fluentui/react-components";
import {
  Add16Regular,
  ArrowRight12Regular,
  ShieldCheckmark20Regular,
  Calendar12Regular,
  Copy16Regular,
  Delete16Regular,
  Edit16Regular,
  History16Regular,
  MoreHorizontal20Regular,
  Play16Filled,
  SearchSparkle16Regular,
} from "@fluentui/react-icons";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { NoServer } from "../components/guide";
import { JobIcon } from "../components/SourceIcon";
import { JobEditor } from "../components/JobEditor";
import { DestinationEditor } from "../components/DestinationEditor";
import { Confirm, Empty, LiveRun, PageHead, ServerPicker, Skeleton, Status } from "../components/ui";
import { api, errText } from "../lib/api";
import { describe } from "../lib/cron";
import { newDestination, newId, newJob } from "../lib/defaults";
import { bytes, relative } from "../lib/format";
import type { Job, Source } from "../lib/types";
import { useApp } from "../state";

export function sourceLabel(s: Source): string {
  switch (s.type) {
    case "paths":
      return s.paths.join(", ");
    case "sqlite":
      return s.path;
    case "command":
      return s.filename;
    case "mongo":
      return s.container || "mongodb";
    default:
      return [s.container || s.host || "local", "databases" in s && s.databases.length ? s.databases.join(", ") : ""].filter(Boolean).join(" · ");
  }
}

export function Jobs() {
  const { t } = useTranslation();
  const { server, servers, configs, loadConfig, saveConfig, live, toast, setPage, refreshStatus } = useApp();
  const [editing, setEditing] = useState<Job | null>(null);
  const [deleting, setDeleting] = useState<Job | null>(null);
  const [addingDest, setAddingDest] = useState(false);
  const [starting, setStarting] = useState<string | null>(null);

  useEffect(() => {
    if (server) void loadConfig(server);
  }, [server, loadConfig]);

  if (!server) return <div className="page"><PageHead title={t("nav.jobs")} desc={t("jobs.desc")} info={t("jobs.info")} /><NoServer page="jobs" /></div>;
  const cfg = configs[server];
  const st = live[server]?.status;
  const dests = Object.fromEntries((cfg?.destinations ?? []).map((d) => [d.id, d.name]));

  const save = async (job: Job) => {
    if (!cfg) return;
    const next = { ...cfg, jobs: cfg.jobs.some((j) => j.id === job.id) ? cfg.jobs.map((j) => (j.id === job.id ? job : j)) : [...cfg.jobs, job] };
    if (await saveConfig(server, next)) {
      setEditing(null);
      toast("success", t("job.saved"));
    }
  };

  const runNow = async (job: Job) => {
    setStarting(job.id);
    try {
      await api.runJob(server, job.id);
      toast("info", t("job.started", { name: job.name }));
      void refreshStatus(server);
    } catch (e) {
      toast("error", errText(e));
    } finally {
      setStarting(null);
    }
  };

  return (
    <div className="page">
      <PageHead
        title={t("nav.jobs")}
        desc={t("jobs.desc")}
        info={t("jobs.info")}
        actions={
          cfg && cfg.jobs.length === 0 ? undefined : (
          <>
            <Button icon={<SearchSparkle16Regular />} disabled={!st} onClick={() => setPage("backup", { server })}>
              {t("servers.whatToBackUp")}
            </Button>
            <Button appearance="primary" icon={<Add16Regular />} disabled={!cfg} onClick={() => setEditing(newJob({ destination: cfg?.destinations[0]?.id ?? "" }))}>
              {t("jobs.new")}
            </Button>
          </>
          )
        }
      />
      <ServerPicker />
      {!cfg ? (
        live[server]?.online === false ? (
          <Empty title={t("jobs.offline", { server: servers.find((s) => s.id === server)?.name })} text={live[server]?.error ?? ""} />
        ) : (
          <div className="stack">
            <Skeleton h={72} r={12} />
            <Skeleton h={72} r={12} />
          </div>
        )
      ) : cfg.jobs.length === 0 ? (
        <Empty
          icon={<ShieldCheckmark20Regular />}
          title={t("jobs.none")}
          text={t("jobs.noneText")}
          action={
            <Button appearance="primary" icon={<SearchSparkle16Regular />} onClick={() => setPage("backup", { server })}>
              {t("servers.whatToBackUp")}
            </Button>
          }
        />
      ) : (
        <div className="rows">
          {cfg.jobs.map((job) => {
            const state = st?.jobs.find((j) => j.id === job.id);
            const running = st?.running.find((r) => r.job_id === job.id);
            const last = state?.last_run;
            return (
              <div key={job.id} className={`row stackrow${job.enabled ? "" : " off"}`}>
                <div style={{ display: "flex", gap: 16, alignItems: "center", width: "100%" }}>
                  <span className="icon">
                    <JobIcon job={job} />
                  </span>
                  <div className="info">
                    <div className="gap-s">
                      <span className="name">{job.name}</span>
                      <div className="tags">
                        {job.sources
                          .filter((s) => t(`source.type.${s.type}`).toLowerCase() !== job.name.trim().toLowerCase())
                          .map((s) => (
                            <span className="tag" key={s.id} title={sourceLabel(s)}>
                              {t(`source.type.${s.type}`)}
                            </span>
                          ))}
                      </div>
                    </div>
                    <div className="meta">
                      <span>
                        {dests[job.destination] ?? "?"}
                        {job.copies.map((c) => (
                          <span key={c} style={{ display: "inline-flex", alignItems: "center", gap: 4 }}>
                            <ArrowRight12Regular /> {dests[c] ?? "?"}
                          </span>
                        ))}
                      </span>
                      <span>
                        <Calendar12Regular /> {describe(job.schedule.cron, t)}
                      </span>
                      {job.enabled && state?.next_run && <span>{t("jobs.next", { when: relative(state.next_run) })}</span>}
                    </div>
                  </div>
                  <div className="side">
                    {last ? <Status status={last.status} /> : <Status status="never" />}
                    {last && (
                      <span>
                        {relative(last.started)} · {bytes(last.bytes_total)}
                      </span>
                    )}
                  </div>
                  <div className="ctl">
                    <Switch checked={job.enabled} aria-label={t("job.enabled")} onChange={(_, d) => void saveConfig(server, { ...cfg, jobs: cfg.jobs.map((j) => (j.id === job.id ? { ...j, enabled: d.checked } : j)) })} />
                    <Button size="small" appearance="primary" icon={starting === job.id ? <Spinner size="extra-tiny" /> : <Play16Filled />} disabled={!!running || starting === job.id} onClick={() => void runNow(job)}>
                      {t("jobs.runNow")}
                    </Button>
                    <Menu>
                      <MenuTrigger disableButtonEnhancement>
                        <Button size="small" appearance="subtle" icon={<MoreHorizontal20Regular />} aria-label={t("common.more")} />
                      </MenuTrigger>
                      <MenuPopover>
                        <MenuList>
                          <MenuItem icon={<Edit16Regular />} onClick={() => setEditing(structuredClone(job))}>{t("common.edit")}</MenuItem>
                          <MenuItem icon={<History16Regular />} onClick={() => setPage("activity", { server, job: job.id })}>{t("jobs.history")}</MenuItem>
                          <MenuItem icon={<Copy16Regular />} onClick={() => setEditing({ ...structuredClone(job), id: newId("j"), name: t("jobs.copyOf", { name: job.name }) })}>{t("jobs.duplicate")}</MenuItem>
                          <MenuItem icon={<Delete16Regular />} onClick={() => setDeleting(job)}>{t("common.delete")}</MenuItem>
                        </MenuList>
                      </MenuPopover>
                    </Menu>
                  </div>
                </div>
                {running && <div style={{ width: "100%" }}><LiveRun p={running} title={job.name} onCancel={() => void api.cancelRun(server, running.run_id)} /></div>}
                {last?.status === "failed" && last.error && !running && <div className="desc bad-text selectable small" style={{ paddingInlineStart: 50 }}>{last.error}</div>}
              </div>
            );
          })}
        </div>
      )}
      {editing && cfg && <JobEditor open job={editing} config={cfg} onClose={() => setEditing(null)} onSave={save} onAddDestination={() => setAddingDest(true)} />}
      {addingDest && cfg && (
        <DestinationEditor
          open
          server={server}
          initial={newDestination("local", t("dest.type.local"))}
          onClose={() => setAddingDest(false)}
          onSave={async (d) => {
            if (await saveConfig(server, { ...cfg, destinations: [...cfg.destinations, d] })) setAddingDest(false);
          }}
        />
      )}
      <Confirm
        open={!!deleting}
        title={t("jobs.deleteTitle", { name: deleting?.name })}
        text={t("jobs.deleteText")}
        confirm={t("common.delete")}
        danger
        onClose={() => setDeleting(null)}
        onConfirm={async () => {
          if (cfg && deleting) await saveConfig(server, { ...cfg, jobs: cfg.jobs.filter((j) => j.id !== deleting.id) });
          setDeleting(null);
        }}
      />
    </div>
  );
}
