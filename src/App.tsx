import { FluentProvider } from "@fluentui/react-components";
import { ArrowDownload16Regular } from "@fluentui/react-icons";
import { AnimatePresence, motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { Floating, ResizeEdges, TitleBar } from "./components/ui";
import { isRtl } from "./i18n";
import type { Settings } from "./lib/types";
import { ICONS, NAV_GROUPS, type Page } from "./nav";
import { Activity } from "./pages/Activity";
import { Alerts } from "./pages/Alerts";
import { BackupWizard } from "./pages/BackupWizard";
import { Jobs } from "./pages/Jobs";
import { Overview } from "./pages/Overview";
import { RestorePage } from "./pages/Restore";
import { Servers } from "./pages/Servers";
import { SettingsPage } from "./pages/SettingsPage";
import { Setup } from "./pages/Setup";
import { Transfer } from "./pages/Transfer";
import { Storage } from "./pages/Storage";
import { AppProvider, useApp } from "./state";
import { theme } from "./theme";

function NavButton({ page }: { page: Page }) {
  const { t } = useTranslation();
  const { page: current, setPage, running, live } = useApp();
  const active = current === page || (page === "jobs" && current === "backup");
  const Icon = ICONS[page][active ? 1 : 0];
  const offline = page === "servers" ? Object.values(live).filter((l) => l.online === false).length : 0;
  return (
    <button className={`nav-item${active ? " active" : ""}`} aria-current={active ? "page" : undefined} onClick={() => setPage(page)}>
      {active && <motion.span layoutId="nav-ind" className="ind" transition={{ type: "spring", stiffness: 500, damping: 36 }} />}
      <Icon />
      <span className="label">{t(`nav.${page}`)}</span>
      {page === "activity" && running.length > 0 && <span className="count">{running.length}</span>}
      {offline > 0 && <span className="dot" role="img" aria-label={t("nav.offline", { count: offline })} />}
    </button>
  );
}

function PageView({ page }: { page: Page }) {
  switch (page) {
    case "overview":
      return <Overview />;
    case "servers":
      return <Servers />;
    case "jobs":
      return <Jobs />;
    case "backup":
      return <BackupWizard />;
    case "storage":
      return <Storage />;
    case "restore":
      return <RestorePage />;
    case "transfer":
      return <Transfer />;
    case "activity":
      return <Activity />;
    case "alerts":
      return <Alerts />;
    default:
      return <SettingsPage />;
  }
}

function Shell() {
  const { t } = useTranslation();
  const { page, setPage, update, settings } = useApp();
  if (!settings.setup_done) return <Setup />;
  return (
    <div className="body">
      <nav className="sidebar" aria-label={t("nav.label")}>
        {NAV_GROUPS.map((g) => (
          <div className="nav-group" key={g.label ?? "main"}>
            {g.label && <div className="nav-label">{t(`nav.group.${g.label}`)}</div>}
            {g.pages.map((p) => (
              <NavButton key={p} page={p} />
            ))}
          </div>
        ))}
        <span className="spacer" />
        {update?.available && (
          <button className="update-chip" onClick={() => setPage("settings")}>
            <ArrowDownload16Regular />
            {t("settings.updateTo", { version: update.available.version })}
          </button>
        )}
        <NavButton page="settings" />
      </nav>
      <main className="content">
        <AnimatePresence mode="wait" initial={false}>
          <motion.div key={page} initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} transition={{ duration: 0.14 }}>
            <PageView page={page} />
          </motion.div>
        </AnimatePresence>
      </main>
    </div>
  );
}

export function App({ initial }: { initial: Settings }) {
  const { i18n } = useTranslation();
  return (
    <FluentProvider theme={theme} dir={isRtl(i18n.language) ? "rtl" : "ltr"} style={{ background: "transparent", height: "100%" }}>
      <AppProvider initial={initial}>
        <div className="app-bg">
          <ResizeEdges />
          <TitleBar />
          <Shell />
          <Floating />
        </div>
      </AppProvider>
    </FluentProvider>
  );
}
