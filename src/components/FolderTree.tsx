import { Checkbox, Spinner } from "@fluentui/react-components";
import { ChevronDown16Regular, ChevronRight16Regular } from "@fluentui/react-icons";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../lib/api";
import { isSkipped } from "../lib/defaults";
import { bytes } from "../lib/format";
import type { FsNode } from "../lib/types";

export interface Root {
  path: string;
  hint: string;
  bytes: number;
  pending?: boolean;
}

export type Sizes = Record<string, { bytes: number; complete: boolean }>;

interface Item {
  path: string;
  name: string;
  dir: boolean;
  hint?: string;
  size?: number;
  pending?: boolean;
}

/** The server's folders as a tree: open a folder to see what is inside and tick any level. Nothing is ticked for
 *  the user. Ticking a folder takes everything in it, so what is inside it is shown ticked and locked. */
export function FolderTree({
  server,
  roots,
  selected,
  onChange,
  sizes,
  measure,
}: {
  server: string;
  roots: Root[];
  selected: Set<string>;
  onChange: (s: Set<string>) => void;
  sizes: Sizes;
  measure: (path: string, first?: boolean) => void;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [kids, setKids] = useState<Record<string, FsNode[] | "loading" | "error">>({});

  const load = async (path: string) => {
    setKids((k) => ({ ...k, [path]: "loading" }));
    try {
      const list = await api.listDir(server, path);
      setKids((k) => ({ ...k, [path]: list }));
      list.filter((n) => n.kind === "dir" && !isSkipped(n.path, n.name)).forEach((n) => measure(n.path, true));
    } catch {
      setKids((k) => ({ ...k, [path]: "error" }));
    }
  };

  const toggle = (path: string) => {
    const next = new Set(open);
    if (next.has(path)) next.delete(path);
    else {
      next.add(path);
      if (!kids[path] || kids[path] === "error") void load(path);
    }
    setOpen(next);
  };

  const all = [...selected];
  const covered = (p: string) => all.some((s) => p.startsWith(`${s}/`));
  const inside = (p: string) => all.some((s) => s.startsWith(`${p}/`));
  const pick = (p: string, on: boolean) => {
    const next = new Set(selected);
    if (on) {
      for (const s of selected) if (s.startsWith(`${p}/`)) next.delete(s);
      next.add(p);
    } else next.delete(p);
    onChange(next);
  };

  const size = (i: Item) => {
    if (i.dir && !i.hint && isSkipped(i.path, i.name)) return "";
    if (!i.dir) return i.size === undefined ? "" : bytes(i.size);
    const s = sizes[i.path];
    if (i.hint && !i.pending) return bytes(i.size ?? 0);
    return !s ? <Spinner size="extra-tiny" /> : s.complete ? bytes(s.bytes) : t("first.tooBig");
  };

  const rows = (items: Item[], depth: number): React.ReactNode =>
    items.map((i) => {
      const skipped = !i.hint && isSkipped(i.path, i.name);
      const locked = covered(i.path);
      const isOpen = open.has(i.path);
      const k = kids[i.path];
      return (
        <div key={i.path}>
          <div className={`check-row tree-row${skipped ? " skipped" : ""}`} style={{ paddingInlineStart: 8 + depth * 20 }}>
            {i.dir ? (
              <button type="button" className="tree-chev" aria-expanded={isOpen} aria-label={t("first.toggle")} onClick={() => toggle(i.path)}>
                {isOpen ? <ChevronDown16Regular /> : <ChevronRight16Regular />}
              </button>
            ) : (
              <span className="tree-chev" />
            )}
            <Checkbox
              checked={locked || selected.has(i.path) ? true : inside(i.path) ? "mixed" : false}
              disabled={skipped || locked}
              title={locked ? t("first.insideTicked") : undefined}
              onChange={(_, d) => pick(i.path, !!d.checked)}
              label={
                <>
                  <span className="mono">{i.hint ? i.path : i.name}</span>
                  {i.hint && <span className="tag">{t(`first.hint.${i.hint}`)}</span>}
                  {skipped && <span className="tag">{t("first.skipped")}</span>}
                </>
              }
            />
            <span className="size">{size(i)}</span>
          </div>
          {i.dir && isOpen && (
            k === "loading" || !k ? (
              <div className="tree-note" style={{ paddingInlineStart: 36 + depth * 20 }}><Spinner size="extra-tiny" /></div>
            ) : k === "error" ? (
              <div className="tree-note bad-text" style={{ paddingInlineStart: 36 + depth * 20 }}>{t("first.loadFailed")}</div>
            ) : k.length === 0 ? (
              <div className="tree-note" style={{ paddingInlineStart: 36 + depth * 20 }}>{t("first.emptyFolder")}</div>
            ) : (
              rows(k.map((n) => ({ path: n.path, name: n.name, dir: n.kind === "dir", size: n.size })), depth + 1)
            )
          )}
        </div>
      );
    });

  return <div className="check-list tree">{rows(roots.map((r) => ({ path: r.path, name: r.path, dir: true, hint: r.hint, size: r.bytes, pending: r.pending })), 0)}</div>;
}
