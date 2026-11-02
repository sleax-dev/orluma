//! What the server is: name, OS, uptime, disks.

use orluma_core::api::Disk;
use std::collections::HashSet;

pub fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "server".into())
}

pub fn uptime_secs() -> u64 {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|n| n.parse::<f64>().ok()))
        .map_or(0, |f| f as u64)
}

pub fn os_name() -> String {
    let text = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
    text.lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME="))
        .map(|v| v.trim_matches('"').to_string())
        .unwrap_or_else(|| "Linux".into())
}

pub fn timezone() -> String {
    if let Ok(tz) = std::env::var("TZ") {
        return tz;
    }
    if let Ok(s) = std::fs::read_to_string("/etc/timezone") {
        return s.trim().to_string();
    }
    std::fs::read_link("/etc/localtime")
        .ok()
        .and_then(|p| p.to_str().and_then(|s| s.split("zoneinfo/").nth(1)).map(str::to_string))
        .unwrap_or_else(|| "UTC".into())
}

const REAL_FS: &[&str] = &[
    "ext2", "ext3", "ext4", "xfs", "btrfs", "zfs", "f2fs", "jfs", "reiserfs", "nfs", "nfs4", "cifs", "fuseblk", "vfat",
    "exfat",
];

/// Mounted filesystems that hold data (not /proc, tmpfs, snaps), one entry per device.
pub fn disks() -> Vec<Disk> {
    let mounts = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for line in mounts.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 3 || !REAL_FS.contains(&f[2]) || f[1].starts_with("/snap/") || !seen.insert(f[0].to_string()) {
            continue;
        }
        let mount = f[1].replace("\\040", " ");
        if let Some((total, available)) = statvfs(&mount)
            && total > 0
        {
            out.push(Disk {
                mount,
                total,
                available,
            });
        }
    }
    out
}

pub fn statvfs(path: &str) -> Option<(u64, u64)> {
    let c = std::ffi::CString::new(path).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: c is a valid C string and s a properly sized out-parameter.
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let bs = s.f_frsize as u64;
    Some((s.f_blocks as u64 * bs, s.f_bavail as u64 * bs))
}
