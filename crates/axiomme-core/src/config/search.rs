use crate::error::{AxiomError, Result};

use super::env::{read_env_usize, read_non_empty_env};

const ENV_RETRIEVAL_BACKEND: &str = "AXIOMME_RETRIEVAL_BACKEND";
const ENV_RERANKER: &str = "AXIOMME_RERANKER";
const ENV_OM_CONTEXT_MAX_ARCHIVES: &str = "AXIOMME_OM_CONTEXT_MAX_ARCHIVES";
const ENV_OM_CONTEXT_MAX_MESSAGES: &str = "AXIOMME_OM_CONTEXT_MAX_MESSAGES";
const ENV_OM_RECENT_HINT_LIMIT: &str = "AXIOMME_OM_RECENT_HINT_LIMIT";
const ENV_OM_HINT_TOTAL_LIMIT: &str = "AXIOMME_OM_HINT_TOTAL_LIMIT";
const ENV_OM_KEEP_RECENT_WITH_OM: &str = "AXIOMME_OM_KEEP_RECENT_WITH_OM";
const ENV_OM_HINT_MAX_LINES: &str = "AXIOMME_OM_HINT_MAX_LINES";
const ENV_OM_HINT_MAX_CHARS: &str = "AXIOMME_OM_HINT_MAX_CHARS";
const ENV_OM_HINT_SUGGESTED_MAX_CHARS: &str = "AXIOMME_OM_HINT_SUGGESTED_MAX_CHARS";
pub(crate) const RETRIEVAL_BACKEND_MEMORY: &str = "memory";
pub(crate) const RETRIEVAL_BACKEND_POLICY_MEMORY_ONLY: &str = "memory_only";
pub(crate) const QUERY_PLAN_BACKEND_POLICY_MEMORY_ONLY: &str = "backend_policy:memory_only";

const DEFAULT_OM_CONTEXT_MAX_ARCHIVES: usize = 2;
const DEFAULT_OM_CONTEXT_MAX_MESSAGES: usize = 8;
const DEFAULT_OM_RECENT_HINT_LIMIT: usize = 2;
const DEFAULT_OM_HINT_TOTAL_LIMIT: usize = 2;
const DEFAULT_OM_KEEP_RECENT_WITH_OM: usize = 1;
const DEFAULT_OM_HINT_MAX_CHARS: usize = 480;
const DEFAULT_OM_HINT_MAX_LINES: usize = 4;
const DEFAULT_OM_HINT_SUGGESTED_MAX_CHARS: usize = 160;

#[derive(Debug, Clone, Default)]
pub(crate) struct SearchConfig {
    pub(crate) reranker: Option<String>,
    pub(crate) om_hint_policy: OmHintPolicy,
    pub(crate) om_hint_bounds: OmHintBounds,
}

impl SearchConfig {
    pub(super) fn from_env() -> Result<Self> {
        validate_retrieval_backend(std::env::var(ENV_RETRIEVAL_BACKEND).ok().as_deref())?;
        Ok(Self {
            reranker: read_non_empty_env(ENV_RERANKER),
            om_hint_policy: OmHintPolicy::from_env(),
            om_hint_bounds: OmHintBounds::from_env(),
        })
    }
}

fn validate_retrieval_backend(raw: Option<&str>) -> Result<()> {
    let Some(raw) = raw else {
        return Ok(());
    };

    let normalized = raw.trim().to_ascii_lowercase();
    if normalized == RETRIEVAL_BACKEND_MEMORY {
        return Ok(());
    }

    let rendered = if normalized.is_empty() {
        "<empty>"
    } else {
        normalized.as_str()
    };
    Err(AxiomError::Validation(format!(
        "invalid {ENV_RETRIEVAL_BACKEND}: {rendered} (expected {RETRIEVAL_BACKEND_MEMORY})"
    )))
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
    use super::validate_retrieval_backend;

    #[test]
    fn retrieval_backend_validation_accepts_unset() {
        validate_retrieval_backend(None).expect("unset backend");
    }

    #[test]
    fn retrieval_backend_validation_accepts_memory() {
        validate_retrieval_backend(Some("memory")).expect("memory backend");
    }

    #[test]
    fn retrieval_backend_validation_rejects_unknown_values() {
        assert!(validate_retrieval_backend(Some("sqlite")).is_err());
        assert!(validate_retrieval_backend(Some("invalid-backend")).is_err());
        assert!(validate_retrieval_backend(Some("bm25")).is_err());
        assert!(validate_retrieval_backend(Some("")).is_err());
    }
}
