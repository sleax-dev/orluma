import { Button, Checkbox, Field, Input, Radio, RadioGroup, Spinner } from "@fluentui/react-components";
import { ArrowLeft16Regular, ArrowRight16Regular, Checkmark12Regular } from "@fluentui/react-icons";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, errText } from "../lib/api";
import { bytes } from "../lib/format";
import { DEFAULT_EXCLUDES, newId, newJob, randomPassword } from "../lib/defaults";
import type { Discovery, FoundDatabase, Source } from "../lib/types";
import { useApp } from "../state";
import { FolderTree, type Sizes } from "../components/FolderTree";
import { Info, NoServer } from "../components/guide";
import { PageHead } from "../components/ui";

/** "What should we back up?" as a page of three steps (databases, folders, where to): the agent's discovery, with
 *  nothing ticked for the user, turned into a destination and a job. */
export function BackupWizard() {
  const { t } = useTranslation();
  const { server, loadConfig, saveConfig, toast, setPage, live } = useApp();
  const [step, setStep] = useState(0);
  const [found, setFound] = useState<Discovery | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [dbs, setDbs] = useState<Set<number>>(new Set());
  const [paths, setPaths] = useState<Set<string>>(new Set());
  const [where, setWhere] = useState<"local" | "existing">("local");
  const [localPath, setLocalPath] = useState("/var/backups/orluma");
  const [existing, setExisting] = useState("");
  const [destinations, setDestinations] = useState<{ id: string; name: string }[]>([]);
  const [busy, setBusy] = useState(false);
  // folder -> measured size; measuring a big folder takes long, so the list shows first and sizes fill in
  const [sizes, setSizes] = useState<Sizes>({});
  const queue = useRef<string[]>([]);
  const active = useRef(0);
  const seen = useRef(new Set<string>());
  const alive = useRef(true);

  // Two measurements at a time (the agent allows two as well); folders just opened jump the queue.
  const pump = () => {
    while (active.current < 2 && queue.current.length) {
      const path = queue.current.shift()!;
      active.current++;
      api
        .pathSize(server!, path, DEFAULT_EXCLUDES)
        .then((r) => alive.current && setSizes((x) => ({ ...x, [path]: r })))
        .catch(() => alive.current && setSizes((x) => ({ ...x, [path]: { bytes: 0, complete: false } })))
        .finally(() => {
          active.current--;
          pump();
        });
    }
  };
  const measure = (path: string, first = false) => {
    if (seen.current.has(path)) return;
    seen.current.add(path);
    if (first) queue.current.unshift(path);
    else queue.current.push(path);
    pump();
  };

  useEffect(() => {
    if (!server) return;
    setStep(0);
    setFound(null);
    setSizes({});
    setError(null);
    queue.current = [];
    seen.current = new Set();
    alive.current = true;
    void loadConfig(server, true).then((c) => {
      const d = c?.destinations.map((x) => ({ id: x.id, name: x.name })) ?? [];
      setDestinations(d);
      if (d[0]) setExisting(d[0].id);
    });
    api
      .discover(server)
      .then((d) => {
        if (!alive.current) return;
        // nothing is ticked for the user: they choose what matters to them
        setFound(d);
        if (d.databases.length === 0) setStep(1); // nothing to choose in step 1
        setDbs(new Set());
        setPaths(new Set());
        d.paths.filter((p) => p.pending).forEach((p) => measure(p.path));
      })
      .catch((e) => alive.current && setError(errText(e)));
    return () => {
      alive.current = false;
    };
    // loadConfig changes whenever configs does, so listing it here re-ran the discovery after every config read
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [server]);

  // what is ticked, against the free space of the disk the backup folder is on (the longest mount that contains it)
  const known = new Map((found?.paths ?? []).filter((p) => !p.pending).map((p) => [p.path, p.bytes]));
  const need = [...paths].reduce((a, p) => a + (known.get(p) ?? sizes[p]?.bytes ?? 0), 0);
  const disk = ((server ? live[server] : undefined)?.status?.disks ?? []).filter((d) => localPath.startsWith(d.mount)).sort((a, b) => b.mount.length - a.mount.length)[0];
  const free = disk?.available ?? 0;

  const toSource = (d: FoundDatabase): Source => {
    const conn = { host: d.host, port: d.port, user: d.user, password: d.password, container: d.container, os_user: d.os_user };
    switch (d.kind) {
      case "postgres":
        return { type: "postgres", id: newId("src"), databases: d.databases, globals: true, ...conn };
      case "mysql":
        return { type: "mysql", id: newId("src"), databases: d.databases, ...conn };
      case "redis":
        return { type: "redis", id: newId("src"), ...conn };
      case "clickhouse":
        return { type: "clickhouse", id: newId("src"), databases: d.databases, ...conn };
      default: {
        const auth = d.user ? `${encodeURIComponent(d.user)}:${encodeURIComponent(d.password)}@` : "";
        return { type: "mongo", id: newId("src"), uri: d.container ? "" : `mongodb://${auth}127.0.0.1:27017/?authSource=admin`, container: d.container };
      }
    }
  };

  const create = async () => {
    if (!server) return;
    const cfg = await loadConfig(server, true);
    if (!cfg || !found) return;
    setBusy(true);
    try {
      let destId = existing;
      if (where === "local" || !destId) {
        destId = newId("d");
        cfg.destinations.push({ id: destId, name: t("first.localName"), kind: { type: "local", path: localPath.trim() }, password: randomPassword(), limit_upload_kib: 0, limit_download_kib: 0 });
      }
      const sources: Source[] = [];
      if (paths.size > 0) sources.push({ type: "paths", id: newId("src"), paths: [...paths], excludes: [...DEFAULT_EXCLUDES], one_file_system: false });
      found.databases.forEach((d, i) => dbs.has(i) && sources.push(toSource(d)));
      if (sources.length === 0) {
        toast("error", t("first.nothing"));
        return;
      }
      cfg.jobs.push(newJob({ name: t("first.jobName"), destination: destId, sources }));
      if (await saveConfig(server, cfg)) {
        toast("success", t("first.created"));
        setPage("jobs", { server });
      }
    } finally {
      setBusy(false);
    }
  };

  if (!server) return <div className="page"><PageHead title={t("first.title")} /><NoServer page="jobs" /></div>;

  const last = step === 2;
  const steps = ["databases", "folders", "where"] as const;
  const count = dbs.size + paths.size;
  const noSpace = where === "local" && need > free && free > 0;

  return (
    <div className="page wizard">
      <PageHead title={t("first.title")} desc={t("first.text")} />
      <ol className="steps" aria-label={t("first.title")}>
        {steps.map((k, i) => (
          <li key={k} className={i === step ? "now" : i < step ? "done" : ""} aria-current={i === step ? "step" : undefined}>
            <button type="button" disabled={!found || i > step} onClick={() => setStep(i)}>
              <span className="n">{i < step ? <Checkmark12Regular /> : i + 1}</span>
              {t(`first.${k}`)}
            </button>
          </li>
        ))}
      </ol>
      <div className="card wizard-body">
        {error && <p className="bad-text">{error}</p>}
        {!found && !error && <Spinner size="small" label={t("first.scanning")} labelPosition="after" />}
        {found && step === 0 &&
          (found.databases.length === 0 ? (
            <p className="muted" style={{ margin: 0 }}>{t("first.noDatabases")}</p>
          ) : (
            <div className="check-list">
              {found.databases.map((d, i) => (
                <div className="check-row" key={i}>
                  <Checkbox
                    checked={dbs.has(i)}
                    onChange={(_, c) => setDbs((s) => { const n = new Set(s); if (c.checked) n.add(i); else n.delete(i); return n; })}
                    label={<><b>{t(`source.type.${d.kind}`)}</b> <span className="muted">· {d.label}</span></>}
                  />
                </div>
              ))}
            </div>
          ))}
        {found && step === 1 && (
          <>
            <div className="form-sect" style={{ margin: 0 }}>
              {t("first.folders")} <Info text={t("first.skipsInfo", { list: DEFAULT_EXCLUDES.join(", ") })} />
            </div>
            <FolderTree server={server} roots={found.paths} selected={paths} onChange={setPaths} sizes={sizes} measure={measure} />
            <p className="muted small" style={{ margin: 0 }}>{t("first.skips")}</p>
          </>
        )}
        {found && step === 2 && (
          <>
            <RadioGroup value={where} onChange={(_, d) => setWhere(d.value as "local" | "existing")}>
              <Radio value="local" label={t("first.whereLocal")} />
              {destinations.length > 0 && <Radio value="existing" label={t("first.whereExisting")} />}
            </RadioGroup>
            {where === "local" ? (
              <Field label={t("first.folderOnServer")} hint={t("first.folderHint")}>
                <Input value={localPath} onChange={(_, d) => setLocalPath(d.value)} />
              </Field>
            ) : (
              <RadioGroup value={existing} onChange={(_, d) => setExisting(d.value)}>
                {destinations.map((d) => <Radio key={d.id} value={d.id} label={d.name} />)}
              </RadioGroup>
            )}
            <p className="muted small" style={{ margin: 0 }}>{t("first.defaults")}</p>
          </>
        )}
      </div>
      {found && count > 0 && <p className="muted small" style={{ margin: 0 }}>{t("first.chosen", { count, size: bytes(need) })}</p>}
      {found && noSpace && <p className="bad-text small" style={{ margin: 0 }}>{t("first.noSpace", { need: bytes(need), free: bytes(free) })}</p>}
      <div className="wizard-foot">
        <Button onClick={() => (step === 0 || (step === 1 && found?.databases.length === 0) ? setPage("jobs", { server }) : setStep(step - 1))}>
          {step === 0 || (step === 1 && found?.databases.length === 0) ? t("first.later") : <><ArrowLeft16Regular /> {t("first.back")}</>}
        </Button>
        {last ? (
          <Button appearance="primary" disabled={busy || count === 0} onClick={() => void create()}>
            {busy ? <Spinner size="tiny" /> : t("first.create")}
          </Button>
        ) : (
          <Button appearance="primary" disabled={!found} iconPosition="after" icon={<ArrowRight16Regular />} onClick={() => setStep(step + 1)}>
            {t("first.next")}
          </Button>
        )}
      </div>
    </div>
  );
}
