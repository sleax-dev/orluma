import {
  Alert20Filled,
  Alert20Regular,
  ArrowCounterclockwise20Filled,
  ArrowCounterclockwise20Regular,
  ShieldCheckmark20Filled,
  ShieldCheckmark20Regular,
  Database20Filled,
  Database20Regular,
  ArrowSwap20Filled,
  ArrowSwap20Regular,
  type FluentIcon,
  Home20Filled,
  Home20Regular,
  Pulse20Filled,
  Pulse20Regular,
  ServerMultiple20Filled,
  ServerMultiple20Regular,
  Settings20Filled,
  Settings20Regular,
} from "@fluentui/react-icons";

export type Page = "overview" | "servers" | "jobs" | "storage" | "restore" | "transfer" | "activity" | "alerts" | "settings" | "backup";

export const ICONS: Record<Page, [FluentIcon, FluentIcon]> = {
  overview: [Home20Regular, Home20Filled],
  servers: [ServerMultiple20Regular, ServerMultiple20Filled],
  jobs: [ShieldCheckmark20Regular, ShieldCheckmark20Filled],
  storage: [Database20Regular, Database20Filled],
  restore: [ArrowCounterclockwise20Regular, ArrowCounterclockwise20Filled],
  transfer: [ArrowSwap20Regular, ArrowSwap20Filled],
  activity: [Pulse20Regular, Pulse20Filled],
  alerts: [Alert20Regular, Alert20Filled],
  settings: [Settings20Regular, Settings20Filled],
  backup: [ShieldCheckmark20Regular, ShieldCheckmark20Filled],
};

export const NAV_GROUPS: { label?: string; pages: Page[] }[] = [
  { pages: ["overview", "servers"] },
  { label: "backup", pages: ["jobs", "storage", "restore", "transfer"] },
  { label: "watch", pages: ["activity", "alerts"] },
];
