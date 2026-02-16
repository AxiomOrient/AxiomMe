use crate::embedding::{
    EMBEDDER_ENV, EMBEDDER_MODEL_ENDPOINT_ENV, EMBEDDER_MODEL_NAME_ENV,
    EMBEDDER_MODEL_TIMEOUT_MS_ENV, EMBEDDER_STRICT_ENV, EmbedderRuntimeConfig,
};
use crate::error::Result;
use crate::llm_io::parse_env_bool;

mod env;
mod indexing;
mod memory;
mod om;
mod search;

pub(crate) use indexing::{
    IndexingConfig, InternalTierPolicy, TierSynthesisMode, should_persist_scope_tiers,
};
#[cfg(test)]
pub(crate) use indexing::{resolve_internal_tier_policy, resolve_tier_synthesis_mode};
pub(crate) use memory::{MemoryConfig, MemoryDedupConfigSnapshot, MemoryExtractorConfigSnapshot};
pub(crate) use om::{
    OmConfig, OmObserverConfigSnapshot, OmReflectorConfigSnapshot, OmRuntimeEnvConfig,
    OmRuntimeLimitsConfig, OmScopeConfig,
};
pub(crate) use search::{OmHintBounds, OmHintPolicy, RetrievalBackend, SearchConfig};

#[derive(Debug, Clone, Default)]
pub(crate) struct AppConfig {
    pub(crate) embedding: EmbedderRuntimeConfig,
    pub(crate) search: SearchConfig,
    pub(crate) indexing: IndexingConfig,
    pub(crate) om: OmConfig,
    pub(crate) memory: MemoryConfig,
}

impl AppConfig {
    pub(crate) fn from_env() -> Result<Self> {
        Ok(Self {
            embedding: EmbedderRuntimeConfig::from_env(),
            search: SearchConfig::from_env()?,
            indexing: IndexingConfig::from_env(),
            om: OmConfig::from_env(),
            memory: MemoryConfig::from_env(),
        })
    }
}

impl EmbedderRuntimeConfig {
    #[must_use]
    fn from_env() -> Self {
        Self {
            kind: env::read_non_empty_env(EMBEDDER_ENV),
            model_endpoint: env::read_non_empty_env(EMBEDDER_MODEL_ENDPOINT_ENV),
            model_name: env::read_non_empty_env(EMBEDDER_MODEL_NAME_ENV),
            model_timeout_ms: env::read_env_u64(EMBEDDER_MODEL_TIMEOUT_MS_ENV),
            strict: parse_env_bool(std::env::var(EMBEDDER_STRICT_ENV).ok().as_deref()),
        }
    }
}
