import { Button, Dialog, DialogActions, DialogBody, DialogContent, DialogSurface, DialogTitle, Dropdown, Field, Input, Option, Spinner, Textarea } from "@fluentui/react-components";
import { CheckmarkCircle16Filled, Copy16Regular, DismissCircle16Filled, Eye16Regular, EyeOff16Regular, Key16Regular } from "@fluentui/react-icons";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api, errText } from "../lib/api";
import { DEST_TYPES, newDestKind, S3_PROVIDERS } from "../lib/defaults";
import type { Destination, DestKind, TestResult } from "../lib/types";
import { useApp } from "../state";
import { TextField } from "./ui";

export function DestinationEditor({ open, server, initial, onClose, onSave }: { open: boolean; server: string; initial: Destination; onClose: () => void; onSave: (d: Destination) => Promise<void> }) {
  const { t } = useTranslation();
  const { live, servers, toast } = useApp();
  const [d, setD] = useState<Destination>(initial);
  const [show, setShow] = useState(false);
  const [test, setTest] = useState<TestResult | null>(null);
  const [testing, setTesting] = useState(false);
  const [authTarget, setAuthTarget] = useState("");
  const isNew = !initial.name || initial.name === t(`dest.type.${initial.kind.type}`);
  const k = d.kind;
  const setK = (p: Partial<DestKind>) => setD({ ...d, kind: { ...k, ...p } as DestKind });
  const agentKey = live[server]?.status?.ssh_public_key ?? "";

  const runTest = async () => {
    setTesting(true);
    setTest(null);
    try {
      setTest(await api.testDestination(server, d));
    } catch (e) {
      setTest({ ok: false, message: errText(e) });
    } finally {
      setTesting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(_, x) => !x.open && onClose()}>
      <DialogSurface className="dialog-xl">
        <DialogBody>
          <DialogTitle>{isNew ? t("dest.newTitle") : t("dest.editTitle", { name: initial.name })}</DialogTitle>
          <DialogContent>
            <div className="stack dialog-scroll">
              <div className="type-grid">
                {DEST_TYPES.map((x) => (
                  <button key={x} className={`type-btn${k.type === x ? " on" : ""}`} onClick={() => { setD({ ...d, kind: newDestKind(x), name: d.name === t(`dest.type.${k.type}`) || !d.name ? t(`dest.type.${x}`) : d.name }); setTest(null); }}>
                    <b>{t(`dest.type.${x}`)}</b>
                    <small>{t(`dest.about.${x}`)}</small>
                  </button>
                ))}
              </div>
              <div className="form">
                <TextField full label={t("dest.name")} value={d.name} onChange={(v) => setD({ ...d, name: v })} />
                {k.type === "local" && <TextField full label={t("dest.path")} hint={t("dest.localHint")} value={k.path} onChange={(v) => setK({ path: v })} />}
                {k.type === "sftp" && (
                  <>
                    <TextField label={t("dest.host")} value={k.host} onChange={(v) => setK({ host: v })} placeholder="backup.example.net" />
                    <TextField label={t("dest.port")} value={k.port} onChange={(v) => setK({ port: Number(v.replace(/\D/g, "")) || 22 })} />
                    <TextField label={t("dest.user")} value={k.user} onChange={(v) => setK({ user: v })} />
                    <TextField label={t("dest.path")} value={k.path} onChange={(v) => setK({ path: v })} />
                    <div className="full stack" style={{ gap: 6 }}>
                      <span className="small muted">{t("dest.sftpKey")}</span>
                      <div className="fingerprint">{agentKey || "…"}</div>
                      <div className="gap-s">
                        <Button size="small" icon={<Copy16Regular />} disabled={!agentKey} onClick={() => void api.copy(agentKey).then(() => toast("success", t("common.copied")))}>
                          {t("common.copy")}
                        </Button>
                        {servers.filter((s) => s.id !== server).length > 0 && (
                          <>
                            <Dropdown placeholder={t("dest.authorizeOn")} value={servers.find((s) => s.id === authTarget)?.name ?? ""} selectedOptions={[authTarget]} onOptionSelect={(_, o) => setAuthTarget(o.optionValue ?? "")} size="small">
                              {servers.filter((s) => s.id !== server).map((s) => <Option key={s.id} value={s.id}>{s.name}</Option>)}
                            </Dropdown>
                            <Button
                              size="small"
                              icon={<Key16Regular />}
                              disabled={!authTarget || !agentKey}
                              onClick={() =>
                                void api
                                  .authorizeOn(authTarget, agentKey)
                                  .then(() => {
                                    const target = servers.find((s) => s.id === authTarget);
                                    if (target) setK({ host: target.host, port: target.port, user: target.user });
                                    toast("success", t("dest.authorized", { server: target?.name }));
                                  })
                                  .catch((e) => toast("error", errText(e)))
                              }
                            >
                              {t("dest.authorize")}
                            </Button>
                          </>
                        )}
                      </div>
                    </div>
                  </>
                )}
                {k.type === "s3" && (
                  <>
                    <Field label={t("dest.provider")} className="full">
                      <Dropdown
                        value={S3_PROVIDERS.find((p) => p.id === k.provider)?.name ?? k.provider}
                        selectedOptions={[k.provider]}
                        onOptionSelect={(_, o) => {
                          const p = S3_PROVIDERS.find((x) => x.id === o.optionValue);
                          if (p) setK({ provider: p.id, endpoint: p.endpoint, region: p.region });
                        }}
                      >
                        {S3_PROVIDERS.map((p) => <Option key={p.id} value={p.id}>{p.name}</Option>)}
                      </Dropdown>
                    </Field>
                    <TextField full label={t("dest.endpoint")} value={k.endpoint} onChange={(v) => setK({ endpoint: v })} />
                    <TextField label={t("dest.bucket")} value={k.bucket} onChange={(v) => setK({ bucket: v })} />
                    <TextField label={t("dest.prefix")} value={k.path} onChange={(v) => setK({ path: v })} placeholder={live[server]?.status?.hostname ?? "server"} />
                    <TextField label={t("dest.region")} value={k.region} onChange={(v) => setK({ region: v })} />
                    <span />
                    <TextField label={t("dest.accessKey")} value={k.access_key} onChange={(v) => setK({ access_key: v })} />
                    <TextField label={t("dest.secretKey")} type="password" value={k.secret_key} onChange={(v) => setK({ secret_key: v })} />
                    <p className="hint small full">{t("dest.s3Hint")}</p>
                  </>
                )}
                {k.type === "b2" && (
                  <>
                    <TextField label={t("dest.bucket")} value={k.bucket} onChange={(v) => setK({ bucket: v })} />
                    <TextField label={t("dest.prefix")} value={k.path} onChange={(v) => setK({ path: v })} />
                    <TextField label={t("dest.keyId")} value={k.account_id} onChange={(v) => setK({ account_id: v })} />
                    <TextField label={t("dest.appKey")} type="password" value={k.account_key} onChange={(v) => setK({ account_key: v })} />
                  </>
                )}
                {k.type === "azure" && (
                  <>
                    <TextField label={t("dest.account")} value={k.account_name} onChange={(v) => setK({ account_name: v })} />
                    <TextField label={t("dest.accountKey")} type="password" value={k.account_key} onChange={(v) => setK({ account_key: v })} />
                    <TextField label={t("dest.container")} value={k.container} onChange={(v) => setK({ container: v })} />
                    <TextField label={t("dest.prefix")} value={k.path} onChange={(v) => setK({ path: v })} />
                  </>
                )}
                {k.type === "gcs" && (
                  <>
                    <TextField label={t("dest.project")} value={k.project_id} onChange={(v) => setK({ project_id: v })} />
                    <TextField label={t("dest.bucket")} value={k.bucket} onChange={(v) => setK({ bucket: v })} />
                    <TextField label={t("dest.prefix")} value={k.path} onChange={(v) => setK({ path: v })} />
                    <Field label={t("dest.credentials")} className="full">
                      <Textarea resize="vertical" value={k.credentials_json} onChange={(_, x) => setK({ credentials_json: x.value })} placeholder='{ "type": "service_account", … }' />
                    </Field>
                  </>
                )}
                {k.type === "rest" && (
                  <>
                    <TextField full label={t("dest.url")} value={k.url} onChange={(v) => setK({ url: v })} placeholder="https://backup.example.net:8000/web-01" />
                    <TextField label={t("dest.user")} value={k.username} onChange={(v) => setK({ username: v })} />
                    <TextField label={t("dest.password")} type="password" value={k.password} onChange={(v) => setK({ password: v })} />
                  </>
                )}
                {k.type === "rclone" && (
                  <>
                    <TextField label={t("dest.remote")} value={k.remote} onChange={(v) => setK({ remote: v })} placeholder="gdrive" />
                    <TextField label={t("dest.path")} value={k.path} onChange={(v) => setK({ path: v })} />
                    <p className="hint small full">{t("dest.rcloneHint")}</p>
                  </>
                )}

                <div className="form-sect">{t("dest.encryption")}</div>
                <Field label={t("dest.repoPassword")} hint={t("dest.repoPasswordHint")} className="full">
                  <Input
                    className="mono"
                    type={show ? "text" : "password"}
                    value={d.password}
                    onChange={(_, x) => setD({ ...d, password: x.value })}
                    contentAfter={
                      <span className="gap-s">
                        <Button size="small" appearance="transparent" icon={show ? <EyeOff16Regular /> : <Eye16Regular />} aria-label={t("dest.show")} onClick={() => setShow(!show)} />
                        <Button size="small" appearance="transparent" icon={<Copy16Regular />} aria-label={t("common.copy")} onClick={() => void api.copy(d.password).then(() => toast("success", t("common.copied")))} />
                      </span>
                    }
                  />
                </Field>
                <TextField label={t("dest.limitUp")} hint={t("dest.limitHint")} value={d.limit_upload_kib} onChange={(v) => setD({ ...d, limit_upload_kib: Number(v.replace(/\D/g, "")) || 0 })} contentAfter="KiB/s" />
                <TextField label={t("dest.limitDown")} value={d.limit_download_kib} onChange={(v) => setD({ ...d, limit_download_kib: Number(v.replace(/\D/g, "")) || 0 })} contentAfter="KiB/s" />
              </div>
              {test && (
                <p className={test.ok ? "ok-line" : "bad-text selectable"} style={{ display: "flex", gap: 8, margin: 0 }}>
                  {test.ok ? <CheckmarkCircle16Filled /> : <DismissCircle16Filled style={{ flex: "none", marginTop: 2 }} />} {test.message}
                </p>
              )}
            </div>
          </DialogContent>
          <DialogActions>
            <Button onClick={onClose}>{t("common.cancel")}</Button>
            <Button disabled={testing} onClick={() => void runTest()}>
              {testing ? <Spinner size="tiny" /> : t("dest.test")}
            </Button>
            <Button appearance="primary" disabled={!d.name.trim() || d.password.length < 8} onClick={() => void onSave({ ...d, name: d.name.trim() })}>
              {t("common.save")}
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  );
}
