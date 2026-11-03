//! What the app keeps on this PC: settings, the server list, its SSH key and the password of the PC's own
//! repository. All of it in the app's data folder, which only this Windows/macOS/Linux user can read.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Option<String>,
    pub setup_done: bool,
    /// Closing the window keeps Orluma in the tray, so scheduled PC copies and alerts keep working.
    pub run_in_background: bool,
    pub start_with_os: bool,
    pub auto_update: bool,
    /// Desktop notifications for failed or warning runs on any server.
    pub notify_desktop: bool,
    pub notify_success: bool,
    /// Where copies to this PC go. Empty means the app's data folder.
    pub pc_storage_dir: String,
    pub pc_syncs: Vec<PcSync>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: None,
            setup_done: false,
            run_in_background: true,
            start_with_os: false,
            auto_update: true,
            notify_desktop: true,
            notify_success: false,
            pc_storage_dir: String::new(),
            pc_syncs: Vec::new(),
        }
    }
}

/// "Copy this server's backups to this PC every N hours, while the app runs."
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PcSync {
    pub id: String,
    pub server_id: String,
    pub destination_id: String,
    /// Only this job's snapshots; empty means all of the destination's.
    pub job_id: String,
    pub every_hours: u32,
    pub enabled: bool,
    /// Apply the job's retention on the PC too. Off keeps every copy (an append-only archive).
    pub apply_retention: bool,
    pub last_run: Option<DateTime<Utc>>,
    pub last_status: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    /// SHA256 fingerprint of the server's host key, pinned on first contact.
    pub host_key: String,
    pub added: Option<DateTime<Utc>>,
}

pub struct Store {
    pub dir: PathBuf,
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(p: &Path) -> T {
    std::fs::read(p)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn write_private(p: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = p.with_extension("tmp");
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(bytes)?;
    }
    #[cfg(not(unix))]
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, p)
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        Self { dir }
    }

    pub fn settings(&self) -> Settings {
        read_json(&self.dir.join("settings.json"))
    }
    pub fn save_settings(&self, s: &Settings) -> std::io::Result<()> {
        write_private(&self.dir.join("settings.json"), &serde_json::to_vec_pretty(s)?)
    }

    pub fn servers(&self) -> Vec<Server> {
        read_json(&self.dir.join("servers.json"))
    }
    pub fn save_servers(&self, s: &[Server]) -> std::io::Result<()> {
        write_private(&self.dir.join("servers.json"), &serde_json::to_vec_pretty(s)?)
    }

    /// The key this app logs in with. Made once, never leaves the PC; its public half goes to each server.
    pub fn ssh_key(&self) -> std::io::Result<russh::keys::PrivateKey> {
        let path = self.dir.join("id_ed25519");
        if let Ok(k) = russh::keys::load_secret_key(&path, None) {
            return Ok(k);
        }
        let mut seed = [0u8; 32];
        rand::fill(&mut seed);
        let pair = russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&seed);
        let mut key = russh::keys::PrivateKey::from(pair);
        key.set_comment(format!("orluma@{}", hostname()));
        let text = key
            .to_openssh(russh::keys::ssh_key::LineEnding::LF)
            .map_err(std::io::Error::other)?;
        write_private(&path, text.as_bytes())?;
        Ok(key)
    }

    pub fn public_key(&self) -> std::io::Result<String> {
        let k = self.ssh_key()?;
        k.public_key().to_openssh().map_err(std::io::Error::other)
    }

    /// The PC repository's password: random, made once. Without it the copies on this PC cannot be read.
    pub fn pc_password(&self) -> std::io::Result<String> {
        let path = self.dir.join("pc-repository.password");
        if let Ok(p) = std::fs::read_to_string(&path)
            && p.trim().len() >= 16
        {
            return Ok(p.trim().to_string());
        }
        let pw = random_token(32);
        write_private(&path, pw.as_bytes())?;
        Ok(pw)
    }

    /// The password of one backup folder on this PC: the PC's own, unless the folder came from another PC.
    pub fn repo_password(&self, folder: &str) -> std::io::Result<String> {
        let imported: std::collections::HashMap<String, String> =
            read_json(&self.dir.join("imported-repositories.json"));
        match imported.get(folder) {
            Some(p) => Ok(p.clone()),
            None => self.pc_password(),
        }
    }

    pub fn save_repo_password(&self, folder: &str, password: &str) -> std::io::Result<()> {
        let path = self.dir.join("imported-repositories.json");
        let mut imported: std::collections::HashMap<String, String> = read_json(&path);
        imported.insert(folder.to_string(), password.to_string());
        write_private(&path, &serde_json::to_vec_pretty(&imported)?)
    }

    pub fn pc_root(&self, s: &Settings) -> PathBuf {
        if s.pc_storage_dir.trim().is_empty() {
            self.dir.join("pc-backups")
        } else {
            PathBuf::from(&s.pc_storage_dir)
        }
    }
}

pub fn random_token(n: usize) -> String {
    const ABC: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let mut b = vec![0u8; n];
    rand::fill(&mut b[..]);
    b.iter().map(|x| ABC[*x as usize % ABC.len()] as char).collect()
}

pub fn hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "pc".into())
}
