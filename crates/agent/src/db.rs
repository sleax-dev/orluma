//! Run history and a few dates the scheduler needs (last prune, last verify), in SQLite.

use chrono::{DateTime, Utc};
use orluma_core::api::{LogLine, RunKind, RunRecord, RunStatus, RunSummary, Trigger};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use std::sync::Mutex;

pub struct Db(Mutex<Connection>);

/// Runs kept per job; older ones go.
const KEEP_RUNS: i64 = 500;

fn enum_str<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn enum_from<T: serde::de::DeserializeOwned + Default>(s: &str) -> T {
    serde_json::from_value(serde_json::Value::String(s.into())).unwrap_or_default()
}

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let c = Connection::open(path)?;
        c.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS runs (
               id TEXT PRIMARY KEY, job_id TEXT NOT NULL, kind TEXT NOT NULL, status TEXT NOT NULL,
               trigger TEXT NOT NULL, started TEXT NOT NULL, finished TEXT,
               bytes_added INTEGER NOT NULL DEFAULT 0, bytes_total INTEGER NOT NULL DEFAULT 0,
               files_new INTEGER NOT NULL DEFAULT 0, files_changed INTEGER NOT NULL DEFAULT 0,
               snapshots TEXT NOT NULL DEFAULT '[]', error TEXT, log TEXT NOT NULL DEFAULT '[]');
             CREATE INDEX IF NOT EXISTS runs_job ON runs(job_id, started);
             CREATE TABLE IF NOT EXISTS marks (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )?;
        // A run that was going when the agent stopped did not finish.
        c.execute(
            "UPDATE runs SET status='failed', finished=started, error='The agent stopped while this was running' WHERE status='running'",
            [],
        )?;
        Ok(Self(Mutex::new(c)))
    }

    fn c(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn save_run(&self, r: &RunRecord) {
        let s = &r.summary;
        let res = self.c().execute(
            "INSERT INTO runs (id, job_id, kind, status, trigger, started, finished, bytes_added, bytes_total, files_new,
               files_changed, snapshots, error, log)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
             ON CONFLICT(id) DO UPDATE SET status=?4, finished=?7, bytes_added=?8, bytes_total=?9, files_new=?10,
               files_changed=?11, snapshots=?12, error=?13, log=?14",
            params![
                s.id,
                s.job_id,
                enum_str(&s.kind),
                enum_str(&s.status),
                enum_str(&s.trigger),
                s.started.to_rfc3339(),
                s.finished.map(|t| t.to_rfc3339()),
                s.bytes_added as i64,
                s.bytes_total as i64,
                s.files_new as i64,
                s.files_changed as i64,
                serde_json::to_string(&s.snapshots).unwrap_or_default(),
                s.error,
                serde_json::to_string(&r.log).unwrap_or_default(),
            ],
        );
        if let Err(e) = res {
            crate::logs::error("history", e);
        }
        if s.finished.is_some() {
            let _ = self.c().execute(
                "DELETE FROM runs WHERE job_id=?1 AND id NOT IN (SELECT id FROM runs WHERE job_id=?1 ORDER BY started DESC LIMIT ?2)",
                params![s.job_id, KEEP_RUNS],
            );
        }
    }

    fn row(r: &rusqlite::Row<'_>, with_log: bool) -> rusqlite::Result<RunRecord> {
        let time = |s: String| {
            DateTime::parse_from_rfc3339(&s)
                .map(|t| t.with_timezone(&Utc))
                .unwrap_or_default()
        };
        let snaps: String = r.get(11)?;
        let summary = RunSummary {
            id: r.get(0)?,
            job_id: r.get(1)?,
            kind: enum_from::<RunKind>(&r.get::<_, String>(2)?),
            status: enum_from::<RunStatus>(&r.get::<_, String>(3)?),
            trigger: enum_from::<Trigger>(&r.get::<_, String>(4)?),
            started: time(r.get(5)?),
            finished: r.get::<_, Option<String>>(6)?.map(time),
            bytes_added: r.get::<_, i64>(7)? as u64,
            bytes_total: r.get::<_, i64>(8)? as u64,
            files_new: r.get::<_, i64>(9)? as u64,
            files_changed: r.get::<_, i64>(10)? as u64,
            snapshots: serde_json::from_str(&snaps).unwrap_or_default(),
            error: r.get(12)?,
        };
        let log: Vec<LogLine> = if with_log {
            serde_json::from_str(&r.get::<_, String>(13)?).unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok(RunRecord { summary, log })
    }

    const COLS: &'static str = "id, job_id, kind, status, trigger, started, finished, bytes_added, bytes_total, files_new, files_changed, snapshots, error, log";

    pub fn runs(&self, job_id: Option<&str>, limit: u32) -> Vec<RunSummary> {
        let c = self.c();
        let sql = format!(
            "SELECT {} FROM runs WHERE (?1 IS NULL OR job_id=?1) ORDER BY started DESC LIMIT ?2",
            Self::COLS
        );
        let Ok(mut st) = c.prepare(&sql) else { return Vec::new() };
        st.query_map(params![job_id, limit], |r| Self::row(r, false))
            .map(|rows| rows.filter_map(Result::ok).map(|r| r.summary).collect())
            .unwrap_or_default()
    }

    pub fn run(&self, id: &str) -> Option<RunRecord> {
        let c = self.c();
        c.query_row(&format!("SELECT {} FROM runs WHERE id=?1", Self::COLS), [id], |r| {
            Self::row(r, true)
        })
        .optional()
        .ok()
        .flatten()
    }

    /// The latest finished backup of a job, and the latest that succeeded.
    pub fn last_runs(&self, job_id: &str) -> (Option<RunSummary>, Option<DateTime<Utc>>) {
        let c = self.c();
        let last = c
            .query_row(
                &format!(
                    "SELECT {} FROM runs WHERE job_id=?1 AND kind='backup' ORDER BY started DESC LIMIT 1",
                    Self::COLS
                ),
                [job_id],
                |r| Self::row(r, false),
            )
            .optional()
            .ok()
            .flatten()
            .map(|r| r.summary);
        let ok: Option<String> = c
            .query_row(
                "SELECT started FROM runs WHERE job_id=?1 AND kind='backup' AND status IN ('success','warning') ORDER BY started DESC LIMIT 1",
                [job_id],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        (
            last,
            ok.and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|t| t.with_timezone(&Utc)),
        )
    }

    pub fn mark(&self, key: &str) -> Option<DateTime<Utc>> {
        let v: Option<String> = self
            .c()
            .query_row("SELECT value FROM marks WHERE key=?1", [key], |r| r.get(0))
            .optional()
            .ok()
            .flatten();
        v.and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|t| t.with_timezone(&Utc))
    }

    pub fn set_mark(&self, key: &str, at: DateTime<Utc>) {
        let _ = self.c().execute(
            "INSERT INTO marks (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=?2",
            params![key, at.to_rfc3339()],
        );
    }
}
