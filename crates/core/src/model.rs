//! The agent's configuration: what to back up, where to, when, how long to keep it, and whom to tell.
//!
//! It lives on the server (`/etc/orluma/agent.json`, root-only) and is edited from the desktop app, which
//! reads it, changes it and writes it back whole. Every field has a default, so a config written by an
//! older version still loads.

use serde::{Deserialize, Serialize};

/// Bumped when a field changes meaning; new fields alone do not bump it.
pub const CONFIG_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    pub version: u32,
    pub agent: AgentSettings,
    pub destinations: Vec<Destination>,
    pub jobs: Vec<Job>,
    pub notifiers: Vec<Notifier>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentSettings {
    /// Shown in alerts and in the app. Empty means the host name.
    pub name: String,
    /// How many runs may go at once. One keeps a small server responsive.
    pub max_parallel: u8,
    /// Install signed agent updates from GitHub by itself.
    pub auto_update: bool,
    /// Alert when a disk is fuller than this.
    pub disk_warn_percent: u8,
    /// IANA zone the schedules are read in. Empty means the server's own.
    pub timezone: String,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            name: String::new(),
            max_parallel: 1,
            auto_update: true,
            disk_warn_percent: 90,
            timezone: String::new(),
        }
    }
}

// ---------------------------------------------------------------- destinations

/// A restic repository. Everything Orluma writes is encrypted with its password before it leaves the server.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Destination {
    pub id: String,
    pub name: String,
    pub kind: DestKind,
    /// The repository password. Lose it and the backups cannot be read by anyone, Orluma included.
    pub password: String,
    /// KiB/s, 0 = unlimited.
    #[serde(default)]
    pub limit_upload_kib: u32,
    #[serde(default)]
    pub limit_download_kib: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DestKind {
    /// A directory on this server (a second disk, a mounted share).
    Local { path: String },
    /// Another server over SSH. The agent logs in with its own key (`/etc/orluma/id_ed25519`).
    Sftp {
        host: String,
        #[serde(default = "ssh_port")]
        port: u16,
        user: String,
        path: String,
    },
    /// Any S3-compatible store: AWS, Cloudflare R2, Backblaze B2, Wasabi, MinIO, Hetzner, Scaleway, iDrive e2...
    S3 {
        #[serde(default)]
        provider: String,
        endpoint: String,
        bucket: String,
        #[serde(default)]
        path: String,
        #[serde(default)]
        region: String,
        access_key: String,
        secret_key: String,
    },
    /// Backblaze B2's own API.
    B2 {
        bucket: String,
        #[serde(default)]
        path: String,
        account_id: String,
        account_key: String,
    },
    Azure {
        account_name: String,
        account_key: String,
        container: String,
        #[serde(default)]
        path: String,
    },
    Gcs {
        project_id: String,
        bucket: String,
        #[serde(default)]
        path: String,
        /// The service account's JSON key, stored next to the config.
        credentials_json: String,
    },
    /// A restic REST server (rest-server, or Orluma's own on a PC).
    Rest {
        url: String,
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: String,
    },
    /// Anything rclone can reach (Google Drive, OneDrive, Dropbox, pCloud, Mega...). Needs rclone installed.
    Rclone {
        remote: String,
        path: String,
        /// Set when Orluma connected the account itself ("drive", "onedrive", "dropbox"): the remote then lives
        /// in Orluma's own rclone config (`/etc/orluma/rclone.conf`). Empty: a remote set up by hand.
        #[serde(default)]
        provider: String,
    },
}

fn ssh_port() -> u16 {
    22
}

// ---------------------------------------------------------------- jobs

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Job {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub sources: Vec<Source>,
    /// The destination every run writes to.
    pub destination: String,
    /// More destinations the new snapshots are copied to after a good run (3-2-1: another site, another provider).
    pub copies: Vec<String>,
    pub schedule: Schedule,
    pub retention: Retention,
    pub verify: Verify,
    pub hooks: Hooks,
    pub options: BackupOptions,
    /// Pinged on start (`/start`), success and failure (`/fail`): healthchecks.io, Uptime Kuma, Cronitor...
    pub healthcheck_url: String,
}

impl Default for Job {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            enabled: true,
            sources: Vec::new(),
            destination: String::new(),
            copies: Vec::new(),
            schedule: Schedule::default(),
            retention: Retention::default(),
            verify: Verify::default(),
            hooks: Hooks::default(),
            options: BackupOptions::default(),
            healthcheck_url: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Source {
    /// Files and folders: one snapshot for all of them.
    Paths {
        id: String,
        paths: Vec<String>,
        #[serde(default)]
        excludes: Vec<String>,
        /// Do not cross into other mounted filesystems.
        #[serde(default)]
        one_file_system: bool,
    },
    /// One snapshot per database (`pg_dump`), plus roles and tablespaces.
    Postgres {
        id: String,
        #[serde(flatten)]
        conn: DbConn,
        /// Empty means every database.
        #[serde(default)]
        databases: Vec<String>,
        #[serde(default = "yes")]
        globals: bool,
    },
    /// MySQL or MariaDB (`mysqldump --single-transaction`), one snapshot per database.
    Mysql {
        id: String,
        #[serde(flatten)]
        conn: DbConn,
        #[serde(default)]
        databases: Vec<String>,
    },
    /// `mongodump --archive`: one snapshot for the whole server.
    Mongo {
        id: String,
        /// mongodb://user:pass@host:port/?authSource=admin
        uri: String,
        #[serde(default)]
        container: String,
    },
    /// The RDB file (`redis-cli --rdb`).
    Redis {
        id: String,
        #[serde(flatten)]
        conn: DbConn,
    },
    /// `sqlite3 .dump`, consistent while the app keeps writing.
    Sqlite { id: String, path: String },
    /// Each table as ClickHouse Native, one snapshot per table.
    Clickhouse {
        id: String,
        #[serde(flatten)]
        conn: DbConn,
        #[serde(default)]
        databases: Vec<String>,
    },
    /// Whatever a command prints, stored as one file.
    Command {
        id: String,
        command: String,
        filename: String,
    },
}

impl Source {
    pub fn id(&self) -> &str {
        match self {
            Source::Paths { id, .. }
            | Source::Postgres { id, .. }
            | Source::Mysql { id, .. }
            | Source::Mongo { id, .. }
            | Source::Redis { id, .. }
            | Source::Sqlite { id, .. }
            | Source::Clickhouse { id, .. }
            | Source::Command { id, .. } => id,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Source::Paths { .. } => "paths",
            Source::Postgres { .. } => "postgres",
            Source::Mysql { .. } => "mysql",
            Source::Mongo { .. } => "mongo",
            Source::Redis { .. } => "redis",
            Source::Sqlite { .. } => "sqlite",
            Source::Clickhouse { .. } => "clickhouse",
            Source::Command { .. } => "command",
        }
    }
}

fn yes() -> bool {
    true
}

/// How to reach a database server: over TCP, or inside a Docker container, or as the local OS user
/// (Postgres peer authentication).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DbConn {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    /// Run the dump tool inside this container (`docker exec`).
    pub container: String,
    /// Run the dump tool as this OS user instead of logging in with a password (`postgres` for peer auth).
    pub os_user: String,
}

/// When a job runs. No cron means only by hand.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Schedule {
    /// Five fields (`0 3 * * *`) or six with seconds first.
    pub cron: String,
    /// Run once at start when a scheduled time passed while the agent was down.
    pub catch_up: bool,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            cron: "0 3 * * *".into(),
            catch_up: true,
        }
    }
}

/// Which snapshots survive (`restic forget --keep-*`). Zero means "not this rule"; all zero keeps everything.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Retention {
    pub last: u32,
    pub hourly: u32,
    pub daily: u32,
    pub weekly: u32,
    pub monthly: u32,
    pub yearly: u32,
    /// Free unused data this often (prune is the slow part), in days. 0 = never.
    pub prune_every_days: u32,
}

impl Default for Retention {
    /// Grandfather-father-son: a week of dailies, a month of weeklies, a year of monthlies.
    fn default() -> Self {
        Self {
            last: 3,
            hourly: 0,
            daily: 7,
            weekly: 4,
            monthly: 12,
            yearly: 2,
            prune_every_days: 7,
        }
    }
}

impl Retention {
    pub fn keeps_everything(&self) -> bool {
        [
            self.last,
            self.hourly,
            self.daily,
            self.weekly,
            self.monthly,
            self.yearly,
        ]
        .iter()
        .all(|n| *n == 0)
    }
}

/// Reading back what was written: the only way to know a backup can be restored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Verify {
    /// `restic check` this often, in days. 0 = never.
    pub every_days: u32,
    /// Percentage of the data to download and decrypt on each check (0 checks only the structure).
    pub read_data_percent: u8,
}

impl Default for Verify {
    fn default() -> Self {
        Self {
            every_days: 7,
            read_data_percent: 5,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hooks {
    /// Before the backup; a failing one stops the run unless `continue_on_error`.
    pub before: Vec<Hook>,
    /// After the backup, whatever happened. `ORLUMA_STATUS` is `success`, `warning` or `failed`.
    pub after: Vec<Hook>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hook {
    pub command: String,
    pub timeout_secs: u32,
    pub continue_on_error: bool,
}

impl Default for Hook {
    fn default() -> Self {
        Self {
            command: String::new(),
            timeout_secs: 600,
            continue_on_error: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupOptions {
    /// restic's compression: auto, max or off.
    pub compression: String,
    /// Skip folders that hold a CACHEDIR.TAG.
    pub exclude_caches: bool,
    /// Skip single files bigger than this, in MiB. 0 = no limit.
    pub exclude_larger_than_mib: u32,
    /// Give the backup the lowest CPU and disk priority (nice/ionice).
    pub low_priority: bool,
}

impl Default for BackupOptions {
    fn default() -> Self {
        Self {
            compression: "auto".into(),
            exclude_caches: true,
            exclude_larger_than_mib: 0,
            low_priority: true,
        }
    }
}

// ---------------------------------------------------------------- alerts

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Notifier {
    pub id: String,
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub kind: NotifierKind,
    /// What it hears about. Empty means failures and warnings only.
    #[serde(default)]
    pub events: Vec<Event>,
    /// Only these jobs. Empty means all of them.
    #[serde(default)]
    pub jobs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotifierKind {
    /// JSON POST. With a secret, `X-Orluma-Signature: sha256=<hex HMAC of the body>`.
    Webhook {
        url: String,
        #[serde(default)]
        secret: String,
    },
    Discord {
        webhook_url: String,
    },
    Slack {
        webhook_url: String,
    },
    Telegram {
        bot_token: String,
        chat_id: String,
    },
    /// ntfy.sh or self-hosted.
    Ntfy {
        #[serde(default = "ntfy_server")]
        server: String,
        topic: String,
        #[serde(default)]
        token: String,
    },
    Gotify {
        server: String,
        token: String,
    },
    Pushover {
        user_key: String,
        app_token: String,
    },
    /// Microsoft Teams (an incoming webhook / Workflows URL).
    Teams {
        webhook_url: String,
    },
    Email {
        smtp_host: String,
        #[serde(default = "smtp_port")]
        smtp_port: u16,
        /// starttls, tls or none.
        #[serde(default = "starttls")]
        security: String,
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: String,
        from: String,
        to: Vec<String>,
    },
}

fn ntfy_server() -> String {
    "https://ntfy.sh".into()
}
fn smtp_port() -> u16 {
    587
}
fn starttls() -> String {
    "starttls".into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Event {
    RunSucceeded,
    RunWarning,
    RunFailed,
    VerifyFailed,
    DiskLow,
    AgentUpdated,
}

impl Event {
    pub const DEFAULT: [Event; 4] = [Event::RunWarning, Event::RunFailed, Event::VerifyFailed, Event::DiskLow];
}

impl AgentConfig {
    /// Everything that would make a run fail before it starts, in one list.
    pub fn validate(&self) -> Vec<String> {
        let mut errs = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for d in &self.destinations {
            if d.id.is_empty() || !seen.insert(format!("d:{}", d.id)) {
                errs.push(format!("destination '{}' needs a unique id", d.name));
            }
            if d.password.len() < 8 {
                errs.push(format!(
                    "destination '{}': the password needs at least 8 characters",
                    d.name
                ));
            }
        }
        for j in &self.jobs {
            if j.id.is_empty() || !seen.insert(format!("j:{}", j.id)) {
                errs.push(format!("job '{}' needs a unique id", j.name));
            }
            if !self.destinations.iter().any(|d| d.id == j.destination) {
                errs.push(format!("job '{}': pick a destination", j.name));
            }
            for c in &j.copies {
                if c == &j.destination || !self.destinations.iter().any(|d| &d.id == c) {
                    errs.push(format!(
                        "job '{}': copy target '{c}' is not another destination",
                        j.name
                    ));
                }
            }
            if j.sources.is_empty() {
                errs.push(format!("job '{}': add something to back up", j.name));
            }
            let mut ids = std::collections::HashSet::new();
            for s in &j.sources {
                if s.id().is_empty() || !ids.insert(s.id()) {
                    errs.push(format!("job '{}': every source needs a unique id", j.name));
                }
            }
            if !j.schedule.cron.trim().is_empty() && crate::schedule::parse(&j.schedule.cron).is_err() {
                errs.push(format!(
                    "job '{}': the schedule '{}' is not a valid cron expression",
                    j.name, j.schedule.cron
                ));
            }
        }
        if !self.agent.timezone.is_empty() && self.agent.timezone.parse::<chrono_tz::Tz>().is_err() {
            errs.push(format!("unknown time zone '{}'", self.agent.timezone));
        }
        errs
    }
}
