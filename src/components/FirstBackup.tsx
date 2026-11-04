import { Button, Checkbox, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Field, Input, Radio, RadioGroup, Spinner } from "@fluentui/react-components";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, errText } from "../lib/api";
import { bytes } from "../lib/format";
import { newId, newJob, randomPassword } from "../lib/defaults";
import type { Discovery, FoundDatabase, Source } from "../lib/types";
import { useApp } from "../state";

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
  const [sizes, setSizes] = useState<Record<string, { bytes: number; complete: boolean }>>({});

  useEffect(() => {
    if (!open) return;
    setFound(null);
    setSizes({});
    setError(null);
    let alive = true;
    void loadConfig(server, true).then((c) => {
      const d = c?.destinations.map((x) => ({ id: x.id, name: x.name })) ?? [];
      setDestinations(d);
      if (d[0]) setExisting(d[0].id);
    });
    api
      .discover(server)
      .then((d) => {
        if (!alive) return;
        setFound(d);
        setDbs(new Set(d.databases.map((_, i) => i)));
        // settings and websites by default; home folders and app data are often caches and can be huge, so they are opt-in
        setPaths(new Set(d.paths.filter((p) => ["config", "web"].includes(p.hint)).map((p) => p.path)));
        for (const p of d.paths.filter((p) => p.pending)) {
          api
            .pathSize(server, p.path)
            .then((s) => alive && setSizes((x) => ({ ...x, [p.path]: s })))
            .catch(() => alive && setSizes((x) => ({ ...x, [p.path]: { bytes: 0, complete: false } })));
        }
      })
      .catch((e) => alive && setError(errText(e)));
    return () => {
      alive = false;
    };
    // loadConfig changes whenever configs does, so listing it here re-ran the discovery after every config read
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, server]);

  // what is ticked, against the free space of the disk the backup folder is on (the longest mount that contains it)
  const need = (found?.paths ?? []).filter((p) => paths.has(p.path)).reduce((a, p) => a + (p.pending ? (sizes[p.path]?.bytes ?? 0) : p.bytes), 0);
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
      if (paths.size > 0) sources.push({ type: "paths", id: newId("src"), paths: [...paths], excludes: [], one_file_system: false });
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
                  <div className="form-sect">{t("first.folders")}</div>
                  <div className="check-list">
                    {found.paths.map((p) => (
                      <div className="check-row" key={p.path}>
                        <Checkbox
                          checked={paths.has(p.path)}
                          onChange={(_, c) => setPaths((s) => { const n = new Set(s); if (c.checked) n.add(p.path); else n.delete(p.path); return n; })}
                          label={<><span className="mono">{p.path}</span> <span className="tag">{t(`first.hint.${p.hint}`)}</span></>}
                        />
                        <span className="size">
                          {!p.pending ? bytes(p.bytes) : !sizes[p.path] ? <Spinner size="extra-tiny" /> : sizes[p.path].complete ? bytes(sizes[p.path].bytes) : t("first.tooBig")}
                        </span>
                      </div>
                    ))}
                  </div>
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
                  {where === "local" && need > free && free > 0 && <p className="bad-text small" style={{ margin: 0 }}>{t("first.noSpace", { need: bytes(need), free: bytes(free) })}</p>}
                  <p className="muted small" style={{ margin: 0 }}>{t("first.defaults")}</p>
                </>
              )}
            </div>
          </DialogContent>
          <DialogActions>
            <Button onClick={onClose}>{t("first.later")}</Button>
            <Button appearance="primary" disabled={!found || busy} onClick={() => void create()}>
              {busy ? <Spinner size="tiny" /> : t("first.create")}
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
