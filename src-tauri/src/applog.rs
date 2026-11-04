//! The desktop app's log: `<app data>/logs/orluma.log`, rotated, secrets removed on the way in. The diagnostics
//! page reads it, and the interface and the Rust side both write to it, so a bug report holds both.

use orluma_core::log::LogFile;
use std::fmt::Display;
use std::sync::OnceLock;

static FILE: OnceLock<LogFile> = OnceLock::new();

pub fn init(path: std::path::PathBuf) {
    let _ = FILE.set(LogFile::new(path, 2 << 20));
    std::panic::set_hook(Box::new(|p| {
        let at = p
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_default();
        let what = p
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| p.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        error("panic", format!("{what} at {at}"));
    }));
}

pub fn write(level: &str, area: &str, msg: &str) {
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
