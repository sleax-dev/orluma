//! Google Drive, OneDrive and Dropbox, connected from the app without a terminal.
//!
//! rclone (a pinned, hash-checked download, like restic) runs `rclone authorize` here on the server. It listens
//! on 127.0.0.1:53682 for the sign-in page's answer; the app forwards that port from the PC over SSH and opens
//! the sign-in page in the browser there. The token rclone gets back is written straight into Orluma's own
//! rclone config (`/etc/orluma/rclone.conf`, root only): it never goes to the app and never into a command
//! line. restic then reaches the account through `rclone serve restic`, which refreshes the token in that file.

use crate::paths::{Paths, set_mode, write_private};
use crate::restic::anyhow_lite::{Error, Result, bail};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::{Child, Command};

const RCLONE_VERSION: &str = "1.75.2";
/// SHA-256 of the release zips, from rclone's signed SHA256SUMS and checked against the downloads.
const RCLONE_SHA256: &[(&str, &str)] = &[
    (
        "amd64",
        "349ac8fba6ff65d6247043f1750cdcb518ec5d500ef91463a10d37c0ccdf3702",
    ),
    (
        "arm64",
        "7e1e8d69654941b7b7df84ee74c5f7fb09cce7d5496947fb7bf16b55ff427d10",
    ),
];

/// The port `rclone authorize` waits on; the providers send the browser back to it.
pub const AUTH_PORT: u16 = 53682;

/// Providers the app connects by itself, and their rclone backend.
fn backend(provider: &str) -> Result<&'static str> {
    Ok(match provider {
        "drive" => "drive",
        "onedrive" => "onedrive",
        "dropbox" => "dropbox",
        p => bail!("unknown cloud provider {p}"),
    })
}

/// Orluma's rclone, installed on first use.
pub async fn ensure(paths: &Paths, http: &reqwest::Client) -> Result<PathBuf> {
    let bin = paths.rclone_bin();
    if bin.is_file() {
        return Ok(bin);
    }
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        a => bail!("there is no rclone build for {a}"),
    };
    let want = RCLONE_SHA256
        .iter()
        .find(|(a, _)| *a == arch)
        .map(|(_, h)| *h)
        .unwrap_or_default();
    let name = format!("rclone-v{RCLONE_VERSION}-linux-{arch}");
    let url = format!("https://github.com/rclone/rclone/releases/download/v{RCLONE_VERSION}/{name}.zip");
    let bytes = http.get(&url).send().await?.error_for_status()?.bytes().await?;
    use sha2::Digest;
    let got = hex::encode(sha2::Sha256::digest(&bytes));
    if got != want {
        bail!("the rclone download did not match its published checksum (got {got})");
    }
    let plain = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
        let mut f = zip.by_name(&format!("{name}/rclone"))?;
        let mut out = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut out)?;
        Ok(out)
    })
    .await
    .map_err(|e| Error(e.to_string()))??;
    std::fs::create_dir_all(bin.parent().unwrap_or(&paths.data))?;
    write_private(&bin, &plain)?;
    set_mode(&bin, 0o700)?;
    Ok(bin)
}

struct Session {
    provider: String,
    child: Child,
}

/// One sign-in at a time: rclone's port is fixed.
static SESSION: tokio::sync::Mutex<Option<Session>> = tokio::sync::Mutex::const_new(None);

/// Starts `rclone authorize` and returns the sign-in address to open in the browser.
pub async fn start(paths: &Paths, http: &reqwest::Client, provider: &str) -> Result<String> {
    let backend = backend(provider)?;
    let bin = ensure(paths, http).await?;
    let mut slot = SESSION.lock().await;
    if let Some(mut old) = slot.take() {
        let _ = old.child.kill().await;
    }
    let mut child = Command::new(&bin)
        .args(["authorize", backend, "--auth-no-open-browser"])
        // nothing from a config file is wanted here; the result is printed, not stored
        .env("RCLONE_CONFIG", "/dev/null")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    // rclone prints "Please go to the following link: http://127.0.0.1:53682/auth?state=..." on stderr
    let mut lines = BufReader::new(child.stderr.take().ok_or_else(|| Error("no stderr".into()))?).lines();
    let find = async {
        let mut seen = Vec::new();
        while let Ok(Some(l)) = lines.next_line().await {
            if let Some(i) = l.find(&format!("http://127.0.0.1:{AUTH_PORT}/auth")) {
                return Ok(l[i..].split_whitespace().next().unwrap_or_default().to_string());
            }
            seen.push(l);
        }
        Err(Error(format!(
            "rclone did not start the sign-in: {}",
            seen.last().map(String::as_str).unwrap_or("no output")
        )))
    };
    let url = tokio::time::timeout(Duration::from_secs(30), find)
        .await
        .map_err(|_| Error("rclone did not start the sign-in in time".into()))??;
    // keep reading stderr so rclone never blocks on a full pipe
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    *slot = Some(Session {
        provider: provider.into(),
        child,
    });
    Ok(url)
}

pub async fn cancel() {
    if let Some(mut s) = SESSION.lock().await.take() {
        let _ = s.child.kill().await;
    }
}

/// Waits (up to ten minutes) for the person to sign in, stores the account as a new remote and checks that it
/// answers. Returns the remote's name.
pub async fn finish(paths: &Paths, http: &reqwest::Client, provider: &str) -> Result<String> {
    let mut s = SESSION
        .lock()
        .await
        .take()
        .ok_or_else(|| Error("no sign-in is waiting; start again".into()))?;
    if s.provider != provider {
        let _ = s.child.kill().await;
        bail!("a sign-in for another provider was waiting; start again");
    }
    let mut out = String::new();
    let mut stdout = s.child.stdout.take().ok_or_else(|| Error("no stdout".into()))?;
    let wait = async {
        stdout.read_to_string(&mut out).await?;
        s.child.wait().await
    };
    let status = match tokio::time::timeout(Duration::from_secs(600), wait).await {
        Ok(r) => r?,
        Err(_) => {
            let _ = s.child.kill().await;
            bail!("nobody signed in within ten minutes");
        }
    };
    let token = token_of(&out).ok_or_else(|| {
        Error(if status.success() {
            "rclone gave no token back".into()
        } else {
            "the sign-in was not completed".into()
        })
    })?;
    let backend = backend(provider)?;
    let mut fields = vec![("type", backend.to_string()), ("token", token.clone())];
    if provider == "onedrive" {
        // rclone's own setup asks Microsoft which drive to use; the same question, answered with the new token
        let (id, kind) = onedrive_drive(http, &token).await?;
        fields.push(("drive_id", id));
        fields.push(("drive_type", kind));
    }
    let name = format!("orluma-{provider}-{}", hex::encode(rand_bytes()));
    write_remote(&paths.rclone_conf(), &name, &fields)?;
    // a remote that does not answer is better found now than at 03:00
    let check = Command::new(paths.rclone_bin())
        .args(["lsd", &format!("{name}:"), "--max-depth", "1"])
        .env("RCLONE_CONFIG", paths.rclone_conf())
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(Duration::from_secs(60), check).await {
        Ok(Ok(o)) if o.status.success() => Ok(name),
        Ok(Ok(o)) => {
            let e = String::from_utf8_lossy(&o.stderr);
            bail!(
                "the account was connected but does not answer: {}",
                e.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("")
            )
        }
        _ => bail!("the account was connected but did not answer in time"),
    }
}

fn rand_bytes() -> [u8; 4] {
    let mut b = [0u8; 4];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = std::io::Read::read_exact(&mut f, &mut b);
    }
    b
}

/// The JSON object rclone prints between "--->" and "<---End paste".
fn token_of(out: &str) -> Option<String> {
    let body = out.split("--->").nth(1).unwrap_or(out);
    let body = body.split("<---").next().unwrap_or(body);
    let t = body.trim();
    (t.starts_with('{') && t.contains("access_token") && serde_json::from_str::<serde_json::Value>(t).is_ok())
        .then(|| t.to_string())
}

async fn onedrive_drive(http: &reqwest::Client, token: &str) -> Result<(String, String)> {
    let access = serde_json::from_str::<serde_json::Value>(token)?
        .get("access_token")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let v: serde_json::Value = http
        .get("https://graph.microsoft.com/v1.0/me/drive")
        .bearer_auth(access)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let id = v.get("id").and_then(|x| x.as_str()).unwrap_or_default();
    let kind = v.get("driveType").and_then(|x| x.as_str()).unwrap_or_default();
    if id.is_empty() || kind.is_empty() {
        bail!("Microsoft did not say which OneDrive belongs to this account");
    }
    Ok((id.to_string(), kind.to_string()))
}

/// Adds or replaces one `[name]` section of an rclone config, keeping the others, root only.
fn write_remote(conf: &std::path::Path, name: &str, fields: &[(&str, String)]) -> Result<()> {
    let old = std::fs::read_to_string(conf).unwrap_or_default();
    let mut out = String::new();
    let mut skip = false;
    for line in old.lines() {
        if line.starts_with('[') {
            skip = line.trim() == format!("[{name}]");
        }
        if !skip {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !out.is_empty() && !out.ends_with("\n\n") {
        out.push('\n');
    }
    out.push_str(&format!("[{name}]\n"));
    for (k, v) in fields {
        out.push_str(&format!("{k} = {}\n", v.replace('\n', "")));
    }
    write_private(conf, out.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_and_config() {
        let out = "Paste the following into your remote machine --->\n{\"access_token\":\"a\",\"token_type\":\"Bearer\",\"refresh_token\":\"r\",\"expiry\":\"2026-11-04T17:00:00Z\"}\n<---End paste\n";
        assert_eq!(
            token_of(out).as_deref(),
            Some(
                "{\"access_token\":\"a\",\"token_type\":\"Bearer\",\"refresh_token\":\"r\",\"expiry\":\"2026-11-04T17:00:00Z\"}"
            )
        );
        assert!(token_of("Failed to get token").is_none());

        let dir = std::env::temp_dir().join(format!("orluma-rclone-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let conf = dir.join("rclone.conf");
        std::fs::write(
            &conf,
            "[mine]\ntype = sftp\nhost = x\n\n[orluma-drive-1]\ntype = drive\ntoken = old\n",
        )
        .unwrap();
        write_remote(
            &conf,
            "orluma-drive-1",
            &[("type", "drive".into()), ("token", "{\"a\":1}".into())],
        )
        .unwrap();
        let s = std::fs::read_to_string(&conf).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(s.contains("[mine]\ntype = sftp\nhost = x\n"), "{s}");
        assert!(s.contains("[orluma-drive-1]\ntype = drive\ntoken = {\"a\":1}\n"), "{s}");
        assert!(!s.contains("token = old"), "{s}");
    }
}
