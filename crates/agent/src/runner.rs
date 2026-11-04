//! A run, start to finish: hooks, the sources one by one, retention, the periodic prune and verify, copies to
//! the other destinations, the health check ping and the alerts. Everything it does goes into the run's log.

use crate::notify::{self, Message};
use crate::restic::anyhow_lite::{Error, Result, bail};
use crate::restic::{Repo, What};
use crate::sources::{self, Unit};
use crate::state::{Running, Shared, lock};
use chrono::Utc;
use orluma_core::api::{AgentEvent, Level, LogLine, RunKind, RunProgress, RunRecord, RunStatus, RunSummary, Trigger};
use orluma_core::model::{Event, Hook, Job};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub struct Run {
    pub agent: Shared,
    pub record: RunRecord,
    pub cancel: CancellationToken,
    progress: RunProgress,
}

impl Run {
    fn new(agent: &Shared, job_id: &str, kind: RunKind, trigger: Trigger) -> Run {
        let id = orluma_core::new_id("r");
        let started = Utc::now();
        let cancel = CancellationToken::new();
        let progress = RunProgress {
            run_id: id.clone(),
            job_id: job_id.into(),
            kind,
            started,
            ..Default::default()
        };
        lock(&agent.running).insert(
            id.clone(),
            Running {
                progress: progress.clone(),
                cancel: cancel.clone(),
            },
        );
        let record = RunRecord {
            summary: RunSummary {
                id,
                job_id: job_id.into(),
                kind,
                status: RunStatus::Running,
                trigger,
                started,
                ..Default::default()
            },
            log: Vec::new(),
        };
        agent.db.save_run(&record);
        let _ = agent.events.send(AgentEvent::Progress(progress.clone()));
        Run {
            agent: agent.clone(),
            record,
            cancel,
            progress,
        }
    }

    pub fn log(&mut self, level: Level, text: impl Into<String>) {
        let text = text.into();
        let line = format!("{} {}", self.record.summary.id, text);
        match level {
            Level::Error => crate::logs::error("run", line),
            Level::Warn => crate::logs::warn("run", line),
            _ => crate::logs::info("run", line),
        }
        self.record.log.push(LogLine {
            at: Utc::now(),
            level,
            text,
        });
        if self.record.log.len().is_multiple_of(10) {
            self.agent.db.save_run(&self.record);
        }
    }
    fn info(&mut self, t: impl Into<String>) {
        self.log(Level::Info, t)
    }
    fn warn(&mut self, t: impl Into<String>) {
        self.log(Level::Warn, t)
    }
    fn error(&mut self, t: impl Into<String>) {
        self.log(Level::Error, t)
    }

    fn step(&mut self, name: &str, index: u32, count: u32) {
        self.progress.step = name.into();
        self.progress.step_index = index;
        self.progress.step_count = count;
        self.progress.percent = 0.0;
        self.progress.bytes_done = 0;
        self.progress.bytes_total = 0;
        self.progress.files_done = 0;
        self.progress.files_total = 0;
        self.agent.progress(self.progress.clone());
    }

    fn finish(mut self, status: RunStatus, error: Option<String>) -> RunSummary {
        self.record.summary.status = status;
        self.record.summary.error = error;
        self.record.summary.finished = Some(Utc::now());
        self.agent.db.save_run(&self.record);
        lock(&self.agent.running).remove(&self.record.summary.id);
        let s = self.record.summary.clone();
        let _ = self.agent.events.send(AgentEvent::Finished(s.clone()));
        s
    }
}

fn human(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1000.0 && i < units.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", units[i])
    }
}

fn duration(d: chrono::TimeDelta) -> String {
    let s = d.num_seconds().max(0);
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m {}s", s / 60, s % 60)
    } else {
        format!("{}h {}m", s / 3600, (s % 3600) / 60)
    }
}

async fn hook(run: &mut Run, h: &Hook, job: &Job, status: Option<&str>) -> Result<()> {
    run.info(format!("Hook: {}", h.command));
    let mut c = tokio::process::Command::new("sh");
    c.arg("-c")
        .arg(&h.command)
        .env("ORLUMA_JOB", &job.id)
        .env("ORLUMA_JOB_NAME", &job.name)
        .kill_on_drop(true);
    if let Some(s) = status {
        c.env("ORLUMA_STATUS", s);
    }
    let out = tokio::time::timeout(Duration::from_secs(h.timeout_secs.max(1) as u64), c.output()).await;
    match out {
        Err(_) => bail!("timed out after {}s", h.timeout_secs),
        Ok(Err(e)) => bail!("{e}"),
        Ok(Ok(o)) => {
            let text = String::from_utf8_lossy(&o.stdout).to_string() + &String::from_utf8_lossy(&o.stderr);
            for l in text.lines().rev().take(5).collect::<Vec<_>>().into_iter().rev() {
                run.info(format!("  {l}"));
            }
            if !o.status.success() {
                bail!("exited with code {}", o.status.code().unwrap_or(-1));
            }
            Ok(())
        }
    }
}

async fn ping(agent: &Shared, url: &str, suffix: &str) {
    if url.is_empty() {
        return;
    }
    let _ = agent
        .http
        .get(format!("{}{}", url.trim_end_matches('/'), suffix))
        .timeout(Duration::from_secs(15))
        .send()
        .await;
}

/// Waits until no other run uses the repository, then marks it busy for this one.
async fn claim_repo(agent: &Shared, dest: &str, run_id: &str, cancel: &CancellationToken) -> Result<()> {
    loop {
        {
            let mut busy = lock(&agent.busy_repos);
            if !busy.contains_key(dest) {
                busy.insert(dest.into(), run_id.into());
                return Ok(());
            }
        }
        tokio::select! {
            _ = cancel.cancelled() => bail!("Cancelled"),
            _ = tokio::time::sleep(Duration::from_secs(3)) => {}
        }
    }
}

fn release_repo(agent: &Shared, dest: &str) {
    lock(&agent.busy_repos).remove(dest);
}

pub fn alert(agent: &Shared, m: Message) {
    let agent = agent.clone();
    tokio::spawn(async move {
        for n in agent.config().notifiers {
            if notify::wants(&n, &m)
                && let Err(e) = notify::send(&agent.http, &n, &m).await
            {
                crate::logs::error("alert", format!("'{}' failed: {e}", n.name));
            }
        }
    });
}

// ---------------------------------------------------------------- backup

pub fn start_backup(agent: &Shared, job_id: &str, trigger: Trigger) -> Result<String> {
    let job = agent
        .config()
        .jobs
        .into_iter()
        .find(|j| j.id == job_id)
        .ok_or_else(|| Error(format!("no job '{job_id}'")))?;
    if lock(&agent.running)
        .values()
        .any(|r| r.progress.job_id == job.id && r.progress.kind == RunKind::Backup)
    {
        bail!("'{}' is already running", job.name);
    }
    let run = Run::new(agent, &job.id, RunKind::Backup, trigger);
    let id = run.record.summary.id.clone();
    tokio::spawn(backup(run, job));
    Ok(id)
}

async fn backup(mut run: Run, job: Job) {
    let agent = run.agent.clone();
    let cancel = run.cancel.clone();
    let _permit = tokio::select! {
        p = agent.slots.acquire() => p.ok(),
        _ = cancel.cancelled() => None,
    };
    if cancel.is_cancelled() {
        run.finish(RunStatus::Cancelled, Some("Cancelled before it started".into()));
        return;
    }
    run.info(format!("Backup '{}' started", job.name));
    ping(&agent, &job.healthcheck_url, "/start").await;
    let result = backup_inner(&mut run, &job).await;
    release_repo(&agent, &job.destination);
    for c in &job.copies {
        release_repo(&agent, c);
    }

    let warnings = run.record.log.iter().any(|l| l.level == Level::Warn);
    let (status, error) = match result {
        Ok(()) if warnings => (RunStatus::Warning, None),
        Ok(()) => (RunStatus::Success, None),
        Err(e) if cancel.is_cancelled() => (RunStatus::Cancelled, Some(e.0)),
        Err(e) => (RunStatus::Failed, Some(e.0)),
    };
    let label = match status {
        RunStatus::Success => "success",
        RunStatus::Warning => "warning",
        _ => "failed",
    };
    for h in &job.hooks.after {
        if let Err(e) = hook(&mut run, h, &job, Some(label)).await {
            run.warn(format!("After-hook failed: {e}"));
        }
    }
    let took = Utc::now() - run.record.summary.started;
    match status {
        RunStatus::Success | RunStatus::Warning => {
            run.info(format!(
                "Done in {}: {} new, {} in total",
                duration(took),
                human(run.record.summary.bytes_added),
                human(run.record.summary.bytes_total)
            ));
            ping(&agent, &job.healthcheck_url, "").await;
        }
        RunStatus::Cancelled => run.warn("Cancelled"),
        _ => {
            run.error(format!(
                "Failed after {}: {}",
                duration(took),
                error.clone().unwrap_or_default()
            ));
            ping(&agent, &job.healthcheck_url, "/fail").await;
        }
    }
    let s = run.finish(status, error.clone());
    let event = match status {
        RunStatus::Success => Event::RunSucceeded,
        RunStatus::Warning => Event::RunWarning,
        RunStatus::Cancelled => return,
        _ => Event::RunFailed,
    };
    let title = match event {
        Event::RunSucceeded => format!("Backup '{}' finished", job.name),
        Event::RunWarning => format!("Backup '{}' finished with warnings", job.name),
        _ => format!("Backup '{}' failed", job.name),
    };
    let body = error.unwrap_or_else(|| format!("{} snapshot(s) written.", s.snapshots.len()));
    alert(
        &agent,
        Message {
            event,
            server: agent.server_name(),
            job_id: Some(job.id.clone()),
            job_name: Some(job.name.clone()),
            title,
            body,
            fields: vec![
                ("New data".into(), human(s.bytes_added)),
                ("Size".into(), human(s.bytes_total)),
                ("Took".into(), duration(took)),
            ],
        },
    );
}

async fn backup_inner(run: &mut Run, job: &Job) -> Result<()> {
    let agent = run.agent.clone();
    let cancel = run.cancel.clone();
    for h in &job.hooks.before {
        if let Err(e) = hook(run, h, job, None).await {
            if h.continue_on_error {
                run.warn(format!("Before-hook failed, continuing: {e}"));
            } else {
                bail!("Before-hook failed: {e}");
            }
        }
    }
    let restic = agent.restic().await?;
    let repo = agent.repo(&job.destination)?;
    let dest_name = agent.destination(&job.destination)?.name;
    claim_repo(&agent, &job.destination, &run.record.summary.id, &cancel).await?;
    if !restic.exists(&repo).await? {
        run.info(format!("Creating the repository on '{dest_name}'"));
        restic.init(&repo).await?;
    }

    // Plan every source first: a database that cannot be listed is an error now, not halfway through.
    let mut units = Vec::new();
    let mut failed = Vec::new();
    for src in &job.sources {
        match sources::plan(src).await {
            Ok(p) => units.extend(p),
            Err(e) => {
                run.error(format!("Source {} ({}): {e}", src.id(), src.kind()));
                failed.push(src.id().to_string());
            }
        }
    }
    let host = agent.server_name();
    let count = units.len() as u32;
    for (i, u) in units.iter().enumerate() {
        if cancel.is_cancelled() {
            bail!("Cancelled");
        }
        run.step(&u.label, i as u32 + 1, count);
        run.info(format!("Backing up {}", u.label));
        let tags = vec![
            "orluma".to_string(),
            format!("job:{}", job.id),
            format!("source:{}", u.source_id),
            format!("kind:{}", u.kind),
        ];
        let what = match &u.unit {
            Unit::Paths {
                paths,
                excludes,
                one_file_system,
            } => What::Paths {
                paths,
                excludes,
                one_file_system: *one_file_system,
            },
            Unit::Stdin { argv, env, filename } => What::Stdin { argv, env, filename },
        };
        let mut base = run.progress.clone();
        let ag = agent.clone();
        let res = restic
            .backup(&repo, what, &tags, &host, &job.options, &cancel, |p| {
                base.percent = p.percent;
                base.bytes_done = p.bytes_done;
                base.bytes_total = p.bytes_total;
                base.files_done = p.files_done;
                base.files_total = p.files_total;
                base.seconds_remaining = p.seconds_remaining;
                ag.progress(base.clone());
            })
            .await;
        match res {
            Ok(r) => {
                let s = &mut run.record.summary;
                s.bytes_added += r.bytes_added;
                s.bytes_total += r.bytes_total;
                s.files_new += r.files_new;
                s.files_changed += r.files_changed;
                if let Some(id) = &r.snapshot {
                    s.snapshots.push(id.clone());
                }
                let short = r.snapshot.as_deref().map_or("-", |s| &s[..s.len().min(8)]).to_string();
                run.info(format!(
                    "  snapshot {short}: {} processed, {} new",
                    human(r.bytes_total),
                    human(r.bytes_added)
                ));
                for e in r.errors {
                    run.warn(format!("  could not read {e}"));
                }
            }
            Err(e) => {
                if cancel.is_cancelled() {
                    bail!("Cancelled");
                }
                run.error(format!("  {}: {e}", u.label));
                failed.push(u.source_id.clone());
            }
        }
    }
    if !failed.is_empty() {
        failed.dedup();
        bail!(
            "{} of the job's sources could not be backed up ({})",
            failed.len(),
            failed.join(", ")
        );
    }

    maintain(run, job, &job.destination, &repo, &restic).await;

    for copy in &job.copies {
        if cancel.is_cancelled() {
            bail!("Cancelled");
        }
        let target = match agent.destination(copy) {
            Ok(d) => d,
            Err(e) => {
                run.warn(format!("Copy: {e}"));
                continue;
            }
        };
        run.step(&format!("copy to {}", target.name), count, count);
        run.info(format!("Copying the new snapshots to '{}'", target.name));
        let to = match Repo::of(&target, &agent.paths) {
            Ok(r) => r,
            Err(e) => {
                run.warn(format!("Copy to '{}': {e}", target.name));
                continue;
            }
        };
        if let Err(e) = claim_repo(&agent, copy, &run.record.summary.id, &cancel).await {
            run.warn(format!("Copy to '{}': {e}", target.name));
            continue;
        }
        let res = async {
            if !restic.exists(&to).await? {
                restic.init(&to).await?;
            }
            restic.copy(&repo, &to, Some(&format!("job:{}", job.id))).await
        }
        .await;
        match res {
            Ok(m) => {
                run.info(format!("  {m}"));
                maintain(run, job, copy, &to, &restic).await;
            }
            Err(e) => run.warn(format!("Copy to '{}' failed: {e}", target.name)),
        }
    }
    Ok(())
}

/// Retention every run; prune and verify when they are due.
async fn maintain(run: &mut Run, job: &Job, dest: &str, repo: &Repo, restic: &crate::restic::Restic) {
    let agent = run.agent.clone();
    let name = agent.destination(dest).map(|d| d.name).unwrap_or_else(|_| dest.into());
    match restic.forget(repo, &job.id, &job.retention).await {
        Ok(m) => run.info(format!("Retention on '{name}': {m}")),
        Err(e) => run.warn(format!("Retention on '{name}' failed: {e}")),
    }
    let now = Utc::now();
    let due = |key: &str, days: u32| {
        days > 0
            && agent
                .db
                .mark(key)
                .is_none_or(|t| now - t >= chrono::Duration::days(days as i64))
    };
    if due(&format!("prune:{dest}"), job.retention.prune_every_days) {
        run.info(format!("Freeing unused data on '{name}' (prune)"));
        match restic.prune(repo).await {
            Ok(()) => agent.db.set_mark(&format!("prune:{dest}"), now),
            Err(e) => run.warn(format!("Prune on '{name}' failed: {e}")),
        }
    }
    if due(&format!("verify:{dest}"), job.verify.every_days) {
        run.info(format!(
            "Verifying '{name}' (reading back {}% of the data)",
            job.verify.read_data_percent
        ));
        match restic.check(repo, job.verify.read_data_percent).await {
            Ok(m) => {
                run.info(format!("  {m}"));
                agent.db.set_mark(&format!("verify:{dest}"), now);
            }
            Err(e) => {
                run.error(format!("Verification of '{name}' failed: {e}"));
                alert(
                    &agent,
                    Message {
                        event: Event::VerifyFailed,
                        server: agent.server_name(),
                        job_id: Some(job.id.clone()),
                        job_name: Some(job.name.clone()),
                        title: format!("Verification of '{name}' failed"),
                        body: e.0.clone(),
                        fields: vec![],
                    },
                );
                // Not a run failure by itself, but nobody should read "success" over a repository with errors.
                run.warn("The repository has errors; see the line above");
            }
        }
    }
}

// ---------------------------------------------------------------- one-off tasks on a destination

/// Verify, prune, copy to the PC, restore: things done once, on request, logged like a backup.
pub fn start_task<F, Fut>(agent: &Shared, label_id: &str, kind: RunKind, dest: String, f: F) -> Result<String>
where
    F: FnOnce(Shared, CancellationToken) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<String>> + Send,
{
    let mut run = Run::new(agent, label_id, kind, Trigger::Manual);
    let id = run.record.summary.id.clone();
    let agent = agent.clone();
    tokio::spawn(async move {
        let cancel = run.cancel.clone();
        if let Err(e) = claim_repo(&agent, &dest, &id_of(&run), &cancel).await {
            run.finish(RunStatus::Cancelled, Some(e.0));
            return;
        }
        run.step(&format!("{kind:?}").to_lowercase(), 1, 1);
        let res = f(agent.clone(), cancel).await;
        release_repo(&agent, &dest);
        match res {
            Ok(m) => {
                run.info(m);
                run.finish(RunStatus::Success, None);
            }
            Err(e) => {
                run.error(e.0.clone());
                run.finish(RunStatus::Failed, Some(e.0));
            }
        }
    });
    Ok(id)
}

fn id_of(run: &Run) -> String {
    run.record.summary.id.clone()
}

pub fn cancel(agent: &Shared, run_id: &str) -> bool {
    lock(&agent.running).get(run_id).map(|r| r.cancel.cancel()).is_some()
}
