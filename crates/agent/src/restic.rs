//! restic does the storing: chunking, deduplication, compression and encryption, in a format with years of
//! use behind it. This module turns a destination into a repository restic can open, installs a verified
//! restic when the server has none, and runs it, reading its JSON output.

use crate::paths::{Paths, write_private};
use anyhow_lite::{Error, Result, bail};
use orluma_core::api::{Node, RepoStats, Snapshot};
use orluma_core::model::{BackupOptions, DestKind, Destination, Retention};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

/// The restic this agent installs when the server has none (or one too old for `--stdin-from-command`).
pub const RESTIC_VERSION: &str = "0.19.1";
const RESTIC_SHA256: &[(&str, &str)] = &[
    (
        "amd64",
        "f415415624dcc452f2a02b8c33641791a8c6d6d3b65bbb3543fcf9a25151585c",
    ),
    (
        "arm64",
        "a5f64aaab53d51e311fa3829124c5b703f2d14cf187d8640b6be3b2b49376465",
    ),
];
const MIN_SYSTEM_RESTIC: (u32, u32) = (0, 17);

pub mod anyhow_lite {
    /// A plain string error: everything here ends up as a line a person reads.
    #[derive(Debug, Clone)]
    pub struct Error(pub String);
    impl std::fmt::Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(&self.0)
        }
    }
    impl<E: std::error::Error> From<E> for Error {
        fn from(e: E) -> Self {
            Error(e.to_string())
        }
    }
    pub type Result<T> = std::result::Result<T, Error>;
    macro_rules! bail {
        ($($t:tt)*) => { return Err($crate::restic::anyhow_lite::Error(format!($($t)*))) };
    }
    pub(crate) use bail;
}

// ---------------------------------------------------------------- the binary

fn parse_version(out: &str) -> Option<(u32, u32, u32)> {
    let v = out.split_whitespace().nth(1)?;
    let mut it = v.split('.').map(|n| n.trim_start_matches('v').parse::<u32>().ok());
    Some((it.next()??, it.next()??, it.next().flatten().unwrap_or(0)))
}

async fn version_of(bin: &std::path::Path) -> Option<(u32, u32, u32)> {
    let out = Command::new(bin)
        .arg("version")
        .kill_on_drop(true)
        .output()
        .await
        .ok()?;
    parse_version(&String::from_utf8_lossy(&out.stdout))
}

/// A restic new enough to use: the server's own, else the one this agent installed, else a fresh download
/// checked against the hash pinned above.
pub async fn ensure(paths: &Paths, http: &reqwest::Client) -> Result<PathBuf> {
    let ours = paths.restic_bin();
    if let Some(v) = version_of(&ours).await
        && (v.0, v.1) >= MIN_SYSTEM_RESTIC
    {
        return Ok(ours);
    }
    for dir in std::env::var("PATH").unwrap_or_default().split(':') {
        let p = PathBuf::from(dir).join("restic");
        if let Some(v) = version_of(&p).await
            && (v.0, v.1) >= MIN_SYSTEM_RESTIC
        {
            return Ok(p);
        }
    }
    install(paths, http).await?;
    Ok(ours)
}

pub async fn installed_version(paths: &Paths) -> Option<String> {
    let mut candidates = vec![paths.restic_bin()];
    candidates.extend(
        std::env::var("PATH")
            .unwrap_or_default()
            .split(':')
            .map(|d| PathBuf::from(d).join("restic")),
    );
    for p in candidates {
        if let Some((a, b, c)) = version_of(&p).await {
            return Some(format!("{a}.{b}.{c}"));
        }
    }
    None
}

async fn install(paths: &Paths, http: &reqwest::Client) -> Result<()> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        a => bail!("restic is not installed and there is no build of it for {a}; install restic 0.17 or newer"),
    };
    let want = RESTIC_SHA256
        .iter()
        .find(|(a, _)| *a == arch)
        .map(|(_, h)| *h)
        .unwrap_or_default();
    let url = format!(
        "https://github.com/restic/restic/releases/download/v{RESTIC_VERSION}/restic_{RESTIC_VERSION}_linux_{arch}.bz2"
    );
    let bytes = http.get(&url).send().await?.error_for_status()?.bytes().await?;
    use sha2::Digest;
    let got = hex::encode(sha2::Sha256::digest(&bytes));
    if got != want {
        bail!("the restic download did not match its published checksum (got {got})");
    }
    let mut plain = Vec::new();
    std::io::Read::read_to_end(&mut bzip2::read::BzDecoder::new(&bytes[..]), &mut plain)?;
    let bin = paths.restic_bin();
    std::fs::create_dir_all(bin.parent().unwrap_or(&paths.data))?;
    write_private(&bin, &plain)?;
    crate::paths::set_mode(&bin, 0o700)?;
    Ok(())
}

// ---------------------------------------------------------------- repositories

/// What restic needs to open one repository.
#[derive(Clone, Debug)]
pub struct Repo {
    pub url: String,
    pub password: String,
    pub env: Vec<(String, String)>,
    pub options: Vec<String>,
}

fn join_path(base: &str, path: &str) -> String {
    let p = path.trim_matches('/');
    if p.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{p}")
    }
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

impl Repo {
    pub fn of(dest: &Destination, paths: &Paths) -> Result<Repo> {
        let mut env = Vec::new();
        let mut options = Vec::new();
        let url = match &dest.kind {
            DestKind::Local { path } => path.clone(),
            DestKind::Sftp { host, port, user, path } => {
                let ssh = format!(
                    "ssh -i {} -p {port} -o BatchMode=yes -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile={} -o ServerAliveInterval=60 -o ServerAliveCountMax=4 {user}@{host} -s sftp",
                    paths.ssh_key().display(),
                    paths.known_hosts().display()
                );
                options.push(format!("sftp.command={ssh}"));
                format!("sftp:{user}@{host}:{path}")
            }
            DestKind::S3 {
                endpoint,
                bucket,
                path,
                region,
                access_key,
                secret_key,
                provider,
            } => {
                env.push(("AWS_ACCESS_KEY_ID".into(), access_key.clone()));
                env.push(("AWS_SECRET_ACCESS_KEY".into(), secret_key.clone()));
                let region = if region.is_empty() && provider == "r2" {
                    "auto"
                } else {
                    region
                };
                if !region.is_empty() {
                    env.push(("AWS_DEFAULT_REGION".into(), region.into()));
                    options.push(format!("s3.region={region}"));
                }
                let ep = if endpoint.is_empty() {
                    "https://s3.amazonaws.com".to_string()
                } else if endpoint.contains("://") {
                    endpoint.trim_end_matches('/').to_string()
                } else {
                    format!("https://{}", endpoint.trim_end_matches('/'))
                };
                format!("s3:{}", join_path(&format!("{ep}/{bucket}"), path))
            }
            DestKind::B2 {
                bucket,
                path,
                account_id,
                account_key,
            } => {
                env.push(("B2_ACCOUNT_ID".into(), account_id.clone()));
                env.push(("B2_ACCOUNT_KEY".into(), account_key.clone()));
                format!("b2:{bucket}:/{}", path.trim_matches('/'))
            }
            DestKind::Azure {
                account_name,
                account_key,
                container,
                path,
            } => {
                env.push(("AZURE_ACCOUNT_NAME".into(), account_name.clone()));
                env.push(("AZURE_ACCOUNT_KEY".into(), account_key.clone()));
                format!("azure:{container}:/{}", path.trim_matches('/'))
            }
            DestKind::Gcs {
                project_id,
                bucket,
                path,
                credentials_json,
            } => {
                let file = paths.secret_file(&format!("gcs-{}.json", dest.id));
                write_private(&file, credentials_json.as_bytes())?;
                env.push(("GOOGLE_PROJECT_ID".into(), project_id.clone()));
                env.push(("GOOGLE_APPLICATION_CREDENTIALS".into(), file.display().to_string()));
                format!("gs:{bucket}:/{}", path.trim_matches('/'))
            }
            DestKind::Rest {
                url,
                username,
                password,
            } => {
                if username.is_empty() {
                    format!("rest:{url}")
                } else {
                    let (scheme, rest) = url.split_once("://").unwrap_or(("https", url.as_str()));
                    format!("rest:{scheme}://{}:{}@{rest}", urlencode(username), urlencode(password))
                }
            }
            DestKind::Rclone { remote, path, provider } => {
                // Remotes Orluma connected live in its own config; ones made by hand stay in rclone's default
                if !provider.is_empty() {
                    env.push(("RCLONE_CONFIG".into(), paths.rclone_conf().display().to_string()));
                    let ours = paths.rclone_bin();
                    if ours.is_file() {
                        options.push(format!("rclone.program={}", ours.display()));
                    }
                }
                format!("rclone:{remote}:{}", path.trim_start_matches('/'))
            }
        };
        if dest.limit_upload_kib > 0 {
            options.push(format!("--limit-upload={}", dest.limit_upload_kib));
        }
        if dest.limit_download_kib > 0 {
            options.push(format!("--limit-download={}", dest.limit_download_kib));
        }
        Ok(Repo {
            url,
            password: dest.password.clone(),
            env,
            options,
        })
    }

    /// A repository that is not in the config: the PC's, reached through a tunnel for one copy.
    pub fn raw(url: &str, password: &str) -> Repo {
        Repo {
            url: url.into(),
            password: password.into(),
            env: Vec::new(),
            options: Vec::new(),
        }
    }

    fn args(&self) -> Vec<String> {
        let mut a = Vec::new();
        for o in &self.options {
            if o.starts_with("--") {
                a.push(o.clone());
            } else {
                a.push("-o".into());
                a.push(o.clone());
            }
        }
        a
    }
}

// ---------------------------------------------------------------- running restic

#[derive(Clone)]
pub struct Restic {
    pub bin: PathBuf,
    pub cache: PathBuf,
}

/// One line restic printed, kept for the run log.
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

fn err_text(o: &Output) -> String {
    let e = o.stderr.trim();
    let line = e.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or(e);
    if line.is_empty() {
        format!("restic exited with code {}", o.code)
    } else if line.contains("wrong password or no key found") {
        // restic's wording blames the password but not where it comes from: usually two destinations share a folder
        format!(
            "{line}. This backup location holds a repository made with a different password (another destination using the same folder?)"
        )
    } else {
        line.to_string()
    }
}

impl Restic {
    pub fn command(&self, repo: &Repo, args: &[&str]) -> Command {
        let mut c = Command::new(&self.bin);
        c.args(repo.args())
            .arg("--cache-dir")
            .arg(&self.cache)
            .args(args)
            .env("RESTIC_REPOSITORY", &repo.url)
            .env("RESTIC_PASSWORD", &repo.password)
            .env("RESTIC_PROGRESS_FPS", "0.5")
            .envs(repo.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        c
    }

    pub async fn run(&self, repo: &Repo, args: &[&str]) -> Result<Output> {
        let o = self.command(repo, args).output().await?;
        Ok(Output {
            code: o.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        })
    }

    async fn ok(&self, repo: &Repo, args: &[&str]) -> Result<Output> {
        let o = self.run(repo, args).await?;
        if o.code != 0 {
            return Err(Error(err_text(&o)));
        }
        Ok(o)
    }

    /// Exit code 10 means "no repository there" (restic 0.17+).
    pub async fn exists(&self, repo: &Repo) -> Result<bool> {
        let o = self.run(repo, &["cat", "config", "--no-lock"]).await?;
        match o.code {
            0 => Ok(true),
            10 => Ok(false),
            _ if o.stderr.contains("Is there a repository at the following location")
                || o.stderr.contains("does not exist") =>
            {
                Ok(false)
            }
            _ => Err(Error(err_text(&o))),
        }
    }

    pub async fn init(&self, repo: &Repo) -> Result<()> {
        if self.exists(repo).await? {
            return Ok(());
        }
        self.ok(repo, &["init"]).await.map(|_| ())
    }

    pub async fn unlock(&self, repo: &Repo) -> Result<()> {
        self.ok(repo, &["unlock"]).await.map(|_| ())
    }

    pub async fn snapshots(&self, repo: &Repo, tag: Option<&str>) -> Result<Vec<Snapshot>> {
        let mut args = vec!["snapshots", "--json", "--no-lock"];
        if let Some(t) = tag {
            args.extend(["--tag", t]);
        }
        let o = self.ok(repo, &args).await?;
        #[derive(Deserialize)]
        struct Raw {
            id: String,
            short_id: Option<String>,
            time: chrono::DateTime<chrono::Utc>,
            hostname: String,
            #[serde(default)]
            paths: Vec<String>,
            #[serde(default)]
            tags: Vec<String>,
            summary: Option<RawSummary>,
        }
        #[derive(Deserialize)]
        struct RawSummary {
            #[serde(default)]
            total_bytes_processed: u64,
            #[serde(default)]
            total_files_processed: u64,
        }
        let raw: Vec<Raw> = serde_json::from_str(o.stdout.trim()).unwrap_or_default();
        let tag_val = |tags: &[String], k: &str| tags.iter().find_map(|t| t.strip_prefix(k).map(str::to_string));
        let mut out: Vec<Snapshot> = raw
            .into_iter()
            .map(|r| Snapshot {
                short_id: r.short_id.unwrap_or_else(|| r.id.chars().take(8).collect()),
                job: tag_val(&r.tags, "job:"),
                source: tag_val(&r.tags, "source:"),
                bytes: r.summary.as_ref().map_or(0, |s| s.total_bytes_processed),
                files: r.summary.as_ref().map_or(0, |s| s.total_files_processed),
                id: r.id,
                time: r.time,
                hostname: r.hostname,
                paths: r.paths,
                tags: r.tags,
            })
            .collect();
        out.sort_by_key(|s| std::cmp::Reverse(s.time));
        Ok(out)
    }

    /// The direct children of `path` in a snapshot.
    pub async fn ls(&self, repo: &Repo, snapshot: &str, path: &str) -> Result<Vec<Node>> {
        let dir = if path.is_empty() { "/" } else { path };
        let o = self.ok(repo, &["ls", "--json", "--no-lock", snapshot, dir]).await?;
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            struct_type: String,
            #[serde(default)]
            name: String,
            #[serde(default)]
            path: String,
            #[serde(default, rename = "type")]
            kind: String,
            #[serde(default)]
            size: u64,
            mtime: Option<chrono::DateTime<chrono::Utc>>,
        }
        let parent = dir.trim_end_matches('/');
        let mut nodes: Vec<Node> = o
            .stdout
            .lines()
            .filter_map(|l| serde_json::from_str::<Raw>(l).ok())
            .filter(|r| r.struct_type == "node" || (r.struct_type.is_empty() && !r.path.is_empty()))
            .filter(|r| {
                let p = r.path.trim_end_matches('/');
                p != parent && p.rsplit_once('/').map_or("", |(a, _)| a) == parent
            })
            .map(|r| Node {
                name: r.name,
                path: r.path,
                kind: r.kind,
                size: r.size,
                mtime: r.mtime,
            })
            .collect();
        nodes.sort_by(|a, b| {
            (a.kind != "dir")
                .cmp(&(b.kind != "dir"))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(nodes)
    }

    pub async fn forget_ids(&self, repo: &Repo, ids: &[String]) -> Result<()> {
        let mut args = vec!["forget"];
        args.extend(ids.iter().map(String::as_str));
        self.ok(repo, &args).await.map(|_| ())
    }

    /// Applies the retention to one job's snapshots, grouped per host and path (each database is its own group).
    pub async fn forget(&self, repo: &Repo, job_id: &str, r: &Retention) -> Result<String> {
        if r.keeps_everything() {
            return Ok("Retention keeps every snapshot".into());
        }
        let tag = format!("job:{job_id}");
        let mut args: Vec<String> = vec![
            "forget".into(),
            "--tag".into(),
            tag,
            "--group-by".into(),
            "host,paths".into(),
        ];
        for (flag, n) in [
            ("--keep-last", r.last),
            ("--keep-hourly", r.hourly),
            ("--keep-daily", r.daily),
            ("--keep-weekly", r.weekly),
            ("--keep-monthly", r.monthly),
            ("--keep-yearly", r.yearly),
        ] {
            if n > 0 {
                args.push(flag.into());
                args.push(n.to_string());
            }
        }
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        let o = self.ok(repo, &a).await?;
        // "remove 2 snapshots:" once per group
        let removed: u64 = o
            .stdout
            .lines()
            .filter_map(|l| {
                l.strip_prefix("remove ")
                    .and_then(|r| r.split_whitespace().next())
                    .and_then(|n| n.parse::<u64>().ok())
            })
            .sum();
        Ok(if removed == 0 {
            "no snapshot was due to go".into()
        } else {
            format!("{removed} old snapshot(s) removed")
        })
    }

    pub async fn prune(&self, repo: &Repo) -> Result<()> {
        self.ok(repo, &["prune"]).await.map(|_| ())
    }

    pub async fn check(&self, repo: &Repo, read_percent: u8) -> Result<String> {
        let subset = format!("--read-data-subset={}%", read_percent.min(100));
        let mut args = vec!["check"];
        if read_percent > 0 {
            args.push(&subset);
        }
        let o = self.run(repo, &args).await?;
        if o.code != 0 {
            let detail = o
                .stdout
                .lines()
                .chain(o.stderr.lines())
                .filter(|l| l.contains("error") || l.contains("Fatal"))
                .take(5)
                .collect::<Vec<_>>()
                .join(" | ");
            bail!("{}", if detail.is_empty() { err_text(&o) } else { detail });
        }
        Ok("No errors were found".into())
    }

    /// Copies snapshots from `from` into `to`, skipping those already there.
    pub async fn copy(&self, from: &Repo, to: &Repo, tag: Option<&str>) -> Result<String> {
        let mut args = vec!["copy", "--from-repo", from.url.as_str()];
        if let Some(t) = tag {
            args.extend(["--tag", t]);
        }
        let mut c = self.command(to, &args);
        c.env("RESTIC_FROM_PASSWORD", &from.password)
            .envs(from.env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        for o in &from.options {
            if !o.starts_with("--") {
                c.arg("-o").arg(o);
            }
        }
        let o = c.output().await?;
        if !o.status.success() {
            bail!(
                "{}",
                String::from_utf8_lossy(&o.stderr)
                    .lines()
                    .last()
                    .unwrap_or("copy failed")
            );
        }
        let copied = String::from_utf8_lossy(&o.stdout).matches("copy started").count();
        Ok(format!("{copied} snapshot(s) copied"))
    }

    pub async fn stats(&self, repo: &Repo) -> Result<RepoStats> {
        #[derive(Deserialize, Default)]
        struct Raw {
            #[serde(default)]
            total_size: u64,
            #[serde(default)]
            snapshots_count: u64,
            #[serde(default)]
            compression_ratio: f64,
        }
        let raw: Raw = serde_json::from_str(
            self.ok(repo, &["stats", "--json", "--no-lock", "--mode", "raw-data"])
                .await?
                .stdout
                .trim(),
        )
        .unwrap_or_default();
        let restore: Raw = serde_json::from_str(
            self.ok(repo, &["stats", "--json", "--no-lock", "--mode", "restore-size"])
                .await?
                .stdout
                .trim(),
        )
        .unwrap_or_default();
        Ok(RepoStats {
            snapshots: raw.snapshots_count,
            stored_bytes: raw.total_size,
            restore_bytes: restore.total_size,
            compression_ratio: raw.compression_ratio,
        })
    }

    pub async fn restore(&self, repo: &Repo, snapshot: &str, include: &[String], target: &str) -> Result<String> {
        let mut args = vec!["restore", snapshot, "--target", target];
        for i in include {
            args.extend(["--include", i.as_str()]);
        }
        let o = self.ok(repo, &args).await?;
        Ok(o.stdout.lines().last().unwrap_or("restored").to_string())
    }
}

// ---------------------------------------------------------------- backups, with progress

#[derive(Clone, Debug, Default)]
pub struct Progress {
    pub percent: f64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    pub seconds_remaining: u64,
}

#[derive(Clone, Debug, Default)]
pub struct BackupResult {
    pub snapshot: Option<String>,
    pub bytes_added: u64,
    pub bytes_total: u64,
    pub files_new: u64,
    pub files_changed: u64,
    /// Files that could not be read: the snapshot exists but lacks them.
    pub errors: Vec<String>,
}

pub enum What<'a> {
    Paths {
        paths: &'a [String],
        excludes: &'a [String],
        one_file_system: bool,
    },
    Stdin {
        argv: &'a [String],
        env: &'a [(String, String)],
        filename: &'a str,
    },
}

impl Restic {
    #[allow(clippy::too_many_arguments)]
    pub async fn backup(
        &self,
        repo: &Repo,
        what: What<'_>,
        tags: &[String],
        host: &str,
        opts: &BackupOptions,
        cancel: &CancellationToken,
        mut progress: impl FnMut(Progress),
    ) -> Result<BackupResult> {
        let mut args: Vec<String> = vec!["backup".into(), "--json".into(), "--host".into(), host.into()];
        for t in tags {
            args.push("--tag".into());
            args.push(t.clone());
        }
        if !opts.compression.is_empty() {
            args.push(format!("--compression={}", opts.compression));
        }
        let mut stdin_env: &[(String, String)] = &[];
        match &what {
            What::Paths {
                paths,
                excludes,
                one_file_system,
            } => {
                if opts.exclude_caches {
                    // CACHEDIR.TAG (cargo, many tools), plus folders that say what they are by a marker file:
                    // a Python virtualenv of any name has pyvenv.cfg, and .nobackup is the usual "skip me" flag
                    args.push("--exclude-caches".into());
                    for marker in ["pyvenv.cfg", ".nobackup"] {
                        args.push(format!("--exclude-if-present={marker}"));
                    }
                }
                if opts.exclude_larger_than_mib > 0 {
                    args.push(format!("--exclude-larger-than={}M", opts.exclude_larger_than_mib));
                }
                if *one_file_system {
                    args.push("--one-file-system".into());
                }
                for e in *excludes {
                    args.push("--exclude".into());
                    args.push(e.clone());
                }
                args.push("--".into());
                args.extend(paths.iter().cloned());
            }
            What::Stdin { argv, env, filename } => {
                args.push("--stdin-filename".into());
                args.push(filename.to_string());
                args.push("--stdin-from-command".into());
                args.push("--".into());
                args.extend(argv.iter().cloned());
                stdin_env = env;
            }
        }
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        let mut cmd = if opts.low_priority {
            let mut c = Command::new("nice");
            let base = self.command(repo, &a);
            let std = base.as_std();
            c.arg("-n").arg("19");
            if which("ionice") {
                c = Command::new("ionice");
                c.args(["-c", "3", "nice", "-n", "19"]);
            }
            c.arg(std.get_program()).args(std.get_args());
            for (k, v) in std.get_envs() {
                if let Some(v) = v {
                    c.env(k, v);
                }
            }
            c.stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            c
        } else {
            self.command(repo, &a)
        };
        cmd.envs(stdin_env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().ok_or_else(|| Error("no stdout".into()))?;
        let stderr = child.stderr.take().ok_or_else(|| Error("no stderr".into()))?;
        let err_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            let mut keep = Vec::new();
            while let Ok(Some(l)) = lines.next_line().await {
                if keep.len() < 50 {
                    keep.push(l);
                }
            }
            keep
        });
        let mut out = BackupResult::default();
        let mut lines = BufReader::new(stdout).lines();
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    let _ = child.kill().await;
                    bail!("Cancelled");
                }
                line = lines.next_line() => {
                    let Ok(Some(line)) = line else { break };
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
                    let n = |k: &str| v.get(k).and_then(serde_json::Value::as_u64).unwrap_or(0);
                    match v.get("message_type").and_then(|m| m.as_str()) {
                        Some("status") => progress(Progress {
                            percent: v.get("percent_done").and_then(serde_json::Value::as_f64).unwrap_or(0.0),
                            bytes_done: n("bytes_done"),
                            bytes_total: n("total_bytes"),
                            files_done: n("files_done"),
                            files_total: n("total_files"),
                            seconds_remaining: n("seconds_remaining"),
                        }),
                        Some("summary") => {
                            out.snapshot = v.get("snapshot_id").and_then(|s| s.as_str()).map(str::to_string);
                            out.bytes_added = n("data_added_packed").max(if n("data_added_packed") == 0 { n("data_added") } else { 0 });
                            out.bytes_total = n("total_bytes_processed");
                            out.files_new = n("files_new");
                            out.files_changed = n("files_changed");
                        }
                        Some("error") => {
                            let msg = v.pointer("/error/message").and_then(|m| m.as_str()).unwrap_or("error");
                            let item = v.get("item").and_then(|m| m.as_str()).unwrap_or("");
                            if out.errors.len() < 20 {
                                out.errors.push(if item.is_empty() { msg.to_string() } else { format!("{item}: {msg}") });
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        let status = child.wait().await?;
        let stderr = err_task.await.unwrap_or_default();
        match status.code() {
            Some(0) => Ok(out),
            // 3: the snapshot was written, but some files could not be read
            Some(3) if out.snapshot.is_some() => {
                if out.errors.is_empty() {
                    out.errors.push("Some files could not be read".into());
                }
                Ok(out)
            }
            code => {
                // A dump tool's own words ("requires authentication") say more than restic's "command failed":
                // they are the last plain line before restic's JSON.
                let tool = stderr
                    .iter()
                    .rev()
                    .find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('{'));
                let last = stderr.iter().rev().find(|l| !l.trim().is_empty());
                let detail = match (tool, last) {
                    (Some(t), Some(l)) if t != l => Some(format!("{}: {l}", t.trim())),
                    (t, l) => t.or(l).cloned(),
                }
                .or_else(|| out.errors.first().cloned());
                bail!(
                    "{}",
                    detail.unwrap_or_else(|| format!("restic exited with code {}", code.unwrap_or(-1)))
                )
            }
        }
    }
}

pub fn which(name: &str) -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(':')
        .any(|d| std::path::Path::new(d).join(name).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrong_password_says_where_to_look() {
        let o = Output {
            code: 1,
            stdout: String::new(),
            stderr: "Fatal: wrong password or no key found\n".into(),
        };
        let e = err_text(&o);
        assert!(e.starts_with("Fatal: wrong password or no key found"));
        assert!(e.contains("different password"));
    }

    #[test]
    fn versions() {
        assert_eq!(
            parse_version("restic 0.19.1 compiled with go1.26 on linux/amd64"),
            Some((0, 19, 1))
        );
        assert_eq!(parse_version("restic 0.17.0"), Some((0, 17, 0)));
    }

    #[test]
    fn repo_urls() {
        let p = Paths::from_env();
        let d = |kind| Destination {
            id: "d".into(),
            name: "d".into(),
            kind,
            password: "secretpw".into(),
            limit_upload_kib: 0,
            limit_download_kib: 0,
        };
        let r = Repo::of(
            &d(DestKind::S3 {
                provider: "r2".into(),
                endpoint: "acc.r2.cloudflarestorage.com".into(),
                bucket: "b".into(),
                path: "/srv1/".into(),
                region: String::new(),
                access_key: "a".into(),
                secret_key: "s".into(),
            }),
            &p,
        )
        .unwrap();
        assert_eq!(r.url, "s3:https://acc.r2.cloudflarestorage.com/b/srv1");
        assert!(r.options.contains(&"s3.region=auto".to_string()));
        let r = Repo::of(
            &d(DestKind::Rest {
                url: "http://127.0.0.1:8000/pc".into(),
                username: "u".into(),
                password: "p@ss".into(),
            }),
            &p,
        )
        .unwrap();
        assert_eq!(r.url, "rest:http://u:p%40ss@127.0.0.1:8000/pc");
        let r = Repo::of(
            &d(DestKind::Sftp {
                host: "h".into(),
                port: 2222,
                user: "root".into(),
                path: "/backup".into(),
            }),
            &p,
        )
        .unwrap();
        assert_eq!(r.url, "sftp:root@h:/backup");
        assert!(r.options[0].contains("-p 2222"));
    }
}
