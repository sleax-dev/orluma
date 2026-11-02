import type { TFunction } from "i18next";

/** The schedules most people want, so nobody has to write cron. */
export type Preset = { id: string; cron: string };

export function presetFor(cron: string): string {
  const c = cron.trim();
  if (!c) return "manual";
  if (/^0 \*\/(\d+) \* \* \*$/.test(c)) return "hours";
  if (/^(\d+) (\d+) \* \* \*$/.test(c)) return "daily";
  if (/^(\d+) (\d+) \* \* (\d)$/.test(c)) return "weekly";
  if (/^(\d+) (\d+) (\d+) \* \*$/.test(c)) return "monthly";
  return "custom";
}

export interface Parts {
  hours: number;
  hour: number;
  minute: number;
  weekday: number;
  monthday: number;
}

export function parts(cron: string): Parts {
  const p: Parts = { hours: 6, hour: 3, minute: 0, weekday: 0, monthday: 1 };
  const f = cron.trim().split(/\s+/);
  if (f.length !== 5) return p;
  const n = (s: string, d: number) => (/^\d+$/.test(s) ? Number(s) : d);
  const every = /^\*\/(\d+)$/.exec(f[1]);
  if (every) p.hours = Number(every[1]);
  p.minute = n(f[0], 0);
  p.hour = n(f[1], 3);
  p.monthday = n(f[2], 1);
  p.weekday = n(f[4], 0);
  return p;
}

export function build(kind: string, p: Parts): string {
  switch (kind) {
    case "manual":
      return "";
    case "hours":
      return `0 */${Math.max(1, Math.min(23, p.hours))} * * *`;
    case "daily":
      return `${p.minute} ${p.hour} * * *`;
    case "weekly":
      return `${p.minute} ${p.hour} * * ${p.weekday}`;
    case "monthly":
      return `${p.minute} ${p.hour} ${p.monthday} * *`;
    default:
      return `${p.minute} ${p.hour} * * *`;
  }
}

const pad = (n: number) => String(n).padStart(2, "0");

/** A schedule in words: "Every day at 03:00". */
export function describe(cron: string, t: TFunction): string {
  const kind = presetFor(cron);
  const p = parts(cron);
  const at = `${pad(p.hour)}:${pad(p.minute)}`;
  switch (kind) {
    case "manual":
      return t("schedule.describe.manual");
    case "hours":
      return t("schedule.describe.hours", { count: p.hours });
    case "daily":
      return t("schedule.describe.daily", { time: at });
    case "weekly":
      return t("schedule.describe.weekly", { day: t(`schedule.weekday.${p.weekday % 7}`), time: at });
    case "monthly":
      return t("schedule.describe.monthly", { day: p.monthday, time: at });
    default:
      return t("schedule.describe.custom", { cron });
  }
}
