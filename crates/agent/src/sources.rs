//! Turns a job's sources into what restic backs up: folders as one snapshot, and every database as a dump
//! streamed straight into restic (`--stdin-from-command`), so a dump never needs room on the disk first and
//! a failing dump tool fails the snapshot instead of saving half a file.

use crate::restic::anyhow_lite::{Error, Result};
use crate::restic::which;
use orluma_core::model::{DbConn, Source};
use tokio::process::Command;

/// One thing restic stores as one snapshot.
#[derive(Clone, Debug)]
pub enum Unit {
    Paths {
        paths: Vec<String>,
        excludes: Vec<String>,
        one_file_system: bool,
    },
    Stdin {
        argv: Vec<String>,
        env: Vec<(String, String)>,
        filename: String,
    },
}

#[derive(Clone, Debug)]
pub struct Planned {
    pub source_id: String,
    pub kind: &'static str,
    /// What the log calls it: "/etc, /root", "postgres app_db".
    pub label: String,
    pub unit: Unit,
}

/// How to run a database tool: inside a container, as another OS user, or directly.
pub(crate) fn wrap(
    conn: &DbConn,
    argv: Vec<String>,
    env: Vec<(String, String)>,
) -> (Vec<String>, Vec<(String, String)>) {
    if !conn.container.is_empty() {
        // `-e NAME` without a value: docker takes the value from its own environment, so a password never
        // appears in a command line (which `ps` shows to every user on the server).
        let mut a = vec!["docker".to_string(), "exec".into(), "-i".into()];
        for (k, _) in &env {
            a.push("-e".into());
            a.push(k.clone());
        }
        if !conn.os_user.is_empty() {
            a.push("-u".into());
            a.push(conn.os_user.clone());
        }
        a.push(conn.container.clone());
        // MariaDB 11 images ship only the new tool names: try the old one, then the new, inside the container.
        match argv.first().map(String::as_str) {
            Some(old @ ("mysqldump" | "mysql")) => {
                let new = if old == "mysql" { "mariadb" } else { "mariadb-dump" };
                a.extend([
                    "sh".into(),
                    "-c".into(),
                    format!("exec \"$(command -v {old} || command -v {new})\" \"$@\""),
                    "sh".into(),
                ]);
                a.extend(argv.into_iter().skip(1));
            }
            _ => a.extend(argv),
        }
        return (a, env);
    }
    if !conn.os_user.is_empty() {
        let mut a = vec!["runuser".to_string(), "-u".into(), conn.os_user.clone(), "--".into()];
        a.extend(argv);
        return (a, env);
    }
    (argv, env)
}

/// mongodump or mongorestore. A URI with a password goes into a config file the tool reads (`--config`), made
/// with mode 600 where the tool runs and removed after, never onto the command line.
pub(crate) fn mongo_tool(tool: &str, uri: &str, args: &[&str]) -> (Vec<String>, Vec<(String, String)>) {
    if uri.is_empty() {
        let mut a = vec![s(tool)];
        a.extend(args.iter().map(|x| s(x)));
        return (a, Vec::new());
    }
    let script = format!(
        "f=$(mktemp) || exit 1; trap 'rm -f \"$f\"' EXIT; printf 'uri: \"%s\"\\n' \"$ORLUMA_MONGO_URI\" > \"$f\"; {tool} --config \"$f\" \"$@\""
    );
    let mut a = vec![s("sh"), s("-c"), script, s("sh")];
    a.extend(args.iter().map(|x| s(x)));
    (a, vec![(s("ORLUMA_MONGO_URI"), uri.to_string())])
}

pub(crate) fn s(v: &str) -> String {
    v.to_string()
}

pub(crate) fn conn_flags(conn: &DbConn, host: &str, port: &str, user: &str) -> Vec<String> {
    let mut f = Vec::new();
    if !conn.host.is_empty() {
        f.extend([s(host), conn.host.clone()]);
    }
    if conn.port != 0 {
        f.extend([s(port), conn.port.to_string()]);
    }
    if !conn.user.is_empty() {
        f.extend([s(user), conn.user.clone()]);
    }
    f
}

pub(crate) async fn query(argv: &[String], env: &[(String, String)]) -> Result<String> {
    let o = Command::new(&argv[0])
        .args(&argv[1..])
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .kill_on_drop(true)
        .output()
        .await?;
    if !o.status.success() {
        let e = String::from_utf8_lossy(&o.stderr);
        return Err(Error(format!(
            "{}: {}",
            argv[0],
            e.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed")
        )));
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

pub(crate) fn safe_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

// ---- Postgres

pub(crate) fn pg_env(conn: &DbConn) -> Vec<(String, String)> {
    if conn.password.is_empty() {
        Vec::new()
    } else {
        vec![(s("PGPASSWORD"), conn.password.clone())]
    }
}

pub async fn postgres_databases(conn: &DbConn) -> Result<Vec<String>> {
    let mut argv = vec![s("psql"), s("-XAtq")];
    argv.extend(conn_flags(conn, "-h", "-p", "-U"));
    argv.extend([
        s("-d"),
        s("postgres"),
        s("-c"),
        s("SELECT datname FROM pg_database WHERE datallowconn AND NOT datistemplate ORDER BY 1"),
    ]);
    let (argv, env) = wrap(conn, argv, pg_env(conn));
    Ok(query(&argv, &env)
        .await?
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

// ---- MySQL / MariaDB

pub(crate) fn mysql_tool(base: &str, conn: &DbConn) -> String {
    // MariaDB 11 renamed the tools; inside a container `wrap` tries both names.
    let maria = match base {
        "mysqldump" => "mariadb-dump",
        _ => "mariadb",
    };
    if conn.container.is_empty() && !which(base) && which(maria) {
        maria.into()
    } else {
        base.into()
    }
}

pub(crate) fn mysql_env(conn: &DbConn) -> Vec<(String, String)> {
    if conn.password.is_empty() {
        Vec::new()
    } else {
        vec![(s("MYSQL_PWD"), conn.password.clone())]
    }
}

pub async fn mysql_databases(conn: &DbConn) -> Result<Vec<String>> {
    let mut argv = vec![mysql_tool("mysql", conn), s("-N"), s("-B")];
    argv.extend(conn_flags(conn, "-h", "-P", "-u"));
    argv.extend([s("-e"), s("SHOW DATABASES")]);
    let (argv, env) = wrap(conn, argv, mysql_env(conn));
    Ok(query(&argv, &env)
        .await?
        .lines()
        .map(str::trim)
        .filter(|d| !d.is_empty() && !["information_schema", "performance_schema", "sys"].contains(d))
        .map(str::to_string)
        .collect())
}

// ---- ClickHouse

pub(crate) fn ch_client(conn: &DbConn) -> Vec<String> {
    let mut a = if !conn.container.is_empty() || which("clickhouse-client") {
        vec![s("clickhouse-client")]
    } else {
        vec![s("clickhouse"), s("client")]
    };
    a.extend(conn_flags(conn, "--host", "--port", "--user"));
    if !conn.password.is_empty() {
        a.extend([s("--password"), conn.password.clone()]);
    }
    a
}

pub async fn clickhouse_tables(conn: &DbConn, databases: &[String]) -> Result<Vec<(String, String)>> {
    let mut argv = ch_client(conn);
    argv.extend([
        s("--format"),
        s("TSV"),
        s("--query"),
        s("SELECT database, name FROM system.tables WHERE database NOT IN ('system','INFORMATION_SCHEMA','information_schema') \
           AND NOT is_temporary AND engine NOT LIKE '%View' AND engine NOT IN ('Dictionary','MaterializedView') AND name NOT LIKE '.inner%' ORDER BY 1, 2"),
    ]);
    let (argv, env) = wrap(conn, argv, Vec::new());
    Ok(query(&argv, &env)
        .await?
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .filter(|(d, _)| databases.is_empty() || databases.iter().any(|x| x == d))
        .map(|(d, t)| (d.to_string(), t.to_string()))
        .collect())
}

/// Every snapshot a source produces this run.
pub async fn plan(src: &Source) -> Result<Vec<Planned>> {
    let id = src.id().to_string();
    let kind = src.kind();
    let one = |label: String, unit: Unit| {
        vec![Planned {
            source_id: id.clone(),
            kind,
            label,
            unit,
        }]
    };
    Ok(match src {
        Source::Paths {
            paths,
            excludes,
            one_file_system,
            ..
        } => {
            let missing: Vec<_> = paths
                .iter()
                .filter(|p| !std::path::Path::new(p).exists())
                .cloned()
                .collect();
            if !missing.is_empty() {
                return Err(Error(format!("not found on the server: {}", missing.join(", "))));
            }
            one(
                paths.join(", "),
                Unit::Paths {
                    paths: paths.clone(),
                    excludes: excludes.clone(),
                    one_file_system: *one_file_system,
                },
            )
        }
        Source::Postgres {
            conn,
            databases,
            globals,
            ..
        } => {
            let dbs = if databases.is_empty() {
                postgres_databases(conn).await?
            } else {
                databases.clone()
            };
            let mut out = Vec::new();
            if *globals {
                let mut argv = vec![s("pg_dumpall"), s("--globals-only")];
                argv.extend(conn_flags(conn, "-h", "-p", "-U"));
                let (argv, env) = wrap(conn, argv, pg_env(conn));
                out.push(Planned {
                    source_id: id.clone(),
                    kind,
                    label: "postgres roles".into(),
                    unit: Unit::Stdin {
                        argv,
                        env,
                        filename: "postgres/globals.sql".into(),
                    },
                });
            }
            for db in dbs {
                // Custom format without compression: restic compresses, and uncompressed dumps deduplicate between runs.
                let mut argv = vec![s("pg_dump"), s("-Fc"), s("-Z0")];
                argv.extend(conn_flags(conn, "-h", "-p", "-U"));
                argv.push(db.clone());
                let (argv, env) = wrap(conn, argv, pg_env(conn));
                out.push(Planned {
                    source_id: id.clone(),
                    kind,
                    label: format!("postgres {db}"),
                    unit: Unit::Stdin {
                        argv,
                        env,
                        filename: format!("postgres/{}.dump", safe_name(&db)),
                    },
                });
            }
            out
        }
        Source::Mysql { conn, databases, .. } => {
            let dbs = if databases.is_empty() {
                mysql_databases(conn).await?
            } else {
                databases.clone()
            };
            dbs.into_iter()
                .map(|db| {
                    let mut argv = vec![
                        mysql_tool("mysqldump", conn),
                        s("--single-transaction"),
                        s("--quick"),
                        s("--routines"),
                        s("--events"),
                        s("--triggers"),
                        s("--hex-blob"),
                    ];
                    argv.extend(conn_flags(conn, "-h", "-P", "-u"));
                    argv.extend([s("--databases"), db.clone()]);
                    let (argv, env) = wrap(conn, argv, mysql_env(conn));
                    Planned {
                        source_id: id.clone(),
                        kind,
                        label: format!("mysql {db}"),
                        unit: Unit::Stdin {
                            argv,
                            env,
                            filename: format!("mysql/{}.sql", safe_name(&db)),
                        },
                    }
                })
                .collect()
        }
        Source::Mongo { uri, container, .. } => {
            let conn = DbConn {
                container: container.clone(),
                ..Default::default()
            };
            // not --quiet: it also hides the reason a dump fails ("requires authentication")
            let (argv, env) = mongo_tool("mongodump", uri, &["--archive"]);
            let (argv, env) = wrap(&conn, argv, env);
            one(
                "mongodb".into(),
                Unit::Stdin {
                    argv,
                    env,
                    filename: "mongo/all.archive".into(),
                },
            )
        }
        Source::Redis { conn, .. } => {
            let mut argv = vec![s("redis-cli")];
            argv.extend(conn_flags(conn, "-h", "-p", "--user"));
            let env = if conn.password.is_empty() {
                Vec::new()
            } else {
                vec![(s("REDISCLI_AUTH"), conn.password.clone())]
            };
            argv.extend([s("--rdb"), s("/dev/stdout")]);
            let (argv, env) = wrap(conn, argv, env);
            one(
                "redis".into(),
                Unit::Stdin {
                    argv,
                    env,
                    filename: "redis/dump.rdb".into(),
                },
            )
        }
        Source::Sqlite { path, .. } => {
            if !std::path::Path::new(path).exists() {
                return Err(Error(format!("not found on the server: {path}")));
            }
            let name = std::path::Path::new(path)
                .file_name()
                .map_or("db".into(), |n| n.to_string_lossy().into_owned());
            one(
                format!("sqlite {path}"),
                Unit::Stdin {
                    argv: vec![s("sqlite3"), path.clone(), s(".dump")],
                    env: Vec::new(),
                    filename: format!("sqlite/{}.sql", safe_name(&name)),
                },
            )
        }
        Source::Clickhouse { conn, databases, .. } => {
            let tables = clickhouse_tables(conn, databases).await?;
            let mut out = Vec::new();
            let mut dbs: Vec<&String> = tables.iter().map(|(d, _)| d).collect();
            dbs.dedup();
            for db in dbs {
                let mut argv = ch_client(conn);
                argv.extend([
                    s("--format"),
                    s("TSVRaw"),
                    s("--query"),
                    format!(
                        "SELECT create_table_query || ';' FROM system.tables WHERE database = '{}' ORDER BY name",
                        db.replace('\'', "''")
                    ),
                ]);
                let (argv, env) = wrap(conn, argv, Vec::new());
                out.push(Planned {
                    source_id: id.clone(),
                    kind,
                    label: format!("clickhouse {db} schema"),
                    unit: Unit::Stdin {
                        argv,
                        env,
                        filename: format!("clickhouse/{}/schema.sql", safe_name(db)),
                    },
                });
            }
            for (db, t) in tables {
                let mut argv = ch_client(conn);
                argv.extend([
                    s("--query"),
                    format!(
                        "SELECT * FROM `{}`.`{}` FORMAT Native",
                        db.replace('`', ""),
                        t.replace('`', "")
                    ),
                ]);
                let (argv, env) = wrap(conn, argv, Vec::new());
                out.push(Planned {
                    source_id: id.clone(),
                    kind,
                    label: format!("clickhouse {db}.{t}"),
                    unit: Unit::Stdin {
                        argv,
                        env,
                        filename: format!("clickhouse/{}/{}.native", safe_name(&db), safe_name(&t)),
                    },
                });
            }
            out
        }
        Source::Command { command, filename, .. } => {
            let name = if filename.is_empty() {
                "command.out".to_string()
            } else {
                filename.trim_start_matches('/').to_string()
            };
            one(
                format!("command {command}"),
                Unit::Stdin {
                    argv: vec![s("sh"), s("-c"), command.clone()],
                    env: Vec::new(),
                    filename: name,
                },
            )
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_and_peer_wrapping() {
        let c = DbConn {
            container: "db".into(),
            os_user: "postgres".into(),
            password: "x".into(),
            ..Default::default()
        };
        let (argv, env) = wrap(&c, vec![s("pg_dump"), s("app")], pg_env(&c));
        assert_eq!(
            argv,
            [
                "docker",
                "exec",
                "-i",
                "-e",
                "PGPASSWORD",
                "-u",
                "postgres",
                "db",
                "pg_dump",
                "app"
            ]
        );
        assert_eq!(
            env,
            [(s("PGPASSWORD"), s("x"))],
            "the value travels in the environment only"
        );
        let (argv, env) = mongo_tool("mongodump", "mongodb://u:secret@127.0.0.1/", &["--archive"]);
        assert!(argv.iter().all(|a| !a.contains("secret")), "{argv:?}");
        assert_eq!(env[0].1, "mongodb://u:secret@127.0.0.1/");
        let c = DbConn {
            os_user: "postgres".into(),
            ..Default::default()
        };
        let (argv, _) = wrap(&c, vec![s("pg_dump"), s("app")], Vec::new());
        assert_eq!(argv, ["runuser", "-u", "postgres", "--", "pg_dump", "app"]);
    }
}
