import { Button, Checkbox, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Field, Input, Radio, RadioGroup, Spinner } from "@fluentui/react-components";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, errText } from "../lib/api";
import { bytes } from "../lib/format";
import { DEFAULT_EXCLUDES, newId, newJob, randomPassword } from "../lib/defaults";
import type { Discovery, FoundDatabase, Source } from "../lib/types";
import { useApp } from "../state";
import { FolderTree, type Sizes } from "./FolderTree";
import { Info } from "./guide";

/** "What should we back up?": the agent's discovery, ticked by default, turned into a destination and a job. */
export function FirstBackup({ server, open, onClose }: { server: string; open: boolean; onClose: () => void }) {
  const { t } = useTranslation();
  const { loadConfig, saveConfig, toast, setPage, live } = useApp();
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
        .pathSize(server, path)
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
    if (!open) return;
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
  }, [open, server]);

  // what is ticked, against the free space of the disk the backup folder is on (the longest mount that contains it)
  const known = new Map((found?.paths ?? []).filter((p) => !p.pending).map((p) => [p.path, p.bytes]));
  const need = [...paths].reduce((a, p) => a + (known.get(p) ?? sizes[p]?.bytes ?? 0), 0);
  const disk = (live[server]?.status?.disks ?? []).filter((d) => localPath.startsWith(d.mount)).sort((a, b) => b.mount.length - a.mount.length)[0];
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
        onClose();
        setPage("jobs", { server });
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(_, d) => !d.open && onClose()}>
      <DialogSurface className="dialog-wide">
        <DialogBody>
          <DialogTitle>{t("first.title")}</DialogTitle>
          <DialogContent>
            <div className="stack dialog-scroll">
              <p className="muted" style={{ margin: 0 }}>{t("first.text")}</p>
              {error && <p className="bad-text">{error}</p>}
              {!found && !error && <Spinner size="small" label={t("first.scanning")} labelPosition="after" />}
              {found && (
                <>
                  <div className="form-sect">{t("first.databases")}</div>
                  {found.databases.length === 0 ? (
                    <p className="muted small" style={{ margin: 0 }}>{t("first.noDatabases")}</p>
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
                  )}
                  <div className="form-sect">
                    {t("first.folders")} <Info text={t("first.skipsInfo", { list: DEFAULT_EXCLUDES.join(", ") })} />
                  </div>
                  <FolderTree server={server} roots={found.paths} selected={paths} onChange={setPaths} sizes={sizes} measure={measure} />
                  <p className="muted small" style={{ margin: 0 }}>{t("first.skips")}</p>
                  <div className="form-sect">{t("first.where")}</div>
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
                  {dbs.size + paths.size > 0 && <p className="muted small" style={{ margin: 0 }}>{t("first.chosen", { count: dbs.size + paths.size, size: bytes(need) })}</p>}
                  {where === "local" && need > free && free > 0 && <p className="bad-text small" style={{ margin: 0 }}>{t("first.noSpace", { need: bytes(need), free: bytes(free) })}</p>}
                  <p className="muted small" style={{ margin: 0 }}>{t("first.defaults")}</p>
                </>
              )}
            </div>
          </DialogContent>
          <DialogActions>
            <Button onClick={onClose}>{t("first.later")}</Button>
            <Button appearance="primary" disabled={!found || busy || dbs.size + paths.size === 0} onClick={() => void create()}>
              {busy ? <Spinner size="tiny" /> : t("first.create")}
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
