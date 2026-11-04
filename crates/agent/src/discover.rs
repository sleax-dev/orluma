//! What on this server is worth backing up: databases (running services and Docker containers, with the
//! credentials their containers were started with) and the usual folders, with their sizes.

use orluma_core::api::{Discovery, FoundDatabase, FoundPath, PathSize, Tool};
use std::time::Duration;
use tokio::process::Command;

async fn out(cmd: &str, args: &[&str]) -> Option<String> {
    // kill_on_drop: a command that outlives the timeout (du on a huge tree) must not keep running
    let o = tokio::time::timeout(
        Duration::from_secs(15),
        Command::new(cmd).args(args).kill_on_drop(true).output(),
    )
    .await
    .ok()?
    .ok()?;
    o.status
        .success()
        .then(|| String::from_utf8_lossy(&o.stdout).into_owned())
}

fn kind_of(image: &str) -> Option<&'static str> {
    let i = image
        .rsplit('/')
        .next()
        .unwrap_or(image)
        .split(':')
        .next()
        .unwrap_or(image);
    Some(match i {
        x if x.starts_with("postgres")
            || x.starts_with("postgis")
            || x.starts_with("timescaledb")
            || x.starts_with("pgvector") =>
        {
            "postgres"
        }
        x if x.starts_with("mysql") || x.starts_with("mariadb") || x.starts_with("percona") => "mysql",
        x if x.starts_with("mongo") => "mongo",
        x if x.starts_with("redis") || x.starts_with("valkey") || x.starts_with("keydb") => "redis",
        x if x.starts_with("clickhouse") => "clickhouse",
        _ => return None,
    })
}

async fn docker(found: &mut Vec<FoundDatabase>, paths: &mut Vec<FoundPath>) {
    let Some(ps) = out("docker", &["ps", "--format", "{{.Names}}\t{{.Image}}"]).await else {
        return;
    };
    for line in ps.lines() {
        let Some((name, image)) = line.split_once('\t') else {
            continue;
        };
        let Some(kind) = kind_of(image) else { continue };
        let env_text = out(
            "docker",
            &["inspect", "--format", "{{range .Config.Env}}{{println .}}{{end}}", name],
        )
        .await
        .unwrap_or_default();
        let env = |k: &str| {
            env_text
                .lines()
                .find_map(|l| l.strip_prefix(&format!("{k}=")))
                .unwrap_or("")
                .to_string()
        };
        let mut db = FoundDatabase {
            kind: kind.into(),
            label: format!("{name} ({image})"),
            container: name.into(),
            ..Default::default()
        };
        match kind {
            "postgres" => {
                db.user = Some(env("POSTGRES_USER"))
                    .filter(|u| !u.is_empty())
                    .unwrap_or_else(|| "postgres".into());
                db.password = env("POSTGRES_PASSWORD");
                let d = env("POSTGRES_DB");
                if !d.is_empty() {
                    db.databases = vec![d];
                }
            }
            "mysql" => {
                let root = env("MYSQL_ROOT_PASSWORD");
                let root = if root.is_empty() {
                    env("MARIADB_ROOT_PASSWORD")
                } else {
                    root
                };
                if root.is_empty() {
                    db.user = env("MYSQL_USER");
                    db.password = env("MYSQL_PASSWORD");
                } else {
                    db.user = "root".into();
                    db.password = root;
                }
            }
            "mongo" => {
                let (u, p) = (env("MONGO_INITDB_ROOT_USERNAME"), env("MONGO_INITDB_ROOT_PASSWORD"));
                if !u.is_empty() {
                    db.user = u;
                    db.password = p;
                }
            }
            "redis" => db.password = env("REDIS_PASSWORD"),
            "clickhouse" => {
                db.user = env("CLICKHOUSE_USER");
                db.password = env("CLICKHOUSE_PASSWORD");
            }
            _ => {}
        }
        found.push(db);
    }
    // Named volumes only: anonymous ones (64 hex characters) are scratch space a container recreates.
    if let Some(vols) = out("docker", &["volume", "ls", "--format", "{{.Name}}\t{{.Mountpoint}}"]).await {
        for (name, mount) in vols.lines().filter_map(|l| l.split_once('\t')) {
            let anonymous = name.len() == 64 && name.chars().all(|c| c.is_ascii_hexdigit());
            if !anonymous {
                paths.push(FoundPath {
                    path: mount.into(),
                    bytes: 0,
                    hint: "docker-volume".into(),
                    pending: true,
                });
            }
        }
    }
}

async fn services(found: &mut Vec<FoundDatabase>) {
    let Some(units) = out(
        "systemctl",
        &[
            "list-units",
            "--type=service",
            "--state=running",
            "--no-legend",
            "--plain",
        ],
    )
    .await
    else {
        return;
    };
    for line in units.lines() {
        let unit = line.split_whitespace().next().unwrap_or("");
        let (kind, port, os_user) = match unit {
            u if u.starts_with("postgresql") => ("postgres", 0, "postgres"),
            u if u.starts_with("mysql") || u.starts_with("mariadb") => ("mysql", 0, ""),
            u if u.starts_with("mongod") => ("mongo", 0, ""),
            u if u.starts_with("redis") || u.starts_with("valkey") => ("redis", 0, ""),
            u if u.starts_with("clickhouse") => ("clickhouse", 0, ""),
            _ => continue,
        };
        if found.iter().any(|f| f.kind == kind && f.container.is_empty()) {
            continue;
        }
        found.push(FoundDatabase {
            kind: kind.into(),
            label: unit.into(),
            port,
            os_user: os_user.into(),
            ..Default::default()
        });
    }
}

async fn tools() -> Vec<Tool> {
    let mut t = Vec::new();
    for (name, args) in [
        ("restic", &["version"][..]),
        ("docker", &["--version"][..]),
        ("pg_dump", &["--version"][..]),
        ("mysqldump", &["--version"][..]),
        ("mariadb-dump", &["--version"][..]),
        ("mongodump", &["--version"][..]),
        ("redis-cli", &["--version"][..]),
        ("sqlite3", &["--version"][..]),
        ("clickhouse", &["--version"][..]),
        ("rclone", &["--version"][..]),
    ] {
        if let Some(o) = out(name, args).await {
            t.push(Tool {
                name: name.into(),
                version: o.lines().next().unwrap_or("").trim().chars().take(80).collect(),
            });
        }
    }
    t
}

const PATHS: &[(&str, &str)] = &[
    ("/etc", "config"),
    ("/root", "home"),
    ("/home", "home"),
    ("/var/www", "web"),
    ("/srv", "data"),
    ("/opt", "data"),
    ("/var/spool/cron", "config"),
    ("/var/mail", "data"),
    ("/var/vmail", "data"),
];

/// At most two folders are measured at a time: `du` on a big tree is disk-heavy.
static SIZING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

/// Size of one folder, measured at low priority and given up on after 25 seconds (`complete: false`).
pub async fn path_size(p: &str) -> PathSize {
    let known = PATHS.iter().any(|(x, _)| x == &p) || p.starts_with("/var/lib/docker/volumes/");
    if !known || p.contains("..") || !std::path::Path::new(p).is_dir() {
        return PathSize::default();
    }
    let Ok(_slot) = SIZING.acquire().await else {
        return PathSize::default();
    };
    let run = Command::new("nice")
        .args(["-n", "19", "du", "-sbx", p])
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(Duration::from_secs(25), run).await {
        Ok(Ok(o)) if o.status.success() || !o.stdout.is_empty() => PathSize {
            bytes: String::from_utf8_lossy(&o.stdout)
                .split_whitespace()
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0),
            complete: true,
        },
        _ => PathSize::default(),
    }
}

/// One discovery at a time: a second request waits for the first instead of starting another round of `du`.
static ONE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn discover() -> Discovery {
    let _one = ONE.lock().await;
    let mut d = Discovery::default();
    docker(&mut d.databases, &mut d.paths).await;
    services(&mut d.databases).await;
    for (p, hint) in PATHS {
        if std::path::Path::new(p).is_dir() {
            d.paths.push(FoundPath {
                path: p.to_string(),
                bytes: 0,
                hint: hint.to_string(),
                pending: true,
            });
        }
    }
    let rank = |h: &str| {
        ["config", "home", "web", "data", "docker-volume"]
            .iter()
            .position(|x| *x == h)
            .unwrap_or(9)
    };
    d.paths.sort_by_key(|p| rank(&p.hint));
    d.tools = tools().await;
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_folders_are_sized() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        assert!(!rt.block_on(path_size("/definitely/not/there")).complete);
        assert!(
            !rt.block_on(path_size("/etc/../root")).complete,
            "paths outside the suggestions are refused"
        );
        let etc = rt.block_on(path_size("/etc"));
        assert!(etc.complete && etc.bytes > 0);
    }
}
