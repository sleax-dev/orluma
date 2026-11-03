import { Button, Checkbox, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Dropdown, Field, Menu, MenuItem, MenuList, MenuPopover, MenuTrigger, Option, Spinner, Switch, Textarea } from "@fluentui/react-components";
import { Add16Regular, Alert20Regular, CheckmarkCircle16Filled, Delete16Regular, DismissCircle16Filled, Edit16Regular, MoreHorizontal20Regular, Send16Regular } from "@fluentui/react-icons";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { NoServer } from "../components/guide";
import { Confirm, Empty, PageHead, ServerPicker, Skeleton, TextField, useAction } from "../components/ui";
import { api, errText } from "../lib/api";
import { newNotifier, newNotifierKind, NOTIFIER_TYPES } from "../lib/defaults";
import { ALERT_EVENTS, DEFAULT_EVENTS, type Notifier, type NotifierKind, type TestResult } from "../lib/types";
import { useApp } from "../state";

function KindFields({ k, set }: { k: NotifierKind; set: (p: Partial<NotifierKind>) => void }) {
  const { t } = useTranslation();
  switch (k.type) {
    case "webhook":
      return (
        <>
          <TextField full label={t("alert.url")} value={k.url} onChange={(v) => set({ url: v })} />
          <TextField full label={t("alert.secret")} hint={t("alert.secretHint")} value={k.secret} onChange={(v) => set({ secret: v })} />
        </>
      );
    case "discord":
    case "slack":
    case "teams":
      return <TextField full label={t("alert.webhookUrl")} hint={t(`alert.hint.${k.type}`)} value={k.webhook_url} onChange={(v) => set({ webhook_url: v })} />;
    case "telegram":
      return (
        <>
          <TextField label={t("alert.botToken")} hint={t("alert.hint.telegram")} value={k.bot_token} onChange={(v) => set({ bot_token: v })} />
          <TextField label={t("alert.chatId")} value={k.chat_id} onChange={(v) => set({ chat_id: v })} />
        </>
      );
    case "ntfy":
      return (
        <>
          <TextField label={t("alert.server")} value={k.server} onChange={(v) => set({ server: v })} />
          <TextField label={t("alert.topic")} value={k.topic} onChange={(v) => set({ topic: v })} />
          <TextField full label={t("alert.token")} hint={t("alert.optional")} value={k.token} onChange={(v) => set({ token: v })} />
        </>
      );
    case "gotify":
      return (
        <>
          <TextField label={t("alert.server")} value={k.server} onChange={(v) => set({ server: v })} />
          <TextField label={t("alert.appToken")} value={k.token} onChange={(v) => set({ token: v })} />
        </>
      );
    case "pushover":
      return (
        <>
          <TextField label={t("alert.userKey")} value={k.user_key} onChange={(v) => set({ user_key: v })} />
          <TextField label={t("alert.appToken")} value={k.app_token} onChange={(v) => set({ app_token: v })} />
        </>
      );
    default:
      return (
        <>
          <TextField label={t("alert.smtpHost")} value={k.smtp_host} onChange={(v) => set({ smtp_host: v })} placeholder="smtp.example.net" />
          <TextField label={t("alert.smtpPort")} value={k.smtp_port} onChange={(v) => set({ smtp_port: Number(v.replace(/\D/g, "")) || 587 })} />
          <Field label={t("alert.security")}>
            <Dropdown value={k.security.toUpperCase()} selectedOptions={[k.security]} onOptionSelect={(_, o) => set({ security: o.optionValue ?? "starttls" })}>
              {["starttls", "tls", "none"].map((s) => <Option key={s} value={s}>{s.toUpperCase()}</Option>)}
            </Dropdown>
          </Field>
          <span />
          <TextField label={t("alert.username")} value={k.username} onChange={(v) => set({ username: v })} />
          <TextField label={t("alert.password")} type="password" value={k.password} onChange={(v) => set({ password: v })} />
          <TextField full label={t("alert.from")} value={k.from} onChange={(v) => set({ from: v })} placeholder="Orluma <alerts@example.net>" />
          <Field label={t("alert.to")} hint={t("alert.toHint")} className="full">
            <Textarea value={k.to.join("\n")} onChange={(_, d) => set({ to: d.value.split(/[\n,]/).map((x) => x.trim()).filter(Boolean) })} />
          </Field>
        </>
      );
  }
}

function NotifierEditor({ server, initial, onClose, onSave }: { server: string; initial: Notifier; onClose: () => void; onSave: (n: Notifier) => Promise<void> }) {
  const { t } = useTranslation();
  const { configs } = useApp();
  const [n, setN] = useState(initial);
  const [test, setTest] = useState<TestResult | null>(null);
  const [testing, setTesting] = useState(false);
  const jobs = configs[server]?.jobs ?? [];
  const events = n.events.length ? n.events : DEFAULT_EVENTS;
  return (
    <Dialog open onOpenChange={(_, d) => !d.open && onClose()}>
      <DialogSurface className="dialog-xl">
        <DialogBody>
          <DialogTitle>{initial.name ? t("alert.editTitle", { name: initial.name }) : t("alert.newTitle")}</DialogTitle>
          <DialogContent>
            <div className="stack dialog-scroll">
              <div className="type-grid">
                {NOTIFIER_TYPES.map((x) => (
                  <button key={x} className={`type-btn${n.kind.type === x ? " on" : ""}`} onClick={() => { setN({ ...n, kind: newNotifierKind(x), name: !n.name || n.name === t(`alert.type.${n.kind.type}`) ? t(`alert.type.${x}`) : n.name }); setTest(null); }}>
                    <b>{t(`alert.type.${x}`)}</b>
                  </button>
                ))}
              </div>
              <div className="form">
                <TextField full label={t("alert.name")} value={n.name} onChange={(v) => setN({ ...n, name: v })} />
                <KindFields k={n.kind} set={(p) => setN({ ...n, kind: { ...n.kind, ...p } as NotifierKind })} />
                <div className="form-sect">{t("alert.events")}</div>
                {ALERT_EVENTS.map((e) => (
                  <Checkbox key={e} checked={events.includes(e)} label={t(`alert.event.${e}`)} onChange={(_, d) => setN({ ...n, events: d.checked ? [...events, e] : events.filter((x) => x !== e) })} />
                ))}
                {jobs.length > 0 && (
                  <>
                    <div className="form-sect">{t("alert.jobs")}</div>
                    <p className="hint small full">{t("alert.jobsHint")}</p>
                    {jobs.map((j) => (
                      <Checkbox key={j.id} checked={n.jobs.includes(j.id)} label={j.name} onChange={(_, d) => setN({ ...n, jobs: d.checked ? [...n.jobs, j.id] : n.jobs.filter((x) => x !== j.id) })} />
                    ))}
                  </>
                )}
              </div>
              {test && (
                <p className={test.ok ? "ok-line" : "bad-text selectable"} style={{ display: "flex", gap: 8, margin: 0 }}>
                  {test.ok ? <CheckmarkCircle16Filled /> : <DismissCircle16Filled style={{ flex: "none", marginTop: 2 }} />} {test.message}
                </p>
              )}
            </div>
          </DialogContent>
          <DialogActions>
            <Button onClick={onClose}>{t("common.cancel")}</Button>
            <Button
              icon={testing ? <Spinner size="extra-tiny" /> : <Send16Regular />}
              disabled={testing}
              onClick={async () => {
                setTesting(true);
                try {
                  setTest(await api.testNotifier(server, n));
                } catch (e) {
                  setTest({ ok: false, message: errText(e) });
                } finally {
                  setTesting(false);
                }
              }}
            >
              {t("alert.test")}
            </Button>
            <Button appearance="primary" disabled={!n.name.trim()} onClick={() => void onSave({ ...n, events })}>
              {t("common.save")}
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}

export function Alerts() {
  const { t } = useTranslation();
  const { server, servers, configs, loadConfig, saveConfig, settings, saveSettings, live } = useApp();
  const [editing, setEditing] = useState<Notifier | null>(null);
  const [deleting, setDeleting] = useState<Notifier | null>(null);
  const { busy, run } = useAction();

  useEffect(() => {
    if (server) void loadConfig(server);
  }, [server, loadConfig]);
  const cfg = server ? configs[server] : undefined;

  const save = async (n: Notifier) => {
    if (!cfg || !server) return;
    const next = { ...cfg, notifiers: cfg.notifiers.some((x) => x.id === n.id) ? cfg.notifiers.map((x) => (x.id === n.id ? n : x)) : [...cfg.notifiers, n] };
    if (await saveConfig(server, next)) setEditing(null);
  };

  return (
    <div className="page">
      <PageHead
        title={t("nav.alerts")}
        desc={t("alerts.desc")}
        info={`${t("alerts.info")}\n\n${t("alerts.healthcheckNote")}`}
        actions={
          <Button appearance="primary" icon={<Add16Regular />} disabled={!cfg} onClick={() => setEditing(newNotifier("discord", t("alert.type.discord")))}>
            {t("alerts.add")}
          </Button>
        }
      />
      {servers.length === 0 && <NoServer page="alerts" />}
      <ServerPicker />
      {server && !cfg && live[server]?.online !== false && <Skeleton h={70} r={12} />}
      {cfg && cfg.notifiers.length === 0 && <Empty icon={<Alert20Regular />} title={t("alerts.none")} text={t("alerts.noneText")} />}
      {cfg && cfg.notifiers.length > 0 && (
        <div className="rows">
          {cfg.notifiers.map((n) => (
            <div className={`row${n.enabled ? "" : " off"}`} key={n.id}>
              <span className="icon">
                <Alert20Regular />
              </span>
              <div className="info">
                <div className="gap-s">
                  <span className="name">{n.name}</span>
                  <span className="tag">{t(`alert.type.${n.kind.type}`)}</span>
                </div>
                <div className="meta">
                  <span>{(n.events.length ? n.events : DEFAULT_EVENTS).map((e) => t(`alert.eventShort.${e}`)).join(" · ")}</span>
                  <span>{n.jobs.length ? t("alerts.someJobs", { count: n.jobs.length }) : t("alerts.allJobs")}</span>
                </div>
              </div>
              <div className="ctl">
                <Switch checked={n.enabled} aria-label={t("job.enabled")} onChange={(_, d) => void saveConfig(server!, { ...cfg, notifiers: cfg.notifiers.map((x) => (x.id === n.id ? { ...x, enabled: d.checked } : x)) })} />
                <Button size="small" icon={busy === n.id ? <Spinner size="extra-tiny" /> : <Send16Regular />} onClick={() => void run(n.id, async () => { const r = await api.testNotifier(server!, n); if (!r.ok) throw r.message; }, t("alert.sent"))}>
                  {t("alert.test")}
                </Button>
                <Menu>
                  <MenuTrigger disableButtonEnhancement>
                    <Button size="small" appearance="subtle" icon={<MoreHorizontal20Regular />} aria-label={t("common.more")} />
                  </MenuTrigger>
                  <MenuPopover>
                    <MenuList>
                      <MenuItem icon={<Edit16Regular />} onClick={() => setEditing(structuredClone(n))}>{t("common.edit")}</MenuItem>
                      <MenuItem icon={<Delete16Regular />} onClick={() => setDeleting(n)}>{t("common.delete")}</MenuItem>
                    </MenuList>
                  </MenuPopover>
                </Menu>
              </div>
            </div>
          ))}
        </div>
      )}

      <section className="block">
        <div className="sect-head">
          <h2>{t("alerts.desktop")}</h2>
        </div>
        <div className="rows">
          <div className="row">
            <div className="info">
              <div className="name">{t("settings.notifyDesktop")}</div>
              <div className="desc">{t("settings.notifyDesktopText")}</div>
            </div>
            <Switch checked={settings.notify_desktop} onChange={(_, d) => void saveSettings({ ...settings, notify_desktop: d.checked })} />
          </div>
          <div className="row">
            <div className="info">
              <div className="name">{t("settings.notifySuccess")}</div>
              <div className="desc">{t("settings.notifySuccessText")}</div>
            </div>
            <Switch checked={settings.notify_success} disabled={!settings.notify_desktop} onChange={(_, d) => void saveSettings({ ...settings, notify_success: d.checked })} />
          </div>
        </div>
      </section>

      {editing && server && <NotifierEditor server={server} initial={editing} onClose={() => setEditing(null)} onSave={save} />}
      <Confirm
        open={!!deleting}
        title={t("alerts.deleteTitle", { name: deleting?.name })}
        confirm={t("common.delete")}
        danger
        onClose={() => setDeleting(null)}
        onConfirm={async () => {
          if (cfg && deleting && server) await saveConfig(server, { ...cfg, notifiers: cfg.notifiers.filter((x) => x.id !== deleting.id) });
          setDeleting(null);
        }}
      />
    </div>
  );
}
