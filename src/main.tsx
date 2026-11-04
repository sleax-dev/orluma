import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { initI18n } from "./i18n";
import { api, DEFAULT_SETTINGS, errText, logEvent } from "./lib/api";
import "@fontsource-variable/inter";
import "./styles.css";

// A desktop app, not a web page: no browser context menu. Copy and paste still work.
addEventListener("contextmenu", (e) => {
  if (!(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement)) e.preventDefault();
});

// What the interface did not catch itself still ends up in the log.
addEventListener("error", (e) => logEvent("error", "window", `${e.message} at ${e.filename}:${e.lineno}`));
addEventListener("unhandledrejection", (e) => logEvent("error", "promise", errText(e.reason)));

// Settings first, so the very first paint is already in the right language.
const settings = await api.getSettings().catch(() => DEFAULT_SETTINGS);
await initI18n(settings.language);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App initial={settings} />
  </StrictMode>,
);
