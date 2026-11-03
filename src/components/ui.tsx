import {
  Button,
  Dialog,
  DialogActions,
  DialogBody,
  DialogContent,
  DialogSurface,
  DialogTitle,
  Field,
  Input,
  type InputProps,
} from "@fluentui/react-components";
import { ArrowClockwise16Regular, Dismiss16Regular, Info16Regular, Maximize16Regular, Subtract16Regular } from "@fluentui/react-icons";
import { AnimatePresence, motion } from "motion/react";
import type { ReactNode } from "react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import logo from "../assets/logo.svg";
import { api } from "../lib/api";
import { bytes, duration, percent } from "../lib/format";
import { Guide, Info } from "./guide";
import type { RunProgress, RunStatus } from "../lib/types";
import { useApp } from "../state";

export function Logo({ size = 22 }: { size?: number }) {
  return <img src={logo} width={size} height={size} alt="" style={{ borderRadius: size * 0.22 }} />;
}

export function TitleBar() {
  const { t } = useTranslation();
  const { settings } = useApp();
  const closeLabel = settings.setup_done && settings.run_in_background ? t("titlebar.closeToTray") : t("titlebar.close");
  const toggleMax = async () => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().toggleMaximize();
  };
  return (
    <div className="titlebar" data-tauri-drag-region onDoubleClick={() => void toggleMax().catch(() => {})}>
      <span className="brand">
        <Logo size={18} />
        <span>Orluma</span>
      </span>
      <span className="grow" data-tauri-drag-region />
      <button className="tb-btn" title={t("titlebar.minimize")} aria-label={t("titlebar.minimize")} onClick={() => void api.minimizeWindow()}>
        <Subtract16Regular />
      </button>
      <button className="tb-btn" title={t("titlebar.maximize")} aria-label={t("titlebar.maximize")} onClick={() => void toggleMax().catch(() => {})}>
        <Maximize16Regular />
      </button>
      <button className="tb-btn close" title={closeLabel} aria-label={closeLabel} onClick={() => void api.closeWindow()}>
        <Dismiss16Regular />
      </button>
    </div>
  );
}

export function PageHead({ title, desc, info, actions }: { title: string; desc?: string; info?: string; actions?: ReactNode }) {
  return (
    <header className="page-head">
      <div>
        <h1>
          {title}
          {info && <Info text={info} />}
        </h1>
        {desc && <p>{desc}</p>}
      </div>
      {actions && <div className="actions">{actions}</div>}
    </header>
  );
}

export function Status({ status }: { status: RunStatus | "online" | "offline" | "never" | "unknown" }) {
  const { t } = useTranslation();
  return <span className={`st ${status}`}>{t(`status.${status}`)}</span>;
}

export function Empty({ icon, title, text, action }: { icon?: ReactNode; title: string; text?: string; action?: ReactNode }) {
  return <Guide icon={icon ?? <Info16Regular />} title={title} text={text} actions={action} />;
}

export function Skeleton({ h = 18, w = "100%", r = 6 }: { h?: number; w?: number | string; r?: number }) {
  return <span className="sk" style={{ height: h, width: w, borderRadius: r }} />;
}

/** The server picker over pages that work on one server. */
export function ServerPicker() {
  const { servers, server, setServer, live } = useApp();
  if (servers.length < 2) return null;
  return (
    <div className="picker" role="tablist">
      {servers.map((s) => {
        const on = live[s.id]?.online;
        return (
          <button key={s.id} role="tab" aria-selected={s.id === server} className={s.id === server ? "on" : ""} onClick={() => setServer(s.id)}>
            <span className={`dot ${on === true ? "online" : on === false ? "offline" : ""}`} />
            {s.name}
          </button>
        );
      })}
    </div>
  );
}

export function LiveRun({ p, title, onCancel }: { p: RunProgress; title: string; onCancel?: () => void }) {
  const { t } = useTranslation();
  const known = p.bytes_total > 0;
  return (
    <div className="live">
      <div className="live-top">
        <span className="st running">{t("status.running")}</span>
        <span className="what ellipsis">{title}</span>
        {p.step_count > 0 && (
          <span className="step ellipsis">
            {t("live.step", { index: p.step_index, count: p.step_count })} · {p.step}
          </span>
        )}
        {onCancel && (
          <span className="right">
            <Button size="small" appearance="subtle" onClick={onCancel}>
              {t("common.cancel")}
            </Button>
          </span>
        )}
      </div>
      <div className={`bar${known ? "" : " indeterminate"}`}>
        <i style={{ transform: `scaleX(${Math.min(1, Math.max(0, p.percent))})` }} />
      </div>
      <div className="live-nums">
        {known && <span>{percent(p.percent)}</span>}
        {known && (
          <span>
            {bytes(p.bytes_done)} / {bytes(p.bytes_total)}
          </span>
        )}
        {p.seconds_remaining > 0 && <span>{t("live.eta", { time: duration(p.seconds_remaining) })}</span>}
      </div>
    </div>
  );
}

export function Floating() {
  const { toasts, dismiss } = useApp();
  return (
    <div className="floating" aria-live="polite">
      <AnimatePresence>
        {toasts.map((x) => (
          <motion.div key={x.id} className={`toast ${x.kind}`} initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 8 }}>
            <span className="selectable">{x.text}</span>
            <Button size="small" appearance="subtle" icon={<Dismiss16Regular />} aria-label="×" onClick={() => dismiss(x.id)} />
          </motion.div>
        ))}
      </AnimatePresence>
    </div>
  );
}

export function Confirm({
  open,
  title,
  text,
  confirm,
  danger,
  onClose,
  onConfirm,
  children,
}: {
  open: boolean;
  title: string;
  text?: string;
  confirm: string;
  danger?: boolean;
  onClose: () => void;
  onConfirm: () => void | Promise<void>;
  children?: ReactNode;
}) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  return (
    <Dialog open={open} onOpenChange={(_, d) => !d.open && onClose()}>
      <DialogSurface>
        <DialogBody>
          <DialogTitle>{title}</DialogTitle>
          <DialogContent>
            <div className="stack">
              {text && <p className="muted" style={{ margin: 0 }}>{text}</p>}
              {children}
            </div>
          </DialogContent>
          <DialogActions>
            <Button onClick={onClose}>{t("common.cancel")}</Button>
            <Button
              appearance="primary"
              disabled={busy}
              style={danger ? { background: "var(--bad)", color: "#0a0a0c" } : undefined}
              onClick={async () => {
                setBusy(true);
                try {
                  await onConfirm();
                } finally {
                  setBusy(false);
                }
              }}
            >
              {confirm}
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}

/** A labelled input bound to a value. */
export function TextField({ label, hint, value, onChange, full, ...rest }: { label: string; hint?: string; value: string | number; onChange: (v: string) => void; full?: boolean } & Omit<InputProps, "value" | "onChange">) {
  return (
    <Field label={label} hint={hint} className={full ? "full" : undefined}>
      <Input value={String(value ?? "")} onChange={(_, d) => onChange(d.value)} {...rest} />
    </Field>
  );
}

export function RefreshButton({ onClick, busy }: { onClick: () => void; busy?: boolean }) {
  const { t } = useTranslation();
  return <Button appearance="subtle" icon={<ArrowClockwise16Regular />} disabled={busy} onClick={onClick} aria-label={t("common.refresh")} title={t("common.refresh")} />;
}

/** Runs an action with a busy flag and turns failures into a toast. */
export function useAction() {
  const { toast } = useApp();
  const [busy, setBusy] = useState<string | null>(null);
  const run = async <T,>(key: string, fn: () => Promise<T>, ok?: string): Promise<T | undefined> => {
    setBusy(key);
    try {
      const r = await fn();
      if (ok) toast("success", ok);
      return r;
    } catch (e) {
      toast("error", typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
      return undefined;
    } finally {
      setBusy(null);
    }
  };
  return { busy, run };
}
