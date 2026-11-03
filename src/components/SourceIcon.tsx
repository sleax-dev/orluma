import { Folder20Regular, WindowConsole20Regular } from "@fluentui/react-icons";
import type { Job, Source } from "../lib/types";

// Fluent has no database logos, so each engine gets its own tinted monogram: told apart at a glance, one style.
const MARKS: Record<string, [string, string]> = {
  postgres: ["Pg", "#5b9bd5"],
  mysql: ["My", "#f29111"],
  mongo: ["Mo", "#4fb860"],
  redis: ["Re", "#e5524a"],
  sqlite: ["Sq", "#4cb3e6"],
  clickhouse: ["CH", "#ffcc33"],
};

export function SourceIcon({ type }: { type: Source["type"] }) {
  if (type === "paths") return <Folder20Regular />;
  if (type === "command") return <WindowConsole20Regular />;
  const [text, color] = MARKS[type] ?? ["DB", "#9aa0aa"];
  return (
    <span className="mark" style={{ color, background: `color-mix(in srgb, ${color} 16%, transparent)` }}>
      {text}
    </span>
  );
}

/** A job takes its icon from its first source; a mix of kinds shows the stack. */
export function JobIcon({ job }: { job: Job }) {
  const first = job.sources[0]?.type;
  return first ? <SourceIcon type={first} /> : <Folder20Regular />;
}
