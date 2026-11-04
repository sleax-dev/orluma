//! Loads a database dump out of a snapshot into a live database server, the way it was dumped: the dump is
//! streamed from `restic dump` into the database's own restore tool, so it never needs room on the disk.
//! The default is a NEW database next to the original; touching an existing one needs `replace`.

use crate::restic::anyhow_lite::{Error, Result};
use crate::restic::{Repo, Restic};
use crate::sources::{ch_client, conn_flags, mysql_env, mysql_tool, pg_env, query, wrap};
use orluma_core::api::RestoreDbRequest;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

type Rewrite = Box<dyn Fn(&[u8]) -> Option<Vec<u8>> + Send>;

fn last_line(s: &str) -> String {
    s.lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_string()
}

fn ident(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 63 || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(Error(format!(
            "'{name}' is not a usable database name (letters, digits, _ and - only)"
        )));
    }
    Ok(())
}

fn tool(argv: &[String], env: &[(String, String)]) -> Command {
    let mut c = Command::new(&argv[0]);
    c.args(&argv[1..])
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    c
}

/// `dump | tool`, line by line through `rewrite` when given. A failing tool wins over the broken pipe it leaves.
async fn pipe(mut dump: Command, mut sink: Command, rewrite: Option<Rewrite>) -> Result<()> {
    dump.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    sink.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut d = dump.spawn()?;
    let mut t = sink
        .spawn()
        .map_err(|e| Error(format!("cannot start {:?}: {e}", sink.as_std().get_program())))?;
    let mut src = d.stdout.take().ok_or_else(|| Error("no dump output".into()))?;
    let mut dst = t.stdin.take().ok_or_else(|| Error("no tool input".into()))?;
    let drain = |e: Option<tokio::process::ChildStderr>| {
        tokio::spawn(async move {
            let mut s = String::new();
            if let Some(mut e) = e {
                let _ = e.read_to_string(&mut s).await;
            }
            s
        })
    };
    let (derr, terr) = (drain(d.stderr.take()), drain(t.stderr.take()));
    let copied: std::io::Result<()> = async {
        match rewrite {
            None => tokio::io::copy(&mut src, &mut dst).await.map(|_| ()),
            Some(f) => {
                let mut r = BufReader::with_capacity(256 * 1024, src);
                let mut line = Vec::new();
                loop {
                    line.clear();
                    if r.read_until(b'\n', &mut line).await? == 0 {
                        break;
                    }
                    match f(&line) {
                        Some(n) => dst.write_all(&n).await?,
                        None => dst.write_all(&line).await?,
                    }
                }
                Ok(())
            }
        }
    }
    .await;
    drop(dst);
    let (ds, ts) = (d.wait().await?, t.wait().await?);
    let (derr, terr) = (derr.await.unwrap_or_default(), terr.await.unwrap_or_default());
    if !ts.success() {
        return Err(Error(format!("the restore tool failed: {}", last_line(&terr))));
    }
    if !ds.success() {
        return Err(Error(format!("reading the backup failed: {}", last_line(&derr))));
    }
    copied.map_err(|e| Error(e.to_string()))
}

fn dump_cmd(restic: &Restic, repo: &Repo, snapshot: &str, path: &str) -> Command {
    restic.command(repo, &["dump", "--no-lock", snapshot, path])
}

pub async fn run(restic: &Restic, repo: &Repo, r: &RestoreDbRequest) -> Result<String> {
    if !r.file.starts_with('/') {
        return Err(Error("pick a dump file inside the snapshot".into()));
    }
    let stem = std::path::Path::new(&r.file)
        .file_stem()
        .map_or(String::new(), |s| s.to_string_lossy().into_owned());
    let dump = || dump_cmd(restic, repo, &r.snapshot, &r.file);
    match r.kind.as_str() {
        "postgres" => {
            if !r.file.ends_with(".dump") {
                return Err(Error(
                    "only database dumps (.dump) can be loaded; roles (globals.sql) are not".into(),
                ));
            }
            let db = if r.database.is_empty() {
                stem.clone()
            } else {
                r.database.clone()
            };
            ident(&db)?;
            let flags = conn_flags(&r.conn, "-h", "-p", "-U");
            let sql = |q: String| {
                let mut a = vec![
                    "psql".to_string(),
                    "-X".into(),
                    "-tA".into(),
                    "-d".into(),
                    "postgres".into(),
                ];
                a.extend(flags.clone());
                a.extend(["-c".into(), q]);
                wrap(&r.conn, a, pg_env(&r.conn))
            };
            let (a, e) = sql(format!("SELECT 1 FROM pg_database WHERE datname = '{db}'"));
            let exists = query(&a, &e).await?.trim() == "1";
            if exists && !r.replace {
                return Err(Error(format!(
                    "database '{db}' already exists: pick another name, or allow replacing it"
                )));
            }
            if !exists {
                let (a, e) = sql(format!("CREATE DATABASE \"{db}\""));
                query(&a, &e).await?;
            }
            let mut argv = vec!["pg_restore".to_string(), "--no-owner".into(), "--no-privileges".into()];
            if exists {
                argv.extend(["--clean".into(), "--if-exists".into()]);
            }
            argv.extend(flags.clone());
            argv.extend(["-d".into(), db.clone()]);
            let (argv, env) = wrap(&r.conn, argv, pg_env(&r.conn));
            pipe(dump(), tool(&argv, &env), None).await?;
            Ok(format!("postgres: restored into database {db}"))
        }
        "mysql" => {
            let db = if r.database.is_empty() {
                stem.clone()
            } else {
                r.database.clone()
            };
            ident(&db)?;
            let mut argv = vec![mysql_tool("mysql", &r.conn)];
            argv.extend(conn_flags(&r.conn, "-h", "-P", "-u"));
            let (a, e) = wrap(
                &r.conn,
                {
                    let mut q = argv.clone();
                    q.extend(["-N".into(), "-e".into(), format!("SHOW DATABASES LIKE '{db}'")]);
                    q
                },
                mysql_env(&r.conn),
            );
            if !query(&a, &e).await?.trim().is_empty() && !r.replace {
                return Err(Error(format!(
                    "database '{db}' already exists: pick another name, or allow replacing it"
                )));
            }
            // mysqldump names its database in CREATE DATABASE / USE: point those at the new name.
            let (from, to) = (format!("`{stem}`"), format!("`{db}`"));
            let rewrite: Option<Rewrite> = (from != to).then(|| {
                Box::new(move |l: &[u8]| {
                    let t = String::from_utf8_lossy(l);
                    (t.starts_with("CREATE DATABASE")
                        || t.starts_with("USE ")
                        || t.starts_with("/*!40000 DROP DATABASE"))
                    .then(|| t.replace(&from, &to).into_bytes())
                }) as Rewrite
            });
            let (argv, env) = wrap(&r.conn, argv, mysql_env(&r.conn));
            pipe(dump(), tool(&argv, &env), rewrite).await?;
            Ok(format!("mysql: restored into database {db}"))
        }
        "sqlite" => {
            let target = if r.database.is_empty() {
                format!("{}-restored.db", stem)
            } else {
                r.database.clone()
            };
            let p = std::path::Path::new(&target);
            if !p.is_absolute() {
                return Err(Error("give the full path of the SQLite file to create".into()));
            }
            if p.exists() {
                if !r.replace {
                    return Err(Error(format!(
                        "{target} already exists: pick another file, or allow replacing it"
                    )));
                }
                // never delete: the old file stays next to it
                let aside = format!("{target}.before-restore-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
                std::fs::rename(p, &aside)?;
            }
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            pipe(dump(), tool(&["sqlite3".into(), target.clone()], &[]), None).await?;
            Ok(format!("sqlite: restored into {target}"))
        }
        "mongo" => {
            let mut args = vec!["--archive"];
            if r.replace {
                args.push("--drop");
            }
            let (argv, env) = crate::sources::mongo_tool("mongorestore", &r.uri, &args);
            let conn = orluma_core::model::DbConn {
                container: r.conn.container.clone(),
                ..Default::default()
            };
            let (argv, env) = wrap(&conn, argv, env);
            pipe(dump(), tool(&argv, &env), None).await?;
            Ok(format!(
                "mongodb: restored every database in the archive{}",
                if r.replace {
                    " (existing collections replaced)"
                } else {
                    ""
                }
            ))
        }
        "clickhouse" => clickhouse(restic, repo, r).await,
        other => Err(Error(format!(
            "loading '{other}' dumps into a server is not supported; download the file instead"
        ))),
    }
}

async fn clickhouse(restic: &Restic, repo: &Repo, r: &RestoreDbRequest) -> Result<String> {
    // the file chosen is the database's schema.sql: /clickhouse/<db>/schema.sql
    let orig = r
        .file
        .trim_end_matches('/')
        .rsplit('/')
        .nth(1)
        .unwrap_or("")
        .to_string();
    if !r.file.ends_with("/schema.sql") {
        return Err(Error("pick the schema.sql snapshot of the database".into()));
    }
    let db = if r.database.is_empty() {
        orig.clone()
    } else {
        r.database.clone()
    };
    ident(&db)?;
    if r.replace {
        return Err(Error(
            "ClickHouse databases are not overwritten: restore into a new database name".into(),
        ));
    }
    let run = |q: String| {
        let mut a = ch_client(&r.conn);
        a.extend(["--query".into(), q]);
        wrap(&r.conn, a, Vec::new())
    };
    let (a, e) = run(format!("SELECT count() FROM system.tables WHERE database = '{db}'"));
    if query(&a, &e).await?.trim() != "0" {
        return Err(Error(format!("database '{db}' already has tables: pick a new name")));
    }
    // A ClickHouse backup is one snapshot per file: the schema, then each table. Pick the schema's snapshot in
    // the list and the tables come from the same run: for each, the first snapshot made at or after the schema's.
    let dir = format!("/clickhouse/{orig}");
    let all = restic.snapshots(repo, None).await?;
    let schema_snap = all
        .iter()
        .find(|x| x.id.starts_with(&r.snapshot) || r.snapshot.starts_with(&x.short_id))
        .ok_or_else(|| Error("the schema snapshot is gone".into()))?;
    let mut tables: std::collections::BTreeMap<String, &orluma_core::api::Snapshot> = Default::default();
    for x in all
        .iter()
        .filter(|x| x.source == schema_snap.source && x.job == schema_snap.job)
    {
        let Some(p) = x.paths.first().and_then(|p| p.strip_prefix(&format!("{dir}/"))) else {
            continue;
        };
        let Some(t) = p.strip_suffix(".native") else { continue };
        if x.time >= schema_snap.time - chrono::Duration::minutes(1)
            && tables.get(t).is_none_or(|old| x.time < old.time)
        {
            tables.insert(t.to_string(), x);
        }
    }
    let (a, e) = run(format!("CREATE DATABASE IF NOT EXISTS `{db}`"));
    query(&a, &e).await?;
    let schema = dump_cmd(restic, repo, &r.snapshot, &format!("{dir}/schema.sql"))
        .output()
        .await?;
    if !schema.status.success() {
        return Err(Error(format!(
            "reading the schema failed: {}",
            last_line(&String::from_utf8_lossy(&schema.stderr))
        )));
    }
    let text = String::from_utf8_lossy(&schema.stdout).into_owned();
    let (mut made, mut failed) = (0, Vec::new());
    for stmt in text.split(";\n").map(str::trim).filter(|s| !s.is_empty()) {
        let stmt = if db != orig {
            let (head, rest) = stmt.split_once('\n').unwrap_or((stmt, ""));
            let head = head.replacen(&format!(" {orig}."), &format!(" {db}."), 1).replacen(
                &format!(" `{orig}`."),
                &format!(" `{db}`."),
                1,
            );
            format!("{head}\n{rest}")
        } else {
            stmt.to_string()
        };
        let (a, e) = run(stmt);
        match query(&a, &e).await {
            Ok(_) => made += 1,
            Err(x) => failed.push(last_line(&x.0)),
        }
    }
    let mut rows = 0;
    for (table, snap) in &tables {
        let mut a = ch_client(&r.conn);
        a.extend(["--query".into(), format!("INSERT INTO `{db}`.`{table}` FORMAT Native")]);
        let (a, e) = wrap(&r.conn, a, Vec::new());
        pipe(
            dump_cmd(restic, repo, &snap.id, &format!("{dir}/{table}.native")),
            tool(&a, &e),
            None,
        )
        .await?;
        rows += 1;
    }
    if !failed.is_empty() {
        return Err(Error(format!(
            "clickhouse: {made} table(s) created, {} failed: {}",
            failed.len(),
            failed[0]
        )));
    }
    Ok(format!("clickhouse: restored {rows} table(s) into database {db}"))
}

#[cfg(test)]
mod tests {
    use super::ident;

    #[test]
    fn names() {
        assert!(ident("app_db-2").is_ok());
        assert!(ident("x'; DROP").is_err());
        assert!(ident("").is_err());
    }
}
