//! Starts jobs on their schedule, catches up on runs missed while the agent was down, warns about full disks
//! and looks for agent updates.

use crate::notify::Message;
use crate::runner;
use crate::state::Shared;
use chrono::{DateTime, Utc};
use orluma_core::api::{JobState, Trigger};
use orluma_core::model::Event;
use orluma_core::schedule;
use std::collections::HashMap;
use std::time::Duration;

pub async fn run(agent: Shared) {
    // job id -> (cron it was computed for, when it next fires)
    let mut next: HashMap<String, (String, DateTime<Utc>)> = HashMap::new();
    let mut first = true;
    let mut last_disk_check = None::<std::time::Instant>;
    let mut last_update_check = None::<std::time::Instant>;
    loop {
        let cfg = agent.config();
        let tz = cfg.agent.timezone.clone();
        let now = Utc::now();
        next.retain(|id, _| cfg.jobs.iter().any(|j| &j.id == id));
        for job in cfg
            .jobs
            .iter()
            .filter(|j| j.enabled && !j.schedule.cron.trim().is_empty())
        {
            let entry = next
                .get(&job.id)
                .filter(|(c, _)| c == &job.schedule.cron)
                .map(|(_, t)| *t);
            match entry {
                None => {
                    if let Some(t) = schedule::next_after(&job.schedule.cron, &tz, now) {
                        next.insert(job.id.clone(), (job.schedule.cron.clone(), t));
                    }
                    // Down when it was due: one run now instead of waiting for the next slot.
                    if first && job.schedule.catch_up {
                        let prev = schedule::previous_before(&job.schedule.cron, &tz, now);
                        let (last, _) = agent.db.last_runs(&job.id);
                        let missed = match (prev, last) {
                            (Some(p), Some(l)) => l.started < p,
                            (Some(_), None) => false, // a new job waits for its first slot
                            _ => false,
                        };
                        if missed && let Err(e) = runner::start_backup(&agent, &job.id, Trigger::CatchUp) {
                            crate::logs::error("schedule", format!("catch-up '{}': {e}", job.name));
                        }
                    }
                }
                Some(t) if now >= t => {
                    if let Err(e) = runner::start_backup(&agent, &job.id, Trigger::Schedule) {
                        crate::logs::error("schedule", format!("'{}': {e}", job.name));
                    }
                    if let Some(n) = schedule::next_after(&job.schedule.cron, &tz, now) {
                        next.insert(job.id.clone(), (job.schedule.cron.clone(), n));
                    }
                }
                Some(_) => {}
            }
        }
        first = false;

        if last_disk_check.is_none_or(|t| t.elapsed() > Duration::from_secs(600)) {
            last_disk_check = Some(std::time::Instant::now());
            check_disks(&agent, cfg.agent.disk_warn_percent);
        }
        if cfg.agent.auto_update && last_update_check.is_none_or(|t| t.elapsed() > Duration::from_secs(6 * 3600)) {
            last_update_check = Some(std::time::Instant::now());
            let a = agent.clone();
            tokio::spawn(async move { crate::update::auto(&a).await });
        }
        tokio::time::sleep(Duration::from_secs(15)).await;
    }
}

fn check_disks(agent: &Shared, warn_percent: u8) {
    if warn_percent == 0 {
        return;
    }
    for d in crate::sys::disks() {
        let used = 100 - (d.available * 100 / d.total.max(1)) as u8;
        let key = format!("disk:{}", d.mount);
        let recently = agent
            .db
            .mark(&key)
            .is_some_and(|t| Utc::now() - t < chrono::Duration::hours(24));
        if used >= warn_percent && !recently {
            agent.db.set_mark(&key, Utc::now());
            runner::alert(
                agent,
                Message {
                    event: Event::DiskLow,
                    server: agent.server_name(),
                    job_id: None,
                    job_name: None,
                    title: format!("Disk {} is {used}% full", d.mount),
                    body: format!("{} free of {}.", gb(d.available), gb(d.total)),
                    fields: vec![],
                },
            );
        }
    }
}

fn gb(b: u64) -> String {
    format!("{:.1} GB", b as f64 / 1e9)
}

/// Next run and last result of every job, for the status call.
pub fn job_states(agent: &Shared) -> Vec<JobState> {
    let cfg = agent.config();
    let now = Utc::now();
    cfg.jobs
        .iter()
        .map(|j| {
            let (last_run, last_success) = agent.db.last_runs(&j.id);
            let next_run = if j.enabled {
                schedule::next_after(&j.schedule.cron, &cfg.agent.timezone, now)
            } else {
                None
            };
            JobState {
                id: j.id.clone(),
                next_run,
                last_run,
                last_success,
            }
        })
        .collect()
}
