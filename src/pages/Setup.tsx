import { Button, Switch } from "@fluentui/react-components";
import { Checkmark12Filled, CheckmarkCircle16Filled, Folder16Regular } from "@fluentui/react-icons";
import { AnimatePresence, motion } from "motion/react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { LanguagePicker } from "../components/LanguagePicker";
import { Logo } from "../components/ui";
import { api } from "../lib/api";
import { useApp } from "../state";

const STEPS = ["welcome", "pc", "done"] as const;

/** First run: language, where PC copies go, then the first server. */
export function Setup() {
  const { t } = useTranslation();
  const { settings, saveSettings, info, setPage } = useApp();
  const [step, setStep] = useState(0);
  const [s, setS] = useState(settings);
  const id = STEPS[step];

  const finish = async (addServer: boolean) => {
    await saveSettings({ ...s, setup_done: true });
    setPage(addServer ? "servers" : "overview", addServer ? { add: "1" } : {});
  };

  return (
    <div className="setup">
      <div className="setup-card">
        <ol className="stepper">
          {STEPS.map((x, i) => (
            <li key={x} className={i < step ? "done" : i === step ? "now" : ""}>
              <span className="dot">{i < step ? <Checkmark12Filled /> : i + 1}</span>
              {t(`setup.steps.${x}`)}
            </li>
          ))}
        </ol>
        <AnimatePresence mode="wait">
          <motion.div key={id} className="stack" style={{ gap: 16 }} initial={{ opacity: 0, x: 12 }} animate={{ opacity: 1, x: 0 }} exit={{ opacity: 0, x: -12 }} transition={{ duration: 0.18 }}>
            {id === "welcome" && (
              <>
                <Logo size={56} />
                <h2>{t("setup.welcomeTitle")}</h2>
                <p>{t("setup.welcomeText")}</p>
                <ul className="promises">
                  {["encrypted", "noPorts", "anywhere", "alerts"].map((p) => (
                    <li key={p}>
                      <CheckmarkCircle16Filled />
                      <span>{t(`setup.promise.${p}`)}</span>
                    </li>
                  ))}
                </ul>
                <div className="lang-line">
                  <span className="muted">{t("settings.language")}</span>
                  <LanguagePicker onChange={(l) => setS({ ...s, language: l })} />
                </div>
              </>
            )}
            {id === "pc" && (
              <>
                <h2>{t("setup.pcTitle")}</h2>
                <p>{t("setup.pcText")}</p>
                <div className="rows">
                  <div className="row">
                    <div className="info">
                      <div className="name">{t("settings.pcFolder")}</div>
                      <div className="desc mono">{s.pc_storage_dir || info?.pc_root}</div>
                    </div>
                    <Button icon={<Folder16Regular />} onClick={async () => { const d = await api.pickFolder(); if (d) setS({ ...s, pc_storage_dir: d }); }}>
                      {t("settings.change")}
                    </Button>
                  </div>
                  <div className="row">
                    <div className="info">
                      <div className="name">{t("settings.startWithOs")}</div>
                      <div className="desc">{t("settings.startWithOsText")}</div>
                    </div>
                    <Switch checked={s.start_with_os} onChange={(_, d) => setS({ ...s, start_with_os: d.checked })} />
                  </div>
                  <div className="row">
                    <div className="info">
                      <div className="name">{t("settings.background")}</div>
                      <div className="desc">{t("settings.backgroundText")}</div>
                    </div>
                    <Switch checked={s.run_in_background} onChange={(_, d) => setS({ ...s, run_in_background: d.checked })} />
                  </div>
                </div>
              </>
            )}
            {id === "done" && (
              <>
                <h2>{t("setup.doneTitle")}</h2>
                <p>{t("setup.doneText")}</p>
                <ul className="promises">
                  {["root", "ssh", "once"].map((p) => (
                    <li key={p}>
                      <CheckmarkCircle16Filled />
                      <span>{t(`setup.need.${p}`)}</span>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </motion.div>
        </AnimatePresence>
        <div className="setup-foot">
          {step > 0 ? <Button appearance="subtle" onClick={() => setStep(step - 1)}>{t("common.back")}</Button> : <span />}
          {id !== "done" ? (
            <Button appearance="primary" onClick={() => setStep(step + 1)}>{t("common.next")}</Button>
          ) : (
            <div className="btn-row">
              <Button appearance="subtle" onClick={() => void finish(false)}>{t("setup.later")}</Button>
              <Button appearance="primary" onClick={() => void finish(true)}>{t("servers.add")}</Button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
