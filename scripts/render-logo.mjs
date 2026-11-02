// Renders docs/brand/logo.svg to the 1024px PNG the app icons are made from:
//   node scripts/render-logo.mjs && npx tauri icon docs/brand/logo-1024.png
import { chromium } from "playwright";
import { fileURLToPath } from "node:url";

const svg = fileURLToPath(new URL("../docs/brand/logo.svg", import.meta.url));
const out = fileURLToPath(new URL("../docs/brand/logo-1024.png", import.meta.url));
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1024, height: 1024 } });
await page.goto(`file://${svg}`);
await page.screenshot({ path: out, omitBackground: true });
await browser.close();
