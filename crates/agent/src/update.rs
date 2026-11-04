//! Self-update from GitHub releases. A new binary is only installed when its minisign signature checks out
//! against the key built into this one, the same key that signs the desktop app's updates.

use crate::notify::Message;
use crate::restic::anyhow_lite::{Error, Result, bail};
use crate::state::{Shared, lock};
use orluma_core::model::Event;

const REPO: &str = "sleax-dev/orluma";

fn asset() -> Result<String> {
    match std::env::consts::ARCH {
        "x86_64" => Ok("orluma-agent-x86_64-linux".into()),
        "aarch64" => Ok("orluma-agent-aarch64-linux".into()),
        a => bail!("no agent build for {a}"),
    }
}

fn newer(a: &str, b: &str) -> bool {
    let p = |s: &str| {
        s.trim_start_matches('v')
            .split('.')
            .map(|n| n.parse::<u64>().unwrap_or(0))
            .collect::<Vec<_>>()
    };
    p(a) > p(b)
}

pub async fn latest(agent: &Shared) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct Rel {
        tag_name: String,
    }
    let r: Rel = agent
        .http
        .get(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(r.tag_name.trim_start_matches('v').to_string())
}

/// Checks, and installs when allowed. Called every few hours by the scheduler.
pub async fn auto(agent: &Shared) {
    let Ok(v) = latest(agent).await else { return };
    if !newer(&v, env!("CARGO_PKG_VERSION")) {
        *lock(&agent.update_available) = None;
        return;
    }
    *lock(&agent.update_available) = Some(v.clone());
    // Never swap the binary under a running backup; the next check tries again.
    if !lock(&agent.running).is_empty() {
        return;
    }
    if let Err(e) = install(agent, &v).await {
        crate::logs::error("update", format!("to {v}: {e}"));
    }
}

pub async fn install(agent: &Shared, version: &str) -> Result<()> {
    let version = if version.is_empty() {
        latest(agent).await?
    } else {
        version.trim_start_matches('v').to_string()
    };
    let name = asset()?;
    let base = format!("https://github.com/{REPO}/releases/download/v{version}/{name}");
    let bin = agent.http.get(&base).send().await?.error_for_status()?.bytes().await?;
    let sig = agent
        .http
        .get(format!("{base}.sig"))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    verify(&bin, &sig)?;
    let exe = std::env::current_exe()?;
    let tmp = exe.with_extension("new");
    std::fs::write(&tmp, &bin)?;
    crate::paths::set_mode(&tmp, 0o755)?;
    std::fs::rename(&tmp, &exe)?;
    crate::runner::alert(
        agent,
        Message {
            event: Event::AgentUpdated,
            server: agent.server_name(),
            job_id: None,
            job_name: None,
            title: format!("Agent updated to {version}"),
            body: format!("From {}.", env!("CARGO_PKG_VERSION")),
            fields: vec![],
        },
    );
    // systemd starts the new binary (Restart=always); give the alert a moment to leave.
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        std::process::exit(0);
    });
    Ok(())
}

/// `sig` is what `tauri signer sign` writes: the minisign signature file, base64-encoded.
pub fn verify(data: &[u8], sig: &str) -> Result<()> {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD;
    let key_text = String::from_utf8(b64.decode(orluma_core::UPDATE_PUBKEY.trim())?)?;
    let pk = minisign_verify::PublicKey::decode(&key_text).map_err(|e| Error(format!("update key: {e}")))?;
    let sig_text = String::from_utf8(b64.decode(sig.trim()).map_err(|e| Error(format!("signature: {e}")))?)?;
    let sig = minisign_verify::Signature::decode(&sig_text).map_err(|e| Error(format!("signature: {e}")))?;
    pk.verify(data, &sig, false)
        .map_err(|_| Error("the download's signature is not valid; nothing was installed".into()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn versions() {
        assert!(super::newer("0.2.0", "0.1.9"));
        assert!(super::newer("1.0.0", "0.10.0"));
        assert!(!super::newer("0.1.0", "0.1.0"));
    }
}
