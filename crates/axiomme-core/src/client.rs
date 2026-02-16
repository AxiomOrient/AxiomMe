use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::config::AppConfig;
use crate::error::Result;
use crate::fs::LocalContextFs;
use crate::index::InMemoryIndex;
use crate::parse::ParserRegistry;
use crate::retrieval::{DrrConfig, DrrEngine};
use crate::state::SqliteStateStore;

mod benchmark;
mod eval;
mod indexing_service;
mod markdown_editor_service;
mod mirror_outbox_service;
mod om_bridge_service;
mod queue_reconcile_service;
mod relation_service;
mod release;
mod request_log_service;
mod resource_service;
mod runtime_service;
mod search;
mod trace;

pub use benchmark::BenchmarkFixtureCreateOptions;

#[derive(Clone)]
pub struct AxiomMe {
    pub fs: LocalContextFs,
    pub state: SqliteStateStore,
    pub index: Arc<RwLock<InMemoryIndex>>,
    config: Arc<AppConfig>,
    markdown_edit_gate: Arc<RwLock<()>>,
    parser_registry: ParserRegistry,
    drr: DrrEngine,
}

impl std::fmt::Debug for AxiomMe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AxiomMe").finish_non_exhaustive()
    }
}

impl AxiomMe {
    pub fn new(root_dir: impl Into<PathBuf>) -> Result<Self> {
        let root = root_dir.into();
        fs::create_dir_all(&root)?;
        let config = Arc::new(AppConfig::from_env()?);
        crate::embedding::configure_runtime(config.embedding.clone())?;
        let fs = LocalContextFs::new(&root);
        let state = SqliteStateStore::open(root.join(".axiomme_state.sqlite3"))?;
        let index = Arc::new(RwLock::new(InMemoryIndex::new()));

        Ok(Self {
            fs,
            state,
            index,
            config,
            markdown_edit_gate: Arc::new(RwLock::new(())),
            parser_registry: ParserRegistry::new(),
            drr: DrrEngine::new(DrrConfig::default()),
        })
    }

    pub fn bootstrap(&self) -> Result<()> {
        self.fs.initialize()?;
        Ok(())
    }

    pub fn prepare_runtime(&self) -> Result<()> {
        self.bootstrap()?;
        self.ensure_scope_tiers()?;
        self.initialize_runtime_index()?;
        Ok(())
    }

    pub fn initialize(&self) -> Result<()> {
        self.prepare_runtime()
    }
}
#[cfg(test)]
mod tests;
