//! Orluma's shared model. The agent and the desktop app both build on it, so the two always agree on what
//! a job, a destination or a run is.

pub mod api;
pub mod model;
pub mod schedule;

/// Short random ids for jobs, sources and destinations (`j_3f9a1c2b`).
pub fn new_id(prefix: &str) -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    format!("{prefix}_{:08x}", h.finish() as u32)
}

/// The minisign key (base64, as Tauri's updater takes it) that signs every release: the desktop app checks
/// its own updates with it, and the agent checks a new agent binary with it before replacing itself.
pub const UPDATE_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDMyMTU0RTkxNEFDRjI1RDYKUldUV0pjOUtrVTRWTWc2aGM0NXVkWlFJM3RHQ0FiazM5VTA1anVoRStVdlR2ZlpDdFdaN1ovZzgK";
