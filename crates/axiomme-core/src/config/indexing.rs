use crate::uri::Scope;

use super::env::read_non_empty_env;

const ENV_TIER_SYNTHESIS: &str = "AXIOMME_TIER_SYNTHESIS";
const ENV_INTERNAL_TIERS: &str = "AXIOMME_INTERNAL_TIERS";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TierSynthesisMode {
    Deterministic,
    SemanticLite,
}

#[must_use]
pub(crate) fn resolve_tier_synthesis_mode(raw: Option<&str>) -> TierSynthesisMode {
    match raw.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if value == "semantic" || value == "semantic-lite" => {
            TierSynthesisMode::SemanticLite
        }
        _ => TierSynthesisMode::Deterministic,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InternalTierPolicy {
    Virtual,
    Persist,
}

#[must_use]
pub(crate) fn resolve_internal_tier_policy(raw: Option<&str>) -> InternalTierPolicy {
    match raw.map(|value| value.trim().to_ascii_lowercase()) {
        Some(value) if matches!(value.as_str(), "persist" | "full" | "files" | "write") => {
            InternalTierPolicy::Persist
        }
        _ => InternalTierPolicy::Virtual,
    }
}

#[must_use]
pub(crate) const fn should_persist_scope_tiers(scope: Scope, policy: InternalTierPolicy) -> bool {
    !scope.is_internal() || matches!(policy, InternalTierPolicy::Persist)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IndexingConfig {
    pub(crate) tier_synthesis_mode: TierSynthesisMode,
    pub(crate) internal_tier_policy: InternalTierPolicy,
}

impl IndexingConfig {
    #[must_use]
    pub(super) fn from_env() -> Self {
        Self {
            tier_synthesis_mode: resolve_tier_synthesis_mode(
                read_non_empty_env(ENV_TIER_SYNTHESIS).as_deref(),
            ),
            internal_tier_policy: resolve_internal_tier_policy(
                read_non_empty_env(ENV_INTERNAL_TIERS).as_deref(),
            ),
        }
    }
}

impl Default for IndexingConfig {
    fn default() -> Self {
        Self {
            tier_synthesis_mode: TierSynthesisMode::Deterministic,
            internal_tier_policy: InternalTierPolicy::Virtual,
        }
    }
}
