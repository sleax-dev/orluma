import {
  AlertUrgent20Filled,
  AlertUrgent20Regular,
  ArrowCounterclockwise20Filled,
  ArrowCounterclockwise20Regular,
  ArrowSync20Filled,
  ArrowSync20Regular,
  Database20Filled,
  Database20Regular,
  type FluentIcon,
  Home20Filled,
  Home20Regular,
  Pulse20Filled,
  Pulse20Regular,
  Server20Filled,
  Server20Regular,
  Settings20Filled,
  Settings20Regular,
} from "@fluentui/react-icons";

export type Page = "overview" | "servers" | "jobs" | "storage" | "restore" | "activity" | "alerts" | "settings";

export const ICONS: Record<Page, [FluentIcon, FluentIcon]> = {
  overview: [Home20Regular, Home20Filled],
  servers: [Server20Regular, Server20Filled],
  jobs: [ArrowSync20Regular, ArrowSync20Filled],
  storage: [Database20Regular, Database20Filled],
  restore: [ArrowCounterclockwise20Regular, ArrowCounterclockwise20Filled],
  activity: [Pulse20Regular, Pulse20Filled],
  alerts: [AlertUrgent20Regular, AlertUrgent20Filled],
  settings: [Settings20Regular, Settings20Filled],
};

export const NAV_GROUPS: { label?: string; pages: Page[] }[] = [
  { pages: ["overview", "servers"] },
  { label: "backup", pages: ["jobs", "storage", "restore"] },
  { label: "watch", pages: ["activity", "alerts"] },
];
