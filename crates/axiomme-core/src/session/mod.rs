use std::sync::{Arc, RwLock};

use crate::config::AppConfig;
use crate::error::Result;
use crate::fs::LocalContextFs;
use crate::index::InMemoryIndex;
use crate::om::OmScope;
use crate::state::SqliteStateStore;

mod archive;
mod commit;
mod context;
mod indexing;
mod lifecycle;
mod memory;
mod memory_extractor;
mod meta;
mod om;
mod paths;

#[cfg(test)]
mod tests;

pub(crate) use om::{OmScopeBinding, resolve_om_scope_binding_for_session_with_config};

#[derive(Clone)]
pub struct Session {
    pub session_id: String,
    fs: LocalContextFs,
    state: SqliteStateStore,
    index: Arc<RwLock<InMemoryIndex>>,
    pub(crate) config: Arc<AppConfig>,
    om_scope_binding_override: Option<OmScopeBinding>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("session_id", &self.session_id)
            .finish_non_exhaustive()
    }
}

impl Session {
    #[must_use]
    pub fn new(
        session_id: impl Into<String>,
        fs: LocalContextFs,
        state: SqliteStateStore,
        index: Arc<RwLock<InMemoryIndex>>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            fs,
            state,
            index,
            config: Arc::new(AppConfig::default()),
            om_scope_binding_override: None,
        }
    }

    #[must_use]
    pub(crate) fn with_config(mut self, config: Arc<AppConfig>) -> Self {
        self.config = config;
        self
    }

    #[must_use]
    pub fn with_om_scope_binding(mut self, scope_binding: OmScopeBinding) -> Self {
        self.om_scope_binding_override = Some(scope_binding);
        self
    }

    pub fn with_om_scope(
        mut self,
        scope: OmScope,
        thread_id: Option<&str>,
        resource_id: Option<&str>,
    ) -> Result<Self> {
        let scope_binding =
            om::resolve_om_scope_binding_explicit(&self.session_id, scope, thread_id, resource_id)?;
        self.om_scope_binding_override = Some(scope_binding);
        Ok(self)
    }
}
