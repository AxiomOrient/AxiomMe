use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock, Weak};
use std::time::SystemTime;

use crate::config::AppConfig;
use crate::error::{AxiomError, Result};
use crate::fs::LocalContextFs;
use crate::index::InMemoryIndex;
use crate::ontology::CompiledOntologySchema;
use crate::parse::ParserRegistry;
use crate::retrieval::{DrrConfig, DrrEngine};
use crate::state::SqliteStateStore;
use crate::uri::AxiomUri;

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

type DocumentEditGate = Arc<RwLock<()>>;
type WeakDocumentEditGate = Weak<RwLock<()>>;
const MARKDOWN_EDIT_GATE_SWEEP_THRESHOLD: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OntologySchemaFingerprint {
    modified: Option<SystemTime>,
    len: u64,
}

#[derive(Debug, Clone)]
struct OntologySchemaCacheEntry {
    fingerprint: OntologySchemaFingerprint,
    compiled: Arc<CompiledOntologySchema>,
}

#[derive(Debug, Default)]
struct MarkdownEditGates {
    by_uri: RwLock<HashMap<String, WeakDocumentEditGate>>,
}

impl MarkdownEditGates {
    fn gate_for(&self, uri: &AxiomUri) -> Result<DocumentEditGate> {
        let key = uri.to_string();
        if let Some(existing) = self
            .by_uri
            .read()
            .map_err(|_| AxiomError::lock_poisoned("markdown gate registry"))?
            .get(&key)
            .and_then(Weak::upgrade)
        {
            return Ok(existing);
        }

        let mut by_uri = self
            .by_uri
            .write()
            .map_err(|_| AxiomError::lock_poisoned("markdown gate registry"))?;
        if let Some(existing) = by_uri.get(&key).and_then(Weak::upgrade) {
            return Ok(existing);
        }
        if by_uri.len() >= MARKDOWN_EDIT_GATE_SWEEP_THRESHOLD {
            by_uri.retain(|_, gate| gate.strong_count() > 0);
        }
        let created = Arc::new(RwLock::new(()));
        by_uri.insert(key, Arc::downgrade(&created));
        Ok(created)
    }
}

#[derive(Clone)]
pub struct AxiomMe {
    pub fs: LocalContextFs,
    pub state: SqliteStateStore,
    pub index: Arc<RwLock<InMemoryIndex>>,
    config: Arc<AppConfig>,
    markdown_edit_gates: Arc<MarkdownEditGates>,
    ontology_schema_cache: Arc<RwLock<Option<OntologySchemaCacheEntry>>>,
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
            markdown_edit_gates: Arc::new(MarkdownEditGates::default()),
            ontology_schema_cache: Arc::new(RwLock::new(None)),
            parser_registry: ParserRegistry::new(),
            drr: DrrEngine::new(DrrConfig::default()),
        })
    }

    pub fn bootstrap(&self) -> Result<()> {
        self.fs.initialize()?;
        self.ensure_default_ontology_schema()?;
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

    fn markdown_gate_for_uri(&self, uri: &AxiomUri) -> Result<DocumentEditGate> {
        self.markdown_edit_gates.gate_for(uri)
    }

    fn ensure_default_ontology_schema(&self) -> Result<()> {
        let schema_uri =
            AxiomUri::parse(crate::ontology::ONTOLOGY_SCHEMA_URI_V1).map_err(|err| {
                AxiomError::Internal(format!("invalid ontology schema URI constant: {err}"))
            })?;
        if self.fs.exists(&schema_uri) {
            return Ok(());
        }
        self.fs.write(
            &schema_uri,
            crate::ontology::DEFAULT_ONTOLOGY_SCHEMA_V1_JSON,
            true,
        )
    }
}
#[cfg(test)]
mod tests;
