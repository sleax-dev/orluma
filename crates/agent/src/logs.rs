//! The agent's log: every line goes to stderr (journald keeps it) and to `/var/lib/orluma/agent.log`, which the
//! app reads over the API for its diagnostics page. Secrets are removed by `orluma_core::log` before a line is kept.

use orluma_core::log::LogFile;
use std::fmt::Display;
use std::sync::OnceLock;

static FILE: OnceLock<LogFile> = OnceLock::new();

pub fn init(path: std::path::PathBuf) {
    let _ = FILE.set(LogFile::new(path, 2 << 20));
}

fn write(level: &str, area: &str, msg: &str) {
    eprintln!("{}", orluma_core::log::line(level, area, msg));
    if let Some(f) = FILE.get() {
        f.write(level, area, msg);
    }
}

pub fn info(area: &str, msg: impl Display) {
    write("INFO", area, &msg.to_string());
}

pub fn warn(area: &str, msg: impl Display) {
    write("WARN", area, &msg.to_string());
}

pub fn error(area: &str, msg: impl Display) {
    write("ERROR", area, &msg.to_string());
}

pub fn tail(n: usize) -> Vec<String> {
    FILE.get().map(|f| f.tail(n)).unwrap_or_default()
}
