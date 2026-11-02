import i18n from "i18next";

const lang = () => i18n.language || "en";

export function bytes(n: number | null | undefined, digits = 1): string {
  if (n == null || !isFinite(n)) return "–";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  let v = n;
  let i = 0;
  while (v >= 1000 && i < units.length - 1) {
    v /= 1000;
    i++;
  }
  const num = new Intl.NumberFormat(lang(), { maximumFractionDigits: i === 0 ? 0 : digits }).format(v);
  return `${num} ${units[i]}`;
}

export function num(n: number): string {
  return new Intl.NumberFormat(lang()).format(n);
}

export function percent(f: number): string {
  return new Intl.NumberFormat(lang(), { style: "percent", maximumFractionDigits: 0 }).format(f);
}

/** "4m 12s", "2h 05m" */
export function duration(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m ${String(s % 60).padStart(2, "0")}s`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ${String(Math.floor((s % 3600) / 60)).padStart(2, "0")}m`;
  return `${Math.floor(s / 86400)}d ${Math.floor((s % 86400) / 3600)}h`;
}

export function between(start: string, end: string | null): string {
  if (!end) return "–";
  return duration((Date.parse(end) - Date.parse(start)) / 1000);
}

/** "3 hours ago", "in 20 minutes" in the UI language. */
export function relative(iso: string | null | undefined): string {
  if (!iso) return "–";
  const diff = (Date.parse(iso) - Date.now()) / 1000;
  const rtf = new Intl.RelativeTimeFormat(lang(), { numeric: "auto" });
  const abs = Math.abs(diff);
  if (abs < 60) return rtf.format(Math.round(diff), "second");
  if (abs < 3600) return rtf.format(Math.round(diff / 60), "minute");
  if (abs < 86400) return rtf.format(Math.round(diff / 3600), "hour");
  if (abs < 86400 * 30) return rtf.format(Math.round(diff / 86400), "day");
  return rtf.format(Math.round(diff / (86400 * 30)), "month");
}

export function dateTime(iso: string | null | undefined): string {
  if (!iso) return "–";
  return new Intl.DateTimeFormat(lang(), { dateStyle: "medium", timeStyle: "short" }).format(new Date(iso));
}

export function time(iso: string | null | undefined): string {
  if (!iso) return "–";
  return new Intl.DateTimeFormat(lang(), { hour: "2-digit", minute: "2-digit", second: "2-digit" }).format(new Date(iso));
}

export const shortId = (id: string) => id.slice(0, 8);
