//! Where the agent keeps things. `ORLUMA_ROOT` prefixes every path, so a test can run a whole agent in a
//! temporary directory without touching `/etc` or `/var`.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Paths {
    /// /etc/orluma: the config and the agent's SSH key. Root only.
    pub etc: PathBuf,
    /// /var/lib/orluma: run history, the restic binary.
    pub data: PathBuf,
    /// /var/cache/orluma: restic's index cache (rebuilt when lost).
    pub cache: PathBuf,
    /// /run/orluma/agent.sock: the only way in. Root only, reached over SSH.
    pub socket: PathBuf,
}

impl Paths {
    pub fn from_env() -> Self {
        let root = std::env::var_os("ORLUMA_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let at = |p: &str| root.join(p.trim_start_matches('/'));
        Self {
            etc: at("/etc/orluma"),
            data: at("/var/lib/orluma"),
            cache: at("/var/cache/orluma"),
            socket: at("/run/orluma/agent.sock"),
        }
    }

    pub fn config(&self) -> PathBuf {
        self.etc.join("agent.json")
    }
    pub fn ssh_key(&self) -> PathBuf {
        self.etc.join("id_ed25519")
    }
    pub fn known_hosts(&self) -> PathBuf {
        self.etc.join("known_hosts")
    }
    pub fn history(&self) -> PathBuf {
        self.data.join("history.db")
    }
    pub fn restic_bin(&self) -> PathBuf {
        self.data.join("bin").join("restic")
    }
    pub fn secret_file(&self, name: &str) -> PathBuf {
        self.etc.join(name)
    }

    pub fn create(&self) -> std::io::Result<()> {
        for d in [&self.etc, &self.data, &self.cache] {
            std::fs::create_dir_all(d)?;
        }
        if let Some(p) = self.socket.parent() {
            std::fs::create_dir_all(p)?;
        }
        set_mode(&self.etc, 0o700)?;
        set_mode(&self.data, 0o700)
    }
}

pub fn set_mode(p: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode))
}

/// Writes through a temporary file and a rename, so a crash never leaves half a file, and creates it
/// root-only before any byte of a secret is in it.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}
