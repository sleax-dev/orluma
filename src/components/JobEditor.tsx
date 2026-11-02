import {
  Button,
  Checkbox,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  Dropdown,
  Field,
  Input,
  Menu,
  MenuItem,
  MenuList,
  MenuPopover,
  MenuTrigger,
  Option,
  Radio,
  RadioGroup,
  Switch,
  Tab,
  TabList,
  Textarea,
} from "@fluentui/react-components";
import { Add16Regular, Delete16Regular } from "@fluentui/react-icons";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { build, describe, parts, presetFor } from "../lib/cron";
import { newSource, SOURCE_TYPES } from "../lib/defaults";
import type { AgentConfig, Hook, Job, Retention, Source } from "../lib/types";
import { TextField } from "./ui";

type TabId = "sources" | "destination" | "schedule" | "retention" | "verify" | "advanced";

const lines = (s: string) => s.split("\n").map((l) => l.trim()).filter(Boolean);
const list = (s: string) => s.split(",").map((l) => l.trim()).filter(Boolean);

function SourceFields({ s, onChange }: { s: Source; onChange: (s: Source) => void }) {
  const { t } = useTranslation();
  const set = (p: Partial<Source>) => onChange({ ...s, ...p } as Source);
  switch (s.type) {
    case "paths":
      return (
        <div className="form">
          <Field label={t("source.paths")} hint={t("source.pathsHint")} className="full">
            <Textarea resize="vertical" value={s.paths.join("\n")} onChange={(_, d) => set({ paths: lines(d.value) })} placeholder={"/var/www\n/etc"} />
          </Field>
          <Field label={t("source.excludes")} hint={t("source.excludesHint")} className="full">
            <Textarea resize="vertical" value={s.excludes.join("\n")} onChange={(_, d) => set({ excludes: lines(d.value) })} placeholder={"*.log\n*/node_modules/*"} />
          </Field>
          <Checkbox className="full" checked={s.one_file_system} onChange={(_, d) => set({ one_file_system: !!d.checked })} label={t("source.oneFs")} />
        </div>
      );
    case "mongo":
      return (
        <div className="form">
          <TextField full label={t("source.uri")} value={s.uri} onChange={(v) => set({ uri: v })} placeholder="mongodb://user:pass@127.0.0.1:27017/?authSource=admin" />
          <TextField label={t("source.container")} hint={t("source.containerHint")} value={s.container} onChange={(v) => set({ container: v })} />
        </div>
      );
    case "sqlite":
      return (
        <div className="form">
          <TextField full label={t("source.file")} value={s.path} onChange={(v) => set({ path: v })} placeholder="/var/lib/app/data.db" />
        </div>
      );
    case "command":
      return (
        <div className="form">
          <TextField full label={t("source.command")} hint={t("source.commandHint")} value={s.command} onChange={(v) => set({ command: v })} placeholder="tar -C /opt/app -cf - config" />
          <TextField label={t("source.filename")} value={s.filename} onChange={(v) => set({ filename: v })} />
        </div>
      );
    default: {
      // postgres, mysql, redis, clickhouse share a connection
      const c = s as Extract<Source, { host: string }>;
      return (
        <div className="form">
          <TextField label={t("source.container")} hint={t("source.containerHint")} value={c.container} onChange={(v) => set({ container: v })} />
          <TextField label={t("source.osUser")} hint={t("source.osUserHint")} value={c.os_user} onChange={(v) => set({ os_user: v })} />
          <TextField label={t("source.host")} value={c.host} onChange={(v) => set({ host: v })} placeholder="127.0.0.1" />
          <TextField label={t("source.port")} value={c.port || ""} onChange={(v) => set({ port: Number(v.replace(/\D/g, "")) || 0 })} />
          <TextField label={t("source.user")} value={c.user} onChange={(v) => set({ user: v })} />
          <TextField label={t("source.password")} type="password" value={c.password} onChange={(v) => set({ password: v })} />
          {"databases" in c && (
            <TextField full label={t("source.databases")} hint={t("source.databasesHint")} value={c.databases.join(", ")} onChange={(v) => set({ databases: list(v) } as Partial<Source>)} />
          )}
          {s.type === "postgres" && <Checkbox className="full" checked={s.globals} onChange={(_, d) => set({ globals: !!d.checked })} label={t("source.globals")} />}
        </div>
      );
    }
  }
}

function HookList({ hooks, onChange }: { hooks: Hook[]; onChange: (h: Hook[]) => void }) {
  const { t } = useTranslation();
  return (
    <div className="list-edit">
      {hooks.map((h, i) => (
        <div className="item" key={i}>
          <Input value={h.command} placeholder="systemctl stop app" onChange={(_, d) => onChange(hooks.map((x, j) => (j === i ? { ...x, command: d.value } : x)))} />
          <Input style={{ width: 90 }} value={String(h.timeout_secs)} contentAfter="s" onChange={(_, d) => onChange(hooks.map((x, j) => (j === i ? { ...x, timeout_secs: Number(d.value.replace(/\D/g, "")) || 0 } : x)))} />
          <Checkbox checked={h.continue_on_error} label={t("job.hookContinue")} onChange={(_, d) => onChange(hooks.map((x, j) => (j === i ? { ...x, continue_on_error: !!d.checked } : x)))} />
          <Button appearance="subtle" icon={<Delete16Regular />} aria-label={t("common.remove")} onClick={() => onChange(hooks.filter((_, j) => j !== i))} />
        </div>
      ))}
      <Button size="small" icon={<Add16Regular />} style={{ alignSelf: "flex-start" }} onClick={() => onChange([...hooks, { command: "", timeout_secs: 600, continue_on_error: false }])}>
        {t("job.addHook")}
      </Button>
    </div>
  );
}

const RETENTION_PRESETS: Record<string, Partial<Retention>> = {
  gfs: { last: 3, hourly: 0, daily: 7, weekly: 4, monthly: 12, yearly: 2 },
  month: { last: 3, hourly: 0, daily: 30, weekly: 0, monthly: 0, yearly: 0 },
  long: { last: 3, hourly: 24, daily: 14, weekly: 8, monthly: 24, yearly: 5 },
  all: { last: 0, hourly: 0, daily: 0, weekly: 0, monthly: 0, yearly: 0 },
};

export function JobEditor({ open, job: initial, config, onClose, onSave, onAddDestination }: { open: boolean; job: Job; config: AgentConfig; onClose: () => void; onSave: (j: Job) => Promise<void>; onAddDestination: () => void }) {
  const { t } = useTranslation();
  const [job, setJob] = useState<Job>(initial);
  const [tab, setTab] = useState<TabId>("sources");
  const [busy, setBusy] = useState(false);
  const set = (p: Partial<Job>) => setJob((j) => ({ ...j, ...p }));
  const kind = presetFor(job.schedule.cron);
  const p = parts(job.schedule.cron);
  const setCron = (k: string, part: Partial<typeof p> = {}) => set({ schedule: { ...job.schedule, cron: build(k, { ...p, ...part }) } });
  const pad = (n: number) => String(n).padStart(2, "0");
  const problems = [!job.name.trim() && t("job.needName"), !job.destination && t("job.needDestination"), job.sources.length === 0 && t("job.needSource")].filter(Boolean) as string[];

  return (
    <Dialog open={open} onOpenChange={(_, d) => !d.open && onClose()}>
      <DialogSurface className="dialog-xl">
        <DialogBody>
          <DialogTitle>{initial.name ? t("job.editTitle", { name: initial.name }) : t("job.newTitle")}</DialogTitle>
          <DialogContent>
            <div className="stack">
              <div className="form">
                <TextField label={t("job.name")} value={job.name} onChange={(v) => set({ name: v })} placeholder={t("job.namePlaceholder")} />
                <Field label={t("job.enabled")}>
                  <Switch checked={job.enabled} onChange={(_, d) => set({ enabled: d.checked })} label={job.enabled ? t("common.on") : t("common.off")} />
                </Field>
              </div>
              <TabList selectedValue={tab} onTabSelect={(_, d) => setTab(d.value as TabId)} size="small">
                {(["sources", "destination", "schedule", "retention", "verify", "advanced"] as TabId[]).map((x) => (
                  <Tab key={x} value={x}>
                    {t(`job.tab.${x}`)}
                    {x === "sources" && job.sources.length > 0 ? ` (${job.sources.length})` : ""}
                  </Tab>
                ))}
              </TabList>
              <div className="dialog-scroll stack">
                {tab === "sources" && (
                  <>
                    {job.sources.length === 0 && <p className="muted" style={{ margin: 0 }}>{t("job.noSources")}</p>}
                    {job.sources.map((s, i) => (
                      <div className="source-card" key={s.id}>
                        <div className="sc-head">
                          <b>{t(`source.type.${s.type}`)}</b>
                          <span className="small muted">{t(`source.about.${s.type}`)}</span>
                          <Button className="right" size="small" appearance="subtle" icon={<Delete16Regular />} aria-label={t("common.remove")} onClick={() => set({ sources: job.sources.filter((_, j) => j !== i) })} />
                        </div>
                        <SourceFields s={s} onChange={(n) => set({ sources: job.sources.map((x, j) => (j === i ? n : x)) })} />
                      </div>
                    ))}
                    <Menu>
                      <MenuTrigger disableButtonEnhancement>
                        <Button icon={<Add16Regular />} style={{ alignSelf: "flex-start" }}>
                          {t("job.addSource")}
                        </Button>
                      </MenuTrigger>
                      <MenuPopover>
                        <MenuList>
                          {SOURCE_TYPES.map((x) => (
                            <MenuItem key={x} onClick={() => set({ sources: [...job.sources, newSource(x)] })}>
                              {t(`source.type.${x}`)}
                            </MenuItem>
                          ))}
                        </MenuList>
                      </MenuPopover>
                    </Menu>
                  </>
                )}

                {tab === "destination" && (
                  <>
                    {config.destinations.length === 0 ? (
                      <div className="stack">
                        <p className="muted" style={{ margin: 0 }}>{t("job.noDestinations")}</p>
                        <Button icon={<Add16Regular />} onClick={onAddDestination} style={{ alignSelf: "flex-start" }}>
                          {t("storage.add")}
                        </Button>
                      </div>
                    ) : (
                      <>
                        <Field label={t("job.destination")} hint={t("job.destinationHint")}>
                          <RadioGroup value={job.destination} onChange={(_, d) => set({ destination: d.value, copies: job.copies.filter((c) => c !== d.value) })}>
                            {config.destinations.map((d) => (
                              <Radio key={d.id} value={d.id} label={d.name} />
                            ))}
                          </RadioGroup>
                        </Field>
                        <Field label={t("job.copies")} hint={t("job.copiesHint")}>
                          <div className="stack" style={{ gap: 2 }}>
                            {config.destinations
                              .filter((d) => d.id !== job.destination)
                              .map((d) => (
                                <Checkbox
                                  key={d.id}
                                  label={d.name}
                                  checked={job.copies.includes(d.id)}
                                  onChange={(_, c) => set({ copies: c.checked ? [...job.copies, d.id] : job.copies.filter((x) => x !== d.id) })}
                                />
                              ))}
                          </div>
                        </Field>
                      </>
                    )}
                  </>
                )}

                {tab === "schedule" && (
                  <>
                    <RadioGroup value={kind === "custom" ? "custom" : kind} onChange={(_, d) => (d.value === "custom" ? set({ schedule: { ...job.schedule, cron: job.schedule.cron || "0 3 * * *" } }) : setCron(d.value))}>
                      {["manual", "hours", "daily", "weekly", "monthly", "custom"].map((k) => (
                        <Radio key={k} value={k} label={t(`schedule.kind.${k}`)} />
                      ))}
                    </RadioGroup>
                    <div className="form">
                      {kind === "hours" && <TextField label={t("schedule.everyHours")} value={p.hours} onChange={(v) => setCron("hours", { hours: Number(v) || 1 })} />}
                      {(kind === "daily" || kind === "weekly" || kind === "monthly") && (
                        <Field label={t("schedule.at")}>
                          <Input type="time" value={`${pad(p.hour)}:${pad(p.minute)}`} onChange={(_, d) => { const [h, m] = d.value.split(":").map(Number); setCron(kind, { hour: h || 0, minute: m || 0 }); }} />
                        </Field>
                      )}
                      {kind === "weekly" && (
                        <Field label={t("schedule.on")}>
                          <Dropdown value={t(`schedule.weekday.${p.weekday % 7}`)} selectedOptions={[String(p.weekday % 7)]} onOptionSelect={(_, d) => setCron("weekly", { weekday: Number(d.optionValue) })}>
                            {[1, 2, 3, 4, 5, 6, 0].map((d) => (
                              <Option key={d} value={String(d)}>{t(`schedule.weekday.${d}`)}</Option>
                            ))}
                          </Dropdown>
                        </Field>
                      )}
                      {kind === "monthly" && <TextField label={t("schedule.dayOfMonth")} value={p.monthday} onChange={(v) => setCron("monthly", { monthday: Math.min(28, Math.max(1, Number(v) || 1)) })} />}
                      {kind === "custom" && <TextField full label={t("schedule.cron")} hint={t("schedule.cronHint")} value={job.schedule.cron} onChange={(v) => set({ schedule: { ...job.schedule, cron: v } })} className="mono" />}
                    </div>
                    <p className="small" style={{ margin: 0 }}>{describe(job.schedule.cron, t)}</p>
                    <Checkbox checked={job.schedule.catch_up} onChange={(_, d) => set({ schedule: { ...job.schedule, catch_up: !!d.checked } })} label={t("schedule.catchUp")} />
                  </>
                )}

                {tab === "retention" && (
                  <>
                    <p className="muted" style={{ margin: 0 }}>{t("retention.text")}</p>
                    <div className="gap-s">
                      {Object.keys(RETENTION_PRESETS).map((k) => (
                        <Button key={k} size="small" onClick={() => set({ retention: { ...job.retention, ...RETENTION_PRESETS[k] } })}>
                          {t(`retention.preset.${k}`)}
                        </Button>
                      ))}
                    </div>
                    <div className="form">
                      {(["last", "hourly", "daily", "weekly", "monthly", "yearly"] as const).map((k) => (
                        <TextField key={k} label={t(`retention.${k}`)} value={job.retention[k]} onChange={(v) => set({ retention: { ...job.retention, [k]: Number(v.replace(/\D/g, "")) || 0 } })} />
                      ))}
                      <TextField full label={t("retention.pruneEvery")} hint={t("retention.pruneHint")} value={job.retention.prune_every_days} onChange={(v) => set({ retention: { ...job.retention, prune_every_days: Number(v.replace(/\D/g, "")) || 0 } })} />
                    </div>
                  </>
                )}

                {tab === "verify" && (
                  <div className="form">
                    <TextField label={t("verify.every")} hint={t("verify.everyHint")} value={job.verify.every_days} onChange={(v) => set({ verify: { ...job.verify, every_days: Number(v.replace(/\D/g, "")) || 0 } })} />
                    <TextField label={t("verify.percent")} hint={t("verify.percentHint")} value={job.verify.read_data_percent} onChange={(v) => set({ verify: { ...job.verify, read_data_percent: Math.min(100, Number(v.replace(/\D/g, "")) || 0) } })} />
                    <TextField full label={t("job.healthcheck")} hint={t("job.healthcheckHint")} value={job.healthcheck_url} onChange={(v) => set({ healthcheck_url: v.trim() })} placeholder="https://hc-ping.com/…" />
                    <div className="form-sect">{t("job.hooksBefore")}</div>
                    <div className="full">
                      <HookList hooks={job.hooks.before} onChange={(h) => set({ hooks: { ...job.hooks, before: h } })} />
                    </div>
                    <div className="form-sect">{t("job.hooksAfter")}</div>
                    <div className="full">
                      <HookList hooks={job.hooks.after} onChange={(h) => set({ hooks: { ...job.hooks, after: h } })} />
                      <p className="hint small muted">{t("job.hooksAfterHint")}</p>
                    </div>
                  </div>
                )}

                {tab === "advanced" && (
                  <div className="form">
                    <Field label={t("advanced.compression")}>
                      <Dropdown value={t(`advanced.comp.${job.options.compression || "auto"}`)} selectedOptions={[job.options.compression || "auto"]} onOptionSelect={(_, d) => set({ options: { ...job.options, compression: d.optionValue ?? "auto" } })}>
                        {["auto", "max", "off"].map((c) => (
                          <Option key={c} value={c}>{t(`advanced.comp.${c}`)}</Option>
                        ))}
                      </Dropdown>
                    </Field>
                    <TextField label={t("advanced.largerThan")} hint={t("advanced.largerThanHint")} value={job.options.exclude_larger_than_mib} onChange={(v) => set({ options: { ...job.options, exclude_larger_than_mib: Number(v.replace(/\D/g, "")) || 0 } })} />
                    <Checkbox className="full" checked={job.options.exclude_caches} onChange={(_, d) => set({ options: { ...job.options, exclude_caches: !!d.checked } })} label={t("advanced.caches")} />
                    <Checkbox className="full" checked={job.options.low_priority} onChange={(_, d) => set({ options: { ...job.options, low_priority: !!d.checked } })} label={t("advanced.lowPriority")} />
                  </div>
                )}
              </div>
              {problems.length > 0 && <p className="small" style={{ margin: 0, color: "var(--warn)" }}>{problems.join(" · ")}</p>}
            </div>
          </DialogContent>
          <DialogActions>
            <Button onClick={onClose}>{t("common.cancel")}</Button>
            <Button
              appearance="primary"
              disabled={problems.length > 0 || busy}
              onClick={async () => {
                setBusy(true);
                try {
                  await onSave({ ...job, name: job.name.trim() });
                } finally {
                  setBusy(false);
                }
              }}
            >
              {t("common.save")}
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
