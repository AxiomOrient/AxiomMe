use crate::error::{AxiomError, Result};

use super::env::{parse_enabled_default_true, read_env_usize, read_non_empty_env};

const ENV_RETRIEVAL_BACKEND: &str = "AXIOMME_RETRIEVAL_BACKEND";
const ENV_RERANKER: &str = "AXIOMME_RERANKER";
const ENV_QUERY_NORMALIZER: &str = "AXIOMME_QUERY_NORMALIZER";
const ENV_OM_CONTEXT_MAX_ARCHIVES: &str = "AXIOMME_OM_CONTEXT_MAX_ARCHIVES";
const ENV_OM_CONTEXT_MAX_MESSAGES: &str = "AXIOMME_OM_CONTEXT_MAX_MESSAGES";
const ENV_OM_RECENT_HINT_LIMIT: &str = "AXIOMME_OM_RECENT_HINT_LIMIT";
const ENV_OM_HINT_TOTAL_LIMIT: &str = "AXIOMME_OM_HINT_TOTAL_LIMIT";
const ENV_OM_KEEP_RECENT_WITH_OM: &str = "AXIOMME_OM_KEEP_RECENT_WITH_OM";
const ENV_OM_HINT_MAX_LINES: &str = "AXIOMME_OM_HINT_MAX_LINES";
const ENV_OM_HINT_MAX_CHARS: &str = "AXIOMME_OM_HINT_MAX_CHARS";
const ENV_OM_HINT_SUGGESTED_MAX_CHARS: &str = "AXIOMME_OM_HINT_SUGGESTED_MAX_CHARS";

const DEFAULT_OM_CONTEXT_MAX_ARCHIVES: usize = 2;
const DEFAULT_OM_CONTEXT_MAX_MESSAGES: usize = 8;
const DEFAULT_OM_RECENT_HINT_LIMIT: usize = 2;
const DEFAULT_OM_HINT_TOTAL_LIMIT: usize = 2;
const DEFAULT_OM_KEEP_RECENT_WITH_OM: usize = 1;
const DEFAULT_OM_HINT_MAX_CHARS: usize = 480;
const DEFAULT_OM_HINT_MAX_LINES: usize = 4;
const DEFAULT_OM_HINT_SUGGESTED_MAX_CHARS: usize = 160;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum RetrievalBackend {
    #[default]
    Sqlite,
    Memory,
}

impl RetrievalBackend {
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::Memory => "memory",
        }
    }

    fn parse(raw: Option<&str>) -> Result<Self> {
        let normalized = raw.map(|value| value.trim().to_ascii_lowercase());
        match normalized.as_deref() {
            None => Ok(Self::Sqlite),
            Some("sqlite") => Ok(Self::Sqlite),
            Some("memory") => Ok(Self::Memory),
            Some(other) => Err(AxiomError::Validation(format!(
                "invalid {ENV_RETRIEVAL_BACKEND}: {other} (expected sqlite|memory)"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SearchConfig {
    pub(crate) retrieval_backend: RetrievalBackend,
    pub(crate) reranker: Option<String>,
    pub(crate) query_normalizer_enabled: bool,
    pub(crate) om_hint_policy: OmHintPolicy,
    pub(crate) om_hint_bounds: OmHintBounds,
}

impl SearchConfig {
    pub(super) fn from_env() -> Result<Self> {
        Ok(Self {
            retrieval_backend: RetrievalBackend::parse(
                std::env::var(ENV_RETRIEVAL_BACKEND).ok().as_deref(),
            )?,
            reranker: read_non_empty_env(ENV_RERANKER),
            query_normalizer_enabled: parse_enabled_default_true(
                std::env::var(ENV_QUERY_NORMALIZER).ok().as_deref(),
            ),
            om_hint_policy: OmHintPolicy::from_env(),
            om_hint_bounds: OmHintBounds::from_env(),
        })
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            retrieval_backend: RetrievalBackend::Sqlite,
            reranker: None,
            query_normalizer_enabled: true,
            om_hint_policy: OmHintPolicy::default(),
            om_hint_bounds: OmHintBounds::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OmHintPolicy {
    pub(crate) context_max_archives: usize,
    pub(crate) context_max_messages: usize,
    pub(crate) recent_hint_limit: usize,
    pub(crate) total_hint_limit: usize,
    pub(crate) keep_recent_with_om: usize,
}

impl Default for OmHintPolicy {
    fn default() -> Self {
        Self {
            context_max_archives: DEFAULT_OM_CONTEXT_MAX_ARCHIVES,
            context_max_messages: DEFAULT_OM_CONTEXT_MAX_MESSAGES,
            recent_hint_limit: DEFAULT_OM_RECENT_HINT_LIMIT,
            total_hint_limit: DEFAULT_OM_HINT_TOTAL_LIMIT,
            keep_recent_with_om: DEFAULT_OM_KEEP_RECENT_WITH_OM,
        }
    }
}

impl OmHintPolicy {
    #[must_use]
    fn from_env() -> Self {
        let defaults = Self::default();
        Self {
            context_max_archives: read_env_usize(
                ENV_OM_CONTEXT_MAX_ARCHIVES,
                defaults.context_max_archives,
                0,
            ),
            context_max_messages: read_env_usize(
                ENV_OM_CONTEXT_MAX_MESSAGES,
                defaults.context_max_messages,
                1,
            ),
            recent_hint_limit: read_env_usize(
                ENV_OM_RECENT_HINT_LIMIT,
                defaults.recent_hint_limit,
                0,
            ),
            total_hint_limit: read_env_usize(ENV_OM_HINT_TOTAL_LIMIT, defaults.total_hint_limit, 0),
            keep_recent_with_om: read_env_usize(
                ENV_OM_KEEP_RECENT_WITH_OM,
                defaults.keep_recent_with_om,
                0,
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OmHintBounds {
    pub(crate) max_lines: usize,
    pub(crate) max_chars: usize,
    pub(crate) max_suggested_chars: usize,
}

impl Default for OmHintBounds {
    fn default() -> Self {
        Self {
            max_lines: DEFAULT_OM_HINT_MAX_LINES,
            max_chars: DEFAULT_OM_HINT_MAX_CHARS,
            max_suggested_chars: DEFAULT_OM_HINT_SUGGESTED_MAX_CHARS,
        }
    }
}

impl OmHintBounds {
    #[must_use]
    fn from_env() -> Self {
        let defaults = Self::default();
        Self {
            max_lines: read_env_usize(ENV_OM_HINT_MAX_LINES, defaults.max_lines, 1),
            max_chars: read_env_usize(ENV_OM_HINT_MAX_CHARS, defaults.max_chars, 1),
            max_suggested_chars: read_env_usize(
                ENV_OM_HINT_SUGGESTED_MAX_CHARS,
                defaults.max_suggested_chars,
                1,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RetrievalBackend;

    #[test]
    fn retrieval_backend_parser_defaults_to_sqlite_when_unset() {
        assert_eq!(
            RetrievalBackend::parse(None).expect("default backend"),
            RetrievalBackend::Sqlite
        );
    }

    #[test]
    fn retrieval_backend_parser_accepts_memory() {
        assert_eq!(
            RetrievalBackend::parse(Some("memory")).expect("memory backend"),
            RetrievalBackend::Memory
        );
    }

    #[test]
    fn retrieval_backend_parser_rejects_unknown_values() {
        assert!(RetrievalBackend::parse(Some("invalid-backend")).is_err());
        assert!(RetrievalBackend::parse(Some("bm25")).is_err());
        assert!(RetrievalBackend::parse(Some("")).is_err());
    }
}
