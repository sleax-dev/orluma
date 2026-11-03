import { Button, Checkbox, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle } from "@fluentui/react-components";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "../lib/api";
import type { Snapshot, Source } from "../lib/types";
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
  const conn = source && "container" in source ? source : undefined;
  const where = conn?.container ? `${t(`source.type.${kind}`)} · ${conn.container}` : conn && "host" in conn && conn.host ? `${t(`source.type.${kind}`)} · ${conn.host}` : t(`source.type.${kind}`);
  const named = kind !== "mongo";

  const start = async () => {
    const ok = await run("rdb", () =>
      api.restoreDb(server, {
        destination,
        snapshot: snapshot.id,
        kind,
        file,
        conn: { host: "", port: 0, user: "", password: "", container: "", os_user: "", ...(source && source.type !== "paths" && source.type !== "sqlite" && source.type !== "mongo" && source.type !== "command" ? source : {}) },
        uri: source?.type === "mongo" ? source.uri : "",
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
              <p className="muted" style={{ margin: 0 }}>{t("restore.db.text", { where })}</p>
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
            <Button appearance="primary" disabled={busy === "rdb" || (named && !name.trim())} onClick={() => void start()}>
              {t("restore.db.start")}
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
