// README screenshots: the built UI in a browser, answered by the demo data in src/lib/mock.ts.
//   npm run build && npm run screenshots          (docs/screenshots/*.png)
//   SHOTS_LANG=tr npm run screenshots              (another language)
import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { chromium } from "playwright";

const lang = process.env.SHOTS_LANG ?? "en";
const out = process.env.SHOTS_DIR ?? "docs/screenshots";
const port = 4199;
mkdirSync(out, { recursive: true });

const server = spawn("npx", ["vite", "preview", "--port", String(port), "--strictPort"], { stdio: "ignore" });
await new Promise((r) => setTimeout(r, 2500));

const shots = [
  { name: "overview", page: "overview" },
  { name: "jobs", page: "jobs" },
  { name: "storage", page: "storage" },
  { name: "restore", page: "restore" },
  { name: "activity", page: "activity" },
  { name: "alerts", page: "alerts" },
  { name: "setup", page: "overview", query: "&setup=1" },
];

const browser = await chromium.launch();
try {
  for (const s of shots) {
    const page = await browser.newPage({ viewport: { width: 1180, height: 760 }, deviceScaleFactor: 2, colorScheme: "dark" });
    await page.goto(`http://localhost:${port}/?page=${s.page}&lang=${lang}${s.query ?? ""}`);
    await page.waitForTimeout(1400);
    // the window has rounded corners and a dark backdrop in the app; give the page the same
    await page.addStyleTag({ content: "html,body{background:#0a0a0c!important}" });
    await page.screenshot({ path: `${out}/${s.name}${lang === "en" ? "" : `-${lang}`}.png` });
    await page.close();
    console.log(`${out}/${s.name}`);
  }
} finally {
  await browser.close();
  server.kill();
}
