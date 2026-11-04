import { Button, Dropdown, Option, Spinner } from "@fluentui/react-components";
import { ArrowClockwise16Regular, ArrowDownload16Regular, Copy16Regular } from "@fluentui/react-icons";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, errText } from "../lib/api";
import { useApp } from "../state";
import { Info } from "./guide";

type Level = "all" | "warn" | "error";
const LEVEL = /^\S+ (INFO|WARN|ERROR)/;

/** The log of this app and of each server's agent, and one report with both that can be copied or saved and sent. */
export function Diagnostics() {
  const { t } = useTranslation();
  const { servers, live, info, toast } = useApp();
  const [src, setSrc] = useState("app");
  const [level, setLevel] = useState<Level>("all");
  const [lines, setLines] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [working, setWorking] = useState(false);
  const view = useRef<HTMLPreElement>(null);

  const load = useCallback(async () => {
    setError(null);
    setLines(null);
    try {
      setLines(src === "app" ? await api.appLogs(400) : await api.agentLogs(src, 400));
    } catch (e) {
      const text = errText(e);
      setError(/404|not found/i.test(text) ? t("diag.oldAgent") : t("diag.failed", { error: text }));
    }
  }, [src, t]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    if (view.current) view.current.scrollTop = view.current.scrollHeight;
  }, [lines, level]);

  const shown = (lines ?? []).filter((l) => {
    const m = LEVEL.exec(l)?.[1];
    return level === "all" || m === "ERROR" || (level === "warn" && m === "WARN");
  });

  const report = async () => {
    const out = [
      "Orluma diagnostics report",
      `Generated: ${new Date().toISOString()}`,
      `App: Orluma ${info?.version ?? "?"} · ${info?.os ?? "?"} · ${navigator.language}`,
      "",
      "== Servers ==",
    ];
    for (const s of servers) {
      const l = live[s.id];
      const st = l?.status;
      out.push(`- ${s.name} (${s.user}@${s.host}:${s.port}): ${l?.online === false ? `OFFLINE ${l.error ?? ""}` : l?.online ? "online" : "unknown"}`);
      if (st) out.push(`    agent ${st.version} · restic ${st.restic ?? "none"} · ${st.os} · ${st.arch} · ${st.disks.map((d) => `${d.mount} ${Math.round((1 - d.available / d.total) * 100)}% used`).join(", ")}`);
      for (const j of st?.jobs ?? []) out.push(`    job ${j.id}: ${j.last_run ? `${j.last_run.status} at ${j.last_run.started}${j.last_run.error ? ` — ${j.last_run.error}` : ""}` : "never ran"}`);
    }
    out.push("", "== App log ==", ...(await api.appLogs(300).catch((e) => [`(unreadable: ${errText(e)})`])));
    for (const s of servers) {
      out.push("", `== Agent log: ${s.name} ==`, ...(await api.agentLogs(s.id, 300).catch((e) => [`(unreadable: ${errText(e)})`])));
    }
    return api.redact(out.join("\n"));
  };

  const act = async (what: "copy" | "save") => {
    setWorking(true);
    try {
      const text = await report();
      if (what === "copy") {
        await navigator.clipboard.writeText(text);
        toast("success", t("diag.copied"));
      } else {
        toast("success", t("diag.saved", { path: await api.exportReport(text) }));
      }
    } catch (e) {
      toast("error", errText(e));
    } finally {
      setWorking(false);
    }
  };

  const levels: Record<Level, string> = { all: t("diag.all"), warn: t("diag.warn"), error: t("diag.error") };
  const names: Record<string, string> = { app: t("diag.thisApp"), ...Object.fromEntries(servers.map((s) => [s.id, s.name])) };

  return (
    <section className="block">
      <div className="sect-head">
        <h2>
          {t("diag.title")} <Info text={t("diag.info")} />
        </h2>
        <div className="gap-s">
          <Button size="small" icon={working ? <Spinner size="extra-tiny" /> : <Copy16Regular />} disabled={working} onClick={() => void act("copy")}>
            {t("diag.copy")}
          </Button>
          <Button size="small" icon={<ArrowDownload16Regular />} disabled={working} onClick={() => void act("save")}>
            {t("diag.save")}
          </Button>
        </div>
      </div>
      <div className="gap-s" style={{ marginBottom: 8 }}>
        <Dropdown size="small" value={names[src] ?? src} selectedOptions={[src]} onOptionSelect={(_, d) => d.optionValue && setSrc(d.optionValue)}>
          {Object.entries(names).map(([k, n]) => (
            <Option key={k} value={k}>{n}</Option>
          ))}
        </Dropdown>
        <Dropdown size="small" value={levels[level]} selectedOptions={[level]} onOptionSelect={(_, d) => d.optionValue && setLevel(d.optionValue as Level)}>
          {Object.entries(levels).map(([k, n]) => (
            <Option key={k} value={k}>{n}</Option>
          ))}
        </Dropdown>
        <Button size="small" appearance="subtle" icon={<ArrowClockwise16Regular />} aria-label={t("diag.refresh")} title={t("diag.refresh")} onClick={() => void load()} />
      </div>
      {error ? (
        <p className="small muted">{error}</p>
      ) : lines === null ? (
        <Spinner size="tiny" />
      ) : shown.length === 0 ? (
        <p className="small muted">{t("diag.empty")}</p>
      ) : (
        <pre className="logview" ref={view} dir="ltr">
          {shown.map((l, i) => (
            <div key={i} className={`logline ${LEVEL.exec(l)?.[1] ?? ""}`}>{l}</div>
          ))}
        </pre>
      )}
    </section>
  );
}
