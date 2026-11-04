import { Button, Spinner, Tooltip } from "@fluentui/react-components";
import { Add16Regular, Checkmark16Regular, Info16Regular, Play16Filled, SearchSparkle16Regular, ServerMultiple20Regular, Desktop16Regular } from "@fluentui/react-icons";
import { type ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, errText } from "../lib/api";
import { useApp } from "../state";
import type { Page } from "../nav";

/** The small "i" next to a title or label: the long explanation lives here, not on the page. */
export function Info({ text }: { text: string }) {
  const { t } = useTranslation();
  return (
    <Tooltip content={<span style={{ whiteSpace: "pre-line" }}>{text}</span>} relationship="description" withArrow positioning="below">
      <button type="button" className="info-btn" aria-label={t("help.what")}>
        <Info16Regular />
      </button>
    </Tooltip>
  );
}

/** An empty screen that says what to do next, once. */
export function Guide({ icon, title, text, actions, children }: { icon: ReactNode; title: string; text?: string; actions?: ReactNode; children?: ReactNode }) {
  return (
    <div className="guide">
      <span className="guide-ico">{icon}</span>
      <h2>{title}</h2>
      {text && <p>{text}</p>}
      {actions && <div className="guide-actions">{actions}</div>}
      {children}
    </div>
  );
}

/** The three things a new user does, shown small so every empty page can point at them. */
function Path() {
  const { t } = useTranslation();
  return (
    <ol className="path" aria-label={t("start.title")}>
      {(["s1", "s2", "s3"] as const).map((k, i) => (
        <li key={k}>
          <span className="n">{i + 1}</span>
          {t(`start.${k}.title`)}
        </li>
      ))}
    </ol>
  );
}

/** A page that works on a server, opened before any server exists. */
export function NoServer({ page, onAdd }: { page: Page; onAdd?: () => void }) {
  const { t } = useTranslation();
  const { setPage } = useApp();
  return (
    <Guide
      icon={<ServerMultiple20Regular />}
      title={t("guide.noServer")}
      text={t(`guide.purpose.${page}`)}
      actions={
        <Button appearance="primary" icon={<Add16Regular />} onClick={onAdd ?? (() => setPage("servers", { add: "1" }))}>
          {t("servers.add")}
        </Button>
      }
    >
      <Path />
    </Guide>
  );
}

const STEP = ["s1", "s2", "s3", "s4"] as const;
type StepState = "done" | "now" | "later";

/** Setup as a short list: what is done, what is next, one button on the next one. */
export function GetStarted({ compact }: { compact?: boolean }) {
  const { t } = useTranslation();
  const { servers, configs, loadConfig, live, settings, setPage, toast, refreshStatus } = useApp();
  const [starting, setStarting] = useState(false);

  useEffect(() => {
    servers.forEach((s) => void loadConfig(s.id));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [servers]);

  const first = servers[0];
  const jobs = servers.flatMap((s) => (configs[s.id]?.jobs ?? []).map((j) => ({ server: s.id, job: j })));
  const ranOk = servers.some((s) => (live[s.id]?.status?.jobs ?? []).some((j) => j.last_success));
  const pcCopy = settings.pc_syncs.length > 0;
  const loaded = servers.every((s) => configs[s.id]);

  const done = [servers.length > 0, jobs.length > 0, ranOk, pcCopy];
  const now = done.findIndex((d, i) => !d && i < 3);
  const state = (i: number): StepState => (done[i] ? "done" : i === now || (i === 3 && now === -1) ? "now" : "later");
  if (done[0] && loaded && done[1] && done[2] && (compact || pcCopy)) return null;

  const runFirst = async () => {
    const target = jobs[0];
    if (!target) return;
    setStarting(true);
    try {
      await api.runJob(target.server, target.job.id);
      toast("info", t("job.started", { name: target.job.name }));
      void refreshStatus(target.server);
    } catch (e) {
      toast("error", errText(e));
    } finally {
      setStarting(false);
    }
  };

  const actions: ReactNode[] = [
    <Button key="1" appearance="primary" icon={<Add16Regular />} onClick={() => setPage("servers", { add: "1" })}>{t("servers.add")}</Button>,
    <Button key="2" appearance="primary" icon={<SearchSparkle16Regular />} disabled={!first} onClick={() => first && setPage("backup", { server: first.id })}>{t("start.s2.action")}</Button>,
    <Button key="3" appearance="primary" icon={starting ? <Spinner size="extra-tiny" /> : <Play16Filled />} disabled={starting || jobs.length === 0} onClick={() => void runFirst()}>{t("start.s3.action")}</Button>,
    <Button key="4" icon={<Desktop16Regular />} onClick={() => setPage("storage")}>{t("start.s4.action")}</Button>,
  ];

  return (
    <section className="start" aria-label={t("start.title")}>
      <header>
        <h2>{t("start.title")}</h2>
        <span className="small muted">{t("start.progress", { done: done.slice(0, 3).filter(Boolean).length, total: 3 })}</span>
      </header>
      <ol>
        {[0, 1, 2, 3].map((i) => {
          const st = state(i);
          return (
            <li key={i} className={st}>
              <span className="mark-n" aria-hidden>{st === "done" ? <Checkmark16Regular /> : i + 1}</span>
              <div className="s-body">
                <div className="t">
                  {t(`start.${STEP[i]}.title`)}
                  {i === 3 && <span className="chip">{t("start.optional")}</span>}
                </div>
                {st !== "done" && <div className="d">{t(`start.${STEP[i]}.text`)}</div>}
              </div>
              {st === "now" && <div className="act">{actions[i]}</div>}
            </li>
          );
        })}
      </ol>
    </section>
  );
}
