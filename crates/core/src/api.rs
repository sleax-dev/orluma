//! What the desktop app and the agent say to each other, over the agent's socket (reached through SSH).
//!
//! Every endpoint is `POST /v1/<name>` with a JSON body and a JSON answer, except the event stream
//! (`GET /v1/events`, server-sent events) and file downloads (`GET /v1/dump`). Errors come back as a
//! non-2xx status with `{"error": "..."}`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const API_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentStatus {
    pub version: String,
    pub api: u32,
    pub hostname: String,
    pub name: String,
    pub os: String,
    pub arch: String,
    pub uptime_secs: u64,
    pub timezone: String,
    /// `None` until restic is installed (the agent installs it on first use).
    pub restic: Option<String>,
    /// The agent's own key, for SFTP destinations on other servers.
    pub ssh_public_key: String,
    pub disks: Vec<Disk>,
    pub running: Vec<RunProgress>,
    pub jobs: Vec<JobState>,
    pub update: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Disk {
    pub mount: String,
    pub total: u64,
    pub available: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct JobState {
    pub id: String,
    pub next_run: Option<DateTime<Utc>>,
    pub last_run: Option<RunSummary>,
    pub last_success: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    #[default]
    Backup,
    Verify,
    Prune,
    Copy,
    Restore,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    #[default]
    Running,
    Success,
    Warning,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    #[default]
    Schedule,
    Manual,
    CatchUp,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RunSummary {
    pub id: String,
    pub job_id: String,
    pub kind: RunKind,
    pub status: RunStatus,
    pub trigger: Trigger,
    pub started: DateTime<Utc>,
    pub finished: Option<DateTime<Utc>>,
    /// New data written to the repository, after deduplication and compression.
    pub bytes_added: u64,
    /// What the sources held.
    pub bytes_total: u64,
    pub files_new: u64,
    pub files_changed: u64,
    pub snapshots: Vec<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RunRecord {
    #[serde(flatten)]
    pub summary: RunSummary,
    pub log: Vec<LogLine>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogLine {
    pub at: DateTime<Utc>,
    pub level: Level,
    pub text: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RunProgress {
    pub run_id: String,
    pub job_id: String,
    pub kind: RunKind,
    /// The source or step being worked on.
    pub step: String,
    pub step_index: u32,
    pub step_count: u32,
    pub percent: f64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    pub seconds_remaining: u64,
    pub started: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AgentEvent {
    Progress(RunProgress),
    Finished(RunSummary),
    ConfigChanged,
}

// ---------------------------------------------------------------- requests

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct IdRequest {
    pub id: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RunsRequest {
    pub job_id: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StartedRun {
    pub run_id: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SnapshotsRequest {
    pub destination: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub short_id: String,
    pub time: DateTime<Utc>,
    pub hostname: String,
    pub paths: Vec<String>,
    pub tags: Vec<String>,
    pub job: Option<String>,
    pub source: Option<String>,
    pub bytes: u64,
    pub files: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LsRequest {
    pub destination: String,
    pub snapshot: String,
    pub path: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    pub path: String,
    /// file, dir or symlink
    pub kind: String,
    pub size: u64,
    pub mtime: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ForgetRequest {
    pub destination: String,
    pub snapshots: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RestoreRequest {
    pub destination: String,
    pub snapshot: String,
    /// Paths inside the snapshot; empty restores all of it.
    pub include: Vec<String>,
    /// Where on the server. Restoring over live files is refused unless `overwrite`.
    pub target: String,
    pub overwrite: bool,
}

/// Load a database dump out of a snapshot into a live database server.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RestoreDbRequest {
    pub destination: String,
    pub snapshot: String,
    /// postgres, mysql, sqlite, mongo or clickhouse: the `kind:` tag of the snapshot.
    pub kind: String,
    /// The dump inside the snapshot (`/postgres/app.dump`); for ClickHouse its folder (`/clickhouse/analytics`).
    pub file: String,
    /// How to reach the database server (not used for SQLite).
    pub conn: crate::model::DbConn,
    pub uri: String,
    /// Into this database; a SQLite file path. Empty means the original name.
    pub database: String,
    /// Allow overwriting what is there. Off refuses to touch an existing database.
    pub replace: bool,
}

/// Make a repository the agent does not keep in its config usable as a destination until the agent restarts
/// (a copy on the PC reached through a tunnel): the id then works wherever a destination id does.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AttachRequest {
    pub id: String,
    pub repository: String,
    pub password: String,
}

/// Copy snapshots to a repository the agent does not keep in its config (the PC's, through a tunnel).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CopyRequest {
    pub from: String,
    pub to_repository: String,
    pub to_password: String,
    /// Only this job's snapshots; empty copies everything.
    pub job_id: String,
    /// Create the repository when it does not exist yet.
    pub init: bool,
    /// Copy into one of the app's own destinations (another server's, say) instead of `to_repository`.
    pub to_destination: Option<crate::model::Destination>,
    /// Or into a destination in the agent's own config.
    pub to: String,
    /// Apply this retention to the job's snapshots in the target after copying (then prune).
    pub retention: Option<crate::model::Retention>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RepoStats {
    pub snapshots: u64,
    /// Bytes stored, after deduplication and compression.
    pub stored_bytes: u64,
    /// What the snapshots would take restored.
    pub restore_bytes: u64,
    pub compression_ratio: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TestResult {
    pub ok: bool,
    pub message: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Discovery {
    pub databases: Vec<FoundDatabase>,
    pub paths: Vec<FoundPath>,
    pub tools: Vec<Tool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FoundDatabase {
    /// postgres, mysql, mongo, redis, clickhouse
    pub kind: String,
    /// What it was found as: a container name, a systemd unit.
    pub label: String,
    pub container: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub os_user: String,
    pub databases: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FoundPath {
    pub path: String,
    pub bytes: u64,
    /// why it is suggested: config, home, web, data, docker-volume
    pub hint: String,
    /// the size is not measured yet: ask `size` for it (measuring a big folder takes long, so the list comes first)
    #[serde(default)]
    pub pending: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LogsRequest {
    pub lines: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PathRequest {
    pub path: String,
}

/// A folder to measure, without what the backup will skip anyway (`du --exclude` patterns, like restic's).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SizeRequest {
    pub path: String,
    #[serde(default)]
    pub excludes: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PathSize {
    pub bytes: u64,
    /// false when measuring took longer than the limit: the folder is too big to size quickly
    pub complete: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateRequest {
    /// Install this version; empty means the latest release.
    pub version: String,
}
