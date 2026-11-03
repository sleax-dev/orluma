// Fails when the UI uses a translation key that a language lacks, when a language is missing an English key or a
// plural form it needs (Intl.PluralRules), or when a language has a key English does not.
// Run with: node --experimental-strip-types scripts/check-i18n.mjs
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import en from "../src/i18n/locales/en.ts";

const walk = (d) => readdirSync(d).flatMap((f) => (statSync(join(d, f)).isDirectory() ? walk(join(d, f)) : [join(d, f)]));
const sources = walk("src").filter((f) => /\.tsx?$/.test(f) && !f.includes("/locales/"));

const keys = new Set();
const templates = [];
for (const f of sources) {
  const text = readFileSync(f, "utf8");
  for (const m of text.matchAll(/\bt\(\s*"([A-Za-z0-9_.]+)"/g)) keys.add(m[1]);
  for (const m of text.matchAll(/\bt\(\s*`([^`]+)`/g)) templates.push(m[1]);
}

// keys built at runtime: the template's prefix names the group, and every key in that group must exist,
// so adding a type to the code without text for it shows up here.
const dynamic = {
  "activity.kind.": 5, "activity.trigger.": 3, "advanced.comp.": 3, "alert.event.": 6, "alert.eventShort.": 6, "alert.hint.": 4,
  "alert.type.": 9, "dest.about.": 8, "dest.type.": 8, "errors.": 6, "first.hint.": 5, "job.tab.": 6, "nav.": 9, "nav.group.": 2,
  "retention.": 9, "retention.preset.": 4, "schedule.kind.": 6, "schedule.weekday.": 7, "setup.need.": 3, "setup.promise.": 4, "setup.steps.": 3,
  "source.about.": 8, "source.type.": 8, "status.": 7,
};
const at = (obj, key) => key.split(".").reduce((o, k) => (o && typeof o === "object" ? o[k] : undefined), obj);
let bad = 0;
const groups = new Set(templates.map((t) => t.replace(/\$\{[^}]*\}.*$/, "")));
for (const g of groups) {
  if (!(g in dynamic)) {
    console.error(`dynamic key group "${g}" is not listed in scripts/check-i18n.mjs`);
    bad++;
    continue;
  }
  const n = Object.keys(at(en, g.slice(0, -1)) ?? {}).length;
  if (n < dynamic[g]) {
    console.error(`"${g}" has ${n} texts in English, expected at least ${dynamic[g]}`);
    bad++;
  }
}

const has = (obj, key) => [key, `${key}_other`].some((k) => at(obj, k) !== undefined);
const flat = (o, p = "") => Object.entries(o).flatMap(([k, v]) => (typeof v === "object" ? flat(v, `${p}${k}.`) : [`${p}${k}`]));
const PLURAL = /_(zero|one|two|few|many|other)$/;
const enKeys = [...new Set(flat(en).map((k) => k.replace(PLURAL, "")))];
const plurals = new Set(flat(en).filter((k) => k.endsWith("_other")).map((k) => k.slice(0, -"_other".length)));

for (const k of keys) {
  if (!has(en, k)) {
    console.error(`en: the UI uses "${k}" but English has no text for it`);
    bad++;
  }
}

const langs = readdirSync("src/i18n/locales").map((f) => f.replace(/\.ts$/, ""));
for (const lang of langs) {
  const locale = (await import(`../src/i18n/locales/${lang}.ts`)).default;
  const forms = new Intl.PluralRules(lang).resolvedOptions().pluralCategories;
  const missing = enKeys.flatMap((k) =>
    plurals.has(k) ? forms.map((f) => `${k}_${f}`).filter((pk) => at(locale, pk) === undefined) : has(locale, k) ? [] : [k],
  );
  const extra = flat(locale).map((k) => k.replace(PLURAL, "")).filter((k) => !enKeys.includes(k));
  if (missing.length || extra.length) {
    bad += missing.length + extra.length;
    if (missing.length) console.error(`${lang}: ${missing.length} missing\n  ${missing.join("\n  ")}`);
    if (extra.length) console.error(`${lang}: ${extra.length} not in English\n  ${[...new Set(extra)].join("\n  ")}`);
  }
}
console.log(`${langs.length} languages, ${enKeys.length} strings checked`);
process.exit(bad ? 1 : 0);
