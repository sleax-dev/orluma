import { Button, Checkbox, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Menu, MenuItem, MenuList, MenuPopover, MenuTrigger, Spinner } from "@fluentui/react-components";
import {
  Add16Regular,
  ArrowDownload16Regular,
  CheckmarkCircle16Filled,
  Circle16Regular,
  Copy16Regular,
  Delete16Regular,
  DismissCircle16Filled,
  Edit16Regular,
  MoreHorizontal20Regular,
  ServerMultiple20Regular,
  SearchSparkle16Regular,
} from "@fluentui/react-icons";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { FirstBackup } from "../components/FirstBackup";
import { Info, NoServer } from "../components/guide";
import { Confirm, PageHead, RefreshButton, Skeleton, Status, TextField, useAction } from "../components/ui";
import { api, errText } from "../lib/api";
import { bytes, duration } from "../lib/format";
import type { Server } from "../lib/types";
import { useApp } from "../state";

type Step = "connect" | "verify" | "installing" | "done";

function AddServer({ open, onClose, onAdded }: { open: boolean; onClose: () => void; onAdded: (s: Server) => void }) {
  const { t } = useTranslation();
  const { info } = useApp();
  const [step, setStep] = useState<Step>("connect");
  const [f, setF] = useState({ name: "", host: "", port: "22", user: "root", password: "" });
  const [keyOnly, setKeyOnly] = useState(false);
  const [fingerprint, setFingerprint] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<"auth" | "agent" | null>(null);

  useEffect(() => {
    if (open) {
      setStep("connect");
      setError(null);
      setF({ name: "", host: "", port: "22", user: "root", password: "" });
    }
  }, [open]);

  useEffect(() => {
    const off = api.listen<string>("setup-step", (s) => setProgress(s === "agent" ? "agent" : "auth"));
    return () => void off.then((x) => x());
  }, []);

  const probe = async () => {
    setBusy(true);
    setError(null);
    try {
      setFingerprint(await api.probeServer(f.host.trim(), Number(f.port) || 22));
      setStep("verify");
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  };

  const install = async () => {
    setStep("installing");
    setProgress("auth");
    setError(null);
    try {
      const s = await api.addServer({ name: f.name, host: f.host.trim(), port: Number(f.port) || 22, user: f.user.trim(), password: keyOnly ? "" : f.password, fingerprint });
      setStep("done");
      onAdded(s);
    } catch (e) {
      setError(friendly(errText(e), t));
      setStep("verify");
    }
  };

  const steps: { id: "auth" | "agent"; label: string }[] = [
    { id: "auth", label: t("add.stepAuth") },
    { id: "agent", label: t("add.stepAgent") },
  ];

  return (
    <Dialog open={open} onOpenChange={(_, d) => !d.open && step !== "installing" && onClose()}>
      <DialogSurface className="dialog-wide">
        <DialogBody>
          <DialogTitle>{t("add.title")}</DialogTitle>
          <DialogContent>
            <div className="stack">
              {step === "connect" && (
                <>
                  <p className="muted" style={{ margin: 0 }}>{t("add.text")}</p>
                  <div className="form">
                    <TextField label={t("add.host")} value={f.host} onChange={(v) => setF({ ...f, host: v })} placeholder="203.0.113.10" autoFocus />
                    <TextField label={t("add.name")} value={f.name} onChange={(v) => setF({ ...f, name: v })} placeholder={f.host || "web-01"} />
                    <TextField label={t("add.user")} value={f.user} onChange={(v) => setF({ ...f, user: v })} hint={t("add.userHint")} />
                    <TextField label={t("add.port")} value={f.port} onChange={(v) => setF({ ...f, port: v.replace(/\D/g, "") })} />
                  </div>
                </>
              )}
              {(step === "verify" || step === "installing") && (
                <>
                  <p style={{ margin: 0 }}>{t("add.verify", { host: f.host })}</p>
                  <div className="fingerprint">{fingerprint}</div>
                  <p className="muted small" style={{ margin: 0 }}>{t("add.verifyHint")}</p>
                  {step === "verify" && (
                    <div className="form">
                      {!keyOnly && <TextField label={t("add.password")} type="password" value={f.password} onChange={(v) => setF({ ...f, password: v })} hint={t("add.passwordHint")} full autoFocus />}
                      <Checkbox className="full" checked={keyOnly} onChange={(_, d) => setKeyOnly(!!d.checked)} label={t("add.keyOnly")} />
                      {keyOnly && info && (
                        <div className="full stack">
                          <span className="small muted">{t("add.keyOnlyHint")}</span>
                          <div className="fingerprint">{info.public_key}</div>
                          <Button size="small" icon={<Copy16Regular />} onClick={() => void api.copy(info.public_key)} style={{ alignSelf: "flex-start" }}>
                            {t("common.copy")}
                          </Button>
                        </div>
                      )}
                    </div>
                  )}
                  {step === "installing" && (
                    <ul className="steps-live">
                      {steps.map((s) => {
                        const done = s.id === "auth" && progress === "agent";
                        const now = s.id === progress;
                        return (
                          <li key={s.id} className={done ? "done" : now ? "now" : ""}>
                            {done ? <CheckmarkCircle16Filled /> : now ? <Spinner size="extra-tiny" /> : <Circle16Regular />}
                            {s.label}
                          </li>
                        );
                      })}
                    </ul>
                  )}
                </>
              )}
              {step === "done" && (
                <p className="ok-line">
                  <CheckmarkCircle16Filled /> {t("add.done")}
                </p>
              )}
              {error && (
                <p className="bad-text selectable" style={{ display: "flex", gap: 8 }}>
                  <DismissCircle16Filled style={{ flex: "none", marginTop: 2 }} /> {error}
                </p>
              )}
            </div>
          </DialogContent>
          <DialogActions>
            {step !== "installing" && <Button onClick={onClose}>{step === "done" ? t("common.close") : t("common.cancel")}</Button>}
            {step === "connect" && (
              <Button appearance="primary" disabled={!f.host.trim() || !f.user.trim() || busy} onClick={() => void probe()}>
                {busy ? <Spinner size="tiny" /> : t("common.next")}
              </Button>
            )}
            {step === "verify" && (
              <Button appearance="primary" disabled={!keyOnly && !f.password} onClick={() => void install()}>
                {t("add.install")}
              </Button>
            )}
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}

/** Error codes from the Rust side, in words. */
function friendly(e: string, t: (k: string) => string): string {
  for (const code of ["AUTH_FAILED", "NOT_ROOT", "NO_SYSTEMD", "UNSUPPORTED_ARCH", "HOST_KEY_CHANGED", "AGENT_UNREACHABLE"]) {
    if (e.startsWith(code)) return `${t(`errors.${code}`)} (${e.slice(code.length + 1).trim()})`;
  }
  return e;
}

export function Servers() {
  const { t } = useTranslation();
  const { servers, live, refreshServers, refreshStatus, params, toast } = useApp();
  const [adding, setAdding] = useState(params.add === "1");
  const [first, setFirst] = useState<string | null>(null);
  const [removing, setRemoving] = useState<Server | null>(null);
  const [uninstall, setUninstall] = useState(true);
  const [purge, setPurge] = useState(false);
  const [renaming, setRenaming] = useState<Server | null>(null);
  const { busy, run } = useAction();

  return (
    <div className="page">
      <PageHead
        title={t("nav.servers")}
        desc={t("servers.desc")}
        info={t("servers.info")}
        actions={
          <>
            <RefreshButton onClick={() => void refreshServers()} />
            <Button appearance="primary" icon={<Add16Regular />} onClick={() => setAdding(true)}>
              {t("servers.add")}
            </Button>
          </>
        }
      />
      {servers.length === 0 ? (
        <NoServer page="servers" onAdd={() => setAdding(true)} />
      ) : (
        <div className="rows">
          {servers.map((s) => {
            const l = live[s.id];
            const st = l?.status;
            return (
              <div className="row" key={s.id} style={{ alignItems: "flex-start" }}>
                <span className="icon">
                  <ServerMultiple20Regular />
                </span>
                <div className="info">
                  <div className="gap-s">
                    <span className="name">{s.name}</span>
                    <Status status={l?.online === true ? "online" : l?.online === false ? "offline" : "unknown"} />
                    {st?.update && <span className="tag">{t("servers.updateAvailable", { version: st.update })}</span>}
                  </div>
                  <div className="desc mono">
                    {s.user}@{s.host}:{s.port}
                  </div>
                  {st ? (
                    <div className="meta">
                      <span>{st.os}</span>
                      {st.disks.slice(0, 2).map((d) => (
                        <span key={d.mount}>
                          {d.mount} · {t("overview.card.free", { size: bytes(d.available) })}
                        </span>
                      ))}
                      <Info
                        text={[
                          `${st.arch} · ${t("servers.agent", { version: st.version })}`,
                          st.restic ? t("servers.restic", { version: st.restic }) : t("servers.noRestic"),
                          `${t("servers.uptime", { time: duration(st.uptime_secs) })} · ${st.timezone}`,
                          `${t("servers.hostKey")}: ${s.host_key}`,
                        ].join("\n")}
                      />
                    </div>
                  ) : l?.online === false ? (
                    <div className="desc bad-text selectable">{friendly(l.error ?? "", t)}</div>
                  ) : (
                    <Skeleton h={14} w={280} />
                  )}
                </div>
                <div className="ctl">
                  <Button size="small" icon={<SearchSparkle16Regular />} disabled={!st} onClick={() => setFirst(s.id)}>
                    {t("servers.whatToBackUp")}
                  </Button>
                  <Menu>
                    <MenuTrigger disableButtonEnhancement>
                      <Button size="small" appearance="subtle" icon={<MoreHorizontal20Regular />} aria-label={t("common.more")} />
                    </MenuTrigger>
                    <MenuPopover>
                      <MenuList>
                        <MenuItem icon={<Edit16Regular />} onClick={() => setRenaming({ ...s })}>
                          {t("servers.rename")}
                        </MenuItem>
                        <MenuItem icon={<ArrowDownload16Regular />} disabled={busy === s.id} onClick={() => void run(s.id, async () => { await api.reinstallAgent(s.id); await refreshStatus(s.id); }, t("servers.agentUpdated"))}>
                          {t("servers.reinstall")}
                        </MenuItem>
                        <MenuItem icon={<Copy16Regular />} disabled={!st} onClick={() => st && void api.copy(st.ssh_public_key).then(() => toast("success", t("common.copied")))}>
                          {t("servers.copyKey")}
                        </MenuItem>
                        <MenuItem icon={<Delete16Regular />} onClick={() => setRemoving(s)}>
                          {t("servers.remove")}
                        </MenuItem>
                      </MenuList>
                    </MenuPopover>
                  </Menu>
                </div>
              </div>
            );
          })}
        </div>
      )}

      <AddServer
        open={adding}
        onClose={() => setAdding(false)}
        onAdded={(s) => {
          void refreshServers();
          setAdding(false);
          setFirst(s.id);
        }}
      />
      {first && <FirstBackup server={first} open onClose={() => setFirst(null)} />}

      <Confirm
        open={!!removing}
        title={t("servers.removeTitle", { name: removing?.name })}
        text={t("servers.removeText")}
        confirm={t("servers.remove")}
        danger
        onClose={() => setRemoving(null)}
        onConfirm={async () => {
          if (!removing) return;
          await run("remove", () => api.removeServer(removing.id, uninstall, purge), t("servers.removed"));
          setRemoving(null);
          await refreshServers();
        }}
      >
        <Checkbox checked={uninstall} onChange={(_, d) => setUninstall(!!d.checked)} label={t("servers.uninstallAgent")} />
        {uninstall && <Checkbox checked={purge} onChange={(_, d) => setPurge(!!d.checked)} label={t("servers.purge")} />}
      </Confirm>

      <Dialog open={!!renaming} onOpenChange={(_, d) => !d.open && setRenaming(null)}>
        <DialogSurface>
          <DialogBody>
            <DialogTitle>{t("servers.rename")}</DialogTitle>
            <DialogContent>
              {renaming && <TextField label={t("add.name")} value={renaming.name} onChange={(v) => setRenaming({ ...renaming, name: v })} />}
            </DialogContent>
            <DialogActions>
              <Button onClick={() => setRenaming(null)}>{t("common.cancel")}</Button>
              <Button
                appearance="primary"
                onClick={async () => {
                  if (renaming) await run("rename", () => api.updateServer(renaming));
                  setRenaming(null);
                  await refreshServers();
                }}
              >
                {t("common.save")}
              </Button>
            </DialogActions>
          </DialogBody>
        </DialogSurface>
      </Dialog>
    </div>
  );
}
