import { Button, Spinner, Switch } from "@fluentui/react-components";
import { ArrowDownload16Regular, ArrowSync16Regular, Copy16Regular, Folder16Regular, KeyMultiple16Regular, Open16Regular } from "@fluentui/react-icons";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Diagnostics } from "../components/Diagnostics";
import { LanguagePicker } from "../components/LanguagePicker";
import { Confirm, Logo, PageHead, useAction } from "../components/ui";
import { api } from "../lib/api";
import { bytes, relative } from "../lib/format";
import { useApp } from "../state";

function Row({ name, desc, children }: { name: string; desc?: string; children: React.ReactNode }) {
  return (
    <div className="row">
      <div className="info">
        <div className="name">{name}</div>
        {desc && <div className="desc">{desc}</div>}
      </div>
      <div className="ctl">{children}</div>
    </div>
  );
}

export function SettingsPage() {
  const { t } = useTranslation();
  const { settings, saveSettings, info, update, setUpdate, servers, configs, loadConfig, saveConfig, toast } = useApp();
  const [kit, setKit] = useState(false);
  const { busy, run } = useAction();
  const set = (p: Partial<typeof settings>) => void saveSettings({ ...settings, ...p });
  const agentsAuto = servers.length > 0 && servers.every((s) => configs[s.id]?.agent.auto_update !== false);

  return (
    <div className="page">
      <PageHead title={t("nav.settings")} />

      <section className="block">
        <div className="block-head">
          <h2>{t("settings.general")}</h2>
        </div>
        <div className="rows">
          <Row name={t("settings.language")}>
            <LanguagePicker onChange={(l) => set({ language: l })} />
          </Row>
          <Row name={t("settings.startWithOs")} desc={t("settings.startWithOsText")}>
            <Switch checked={settings.start_with_os} onChange={(_, d) => set({ start_with_os: d.checked })} />
          </Row>
          <Row name={t("settings.background")} desc={t("settings.backgroundText")}>
            <Switch checked={settings.run_in_background} onChange={(_, d) => set({ run_in_background: d.checked })} />
          </Row>
        </div>
      </section>

      <section className="block">
        <div className="block-head">
          <h2>{t("settings.updates")}</h2>
        </div>
        <div className="rows">
          <Row
            name={update?.available ? t("settings.updateAvailable", { version: update.available.version }) : t("settings.upToDate", { version: update?.current ?? info?.version ?? "" })}
            desc={update?.error ?? (update?.last_checked ? t("settings.lastChecked", { when: relative(new Date(update.last_checked * 1000).toISOString()) }) : undefined)}
          >
            {update?.installing ? (
              <span className="small muted">
                <Spinner size="extra-tiny" /> {t("settings.downloading", { done: bytes(update.downloaded), total: bytes(update.total ?? 0) })}
              </span>
            ) : update?.available ? (
              <Button appearance="primary" icon={<ArrowDownload16Regular />} onClick={() => void api.installUpdate().catch(() => {})}>
                {t("settings.installUpdate")}
              </Button>
            ) : (
              <Button icon={update?.checking ? <Spinner size="extra-tiny" /> : <ArrowSync16Regular />} disabled={update?.checking} onClick={() => void api.checkUpdate().then(setUpdate)}>
                {t("settings.checkNow")}
              </Button>
            )}
          </Row>
          <Row name={t("settings.autoUpdate")} desc={t("settings.autoUpdateText")}>
            <Switch checked={settings.auto_update} onChange={(_, d) => set({ auto_update: d.checked })} />
          </Row>
          <Row name={t("settings.agentsAuto")} desc={t("settings.agentsAutoText")}>
            <Switch
              checked={agentsAuto}
              disabled={servers.length === 0}
              onChange={(_, d) =>
                void run("agents", async () => {
                  for (const s of servers) {
                    const cfg = await loadConfig(s.id, true);
                    if (cfg) await saveConfig(s.id, { ...cfg, agent: { ...cfg.agent, auto_update: d.checked } });
                  }
                })
              }
            />
          </Row>
        </div>
      </section>

      <section className="block">
        <div className="block-head">
          <h2>{t("settings.thisPc")}</h2>
        </div>
        <div className="rows">
          <Row name={t("settings.pcFolder")} desc={info?.pc_root}>
            <Button
              icon={<Folder16Regular />}
              onClick={async () => {
                const dir = await api.pickFolder();
                if (dir) set({ pc_storage_dir: dir });
              }}
            >
              {t("settings.change")}
            </Button>
          </Row>
          <Row name={t("settings.appKey")} desc={t("settings.appKeyText")}>
            <Button icon={<Copy16Regular />} disabled={!info} onClick={() => info && void api.copy(info.public_key).then(() => toast("success", t("common.copied")))}>
              {t("common.copy")}
            </Button>
          </Row>
          <Row name={t("settings.dataFolder")} desc={info?.data_dir}>
            <Button icon={<Open16Regular />} disabled={!info} onClick={() => info && void api.reveal(info.data_dir)}>
              {t("pc.open")}
            </Button>
          </Row>
        </div>
      </section>

      <section className="block">
        <div className="block-head">
          <h2>{t("settings.safety")}</h2>
          <p className="muted">{t("settings.safetyText")}</p>
        </div>
        <div className="rows">
          <Row name={t("settings.recoveryKit")} desc={t("settings.recoveryKitText")}>
            <Button icon={busy === "kit" ? <Spinner size="extra-tiny" /> : <KeyMultiple16Regular />} onClick={() => setKit(true)}>
              {t("settings.exportKit")}
            </Button>
          </Row>
        </div>
      </section>

      <Diagnostics />

      <section className="block">
        <div className="block-head">
          <h2>{t("settings.about")}</h2>
        </div>
        <div className="panel" style={{ flexDirection: "row", alignItems: "center", gap: 14 }}>
          <Logo size={44} />
          <div className="grow">
            <div style={{ fontWeight: 600 }}>Orluma {info?.version}</div>
            <div className="small muted">{t("settings.aboutText")}</div>
          </div>
          <Button appearance="subtle" onClick={() => void api.openUrl("https://github.com/sleax-dev/orluma")}>GitHub</Button>
          <Button appearance="subtle" onClick={() => void api.openUrl("https://github.com/sleax-dev/orluma/blob/main/CHANGELOG.md")}>{t("settings.changelog")}</Button>
        </div>
      </section>

      <Confirm
        open={kit}
        title={t("settings.recoveryKit")}
        text={t("settings.kitWarn")}
        confirm={t("settings.exportKit")}
        onClose={() => setKit(false)}
        onConfirm={async () => {
          const target = await api.pickSave("orluma-recovery-kit.json");
          setKit(false);
          if (!target) return;
          const missing = await run("kit", () => api.recoveryKit(target));
          if (missing === undefined) return;
          toast(missing ? "error" : "success", missing ? t("settings.kitPartial", { count: missing }) : t("settings.kitSaved"));
        }}
      />
    </div>
  );
}
