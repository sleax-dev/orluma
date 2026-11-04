//! The agent's shared state: config, history, what is running, and the event stream the app listens to.

use crate::db::Db;
use crate::paths::Paths;
use crate::restic::anyhow_lite::{Error, Result};
use crate::restic::{Repo, Restic};
use orluma_core::api::{AgentEvent, RunProgress};
use orluma_core::model::{AgentConfig, CONFIG_VERSION, Destination};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::{Semaphore, broadcast};
use tokio_util::sync::CancellationToken;

pub struct Running {
    pub progress: RunProgress,
    pub cancel: CancellationToken,
}

pub struct Agent {
    pub paths: Paths,
    pub config: RwLock<AgentConfig>,
    pub db: Db,
    pub events: broadcast::Sender<AgentEvent>,
    pub running: Mutex<HashMap<String, Running>>,
    pub slots: Semaphore,
    /// Destinations a run is writing to: two runs on one repository would only wait on restic's lock.
    pub busy_repos: Mutex<HashMap<String, String>>,
    pub http: reqwest::Client,
    pub update_available: Mutex<Option<String>>,
    /// Repositories attached for this process only (see `AttachRequest`).
    pub attached: Mutex<HashMap<String, Repo>>,
    restic: tokio::sync::Mutex<Option<Restic>>,
}

pub type Shared = Arc<Agent>;

pub fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Agent {
    pub fn new(paths: Paths) -> std::io::Result<Shared> {
        paths.create()?;
        let config = load_config(&paths);
        let db = Db::open(&paths.history()).map_err(std::io::Error::other)?;
        let http = reqwest::Client::builder()
            .user_agent(concat!("Orluma-Agent/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(std::io::Error::other)?;
        let slots = config.agent.max_parallel.max(1) as usize;
        Ok(Arc::new(Agent {
            paths,
            config: RwLock::new(config),
            db,
            events: broadcast::channel(256).0,
            running: Mutex::new(HashMap::new()),
            slots: Semaphore::new(slots),
            busy_repos: Mutex::new(HashMap::new()),
            http,
            update_available: Mutex::new(None),
            attached: Mutex::new(HashMap::new()),
            restic: tokio::sync::Mutex::new(None),
        }))
    }

    pub fn config(&self) -> AgentConfig {
        self.config.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn save_config(&self, mut cfg: AgentConfig) -> Result<()> {
        let errs = cfg.validate();
        if !errs.is_empty() {
            return Err(Error(errs.join("\n")));
        }
        cfg.version = CONFIG_VERSION;
        let bytes = serde_json::to_vec_pretty(&cfg)?;
        crate::paths::write_private(&self.paths.config(), &bytes)?;
        *self.config.write().unwrap_or_else(|e| e.into_inner()) = cfg;
        let _ = self.events.send(AgentEvent::ConfigChanged);
        Ok(())
    }

    pub fn server_name(&self) -> String {
        let n = self.config().agent.name;
        if n.is_empty() { crate::sys::hostname() } else { n }
    }

    pub fn destination(&self, id: &str) -> Result<Destination> {
        self.config()
            .destinations
            .into_iter()
            .find(|d| d.id == id)
            .ok_or_else(|| Error(format!("no destination '{id}'")))
    }

    pub fn repo(&self, id: &str) -> Result<Repo> {
        if let Some(r) = lock(&self.attached).get(id) {
            return Ok(r.clone());
        }
        Repo::of(&self.destination(id)?, &self.paths)
    }

    /// restic, installed on first use.
    pub async fn restic(&self) -> Result<Restic> {
        let mut r = self.restic.lock().await;
        if let Some(r) = r.as_ref() {
            return Ok(r.clone());
        }
        let bin = crate::restic::ensure(&self.paths, &self.http).await?;
        let made = Restic {
            bin,
            cache: self.paths.cache.join("restic"),
        };
        *r = Some(made.clone());
        Ok(made)
    }

    pub fn progress(&self, p: RunProgress) {
        if let Some(r) = lock(&self.running).get_mut(&p.run_id) {
            r.progress = p.clone();
        }
        let _ = self.events.send(AgentEvent::Progress(p));
    }
}

fn load_config(paths: &Paths) -> AgentConfig {
    match std::fs::read(paths.config()) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            crate::logs::error(
                "config",
                format!("{e}; starting with an empty one (the file is kept as agent.json.broken)"),
            );
            let _ = std::fs::copy(paths.config(), paths.config().with_extension("json.broken"));
            AgentConfig {
                version: CONFIG_VERSION,
                ..Default::default()
            }
        }),
        Err(_) => AgentConfig {
            version: CONFIG_VERSION,
            ..Default::default()
        },
    }
}
