//! Orluma's desktop app: the servers' control room. It talks to each server's agent over SSH, keeps a
//! repository on this PC that servers copy into, shows alerts as desktop notifications, and updates itself
//! from signed releases.

mod applog;
mod install;
mod lan;
mod pcrepo;
mod ssh;
mod store;

use chrono::Utc;
use futures_util::StreamExt;
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ssh::{Auth, Conn};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use store::{PcSync, Server, Settings, Store};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WindowEvent};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;

type Res<T> = Result<T, String>;

pub struct Inner {
    store: Store,
    settings: Mutex<Settings>,
    servers: Mutex<Vec<Server>>,
    conns: tokio::sync::Mutex<HashMap<String, Arc<Conn>>>,
    listening: Mutex<std::collections::HashSet<String>>,
    pc: tokio::sync::Mutex<Option<pcrepo::Server>>,
    lan: tokio::sync::Mutex<Option<lan::Share>>,
    http: reqwest::Client,
    update: Mutex<UpdateStatus>,
    pending_update: Mutex<Option<tauri_plugin_updater::Update>>,
    tray: Mutex<Option<TrayIcon>>,
}

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Inner {
    fn server(&self, id: &str) -> Res<Server> {
        lock(&self.servers)
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or_else(|| format!("unknown server {id}"))
    }

    /// The live connection to a server, opened (and its host key checked) on demand.
    async fn conn(&self, id: &str) -> Res<Arc<Conn>> {
        let mut conns = self.conns.lock().await;
        if let Some(c) = conns.get(id)
            && !c.is_closed()
        {
            return Ok(c.clone());
        }
        let s = self.server(id)?;
        let key = Arc::new(self.store.ssh_key().map_err(|e| e.to_string())?);
        let t = std::time::Instant::now();
        let c = ssh::connect(&s.host, s.port, &s.user, Auth::Key(key), Some(s.host_key.clone()))
            .await
            .map_err(|e| {
                applog::error(
                    "ssh",
                    format!(
                        "{} ({}:{}) connect failed after {}ms: {}",
                        s.name,
                        s.host,
                        s.port,
                        t.elapsed().as_millis(),
                        e.0
                    ),
                );
                e.0
            })?;
        applog::info(
            "ssh",
            format!(
                "{} ({}:{}) connected in {}ms",
                s.name,
                s.host,
                s.port,
                t.elapsed().as_millis()
            ),
        );
        let c = Arc::new(c);
        conns.insert(id.into(), c.clone());
        Ok(c)
    }

    async fn drop_conn(&self, id: &str) {
        if let Some(c) = self.conns.lock().await.remove(id) {
            c.close().await;
        }
    }

    /// POST to the agent; on a dropped connection, one retry on a fresh one.
    async fn call(&self, id: &str, path: &str, body: &Value) -> Res<Value> {
        let t = std::time::Instant::now();
        let r = self.call_once(id, path, body).await;
        let ms = t.elapsed().as_millis();
        let routine = matches!(path, "status" | "logs");
        match &r {
            Err(e) => applog::error("agent", format!("{id} {path} failed after {ms}ms: {e}")),
            Ok(_) if ms >= 3000 => applog::warn("agent", format!("{id} {path} was slow: {ms}ms")),
            Ok(_) if !routine => applog::info("agent", format!("{id} {path} {ms}ms")),
            _ => {}
        }
        r
    }

    async fn call_once(&self, id: &str, path: &str, body: &Value) -> Res<Value> {
        let bytes = serde_json::to_vec(body).map_err(|e| e.to_string())?;
        let mut last = String::new();
        for attempt in 0..2 {
            let c = self.conn(id).await?;
            match c.agent("POST", &format!("/v1/{path}"), Some(bytes.clone())).await {
                Ok((status, body)) => {
                    let v: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                    if (200..300).contains(&status) {
                        return Ok(v);
                    }
                    return Err(v
                        .get("error")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| format!("agent answered HTTP {status}")));
                }
                Err(e) => {
                    last = e.0;
                    if attempt == 0 && !last.starts_with("AGENT_UNREACHABLE") {
                        self.drop_conn(id).await;
                    } else {
                        break;
                    }
                }
            }
        }
        Err(last)
    }

    async fn pc_server(&self) -> Res<(std::net::SocketAddr, String, String)> {
        let mut pc = self.pc.lock().await;
        if pc.is_none() {
            let root = self.store.pc_root(&lock(&self.settings));
            *pc = Some(pcrepo::start(root).await.map_err(|e| e.to_string())?);
        }
        let s = pc.as_ref().ok_or("PC repository failed to start")?;
        Ok((s.addr, s.user.clone(), s.password.clone()))
    }
}

// ---------------------------------------------------------------- settings & app

#[derive(Serialize)]
struct AppInfo {
    version: String,
    public_key: String,
    data_dir: String,
    pc_root: String,
    os: String,
}

#[tauri::command]
fn get_settings(s: State<'_, AppState>) -> Settings {
    lock(&s.0.settings).clone()
}

#[tauri::command]
fn set_settings(app: AppHandle, s: State<'_, AppState>, settings: Settings) -> Res<()> {
    let old = lock(&s.0.settings).clone();
    if old.start_with_os != settings.start_with_os {
        use tauri_plugin_autostart::ManagerExt;
        let a = app.autolaunch();
        let _ = if settings.start_with_os {
            a.enable()
        } else {
            a.disable()
        };
    }
    if old.pc_storage_dir != settings.pc_storage_dir {
        // The PC server serves the old folder until restarted; the next copy starts it on the new one.
        if let Ok(mut pc) = s.0.pc.try_lock() {
            *pc = None;
        }
    }
    if old.language != settings.language {
        refresh_tray_menu(&app, settings.language.as_deref().unwrap_or("en"));
    }
    s.0.store.save_settings(&settings).map_err(|e| e.to_string())?;
    *lock(&s.0.settings) = settings;
    Ok(())
}

#[tauri::command]
fn app_info(app: AppHandle, s: State<'_, AppState>) -> Res<AppInfo> {
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        public_key: s.0.store.public_key().map_err(|e| e.to_string())?,
        data_dir: s.0.store.dir.display().to_string(),
        pc_root: s.0.store.pc_root(&lock(&s.0.settings)).display().to_string(),
        os: std::env::consts::OS.into(),
    })
}

#[tauri::command]
fn minimize_window(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.minimize();
    }
}

#[tauri::command]
fn close_window(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.close();
    }
}

// ---------------------------------------------------------------- servers

#[tauri::command]
fn list_servers(s: State<'_, AppState>) -> Vec<Server> {
    lock(&s.0.servers).clone()
}

#[tauri::command]
async fn probe_server(host: String, port: u16) -> Res<String> {
    ssh::probe(host.trim(), port).await.map_err(|e| e.0)
}

#[derive(Deserialize)]
pub struct NewServer {
    name: String,
    host: String,
    port: u16,
    user: String,
    /// Used once to authorize the app's key; never stored. Empty when the key is already authorized.
    password: String,
    fingerprint: String,
}

/// Adds a server: log in (password once, or the app's key), authorize the app's key, install the agent.
#[tauri::command]
async fn add_server(app: AppHandle, s: State<'_, AppState>, server: NewServer) -> Res<Server> {
    let st = s.0.clone();
    let host = server.host.trim().to_string();
    let key = Arc::new(st.store.ssh_key().map_err(|e| e.to_string())?);
    let fp = Some(server.fingerprint.clone()).filter(|f| !f.is_empty());
    let conn = if server.password.is_empty() {
        ssh::connect(&host, server.port, &server.user, Auth::Key(key.clone()), fp.clone())
            .await
            .map_err(|e| e.0)?
    } else {
        let c = ssh::connect(
            &host,
            server.port,
            &server.user,
            Auth::Password(server.password.clone()),
            fp.clone(),
        )
        .await
        .map_err(|e| e.0)?;
        install::authorize_key(&c, &st.store.public_key().map_err(|e| e.to_string())?)
            .await
            .map_err(|e| e.0)?;
        c.close().await;
        // From here on the key is what logs in.
        ssh::connect(&host, server.port, &server.user, Auth::Key(key), fp)
            .await
            .map_err(|e| format!("the app's key was added but does not log in: {}", e.0))?
    };
    let _ = app.emit("setup-step", "agent");
    install::install_agent(&conn, &st.http).await.map_err(|e| e.0)?;
    let saved = Server {
        id: orluma_core::new_id("s"),
        name: if server.name.trim().is_empty() {
            host.clone()
        } else {
            server.name.trim().into()
        },
        host,
        port: server.port,
        user: server.user,
        host_key: conn.fingerprint.clone(),
        added: Some(Utc::now()),
    };
    st.conns.lock().await.insert(saved.id.clone(), Arc::new(conn));
    {
        let mut list = lock(&st.servers);
        list.push(saved.clone());
        st.store.save_servers(&list).map_err(|e| e.to_string())?;
    }
    listen(app, st, saved.id.clone());
    Ok(saved)
}

#[tauri::command]
async fn update_server(s: State<'_, AppState>, server: Server) -> Res<()> {
    let mut list = lock(&s.0.servers);
    let Some(x) = list.iter_mut().find(|x| x.id == server.id) else {
        return Err("unknown server".into());
    };
    x.name = server.name;
    s.0.store.save_servers(&list).map_err(|e| e.to_string())
}

#[tauri::command]
async fn reinstall_agent(s: State<'_, AppState>, id: String) -> Res<()> {
    let c = s.0.conn(&id).await?;
    install::install_agent(&c, &s.0.http).await.map_err(|e| e.0)?;
    Ok(())
}

#[tauri::command]
async fn remove_server(s: State<'_, AppState>, id: String, uninstall: bool, purge: bool) -> Res<()> {
    if uninstall {
        let c = s.0.conn(&id).await?;
        install::uninstall_agent(&c, purge).await.map_err(|e| e.0)?;
    }
    s.0.drop_conn(&id).await;
    lock(&s.0.listening).remove(&id);
    let mut list = lock(&s.0.servers);
    list.retain(|x| x.id != id);
    s.0.store.save_servers(&list).map_err(|e| e.to_string())?;
    let mut st = lock(&s.0.settings);
    st.pc_syncs.retain(|p| p.server_id != id);
    s.0.store.save_settings(&st).map_err(|e| e.to_string())
}

/// Lets one server's agent log in to another for an SFTP destination: adds its public key to the target's
/// ~/.ssh/authorized_keys, over the connection Orluma already has to the target.
#[tauri::command]
async fn authorize_on_server(s: State<'_, AppState>, server: String, public_key: String) -> Res<()> {
    if !public_key.starts_with("ssh-") {
        return Err("not an SSH public key".into());
    }
    let c = s.0.conn(&server).await?;
    install::authorize_key(&c, &public_key).await.map_err(|e| e.0)
}

/// The interface's own log lines (caught errors, failed actions) go to the same file as the Rust side's.
#[tauri::command]
fn log_event(level: String, area: String, msg: String) {
    let level = match level.as_str() {
        "error" => "ERROR",
        "warn" => "WARN",
        _ => "INFO",
    };
    applog::write(level, &format!("ui:{}", area.chars().take(8).collect::<String>()), &msg);
}

#[tauri::command]
fn log_tail(lines: usize) -> Vec<String> {
    applog::tail(lines.clamp(1, 5000))
}

/// Applies the same masking as the log files to text built by the interface (a diagnostics report).
#[tauri::command]
fn log_redact(text: String) -> String {
    orluma_core::log::redact(&text)
}

/// Saves a diagnostics report in the Downloads folder and shows it there.
#[tauri::command]
fn log_export(app: AppHandle, text: String) -> Res<String> {
    let dir = app
        .path()
        .download_dir()
        .or_else(|_| app.path().app_data_dir())
        .map_err(|e| e.to_string())?;
    let file = dir.join(format!("orluma-diagnostics-{}.txt", Utc::now().format("%Y%m%d-%H%M%S")));
    std::fs::write(&file, orluma_core::log::redact(&text)).map_err(|e| e.to_string())?;
    let _ = tauri_plugin_opener::reveal_item_in_dir(&file);
    Ok(file.display().to_string())
}

/// Connects a Google Drive, OneDrive or Dropbox account to a server: the agent starts rclone's sign-in there, this
/// PC forwards rclone's port over SSH and opens the sign-in page in the browser, and the agent stores the token
/// in its own rclone config. The token never comes to this PC. Returns the rclone remote's name.
#[tauri::command]
async fn cloud_connect(s: State<'_, AppState>, server: String, provider: String) -> Res<String> {
    let body = serde_json::json!({ "provider": provider });
    let start = s.0.call(&server, "cloud/connect", &body).await?;
    let url = start.get("url").and_then(Value::as_str).unwrap_or_default().to_string();
    let port = start.get("port").and_then(Value::as_u64).unwrap_or(53682) as u16;
    // only rclone's own local sign-in page is ever opened from here
    if !url.starts_with(&format!("http://127.0.0.1:{port}/")) {
        let _ = s.0.call(&server, "cloud/cancel", &serde_json::json!({})).await;
        return Err(format!("unexpected sign-in address from the server: {url}"));
    }
    let conn = s.0.conn(&server).await?;
    let forward = match conn.local_forward(port).await {
        Ok(f) => f,
        Err(e) => {
            let _ = s.0.call(&server, "cloud/cancel", &serde_json::json!({})).await;
            return Err(e.0);
        }
    };
    let _ = tauri_plugin_opener::open_url(&url, None::<&str>);
    let done = s.0.call(&server, "cloud/finish", &body).await;
    forward.abort();
    done?
        .get("remote")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "no remote came back".to_string())
}

#[tauri::command]
async fn cloud_cancel(s: State<'_, AppState>, server: String) -> Res<()> {
    s.0.call(&server, "cloud/cancel", &serde_json::json!({}))
        .await
        .map(|_| ())
}

/// Any agent call: `path` is the part after /v1/ (status, config/get, jobs/run, ...).
#[tauri::command]
async fn agent(s: State<'_, AppState>, server: String, path: String, body: Option<Value>) -> Res<Value> {
    s.0.call(&server, &path, &body.unwrap_or_else(|| serde_json::json!({})))
        .await
}

// ---------------------------------------------------------------- events from the agents

#[derive(Clone, Serialize)]
struct ServerEvent {
    server: String,
    event: Value,
}

#[derive(Clone, Serialize)]
struct ServerState {
    server: String,
    online: bool,
    error: Option<String>,
}

/// Follows a server's event stream for as long as it is in the list, reconnecting when it drops.
fn listen(app: AppHandle, st: Arc<Inner>, id: String) {
    if !lock(&st.listening).insert(id.clone()) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut backoff = 5u64;
        while lock(&st.listening).contains(&id) {
            let res = async {
                let c = st.conn(&id).await?;
                let resp = c.agent_raw("GET", "/v1/events", None).await.map_err(|e| e.0)?;
                if !resp.status().is_success() {
                    return Err(format!("events: HTTP {}", resp.status()));
                }
                let _ = app.emit(
                    "server-state",
                    ServerState {
                        server: id.clone(),
                        online: true,
                        error: None,
                    },
                );
                let mut body = resp.into_body().into_data_stream();
                let mut buf = String::new();
                while let Some(chunk) = body.next().await {
                    let chunk = chunk.map_err(|e| e.to_string())?;
                    buf.push_str(&String::from_utf8_lossy(&chunk));
                    while let Some(i) = buf.find("\n\n") {
                        let frame: String = buf.drain(..i + 2).collect();
                        for line in frame.lines() {
                            if let Some(data) = line.strip_prefix("data:")
                                && let Ok(event) = serde_json::from_str::<Value>(data.trim())
                            {
                                on_event(&app, &st, &id, &event);
                                let _ = app.emit(
                                    "agent-event",
                                    ServerEvent {
                                        server: id.clone(),
                                        event,
                                    },
                                );
                            }
                        }
                    }
                }
                Err::<(), String>("event stream closed".into())
            }
            .await;
            if let Err(e) = res {
                let _ = app.emit(
                    "server-state",
                    ServerState {
                        server: id.clone(),
                        online: false,
                        error: Some(e),
                    },
                );
                st.drop_conn(&id).await;
            } else {
                backoff = 5;
            }
            tokio::time::sleep(Duration::from_secs(backoff)).await;
            backoff = (backoff * 2).min(120);
        }
    });
}

/// Desktop notifications for finished runs.
fn on_event(app: &AppHandle, st: &Inner, server: &str, event: &Value) {
    if event.get("event").and_then(Value::as_str) != Some("finished") {
        return;
    }
    let settings = lock(&st.settings).clone();
    if !settings.notify_desktop {
        return;
    }
    let status = event.get("status").and_then(Value::as_str).unwrap_or("");
    if status == "success" && !settings.notify_success {
        return;
    }
    if status == "cancelled" || status == "running" {
        return;
    }
    let name = st.server(server).map(|s| s.name).unwrap_or_else(|_| server.into());
    let lang = settings.language.unwrap_or_else(|| "en".into());
    let (title, body) = notification_text(
        &lang,
        status,
        &name,
        event.get("error").and_then(Value::as_str).unwrap_or(""),
    );
    let _ = app.notification().builder().title(title).body(body).show();
}

fn notification_text(lang: &str, status: &str, server: &str, error: &str) -> (String, String) {
    let tr = lang == "tr";
    let title = match (status, tr) {
        ("success", false) => "Backup finished",
        ("success", true) => "Yedek tamamlandı",
        ("warning", false) => "Backup finished with warnings",
        ("warning", true) => "Yedek uyarılarla tamamlandı",
        (_, false) => "Backup failed",
        (_, true) => "Yedek başarısız",
    };
    let body = if error.is_empty() {
        server.to_string()
    } else {
        format!("{server}: {error}")
    };
    (title.into(), body)
}

// ---------------------------------------------------------------- downloads and the PC repository

#[derive(Clone, Serialize)]
struct DownloadProgress {
    id: String,
    bytes: u64,
    done: bool,
    error: Option<String>,
}

#[derive(Deserialize)]
pub struct DownloadRequest {
    id: String,
    server: String,
    destination: String,
    snapshot: String,
    path: String,
    zip: bool,
    target: String,
}

/// Streams a file or folder (as zip) from a snapshot on the server to a file on this PC.
#[tauri::command]
async fn download(app: AppHandle, s: State<'_, AppState>, req: DownloadRequest) -> Res<u64> {
    let enc = |v: &str| {
        v.bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect::<String>()
    };
    let url = format!(
        "/v1/dump?destination={}&snapshot={}&path={}&zip={}",
        enc(&req.destination),
        enc(&req.snapshot),
        enc(&req.path),
        req.zip
    );
    let c = s.0.conn(&req.server).await?;
    let resp = c.agent_raw("GET", &url, None).await.map_err(|e| e.0)?;
    if !resp.status().is_success() {
        let body = resp
            .into_body()
            .collect()
            .await
            .map(|b| b.to_bytes())
            .unwrap_or_default();
        return Err(String::from_utf8_lossy(&body).into_owned());
    }
    let tmp = format!("{}.part", req.target);
    let mut f = tokio::fs::File::create(&tmp).await.map_err(|e| e.to_string())?;
    let mut body = resp.into_body().into_data_stream();
    let mut total = 0u64;
    let mut last = 0u64;
    use tokio::io::AsyncWriteExt;
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        f.write_all(&chunk).await.map_err(|e| e.to_string())?;
        total += chunk.len() as u64;
        if total - last > 512 * 1024 {
            last = total;
            let _ = app.emit(
                "download-progress",
                DownloadProgress {
                    id: req.id.clone(),
                    bytes: total,
                    done: false,
                    error: None,
                },
            );
        }
    }
    f.flush().await.map_err(|e| e.to_string())?;
    drop(f);
    tokio::fs::rename(&tmp, &req.target).await.map_err(|e| e.to_string())?;
    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            id: req.id,
            bytes: total,
            done: true,
            error: None,
        },
    );
    Ok(total)
}

/// Starts a copy from a server's destination into the PC repository, through the reverse tunnel.
async fn pc_copy(st: &Inner, sync: &PcSync) -> Res<String> {
    let (addr, user, pw) = st.pc_server().await?;
    let conn = st.conn(&sync.server_id).await?;
    let port = conn.reverse_tunnel(addr).await.map_err(|e| e.0)?;
    let password = st.store.pc_password().map_err(|e| e.to_string())?;
    let repo = format!("rest:http://{user}:{pw}@127.0.0.1:{port}/{}/", sync.server_id);
    let mut retention = Value::Null;
    if sync.apply_retention && !sync.job_id.is_empty() {
        let cfg = st.call(&sync.server_id, "config/get", &serde_json::json!({})).await?;
        if let Some(job) = cfg.get("jobs").and_then(Value::as_array).and_then(|j| {
            j.iter()
                .find(|j| j.get("id").and_then(Value::as_str) == Some(&sync.job_id))
        }) {
            retention = job.get("retention").cloned().unwrap_or(Value::Null);
        }
    }
    let r = st
        .call(
            &sync.server_id,
            "copy",
            &serde_json::json!({"from": sync.destination_id, "to_repository": repo, "to_password": password, "job_id": sync.job_id, "init": true, "retention": retention}),
        )
        .await?;
    Ok(r.get("run_id").and_then(Value::as_str).unwrap_or_default().to_string())
}

/// Copies now and records the result on the sync entry when the run ends.
async fn run_pc_sync(app: AppHandle, st: Arc<Inner>, sync: PcSync) -> Res<String> {
    let run_id = pc_copy(&st, &sync).await;
    let finish = |status: &str, error: Option<String>| {
        let mut s = lock(&st.settings);
        if let Some(x) = s.pc_syncs.iter_mut().find(|x| x.id == sync.id) {
            x.last_run = Some(Utc::now());
            x.last_status = Some(status.into());
            x.last_error = error;
        }
        let _ = st.store.save_settings(&s);
        let _ = app.emit("pc-sync", s.pc_syncs.clone());
    };
    let run_id = match run_id {
        Ok(r) => r,
        Err(e) => {
            finish("failed", Some(e.clone()));
            return Err(e);
        }
    };
    {
        let mut s = lock(&st.settings);
        if let Some(x) = s.pc_syncs.iter_mut().find(|x| x.id == sync.id) {
            x.last_status = Some("running".into());
        }
        let _ = app.emit("pc-sync", s.pc_syncs.clone());
    }
    let (st2, id) = (st.clone(), run_id.clone());
    tauri::async_runtime::spawn(async move {
        // The tunnel lives as long as the connection; keep asking until the agent says the copy is over.
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            match st2
                .call(&sync.server_id, "runs/get", &serde_json::json!({"id": id}))
                .await
            {
                Ok(r) => {
                    let status = r.get("status").and_then(Value::as_str).unwrap_or("failed").to_string();
                    if status != "running" {
                        let err = r.get("error").and_then(Value::as_str).map(str::to_string);
                        let mut s = lock(&st2.settings);
                        if let Some(x) = s.pc_syncs.iter_mut().find(|x| x.id == sync.id) {
                            x.last_run = Some(Utc::now());
                            x.last_status = Some(status);
                            x.last_error = err;
                        }
                        let _ = st2.store.save_settings(&s);
                        let _ = app.emit("pc-sync", s.pc_syncs.clone());
                        break;
                    }
                }
                Err(_) => continue,
            }
        }
    });
    Ok(run_id)
}

#[tauri::command]
async fn pc_sync_now(app: AppHandle, s: State<'_, AppState>, id: String) -> Res<String> {
    let sync = lock(&s.0.settings)
        .pc_syncs
        .iter()
        .find(|x| x.id == id)
        .cloned()
        .ok_or("unknown PC copy")?;
    run_pc_sync(app, s.0.clone(), sync).await
}

// ---------------------------------------------------------------- moving backups around

/// Puts a backup folder of this PC onto one of a server's destinations: the PC's copy is the source, so an old
/// backup (or one received from another PC) can be loaded back into any server and restored there.
#[tauri::command]
async fn pc_push(s: State<'_, AppState>, server: String, folder: String, destination: String) -> Res<String> {
    let st = &s.0;
    if !st.pc_folders().contains(&folder) {
        return Err("that backup folder is not on this PC".into());
    }
    let (addr, user, pw) = st.pc_server().await?;
    let conn = st.conn(&server).await?;
    let port = conn.reverse_tunnel(addr).await.map_err(|e| e.0)?;
    let password = st.store.repo_password(&folder).map_err(|e| e.to_string())?;
    let id = format!("tmp:pc-{folder}");
    st.call(
        &server,
        "destinations/attach",
        &serde_json::json!({"id": id, "repository": format!("rest:http://{user}:{pw}@127.0.0.1:{port}/{folder}/"), "password": password}),
    )
    .await?;
    let r = st
        .call(
            &server,
            "copy",
            &serde_json::json!({"from": id, "to": destination, "init": true}),
        )
        .await?;
    Ok(r.get("run_id").and_then(Value::as_str).unwrap_or_default().to_string())
}

#[derive(Serialize)]
struct ShareInfo {
    code: String,
    port: u16,
    ip: Option<String>,
    name: String,
}

#[tauri::command]
async fn lan_share_start(app: AppHandle, s: State<'_, AppState>, folders: Vec<String>) -> Res<ShareInfo> {
    let st = &s.0;
    let known = st.pc_folders();
    let mut repos = Vec::new();
    for f in folders {
        if !known.contains(&f) {
            return Err(format!("{f} is not a backup folder on this PC"));
        }
        repos.push((f.clone(), st.store.repo_password(&f).map_err(|e| e.to_string())?));
    }
    let root = st.store.pc_root(&lock(&st.settings));
    let name = store::hostname();
    let mut slot = st.lan.lock().await;
    if let Some(old) = slot.take() {
        old.stop();
    }
    let a = app.clone();
    let share = lan::share(&root, repos, &name, move |p| {
        let _ = a.emit("lan-progress", p);
    })
    .await?;
    let info = ShareInfo {
        code: share.code.clone(),
        port: share.port,
        ip: lan::local_ip(),
        name,
    };
    *slot = Some(share);
    Ok(info)
}

#[tauri::command]
async fn lan_share_stop(s: State<'_, AppState>) -> Res<()> {
    if let Some(old) = s.0.lan.lock().await.take() {
        old.stop();
    }
    Ok(())
}

#[tauri::command]
async fn lan_scan() -> Vec<lan::Peer> {
    lan::scan().await
}

/// Takes what another PC offers into this PC's backup folder; returns the folders it made.
#[tauri::command]
async fn lan_receive(app: AppHandle, s: State<'_, AppState>, addr: String, code: String) -> Res<Vec<String>> {
    let st = &s.0;
    let root = st.store.pc_root(&lock(&st.settings));
    tokio::fs::create_dir_all(&root).await.map_err(|e| e.to_string())?;
    let a = app.clone();
    let got = lan::receive(&addr, &code, &root, move |p| {
        let _ = a.emit("lan-progress", p);
    })
    .await?;
    for (folder, password) in &got.repos {
        st.store
            .save_repo_password(folder, password)
            .map_err(|e| e.to_string())?;
    }
    Ok(got.repos.into_iter().map(|(f, _)| f).collect())
}

impl Inner {
    /// Backup folders on this PC: one repository each.
    fn pc_folders(&self) -> Vec<String> {
        let root = self.store.pc_root(&lock(&self.settings));
        pcrepo::usage(&root).into_iter().map(|(n, _)| n).collect()
    }
}

#[derive(Serialize)]
struct PcUsage {
    root: String,
    free: u64,
    repos: Vec<(String, u64)>,
}

#[tauri::command]
fn pc_usage(s: State<'_, AppState>) -> PcUsage {
    let root = s.0.store.pc_root(&lock(&s.0.settings));
    let _ = std::fs::create_dir_all(&root);
    PcUsage {
        free: free_space(&root),
        repos: pcrepo::usage(&root),
        root: root.display().to_string(),
    }
}

fn free_space(p: &std::path::Path) -> u64 {
    #[cfg(unix)]
    {
        let Ok(c) = std::ffi::CString::new(p.display().to_string()) else {
            return 0;
        };
        let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
        // SAFETY: valid path and out-parameter.
        if unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0 {
            return s.f_bavail as u64 * s.f_frsize as u64;
        }
        0
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = p.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut free: u64 = 0;
        // SAFETY: wide is NUL-terminated; the out-parameter is a valid u64.
        let ok = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if ok != 0 { free } else { 0 }
    }
}

/// Every server, destination and password in one file: what you need to restore with nothing but restic,
/// even if every server and this PC are gone.
#[tauri::command]
async fn recovery_kit(s: State<'_, AppState>, target: String) -> Res<u32> {
    let servers = lock(&s.0.servers).clone();
    let mut out = Vec::new();
    let mut missing = 0;
    for srv in &servers {
        let cfg = s.0.call(&srv.id, "config/get", &serde_json::json!({})).await;
        if cfg.is_err() {
            missing += 1;
        }
        out.push(serde_json::json!({
            "server": srv.name, "host": srv.host, "port": srv.port, "user": srv.user, "host_key": srv.host_key,
            "destinations": cfg.ok().and_then(|c| c.get("destinations").cloned()).unwrap_or(Value::Null),
        }));
    }
    let kit = serde_json::json!({
        "orluma_recovery_kit": 1,
        "created": Utc::now(),
        "how_to_restore": "Install restic (https://restic.net). For a destination: set RESTIC_PASSWORD to its password and the repository URL as shown, then `restic snapshots` and `restic restore <id> --target <folder>`. S3/B2/Azure/GCS destinations also need the keys listed with them.",
        "pc_repository": { "folder": s.0.store.pc_root(&lock(&s.0.settings)).display().to_string(), "password": s.0.store.pc_password().map_err(|e| e.to_string())? },
        "servers": out,
    });
    store::write_private(
        std::path::Path::new(&target),
        &serde_json::to_vec_pretty(&kit).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(missing)
}

/// Runs the PC copies that are due, every minute while the app is open.
fn spawn_pc_scheduler(app: AppHandle, st: Arc<Inner>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        loop {
            let due: Vec<PcSync> = lock(&st.settings)
                .pc_syncs
                .iter()
                .filter(|p| p.enabled && p.every_hours > 0 && p.last_status.as_deref() != Some("running"))
                .filter(|p| {
                    p.last_run
                        .is_none_or(|t| Utc::now() - t >= chrono::Duration::hours(p.every_hours as i64))
                })
                .cloned()
                .collect();
            for p in due {
                let _ = run_pc_sync(app.clone(), st.clone(), p).await;
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    });
}

// ---------------------------------------------------------------- updates (signed, from GitHub releases)

#[derive(Clone, Default, Serialize)]
struct UpdateInfo {
    version: String,
    notes: Option<String>,
    date: Option<String>,
}

#[derive(Clone, Default, Serialize)]
struct UpdateStatus {
    current: String,
    available: Option<UpdateInfo>,
    checking: bool,
    installing: bool,
    downloaded: u64,
    total: Option<u64>,
    error: Option<String>,
    last_checked: Option<i64>,
}

fn set_update(app: &AppHandle, s: &Inner, f: impl FnOnce(&mut UpdateStatus)) {
    let snapshot = {
        let mut u = lock(&s.update);
        f(&mut u);
        u.clone()
    };
    let _ = app.emit("update-status", snapshot);
}

async fn check_update_inner(app: &AppHandle, s: &Inner) -> UpdateStatus {
    set_update(app, s, |u| {
        u.checking = true;
        u.error = None;
    });
    let found = match app.updater() {
        Ok(up) => up.check().await,
        Err(e) => Err(e),
    };
    match found {
        Ok(update) => {
            let info = update.as_ref().map(|u| UpdateInfo {
                version: u.version.clone(),
                notes: u.body.clone(),
                date: u.date.map(|d| d.to_string()),
            });
            *lock(&s.pending_update) = update;
            set_update(app, s, |u| {
                u.available = info;
                u.checking = false;
                u.last_checked = Some(Utc::now().timestamp());
            });
        }
        Err(tauri_plugin_updater::Error::ReleaseNotFound) => set_update(app, s, |u| {
            u.checking = false;
            u.last_checked = Some(Utc::now().timestamp());
        }),
        Err(e) => set_update(app, s, |u| {
            u.checking = false;
            u.error = Some(e.to_string());
        }),
    }
    lock(&s.update).clone()
}

async fn install_update_inner(app: &AppHandle, s: &Inner) -> Res<()> {
    let update = lock(&s.pending_update).clone().ok_or("no update available")?;
    set_update(app, s, |u| {
        u.installing = true;
        u.downloaded = 0;
        u.error = None;
    });
    let (mut done, mut reported) = (0u64, 0u64);
    let res = update
        .download_and_install(
            |chunk, total| {
                done += chunk as u64;
                if done - reported >= 256 * 1024 || Some(done) == total {
                    reported = done;
                    set_update(app, s, |u| {
                        u.downloaded = done;
                        u.total = total;
                    });
                }
            },
            || {},
        )
        .await;
    match res {
        Ok(()) => {
            #[cfg(not(windows))]
            app.restart();
            #[cfg(windows)]
            Ok(())
        }
        Err(e) => {
            set_update(app, s, |u| {
                u.installing = false;
                u.error = Some(e.to_string());
            });
            Err(e.to_string())
        }
    }
}

#[tauri::command]
fn update_status(s: State<'_, AppState>) -> UpdateStatus {
    lock(&s.0.update).clone()
}

#[tauri::command]
async fn check_update(app: AppHandle, s: State<'_, AppState>) -> Res<UpdateStatus> {
    Ok(check_update_inner(&app, &s.0).await)
}

#[tauri::command]
async fn install_update(app: AppHandle, s: State<'_, AppState>) -> Res<()> {
    install_update_inner(&app, &s.0).await
}

fn spawn_updater(app: AppHandle, st: Arc<Inner>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            let status = check_update_inner(&app, &st).await;
            let (auto, ready) = {
                let s = lock(&st.settings);
                (s.auto_update, s.setup_done)
            };
            if auto && ready && status.available.is_some() {
                let _ = install_update_inner(&app, &st).await;
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
}

// ---------------------------------------------------------------- window and tray

fn show_main(app: &AppHandle) {
    let w = match app.get_webview_window("main") {
        Some(w) => Some(w),
        None => create_main(app).ok(),
    };
    if let Some(w) = w {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn create_main(app: &AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .cloned()
        .ok_or(tauri::Error::WindowNotFound)?;
    let w = tauri::WebviewWindowBuilder::from_config(app, &config)?.build()?;
    // The window cannot be resized by hand, so make sure the fixed size fits small screens (1366x768 at 125 %).
    if let Ok(Some(m)) = w
        .current_monitor()
        .map(|m| m.or_else(|| w.primary_monitor().ok().flatten()))
    {
        let screen = m.size().to_logical::<f64>(m.scale_factor());
        let _ = w.set_size(tauri::LogicalSize::new(
            1180f64.min(screen.width - 48.0),
            760f64.min(screen.height - 96.0),
        ));
        let _ = w.center();
    }
    #[cfg(windows)]
    {
        use tauri::window::{Effect, EffectsBuilder};
        let _ = w.set_effects(EffectsBuilder::new().effect(Effect::Mica).build());
    }
    Ok(w)
}

fn tray_labels(lang: &str) -> (&'static str, &'static str) {
    match lang {
        "tr" => ("Orluma'yı aç", "Çıkış"),
        "de" => ("Orluma öffnen", "Beenden"),
        "es" => ("Abrir Orluma", "Salir"),
        "fr" => ("Ouvrir Orluma", "Quitter"),
        "pt" => ("Abrir o Orluma", "Sair"),
        "ru" => ("Открыть Orluma", "Выход"),
        "zh" => ("打开 Orluma", "退出"),
        "ja" => ("Orluma を開く", "終了"),
        "ar" => ("فتح Orluma", "خروج"),
        _ => ("Open Orluma", "Quit"),
    }
}

fn tray_menu(app: &AppHandle, lang: &str) -> tauri::Result<Menu<tauri::Wry>> {
    let (open, quit) = tray_labels(lang);
    Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", open, true, None::<&str>)?,
            &MenuItem::with_id(app, "quit", quit, true, None::<&str>)?,
        ],
    )
}

fn refresh_tray_menu(app: &AppHandle, lang: &str) {
    let state = app.state::<AppState>();
    if let (Some(tray), Ok(menu)) = (lock(&state.0.tray).as_ref(), tray_menu(app, lang)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn build_tray(app: &AppHandle, lang: &str) -> tauri::Result<TrayIcon> {
    let mut b = TrayIconBuilder::with_id("main")
        .tooltip("Orluma")
        .menu(&tray_menu(app, lang)?)
        .show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        b = b.icon(icon.clone());
    }
    b.on_menu_event(|app, e| match e.id.as_ref() {
        "open" => show_main(app),
        "quit" => app.exit(0),
        _ => {}
    })
    .on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            show_main(tray.app_handle());
        }
    })
    .build(app)
}

pub fn run() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let background = std::env::args().any(|a| a == "--background");
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show_main(app)))
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ))
        .setup(move |app| {
            let dir = app.path().app_data_dir()?;
            applog::init(dir.join("logs").join("orluma.log"));
            applog::info(
                "app",
                format!(
                    "Orluma {} started on {} {}",
                    env!("CARGO_PKG_VERSION"),
                    std::env::consts::OS,
                    std::env::consts::ARCH
                ),
            );
            let store = Store::new(dir);
            let settings = store.settings();
            let servers = store.servers();
            let http = reqwest::Client::builder()
                .user_agent(concat!("Orluma/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(300))
                .build()
                .map_err(|e| e.to_string())?;
            let st = Arc::new(Inner {
                store,
                settings: Mutex::new(settings.clone()),
                servers: Mutex::new(servers.clone()),
                conns: tokio::sync::Mutex::new(HashMap::new()),
                listening: Mutex::new(Default::default()),
                pc: tokio::sync::Mutex::new(None),
                lan: tokio::sync::Mutex::new(None),
                http,
                update: Mutex::new(UpdateStatus {
                    current: app.package_info().version.to_string(),
                    ..Default::default()
                }),
                pending_update: Mutex::new(None),
                tray: Mutex::new(None),
            });
            app.manage(AppState(st.clone()));
            *lock(&st.tray) = build_tray(app.handle(), settings.language.as_deref().unwrap_or("en")).ok();
            if !(background && settings.setup_done && settings.run_in_background) {
                show_main(app.handle());
            }
            for s in servers {
                listen(app.handle().clone(), st.clone(), s.id);
            }
            spawn_pc_scheduler(app.handle().clone(), st.clone());
            spawn_updater(app.handle().clone(), st);
            Ok(())
        })
        .on_window_event(|_, _: &WindowEvent| {})
        .invoke_handler(tauri::generate_handler![
            get_settings,
            set_settings,
            log_event,
            log_tail,
            log_redact,
            log_export,
            cloud_connect,
            cloud_cancel,
            app_info,
            minimize_window,
            close_window,
            list_servers,
            probe_server,
            add_server,
            update_server,
            reinstall_agent,
            remove_server,
            agent,
            authorize_on_server,
            download,
            pc_sync_now,
            pc_usage,
            pc_push,
            lan_share_start,
            lan_share_stop,
            lan_scan,
            lan_receive,
            recovery_kit,
            update_status,
            check_update,
            install_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Orluma");
    app.run(|app, event| {
        if let RunEvent::ExitRequested { api, code, .. } = event {
            // Closing the window keeps Orluma in the tray in background mode; Quit (code set) really exits.
            let state = app.state::<AppState>();
            let keep = {
                let s = lock(&state.0.settings);
                s.run_in_background && s.setup_done
            };
            if code.is_none() && keep {
                api.prevent_exit();
            }
        }
    });
}
