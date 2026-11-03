//! The agent's API, on a root-only Unix socket. Nothing listens on the network: the desktop app reaches the
//! socket through SSH, so who may talk to the agent is exactly who may log in to the server as root.

use crate::restic::Repo;
use crate::restic::anyhow_lite::Error;
use crate::runner;
use crate::state::{Shared, lock};
use axum::body::Body;
use axum::extract::{Json, Query, State};
use axum::http::{StatusCode, header};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures_util::StreamExt;
use orluma_core::api::*;
use orluma_core::model::{AgentConfig, Destination, Notifier};
use serde::Deserialize;
use std::convert::Infallible;

pub struct ApiErr(StatusCode, String);

impl IntoResponse for ApiErr {
    fn into_response(self) -> Response {
        (self.0, Json(ApiError { error: self.1 })).into_response()
    }
}

impl From<Error> for ApiErr {
    fn from(e: Error) -> Self {
        ApiErr(StatusCode::BAD_REQUEST, e.0)
    }
}

type R<T> = Result<Json<T>, ApiErr>;

async fn status(State(a): State<Shared>) -> R<AgentStatus> {
    let pubkey = std::fs::read_to_string(a.paths.ssh_key().with_extension("pub"))
        .unwrap_or_default()
        .trim()
        .to_string();
    Ok(Json(AgentStatus {
        version: env!("CARGO_PKG_VERSION").into(),
        api: API_VERSION,
        hostname: crate::sys::hostname(),
        name: a.server_name(),
        os: crate::sys::os_name(),
        arch: std::env::consts::ARCH.into(),
        uptime_secs: crate::sys::uptime_secs(),
        timezone: Some(a.config().agent.timezone)
            .filter(|t| !t.is_empty())
            .unwrap_or_else(crate::sys::timezone),
        restic: crate::restic::installed_version(&a.paths).await,
        ssh_public_key: pubkey,
        disks: crate::sys::disks(),
        running: lock(&a.running).values().map(|r| r.progress.clone()).collect(),
        jobs: crate::scheduler::job_states(&a),
        update: lock(&a.update_available).clone(),
    }))
}

async fn config_get(State(a): State<Shared>) -> R<AgentConfig> {
    Ok(Json(a.config()))
}

async fn config_set(State(a): State<Shared>, Json(cfg): Json<AgentConfig>) -> R<()> {
    a.save_config(cfg)?;
    Ok(Json(()))
}

async fn job_run(State(a): State<Shared>, Json(r): Json<IdRequest>) -> R<StartedRun> {
    Ok(Json(StartedRun {
        run_id: runner::start_backup(&a, &r.id, Trigger::Manual)?,
    }))
}

async fn run_cancel(State(a): State<Shared>, Json(r): Json<IdRequest>) -> R<bool> {
    Ok(Json(runner::cancel(&a, &r.id)))
}

async fn runs_list(State(a): State<Shared>, Json(r): Json<RunsRequest>) -> R<Vec<RunSummary>> {
    Ok(Json(a.db.runs(r.job_id.as_deref(), r.limit.unwrap_or(100).min(1000))))
}

async fn run_get(State(a): State<Shared>, Json(r): Json<IdRequest>) -> R<RunRecord> {
    a.db.run(&r.id)
        .map(Json)
        .ok_or(ApiErr(StatusCode::NOT_FOUND, "no such run".into()))
}

async fn events(State(a): State<Shared>) -> Sse<impl futures_util::Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = tokio_stream::wrappers::BroadcastStream::new(a.events.subscribe());
    let stream = rx.filter_map(|e| async move { e.ok().and_then(|e| SseEvent::default().json_data(e).ok()).map(Ok) });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ---- destinations

async fn dest_test(State(a): State<Shared>, Json(d): Json<Destination>) -> R<TestResult> {
    let restic = a.restic().await?;
    let repo = Repo::of(&d, &a.paths)?;
    Ok(Json(match restic.exists(&repo).await {
        Ok(true) => TestResult {
            ok: true,
            message: "A repository is there and the password opens it".into(),
        },
        Ok(false) => TestResult {
            ok: true,
            message: "Reachable. The repository will be created on the first run".into(),
        },
        Err(e) => TestResult {
            ok: false,
            message: e.0,
        },
    }))
}

async fn dest_init(State(a): State<Shared>, Json(r): Json<IdRequest>) -> R<TestResult> {
    let restic = a.restic().await?;
    restic.init(&a.repo(&r.id)?).await?;
    Ok(Json(TestResult {
        ok: true,
        message: "Ready".into(),
    }))
}

async fn dest_stats(State(a): State<Shared>, Json(r): Json<IdRequest>) -> R<RepoStats> {
    let restic = a.restic().await?;
    Ok(Json(restic.stats(&a.repo(&r.id)?).await?))
}

async fn dest_unlock(State(a): State<Shared>, Json(r): Json<IdRequest>) -> R<()> {
    let restic = a.restic().await?;
    restic.unlock(&a.repo(&r.id)?).await?;
    Ok(Json(()))
}

#[derive(Deserialize)]
struct VerifyReq {
    id: String,
    #[serde(default)]
    read_data_percent: u8,
}

async fn dest_verify(State(a): State<Shared>, Json(r): Json<VerifyReq>) -> R<StartedRun> {
    let repo = a.repo(&r.id)?;
    let pct = r.read_data_percent;
    let id = r.id.clone();
    let run_id = runner::start_task(
        &a,
        &format!("dest:{}", r.id),
        RunKind::Verify,
        r.id.clone(),
        move |a, _| async move {
            let restic = a.restic().await?;
            let m = restic.check(&repo, pct).await?;
            a.db.set_mark(&format!("verify:{id}"), chrono::Utc::now());
            Ok(m)
        },
    )?;
    Ok(Json(StartedRun { run_id }))
}

async fn dest_prune(State(a): State<Shared>, Json(r): Json<IdRequest>) -> R<StartedRun> {
    let repo = a.repo(&r.id)?;
    let id = r.id.clone();
    let run_id = runner::start_task(
        &a,
        &format!("dest:{}", r.id),
        RunKind::Prune,
        r.id.clone(),
        move |a, _| async move {
            a.restic().await?.prune(&repo).await?;
            a.db.set_mark(&format!("prune:{id}"), chrono::Utc::now());
            Ok("Unused data was freed".into())
        },
    )?;
    Ok(Json(StartedRun { run_id }))
}

// ---- snapshots

async fn snapshots(State(a): State<Shared>, Json(r): Json<SnapshotsRequest>) -> R<Vec<Snapshot>> {
    let restic = a.restic().await?;
    Ok(Json(restic.snapshots(&a.repo(&r.destination)?, None).await?))
}

async fn snapshot_ls(State(a): State<Shared>, Json(r): Json<LsRequest>) -> R<Vec<Node>> {
    let restic = a.restic().await?;
    Ok(Json(restic.ls(&a.repo(&r.destination)?, &r.snapshot, &r.path).await?))
}

async fn snapshot_forget(State(a): State<Shared>, Json(r): Json<ForgetRequest>) -> R<()> {
    let restic = a.restic().await?;
    restic.forget_ids(&a.repo(&r.destination)?, &r.snapshots).await?;
    Ok(Json(()))
}

async fn restore(State(a): State<Shared>, Json(r): Json<RestoreRequest>) -> R<StartedRun> {
    let repo = a.repo(&r.destination)?;
    let target = std::path::PathBuf::from(&r.target);
    if !target.is_absolute() || target == std::path::Path::new("/") {
        return Err(ApiErr(
            StatusCode::BAD_REQUEST,
            "choose a folder to restore into (not /)".into(),
        ));
    }
    if !r.overwrite && target.read_dir().is_ok_and(|mut d| d.next().is_some()) {
        return Err(ApiErr(
            StatusCode::CONFLICT,
            format!("{} is not empty; pick an empty folder or allow overwriting", r.target),
        ));
    }
    let run_id = runner::start_task(
        &a,
        "restore",
        RunKind::Restore,
        r.destination.clone(),
        move |a, _| async move {
            std::fs::create_dir_all(&r.target)?;
            let m = a
                .restic()
                .await?
                .restore(&repo, &r.snapshot, &r.include, &r.target)
                .await?;
            Ok(format!("Restored into {}: {m}", r.target))
        },
    )?;
    Ok(Json(StartedRun { run_id }))
}

async fn restore_db(State(a): State<Shared>, Json(r): Json<RestoreDbRequest>) -> R<StartedRun> {
    let repo = a.repo(&r.destination)?;
    let run_id = runner::start_task(
        &a,
        "restore-db",
        RunKind::Restore,
        r.destination.clone(),
        move |a, _| async move { crate::restore_db::run(&a.restic().await?, &repo, &r).await },
    )?;
    Ok(Json(StartedRun { run_id }))
}

async fn attach(State(a): State<Shared>, Json(r): Json<AttachRequest>) -> R<()> {
    if !r.id.starts_with("tmp:") || r.id.len() > 64 {
        return Err(ApiErr(StatusCode::BAD_REQUEST, "attached ids start with tmp:".into()));
    }
    lock(&a.attached).insert(r.id, Repo::raw(&r.repository, &r.password));
    Ok(Json(()))
}

async fn copy(State(a): State<Shared>, Json(r): Json<CopyRequest>) -> R<StartedRun> {
    let from = a.repo(&r.from)?;
    let to = match &r.to_destination {
        Some(d) => Repo::of(d, &a.paths)?,
        None if !r.to.is_empty() => a.repo(&r.to)?,
        None => Repo::raw(&r.to_repository, &r.to_password),
    };
    let label = if r.job_id.is_empty() {
        format!("dest:{}", r.from)
    } else {
        r.job_id.clone()
    };
    let run_id = runner::start_task(&a, &label, RunKind::Copy, r.from.clone(), move |a, _| async move {
        let restic = a.restic().await?;
        if r.init && !restic.exists(&to).await? {
            restic.init(&to).await?;
        }
        let tag = (!r.job_id.is_empty()).then(|| format!("job:{}", r.job_id));
        let mut m = restic.copy(&from, &to, tag.as_deref()).await?;
        if let Some(ret) = r.retention.as_ref().filter(|_| !r.job_id.is_empty()) {
            m.push_str(&format!("; retention: {}", restic.forget(&to, &r.job_id, ret).await?));
            restic.prune(&to).await?;
        }
        Ok(m)
    })?;
    Ok(Json(StartedRun { run_id }))
}

#[derive(Deserialize)]
struct DumpQuery {
    destination: String,
    snapshot: String,
    path: String,
    #[serde(default)]
    zip: bool,
}

/// Streams a file (or a folder as a zip) out of a snapshot, for "download to this PC".
async fn dump(State(a): State<Shared>, Query(q): Query<DumpQuery>) -> Result<Response, ApiErr> {
    let restic = a.restic().await?;
    let repo = a.repo(&q.destination)?;
    let mut args = vec!["dump", "--no-lock"];
    if q.zip {
        args.extend(["--archive", "zip"]);
    }
    args.extend([q.snapshot.as_str(), q.path.as_str()]);
    let mut cmd = restic.command(&repo, &args);
    cmd.kill_on_drop(false);
    let mut child = cmd
        .spawn()
        .map_err(|e| ApiErr(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or(ApiErr(StatusCode::INTERNAL_SERVER_ERROR, "no output".into()))?;
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    let stream = tokio_util::io::ReaderStream::with_capacity(stdout, 256 * 1024);
    Ok((
        [(header::CONTENT_TYPE, "application/octet-stream")],
        Body::from_stream(stream),
    )
        .into_response())
}

async fn discover() -> R<Discovery> {
    Ok(Json(crate::discover::discover().await))
}

async fn notifier_test(State(a): State<Shared>, Json(n): Json<Notifier>) -> R<TestResult> {
    let m = crate::notify::Message {
        event: orluma_core::model::Event::RunSucceeded,
        server: a.server_name(),
        job_id: None,
        job_name: None,
        title: "Test alert from Orluma".into(),
        body: "If you can read this, alerts from this server reach you.".into(),
        fields: vec![("Agent".into(), env!("CARGO_PKG_VERSION").into())],
    };
    Ok(Json(match crate::notify::send(&a.http, &n, &m).await {
        Ok(()) => TestResult {
            ok: true,
            message: "Sent".into(),
        },
        Err(e) => TestResult {
            ok: false,
            message: e.0,
        },
    }))
}

async fn update_check(State(a): State<Shared>) -> R<String> {
    let v = crate::update::latest(&a).await?;
    Ok(Json(v))
}

async fn update_install(State(a): State<Shared>, Json(r): Json<UpdateRequest>) -> R<()> {
    crate::update::install(&a, &r.version).await?;
    Ok(Json(()))
}

pub fn router(agent: Shared) -> axum::Router {
    axum::Router::new()
        .route("/v1/status", post(status))
        .route("/v1/config/get", post(config_get))
        .route("/v1/config/set", post(config_set))
        .route("/v1/jobs/run", post(job_run))
        .route("/v1/runs/cancel", post(run_cancel))
        .route("/v1/runs/list", post(runs_list))
        .route("/v1/runs/get", post(run_get))
        .route("/v1/events", get(events))
        .route("/v1/destinations/test", post(dest_test))
        .route("/v1/destinations/init", post(dest_init))
        .route("/v1/destinations/stats", post(dest_stats))
        .route("/v1/destinations/unlock", post(dest_unlock))
        .route("/v1/destinations/verify", post(dest_verify))
        .route("/v1/destinations/prune", post(dest_prune))
        .route("/v1/snapshots/list", post(snapshots))
        .route("/v1/snapshots/ls", post(snapshot_ls))
        .route("/v1/snapshots/forget", post(snapshot_forget))
        .route("/v1/restore", post(restore))
        .route("/v1/copy", post(copy))
        .route("/v1/restore-db", post(restore_db))
        .route("/v1/destinations/attach", post(attach))
        .route("/v1/dump", get(dump))
        .route("/v1/discover", post(discover))
        .route("/v1/notifiers/test", post(notifier_test))
        .route("/v1/agent/update-check", post(update_check))
        .route("/v1/agent/update", post(update_install))
        .with_state(agent)
}

pub async fn serve(agent: Shared) -> std::io::Result<()> {
    let path = agent.paths.socket.clone();
    let _ = std::fs::remove_file(&path);
    let listener = tokio::net::UnixListener::bind(&path)?;
    crate::paths::set_mode(&path, 0o600)?;
    eprintln!(
        "orluma-agent {} listening on {}",
        env!("CARGO_PKG_VERSION"),
        path.display()
    );
    axum::serve(listener, router(agent)).await
}
