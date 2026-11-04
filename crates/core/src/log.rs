//! Logging shared by the agent and the desktop app: one line per event, secrets removed before a line is
//! written (so a log can be pasted into a bug report as it is), and a file that rotates instead of growing.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Mutex;

const KEYS: &[&str] = &[
    "password",
    "passwd",
    "passphrase",
    "secret",
    "token",
    "apikey",
    "api_key",
    "api-key",
    "access_key",
    "secret_key",
    "account_key",
    "private_key",
    "authorization",
    "webhook",
    "x-mac",
];

fn is_end(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '&' | ',' | ';' | '}' | ')' | '<' | '>')
}

/// `://user:secret@host` -> `://user:***@host`, Discord webhook tokens and Telegram bot tokens.
fn mask_urls(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("://") {
        out.push_str(&rest[..i + 3]);
        rest = &rest[i + 3..];
        let end = rest.find(is_end).unwrap_or(rest.len());
        let (url, tail) = rest.split_at(end);
        let (auth, path) = url.split_at(url.find('/').unwrap_or(url.len()));
        match auth.rsplit_once('@') {
            Some((cred, host)) => match cred.split_once(':') {
                Some((user, _)) => out.push_str(&format!("{user}:***@{host}")),
                None => out.push_str(auth),
            },
            None => out.push_str(auth),
        }
        let mut path = path.to_string();
        if let Some(at) = path.find("/webhooks/") {
            let after = at + "/webhooks/".len();
            if let Some(slash) = path[after..].find('/') {
                path.replace_range(after + slash + 1.., "***");
            }
        }
        if let Some(at) = path.find("/bot") {
            let after = at + "/bot".len();
            let tok_end = path[after..].find('/').map_or(path.len(), |n| after + n);
            if path[after..tok_end].contains(':') {
                path.replace_range(after..tok_end, "***");
            }
        }
        out.push_str(&path);
        rest = tail;
    }
    out.push_str(rest);
    out
}

/// `password=abc`, `"token": "abc"`, `Authorization: Bearer abc` -> the value becomes `***`.
fn mask_keys(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut at = 0;
    while at < s.len() {
        let hit = KEYS
            .iter()
            .filter_map(|k| lower[at..].find(k).map(|i| (at + i, k.len())))
            .min_by_key(|(i, l)| (*i, usize::MAX - *l));
        let Some((i, len)) = hit else { break };
        let mut j = i + len;
        let sep = |c: char| matches!(c, '"' | '\'' | ' ');
        while s[j..].starts_with(sep) {
            j += 1;
        }
        if s[j..].starts_with(['=', ':']) {
            j += 1;
            while s[j..].starts_with(sep) {
                j += 1;
            }
            // `Authorization: Bearer abc`: the scheme word is not the secret
            for scheme in ["Bearer ", "Basic ", "bearer ", "basic "] {
                if s[j..].starts_with(scheme) {
                    j += scheme.len();
                }
            }
            let end = s[j..].find(is_end).map_or(s.len(), |n| j + n);
            out.push_str(&s[at..j]);
            if end > j {
                out.push_str("***");
            }
            at = end;
        } else {
            out.push_str(&s[at..i + len]);
            at = i + len;
        }
    }
    out.push_str(&s[at..]);
    out
}

pub fn redact(s: &str) -> String {
    mask_keys(&mask_urls(s))
}

/// `2026-11-04T01:22:33.123Z INFO  agent    message`, one line, secrets masked.
pub fn line(level: &str, area: &str, msg: &str) -> String {
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ");
    let msg = redact(msg).replace(['\r', '\n'], " | ");
    let msg: String = msg.chars().take(2000).collect();
    format!("{ts} {level:<5} {area:<8} {msg}")
}

/// A log file that is moved to `<name>.1` when it passes `max` bytes, so at most about twice that is kept.
pub struct LogFile {
    path: PathBuf,
    max: u64,
    lock: Mutex<()>,
}

impl LogFile {
    pub fn new(path: PathBuf, max: u64) -> Self {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        Self {
            path,
            max,
            lock: Mutex::new(()),
        }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    fn old(&self) -> PathBuf {
        let mut o = self.path.clone().into_os_string();
        o.push(".1");
        PathBuf::from(o)
    }

    /// Writes one line; a log that cannot be written must never break what it describes.
    pub fn write(&self, level: &str, area: &str, msg: &str) {
        let line = line(level, area, msg);
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        if std::fs::metadata(&self.path).is_ok_and(|m| m.len() > self.max) {
            let _ = std::fs::rename(&self.path, self.old());
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&self.path) {
            let _ = writeln!(f, "{line}");
        }
    }

    /// The last `n` lines (older file first), reading no more than the last 1 MiB of each file.
    pub fn tail(&self, n: usize) -> Vec<String> {
        let _g = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut lines = Vec::new();
        for p in [self.old(), self.path.clone()] {
            let Ok(mut f) = std::fs::File::open(&p) else { continue };
            let len = f.metadata().map_or(0, |m| m.len());
            let from = len.saturating_sub(1 << 20);
            let mut buf = Vec::new();
            if f.seek(SeekFrom::Start(from)).is_ok() && f.read_to_end(&mut buf).is_ok() {
                let text = String::from_utf8_lossy(&buf);
                let mut it = text.lines();
                if from > 0 {
                    it.next(); // a cut line
                }
                lines.extend(it.map(str::to_string));
            }
        }
        let skip = lines.len().saturating_sub(n);
        lines.split_off(skip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_do_not_survive() {
        for (input, gone) in [
            ("password=hunter2 next", "hunter2"),
            ("{\"password\": \"hunter2\", \"x\": 1}", "hunter2"),
            ("failed: postgres://app:hunter2@db:5432/x", "hunter2"),
            ("Authorization: Bearer hunter2", "hunter2"),
            ("POST https://discord.com/api/webhooks/123/hunter2", "hunter2"),
            ("https://api.telegram.org/bot123456:hunter2/sendMessage", "hunter2"),
            ("RESTIC_PASSWORD=hunter2 restic backup", "hunter2"),
            ("secret_key: hunter2", "hunter2"),
        ] {
            let r = redact(input);
            assert!(!r.contains(gone), "{input} -> {r}");
            assert!(r.contains("***"), "{input} -> {r}");
        }
        assert_eq!(redact("backup finished in 3s"), "backup finished in 3s");
        assert_eq!(redact("user=root host=10.0.0.1"), "user=root host=10.0.0.1");
        assert!(redact("https://discord.com/api/webhooks/123/x").contains("/webhooks/123/"));
    }

    #[test]
    fn file_rotates_and_tails() {
        let dir = std::env::temp_dir().join(format!("orluma-log-{}", std::process::id()));
        let f = LogFile::new(dir.join("t.log"), 300);
        for i in 0..30 {
            f.write("INFO", "test", &format!("line {i} password=abc"));
        }
        let t = f.tail(1000);
        assert!(t.last().unwrap().contains("line 29"));
        assert!(t.iter().all(|l| !l.contains("abc")));
        assert!(t.len() < 30, "the oldest lines were dropped by rotation");
        assert_eq!(f.tail(2).len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
