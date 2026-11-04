//! orluma-agent: Orluma on a server. `orluma-agent install` sets it up as a systemd service; the desktop app
//! does that for you over SSH.

mod db;
mod discover;
mod logs;
mod notify;
mod paths;
mod rclone;
mod restic;
mod restore_db;
mod runner;
mod scheduler;
mod server;
mod sources;
mod state;
mod sys;
mod update;

use clap::{Parser, Subcommand};
use paths::Paths;
use std::process::Command;

#[derive(Parser)]
#[command(name = "orluma-agent", version, about = "Orluma's backup agent")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the agent (what the systemd service does).
    Serve,
    /// Install this binary as a systemd service and start it.
    Install,
    /// Stop and remove the service. --purge also deletes the config and history (not the backups).
    Uninstall {
        #[arg(long)]
        purge: bool,
    },
    /// Print the agent's public SSH key (for SFTP destinations on other servers).
    PublicKey,
}

const BIN: &str = "/usr/local/bin/orluma-agent";
const UNIT: &str = "/etc/systemd/system/orluma-agent.service";
const UNIT_TEXT: &str = "[Unit]
Description=Orluma backup agent
Documentation=https://github.com/sleax-dev/orluma
After=network-online.target docker.service postgresql.service mysql.service mariadb.service
Wants=network-online.target

[Service]
ExecStart=/usr/local/bin/orluma-agent serve
Restart=always
RestartSec=5
KillMode=mixed
TimeoutStopSec=60

[Install]
WantedBy=multi-user.target
";

fn ensure_key(paths: &Paths) {
    let key = paths.ssh_key();
    if key.exists() {
        return;
    }
    let comment = format!("orluma@{}", sys::hostname());
    let ok = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C", &comment, "-f"])
        .arg(&key)
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        eprintln!(
            "ssh-keygen is not available: SFTP destinations will not work until it is (apt install openssh-client)"
        );
    }
}

fn sh(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd).args(args).status().is_ok_and(|s| s.success())
}

fn install() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    if exe != std::path::Path::new(BIN) {
        let tmp = format!("{BIN}.new");
        std::fs::copy(&exe, &tmp)?;
        paths::set_mode(std::path::Path::new(&tmp), 0o755)?;
        std::fs::rename(&tmp, BIN)?;
    }
    let p = Paths::from_env();
    p.create()?;
    ensure_key(&p);
    std::fs::write(UNIT, UNIT_TEXT)?;
    sh("systemctl", &["daemon-reload"]);
    sh("systemctl", &["enable", "orluma-agent"]);
    if !sh("systemctl", &["restart", "orluma-agent"]) {
        return Err(std::io::Error::other(
            "systemctl could not start orluma-agent; see journalctl -u orluma-agent",
        ));
    }
    println!("orluma-agent {} is installed and running", env!("CARGO_PKG_VERSION"));
    Ok(())
}

fn uninstall(purge: bool) -> std::io::Result<()> {
    sh("systemctl", &["disable", "--now", "orluma-agent"]);
    let _ = std::fs::remove_file(UNIT);
    sh("systemctl", &["daemon-reload"]);
    let _ = std::fs::remove_file(BIN);
    if purge {
        let p = Paths::from_env();
        for d in [&p.etc, &p.data, &p.cache] {
            let _ = std::fs::remove_dir_all(d);
        }
    }
    println!(
        "orluma-agent removed{}. Backups in your destinations are untouched.",
        if purge { " with its config and history" } else { "" }
    );
    Ok(())
}

fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cli = Cli::parse();
    let res = match cli.cmd.unwrap_or(Cmd::Serve) {
        Cmd::Install => install(),
        Cmd::Uninstall { purge } => uninstall(purge),
        Cmd::PublicKey => {
            let p = Paths::from_env();
            ensure_key(&p);
            std::fs::read_to_string(p.ssh_key().with_extension("pub")).map(|k| print!("{k}"))
        }
        Cmd::Serve => serve(),
    };
    if let Err(e) = res {
        eprintln!("orluma-agent: {e}");
        std::process::exit(1);
    }
}

fn serve() -> std::io::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let paths = Paths::from_env();
        logs::init(paths.data.join("agent.log"));
        let agent = state::Agent::new(paths.clone())?;
        ensure_key(&paths);
        tokio::spawn(scheduler::run(agent.clone()));
        // restic in the background, so the first backup does not wait for the download.
        let a = agent.clone();
        tokio::spawn(async move {
            if let Err(e) = a.restic().await {
                logs::warn("restic", e);
            }
        });
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            r = server::serve(agent.clone()) => r,
            _ = term.recv() => {
                for r in state::lock(&agent.running).values() {
                    r.cancel.cancel();
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                Ok(())
            }
        }
    })
}
