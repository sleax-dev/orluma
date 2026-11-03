import { Button, Checkbox, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Dropdown, Field, Menu, MenuItem, MenuList, MenuPopover, MenuTrigger, Option, Spinner, Switch } from "@fluentui/react-components";
import {
  Add16Regular,
  ArrowSync16Regular,
  Broom16Regular,
  Cloud20Regular,
  Database20Regular,
  Delete16Regular,
  Desktop20Regular,
  Edit16Regular,
  Folder16Regular,
  HardDrive20Regular,
  LockOpen16Regular,
  MoreHorizontal20Regular,
  PlugConnected16Regular,
  ServerLink20Regular,
  ShieldTask16Regular,
} from "@fluentui/react-icons";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Info, NoServer } from "../components/guide";
import { DestinationEditor } from "../components/DestinationEditor";
import { Confirm, Empty, PageHead, ServerPicker, Skeleton, Status, TextField, useAction } from "../components/ui";
import { api, errText } from "../lib/api";
import { destSummary, newDestination, newId } from "../lib/defaults";
import { bytes, relative } from "../lib/format";
import type { Destination, PcSync, PcUsage, RepoStats } from "../lib/types";
import { useApp } from "../state";

function DestIcon({ d }: { d: Destination }) {
  switch (d.kind.type) {
    case "local":
      return <HardDrive20Regular />;
    case "sftp":
    case "rest":
      return <ServerLink20Regular />;
    default:
      return <Cloud20Regular />;
  }
}

function PcSection() {
  const { t } = useTranslation();
  const { settings, saveSettings, servers, configs, loadConfig, toast } = useApp();
  const [usage, setUsage] = useState<PcUsage | null>(null);
  const [adding, setAdding] = useState<PcSync | null>(null);
  const { busy, run } = useAction();

  useEffect(() => {
    void api.pcUsage().then(setUsage).catch(() => {});
    servers.forEach((s) => void loadConfig(s.id));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [servers, settings.pc_storage_dir]);

  const name = (s: string) => servers.find((x) => x.id === s)?.name ?? s;
  const dest = (s: string, d: string) => configs[s]?.destinations.find((x) => x.id === d)?.name ?? d;
  const job = (s: string, j: string) => (j ? configs[s]?.jobs.find((x) => x.id === j)?.name ?? j : t("pc.allJobs"));
  const update = (p: PcSync) => saveSettings({ ...settings, pc_syncs: settings.pc_syncs.map((x) => (x.id === p.id ? p : x)) });
  const used = (usage?.repos ?? []).reduce((a, [, b]) => a + b, 0);

  return (
    <section className="block">
      <div className="sect-head">
        <h2>{t("pc.title")}<Info text={t("pc.text")} /></h2>
        <Button size="small" icon={<Add16Regular />} disabled={servers.length === 0} onClick={() => setAdding({ id: newId("pc"), server_id: servers[0]?.id ?? "", destination_id: "", job_id: "", every_hours: 24, enabled: true, apply_retention: true, last_run: null, last_status: null, last_error: null })}>
          {t("pc.add")}
        </Button>
      </div>
      <div className="panel">
        <div className="pc-head">
          <span className="big">
            <Desktop20Regular />
          </span>
          <div className="grow">
            <div style={{ fontWeight: 600 }}>{t("pc.folder")}</div>
            <div className="small muted mono ellipsis">{usage?.root ?? "…"}</div>
          </div>
          <div className="side" style={{ display: "flex", flexDirection: "column", alignItems: "flex-end", fontSize: 12 }}>
            <span>{t("pc.used", { size: bytes(used) })}</span>
            <span className="muted">{t("pc.free", { size: bytes(usage?.free ?? 0) })}</span>
          </div>
          <Button size="small" icon={<Folder16Regular />} onClick={() => usage && void api.reveal(usage.root)}>
            {t("pc.open")}
          </Button>
        </div>
        {settings.pc_syncs.length === 0 ? (
          <p className="small">{t("pc.none")}</p>
        ) : (
          <div className="rows">
            {settings.pc_syncs.map((p) => (
              <div className={`row${p.enabled ? "" : " off"}`} key={p.id}>
                <div className="info">
                  <div className="name">
                    {name(p.server_id)} · {dest(p.server_id, p.destination_id)}
                  </div>
                  <div className="meta">
                    <span>{job(p.server_id, p.job_id)}</span>
                    <span>{t("pc.every", { count: p.every_hours })}</span>
                    <span>{p.apply_retention ? t("pc.retention") : t("pc.keepAll")}</span>
                  </div>
                  {p.last_error && <div className="desc bad-text small selectable">{p.last_error}</div>}
                </div>
                <div className="side">
                  {p.last_status ? <Status status={(p.last_status as "success") ?? "never"} /> : <Status status="never" />}
                  <span>{relative(p.last_run)}</span>
                </div>
                <div className="ctl">
                  <Switch checked={p.enabled} aria-label={t("job.enabled")} onChange={(_, d) => void update({ ...p, enabled: d.checked })} />
                  <Button size="small" icon={busy === p.id ? <Spinner size="extra-tiny" /> : <ArrowSync16Regular />} disabled={busy === p.id || p.last_status === "running"} onClick={() => void run(p.id, () => api.pcSyncNow(p.id), t("pc.started"))}>
                    {t("pc.now")}
                  </Button>
                  <Button size="small" appearance="subtle" icon={<Delete16Regular />} aria-label={t("common.delete")} onClick={() => void saveSettings({ ...settings, pc_syncs: settings.pc_syncs.filter((x) => x.id !== p.id) })} />
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
      {adding && (
        <Dialog open onOpenChange={(_, d) => !d.open && setAdding(null)}>
          <DialogSurface>
            <DialogBody>
              <DialogTitle>{t("pc.add")}</DialogTitle>
              <DialogContent>
                <div className="form">
                  <Field label={t("pc.server")} className="full">
                    <Dropdown value={name(adding.server_id)} selectedOptions={[adding.server_id]} onOptionSelect={(_, o) => { void loadConfig(o.optionValue ?? ""); setAdding({ ...adding, server_id: o.optionValue ?? "", destination_id: "", job_id: "" }); }}>
                      {servers.map((s) => <Option key={s.id} value={s.id}>{s.name}</Option>)}
                    </Dropdown>
                  </Field>
                  <Field label={t("pc.from")} className="full">
                    <Dropdown value={adding.destination_id ? dest(adding.server_id, adding.destination_id) : ""} placeholder={t("pc.pickDestination")} selectedOptions={[adding.destination_id]} onOptionSelect={(_, o) => setAdding({ ...adding, destination_id: o.optionValue ?? "" })}>
                      {(configs[adding.server_id]?.destinations ?? []).map((d) => <Option key={d.id} value={d.id}>{d.name}</Option>)}
                    </Dropdown>
                  </Field>
                  <Field label={t("pc.job")} className="full">
                    <Dropdown value={job(adding.server_id, adding.job_id)} selectedOptions={[adding.job_id]} onOptionSelect={(_, o) => setAdding({ ...adding, job_id: o.optionValue ?? "" })}>
                      <Option value="">{t("pc.allJobs")}</Option>
                      {(configs[adding.server_id]?.jobs ?? []).map((j) => <Option key={j.id} value={j.id}>{j.name}</Option>)}
                    </Dropdown>
                  </Field>
                  <TextField label={t("pc.everyHours")} value={adding.every_hours} onChange={(v) => setAdding({ ...adding, every_hours: Number(v.replace(/\D/g, "")) || 0 })} />
                  <Checkbox className="full" checked={adding.apply_retention} disabled={!adding.job_id} onChange={(_, d) => setAdding({ ...adding, apply_retention: !!d.checked })} label={t("pc.applyRetention")} />
                  <p className="hint small full">{t("pc.addHint")}</p>
                </div>
              </DialogContent>
              <DialogActions>
                <Button onClick={() => setAdding(null)}>{t("common.cancel")}</Button>
                <Button
                  appearance="primary"
                  disabled={!adding.destination_id}
                  onClick={async () => {
                    await saveSettings({ ...settings, pc_syncs: [...settings.pc_syncs, { ...adding, apply_retention: adding.apply_retention && !!adding.job_id }] });
                    setAdding(null);
                    toast("success", t("pc.added"));
                  }}
                >
                  {t("common.save")}
                </Button>
              </DialogActions>
            </DialogBody>
          </DialogSurface>
        </Dialog>
      )}
    </section>
  );
}

export function Storage() {
  const { t } = useTranslation();
  const { server, servers, configs, loadConfig, saveConfig, live, setPage } = useApp();
  const [stats, setStats] = useState<Record<string, RepoStats | string>>({});
  const [editing, setEditing] = useState<Destination | null>(null);
  const [deleting, setDeleting] = useState<Destination | null>(null);
  const { busy, run } = useAction();

  useEffect(() => {
    if (server) void loadConfig(server);
  }, [server, loadConfig]);

  const cfg = server ? configs[server] : undefined;

  useEffect(() => {
    if (!server || !cfg) return;
    setStats({});
    for (const d of cfg.destinations) {
      api
        .repoStats(server, d.id)
        .then((s) => setStats((x) => ({ ...x, [d.id]: s })))
        .catch((e) => setStats((x) => ({ ...x, [d.id]: errText(e) })));
    }
  }, [server, cfg]);

  const usedBy = (id: string) => cfg?.jobs.filter((j) => j.destination === id || j.copies.includes(id)).map((j) => j.name) ?? [];

  return (
    <div className="page">
      <PageHead
        title={t("nav.storage")}
        desc={t("storage.desc")}
        info={t("storage.info")}
        actions={
          cfg && cfg.destinations.length > 0 ? (
            <Button appearance="primary" icon={<Add16Regular />} onClick={() => setEditing(newDestination("local", t("dest.type.local")))}>
              {t("storage.add")}
            </Button>
          ) : undefined
        }
      />
      {servers.length === 0 && <NoServer page="storage" />}
      <ServerPicker />
      {server && !cfg && live[server]?.online !== false && <Skeleton h={80} r={12} />}
      {cfg && cfg.destinations.length === 0 && (
        <Empty
          icon={<Database20Regular />}
          title={t("storage.none")}
          text={t("storage.noneText")}
          action={<Button appearance="primary" icon={<Add16Regular />} onClick={() => setEditing(newDestination("local", t("dest.type.local")))}>{t("storage.add")}</Button>}
        />
      )}
      {cfg && cfg.destinations.length > 0 && (
        <div className="rows">
          {cfg.destinations.map((d) => {
            const s = stats[d.id];
            const jobs = usedBy(d.id);
            return (
              <div className="row" key={d.id}>
                <span className="icon">
                  <DestIcon d={d} />
                </span>
                <div className="info">
                  <div className="gap-s">
                    <span className="name">{d.name}</span>
                    <span className="tag">{t(`dest.type.${d.kind.type}`)}</span>
                  </div>
                  <div className="desc mono ellipsis">{destSummary(d.kind)}</div>
                  <div className="meta">
                    {s === undefined ? (
                      <Skeleton h={12} w={220} />
                    ) : typeof s === "string" ? (
                      <span className="bad-text ellipsis selectable" title={s}>{s}</span>
                    ) : (
                      <>
                        <span>{t("storage.snapshots", { count: s.snapshots })}</span>
                        <span>{t("storage.stored", { size: bytes(s.stored_bytes) })}</span>
                        {s.restore_bytes > 0 && s.stored_bytes > 0 && <span>{t("storage.saving", { ratio: (s.restore_bytes / s.stored_bytes).toFixed(1) })}</span>}
                      </>
                    )}
                    <span>{jobs.length ? t("storage.usedBy", { jobs: jobs.join(", ") }) : t("storage.unused")}</span>
                  </div>
                </div>
                <div className="ctl">
                  <Button size="small" icon={busy === `v${d.id}` ? <Spinner size="extra-tiny" /> : <ShieldTask16Regular />} onClick={() => void run(`v${d.id}`, () => api.verify(server!, d.id, 5), t("storage.verifyStarted"))}>
                    {t("storage.verify")}
                  </Button>
                  <Menu>
                    <MenuTrigger disableButtonEnhancement>
                      <Button size="small" appearance="subtle" icon={<MoreHorizontal20Regular />} aria-label={t("common.more")} />
                    </MenuTrigger>
                    <MenuPopover>
                      <MenuList>
                        <MenuItem icon={<Edit16Regular />} onClick={() => setEditing(structuredClone(d))}>{t("common.edit")}</MenuItem>
                        <MenuItem icon={<PlugConnected16Regular />} onClick={() => void run(`t${d.id}`, async () => { const r = await api.testDestination(server!, d); if (!r.ok) throw r.message; return r; }, t("storage.reachable"))}>{t("dest.test")}</MenuItem>
                        <MenuItem icon={<Broom16Regular />} onClick={() => void run(`p${d.id}`, () => api.prune(server!, d.id), t("storage.pruneStarted"))}>{t("storage.prune")}</MenuItem>
                        <MenuItem icon={<LockOpen16Regular />} onClick={() => void run(`u${d.id}`, () => api.unlock(server!, d.id), t("storage.unlocked"))}>{t("storage.unlock")}</MenuItem>
                        <MenuItem onClick={() => setPage("restore", { server: server!, destination: d.id })}>{t("storage.browse")}</MenuItem>
                        <MenuItem icon={<Delete16Regular />} disabled={jobs.length > 0} onClick={() => setDeleting(d)}>{t("common.delete")}</MenuItem>
                      </MenuList>
                    </MenuPopover>
                  </Menu>
                </div>
              </div>
            );
          })}
        </div>
      )}
      {servers.length > 0 && <PcSection />}
      {editing && server && cfg && (
        <DestinationEditor
          open
          server={server}
          initial={editing}
          onClose={() => setEditing(null)}
          onSave={async (d) => {
            const next = { ...cfg, destinations: cfg.destinations.some((x) => x.id === d.id) ? cfg.destinations.map((x) => (x.id === d.id ? d : x)) : [...cfg.destinations, d] };
            if (await saveConfig(server, next)) setEditing(null);
          }}
        />
      )}
      <Confirm
        open={!!deleting}
        title={t("storage.deleteTitle", { name: deleting?.name })}
        text={t("storage.deleteText")}
        confirm={t("common.delete")}
        danger
        onClose={() => setDeleting(null)}
        onConfirm={async () => {
          if (cfg && deleting && server) await saveConfig(server, { ...cfg, destinations: cfg.destinations.filter((x) => x.id !== deleting.id) });
          setDeleting(null);
        }}
      />
    </div>
  );
}
