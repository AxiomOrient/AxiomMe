use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::error::Result;
use crate::fs::LocalContextFs;
use crate::index::InMemoryHybridIndex;
use crate::parse::ParserRegistry;
use crate::qdrant::{QdrantConfig, QdrantMirror};
use crate::retrieval::{DrrConfig, DrrEngine};
use crate::state::SqliteStateStore;

mod benchmark_gate_service;
mod benchmark_logging_service;
mod benchmark_metrics_service;
mod benchmark_report_service;
mod benchmark_run_service;
mod benchmark_service;
mod eval_execution_service;
mod eval_golden_service;
mod eval_logging_service;
mod eval_report_service;
mod eval_service;
mod indexing_service;
mod markdown_editor_service;
mod mirror_outbox_service;
mod queue_reconcile_service;
mod relation_service;
mod release_benchmark_service;
mod release_evidence_service;
mod release_pack_service;
mod release_reliability_service;
mod release_security_service;
mod request_log_service;
mod resource_service;
mod runtime_service;
mod search_service;
mod trace_eval_service;
mod trace_metrics_service;
mod trace_replay_service;

#[derive(Clone)]
pub struct AxiomMe {
    pub fs: LocalContextFs,
    pub state: SqliteStateStore,
    pub index: Arc<RwLock<InMemoryHybridIndex>>,
    pub qdrant: Option<QdrantMirror>,
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
        let fs = LocalContextFs::new(&root);
        let state = SqliteStateStore::open(root.join(".axiomme_state.sqlite3"))?;
        let index = Arc::new(RwLock::new(InMemoryHybridIndex::new()));
        let qdrant = QdrantConfig::from_env()
            .map(QdrantMirror::new)
            .transpose()?;

        Ok(Self {
            fs,
            state,
            index,
            qdrant,
            markdown_edit_gate: Arc::new(RwLock::new(())),
            parser_registry: ParserRegistry::new(),
            drr: DrrEngine::new(DrrConfig::default()),
        })
    }

    pub fn initialize(&self) -> Result<()> {
        self.fs.initialize()?;
        self.ensure_qdrant_collection()?;
        self.ensure_scope_tiers()?;
        self.initialize_runtime_index()?;
        Ok(())
    }
}
#[cfg(test)]
mod tests;
