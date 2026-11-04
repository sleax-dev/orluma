import { Button, Checkbox, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Dropdown, Field, Option, Spinner } from "@fluentui/react-components";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../lib/api";
import { sourceFromFound } from "../lib/defaults";
import type { DbConn, Snapshot, Source } from "../lib/types";
import { useApp } from "../state";
import { TextField, useAction } from "./ui";

/** Dump kinds that can be loaded straight into a running server. Redis has no live import; globals.sql is roles. */
export function dbRestorable(kind: string, s: Snapshot): boolean {
  const file = s.paths[0] ?? "";
  if (kind === "postgres") return file.endsWith(".dump");
  if (kind === "clickhouse") return file.endsWith("/schema.sql");
  return ["mysql", "sqlite", "mongo"].includes(kind);
}

const stem = (p: string) => (p.split("/").pop() ?? "").replace(/\.(dump|sql|archive)$/, "").replace(/\.sqlite3?$|\.db$/, "");

interface Target {
  key: string;
  label: string;
  conn: DbConn;
  uri: string;
}

const NO_CONN: DbConn = { host: "", port: 0, user: "", password: "", container: "", os_user: "" };

/** Where a source's database lives, as a restore target. */
function targetOf(s: Source, label: string): Target | null {
  if (s.type === "mongo") return { key: `c:${s.container}|${s.uri ? "auth" : ""}`, label, conn: { ...NO_CONN, container: s.container }, uri: s.uri };
  if (!("container" in s)) return null;
  const conn = { host: s.host, port: s.port, user: s.user, password: s.password, container: s.container, os_user: s.os_user };
  return { key: `c:${s.container}|${s.host}:${s.port}`, label, conn, uri: "" };
}

export function RestoreDb({ server, destination, snapshot, kind, source, onClose }: { server: string; destination: string; snapshot: Snapshot; kind: string; source?: Source; onClose: () => void }) {
  const { t } = useTranslation();
  const file = snapshot.paths[0] ?? "";
  const original = kind === "clickhouse" ? (file.split("/").slice(-2)[0] ?? "") : stem(file);
  const sqlitePath = source?.type === "sqlite" ? source.path : "";
  const [name, setName] = useState(
    kind === "sqlite"
      ? sqlitePath.replace(/(\.[A-Za-z0-9]+)?$/, (e) => `.restored${e ?? ""}`) || `/var/tmp/${original}.restored.sqlite`
      : `${original}_restored`,
  );
  const [replace, setReplace] = useState(false);
  const { toast, setPage } = useApp();
  const { busy, run } = useAction();
  const named = kind !== "mongo";
  // Where to load it: the database it was backed up from, or any of the same kind on this server (a backup
  // moved here from another server has no source on this one). Found by the agent, logins included.
  const origin = source ? targetOf(source, "") : null;
  const [targets, setTargets] = useState<Target[] | null>(kind === "sqlite" ? [] : null);
  const [pick, setPick] = useState("");
  useEffect(() => {
    if (kind === "sqlite") return;
    let alive = true;
    api
      .discover(server)
      .catch(() => ({ databases: [] }))
      .then((d) => {
        if (!alive) return;
        const found = d.databases
          .filter((x) => x.kind === kind)
          .map((x) => targetOf(sourceFromFound(x), x.label))
          .filter((x): x is Target => !!x);
        const list = [...(origin ? [{ ...origin, label: t("restore.db.targetOriginal") }] : []), ...found.filter((f) => f.key !== origin?.key)];
        setTargets(list);
        setPick(list[0]?.key ?? "");
      });
    return () => {
      alive = false;
    };
    // once per dialog
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [server, kind]);
  const target = targets?.find((x) => x.key === pick);
  const where = target ? target.label : t(`source.type.${kind}`);

  const start = async () => {
    const ok = await run("rdb", () =>
      api.restoreDb(server, {
        destination,
        snapshot: snapshot.id,
        kind,
        file,
        conn: target?.conn ?? NO_CONN,
        uri: target?.uri ?? "",
        database: named ? name.trim() : "",
        replace,
      }),
    );
    if (ok) {
      toast("success", t("restore.db.started"));
      onClose();
      setPage("activity", { server });
    }
  };

  return (
    <Dialog open onOpenChange={(_, d) => !d.open && onClose()}>
      <DialogSurface className="dialog-wide">
        <DialogBody>
          <DialogTitle>{t("restore.db.title", { name: original || t(`source.type.${kind}`) })}</DialogTitle>
          <DialogContent>
            <div className="stack">
              <p className="muted" style={{ margin: 0 }}>{t("restore.db.text", { where: t(`source.type.${kind}`) })}</p>
              {kind !== "sqlite" &&
                (targets === null ? (
                  <Spinner size="tiny" label={t("restore.db.looking")} labelPosition="after" />
                ) : targets.length === 0 ? (
                  <p className="small bad-text" style={{ margin: 0 }}>{t("restore.db.noTarget", { kind: t(`source.type.${kind}`) })}</p>
                ) : (
                  <Field label={t("restore.db.target")}>
                    <Dropdown value={where} selectedOptions={[pick]} onOptionSelect={(_, d) => setPick(d.optionValue ?? "")}>
                      {targets.map((x) => (
                        <Option key={x.key} value={x.key}>{x.label}</Option>
                      ))}
                    </Dropdown>
                  </Field>
                ))}
              {named && (
                <TextField
                  label={kind === "sqlite" ? t("restore.db.file") : t("restore.db.name")}
                  hint={kind === "clickhouse" ? t("restore.db.chNote") : t("restore.db.nameHint")}
                  value={name}
                  onChange={setName}
                  className="mono"
                />
              )}
              {kind === "mongo" && <p className="small muted" style={{ margin: 0 }}>{t("restore.db.mongoNote")}</p>}
              {kind !== "clickhouse" && (
                <Checkbox checked={replace} onChange={(_, d) => setReplace(!!d.checked)} label={kind === "sqlite" ? t("restore.db.replaceFile") : t("restore.db.replace")} />
              )}
              {replace && <p className="small" style={{ margin: 0, color: "var(--warn)" }}>{kind === "sqlite" ? t("restore.db.replaceFileWarn") : t("restore.db.replaceWarn")}</p>}
            </div>
          </DialogContent>
          <DialogActions>
            <Button onClick={onClose}>{t("common.cancel")}</Button>
            <Button appearance="primary" disabled={busy === "rdb" || (named && !name.trim()) || (kind !== "sqlite" && !target)} onClick={() => void start()}>
              {t("restore.db.start")}
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
