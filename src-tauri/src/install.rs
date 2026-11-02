//! Putting the agent on a server: check what it is, fetch the signed agent build, upload it over SSH and
//! let it install itself as a systemd service. Also authorizes this app's key, so the password typed when
//! adding a server is used once and never stored.

use crate::ssh::{Conn, Error, Result};

const REPO: &str = "sleax-dev/orluma";

pub struct ServerFacts {
    pub arch: String,
    pub root: bool,
    pub systemd: bool,
}

pub async fn facts(conn: &Conn) -> Result<ServerFacts> {
    let o = conn
        .exec(
            "uname -m; id -u; command -v systemctl >/dev/null && echo systemd || echo none",
            None,
        )
        .await?;
    let mut l = o.stdout.lines();
    Ok(ServerFacts {
        arch: l.next().unwrap_or("").trim().to_string(),
        root: l.next().map(str::trim) == Some("0"),
        systemd: l.next().map(str::trim) == Some("systemd"),
    })
}

pub async fn authorize_key(conn: &Conn, public_key: &str) -> Result<()> {
    let key = public_key.trim().replace('\'', "");
    let cmd = format!(
        "umask 077; mkdir -p ~/.ssh && touch ~/.ssh/authorized_keys && (grep -qxF '{key}' ~/.ssh/authorized_keys || echo '{key}' >> ~/.ssh/authorized_keys)"
    );
    let o = conn.exec(&cmd, None).await?;
    if o.code != 0 {
        return Err(Error(format!(
            "could not add the app's key to ~/.ssh/authorized_keys: {}",
            o.stderr.trim()
        )));
    }
    Ok(())
}

fn asset(arch: &str) -> Result<&'static str> {
    match arch {
        "x86_64" | "amd64" => Ok("orluma-agent-x86_64-linux"),
        "aarch64" | "arm64" => Ok("orluma-agent-aarch64-linux"),
        a => Err(Error(format!(
            "UNSUPPORTED_ARCH: there is no agent build for {a} (x86_64 and arm64 are supported)"
        ))),
    }
}

fn verify(data: &[u8], sig_b64: &str) -> Result<()> {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD;
    let dec = |s: &str| {
        b64.decode(s.trim())
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .ok_or_else(|| Error("bad signature encoding".into()))
    };
    let pk = minisign_verify::PublicKey::decode(&dec(orluma_core::UPDATE_PUBKEY)?).map_err(|e| Error(e.to_string()))?;
    let sig = minisign_verify::Signature::decode(&dec(sig_b64)?).map_err(|e| Error(e.to_string()))?;
    pk.verify(data, &sig, false)
        .map_err(|_| Error("the downloaded agent's signature is not valid; nothing was installed".into()))
}

/// The agent build for `arch`: a local file when ORLUMA_AGENT_BINARY points at one (development), otherwise
/// the release matching this app, checked against the release key.
async fn agent_binary(http: &reqwest::Client, arch: &str) -> Result<Vec<u8>> {
    if let Ok(p) = std::env::var("ORLUMA_AGENT_BINARY") {
        return std::fs::read(&p).map_err(|e| Error(format!("ORLUMA_AGENT_BINARY {p}: {e}")));
    }
    let name = asset(arch)?;
    let version = env!("CARGO_PKG_VERSION");
    let base = format!("https://github.com/{REPO}/releases/download/v{version}/{name}");
    let get = |url: String| async move {
        let r = http.get(&url).send().await.map_err(|e| Error(e.to_string()))?;
        if !r.status().is_success() {
            return Err(Error(format!("download {url}: HTTP {}", r.status())));
        }
        r.bytes().await.map_err(|e| Error(e.to_string()))
    };
    let bin = get(base.clone()).await?;
    let sig = get(format!("{base}.sig")).await?;
    verify(&bin, &String::from_utf8_lossy(&sig))?;
    Ok(bin.to_vec())
}

pub async fn install_agent(conn: &Conn, http: &reqwest::Client) -> Result<String> {
    let f = facts(conn).await?;
    if !f.root {
        return Err(Error("NOT_ROOT: log in as root. The agent reads every file and database it backs up, and its socket is root-only".into()));
    }
    if !f.systemd {
        return Err(Error(
            "NO_SYSTEMD: the agent runs as a systemd service; this server has no systemd".into(),
        ));
    }
    let bin = agent_binary(http, &f.arch).await?;
    let tmp = format!("/tmp/orluma-agent-{}", crate::store::random_token(8));
    let cmd = format!("cat > {tmp} && chmod 755 {tmp} && {tmp} install; s=$?; rm -f {tmp}; exit $s");
    let o = conn.exec(&cmd, Some(&bin)).await?;
    if o.code != 0 {
        return Err(Error(format!(
            "installing the agent failed: {}",
            (o.stderr + &o.stdout).trim()
        )));
    }
    // The service needs a moment before its socket answers.
    for _ in 0..20 {
        if let Ok((200, _)) = conn.agent("POST", "/v1/status", Some(b"{}".to_vec())).await {
            return Ok(o.stdout.trim().to_string());
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    Err(Error(
        "the agent was installed but does not answer; see `journalctl -u orluma-agent` on the server".into(),
    ))
}

pub async fn uninstall_agent(conn: &Conn, purge: bool) -> Result<()> {
    let o = conn
        .exec(
            &format!(
                "/usr/local/bin/orluma-agent uninstall{}",
                if purge { " --purge" } else { "" }
            ),
            None,
        )
        .await?;
    if o.code != 0 {
        return Err(Error(o.stderr.trim().to_string()));
    }
    Ok(())
}
