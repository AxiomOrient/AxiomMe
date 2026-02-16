use crate::llm_io::parse_env_bool;

use super::env::{
    read_env_u16, read_env_u32, read_env_u64, read_env_usize_optional, read_non_empty_env,
};

const ENV_MEMORY_EXTRACTOR_MODE: &str = "AXIOMME_MEMORY_EXTRACTOR_MODE";
const ENV_MEMORY_LLM_ENDPOINT: &str = "AXIOMME_MEMORY_LLM_ENDPOINT";
const ENV_MEMORY_LLM_MODEL: &str = "AXIOMME_MEMORY_LLM_MODEL";
const ENV_MEMORY_LLM_TIMEOUT_MS: &str = "AXIOMME_MEMORY_LLM_TIMEOUT_MS";
const ENV_MEMORY_LLM_MAX_OUTPUT_TOKENS: &str = "AXIOMME_MEMORY_LLM_MAX_OUTPUT_TOKENS";
const ENV_MEMORY_LLM_TEMPERATURE_MILLI: &str = "AXIOMME_MEMORY_LLM_TEMPERATURE_MILLI";
const ENV_MEMORY_LLM_STRICT: &str = "AXIOMME_MEMORY_LLM_STRICT";
const ENV_MEMORY_LLM_MAX_MESSAGES: &str = "AXIOMME_MEMORY_LLM_MAX_MESSAGES";
const ENV_MEMORY_LLM_MAX_CHARS_PER_MESSAGE: &str = "AXIOMME_MEMORY_LLM_MAX_CHARS_PER_MESSAGE";
const ENV_MEMORY_DEDUP_SIMILARITY_MILLI: &str = "AXIOMME_MEMORY_DEDUP_SIMILARITY_MILLI";
const ENV_MEMORY_DEDUP_MODE: &str = "AXIOMME_MEMORY_DEDUP_MODE";
const ENV_MEMORY_DEDUP_LLM_ENDPOINT: &str = "AXIOMME_MEMORY_DEDUP_LLM_ENDPOINT";
const ENV_MEMORY_DEDUP_LLM_MODEL: &str = "AXIOMME_MEMORY_DEDUP_LLM_MODEL";
const ENV_MEMORY_DEDUP_LLM_TIMEOUT_MS: &str = "AXIOMME_MEMORY_DEDUP_LLM_TIMEOUT_MS";
const ENV_MEMORY_DEDUP_LLM_MAX_OUTPUT_TOKENS: &str = "AXIOMME_MEMORY_DEDUP_LLM_MAX_OUTPUT_TOKENS";
const ENV_MEMORY_DEDUP_LLM_TEMPERATURE_MILLI: &str = "AXIOMME_MEMORY_DEDUP_LLM_TEMPERATURE_MILLI";
const ENV_MEMORY_DEDUP_LLM_STRICT: &str = "AXIOMME_MEMORY_DEDUP_LLM_STRICT";
const ENV_MEMORY_DEDUP_LLM_MAX_MATCHES: &str = "AXIOMME_MEMORY_DEDUP_LLM_MAX_MATCHES";

const DEFAULT_MEMORY_DEDUP_SIMILARITY_MILLI: u16 = 900;

#[derive(Debug, Clone, Default)]
pub(crate) struct MemoryConfig {
    pub(crate) extractor: MemoryExtractorConfigSnapshot,
    pub(crate) dedup: MemoryDedupConfigSnapshot,
}

impl MemoryConfig {
    #[must_use]
    pub(super) fn from_env() -> Self {
        Self {
            extractor: MemoryExtractorConfigSnapshot::from_env(),
            dedup: MemoryDedupConfigSnapshot::from_env(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct MemoryExtractorConfigSnapshot {
    pub(crate) mode: Option<String>,
    pub(crate) llm_endpoint: Option<String>,
    pub(crate) llm_model: Option<String>,
    pub(crate) llm_timeout_ms: Option<u64>,
    pub(crate) llm_max_output_tokens: Option<u32>,
    pub(crate) llm_temperature_milli: Option<u16>,
    pub(crate) llm_strict: bool,
    pub(crate) llm_max_messages: Option<usize>,
    pub(crate) llm_max_chars_per_message: Option<usize>,
}

impl MemoryExtractorConfigSnapshot {
    #[must_use]
    fn from_env() -> Self {
        Self {
            mode: read_non_empty_env(ENV_MEMORY_EXTRACTOR_MODE),
            llm_endpoint: read_non_empty_env(ENV_MEMORY_LLM_ENDPOINT),
            llm_model: read_non_empty_env(ENV_MEMORY_LLM_MODEL),
            llm_timeout_ms: read_env_u64(ENV_MEMORY_LLM_TIMEOUT_MS).filter(|value| *value >= 200),
            llm_max_output_tokens: read_env_u32(ENV_MEMORY_LLM_MAX_OUTPUT_TOKENS)
                .filter(|value| *value > 0),
            llm_temperature_milli: read_env_u16(ENV_MEMORY_LLM_TEMPERATURE_MILLI),
            llm_strict: parse_env_bool(std::env::var(ENV_MEMORY_LLM_STRICT).ok().as_deref()),
            llm_max_messages: read_env_usize_optional(ENV_MEMORY_LLM_MAX_MESSAGES)
                .filter(|value| *value > 0),
            llm_max_chars_per_message: read_env_usize_optional(
                ENV_MEMORY_LLM_MAX_CHARS_PER_MESSAGE,
            )
            .filter(|value| *value > 0),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MemoryDedupConfigSnapshot {
    pub(crate) mode: Option<String>,
    pub(crate) similarity_threshold: f32,
    pub(crate) llm_endpoint: Option<String>,
    pub(crate) llm_model: Option<String>,
    pub(crate) llm_timeout_ms: Option<u64>,
    pub(crate) llm_max_output_tokens: Option<u32>,
    pub(crate) llm_temperature_milli: Option<u16>,
    pub(crate) llm_strict: bool,
    pub(crate) llm_max_matches: Option<usize>,
}

impl MemoryDedupConfigSnapshot {
    #[must_use]
    fn from_env() -> Self {
        Self {
            mode: read_non_empty_env(ENV_MEMORY_DEDUP_MODE),
            similarity_threshold: parse_similarity_threshold(
                std::env::var(ENV_MEMORY_DEDUP_SIMILARITY_MILLI)
                    .ok()
                    .as_deref(),
            ),
            llm_endpoint: read_non_empty_env(ENV_MEMORY_DEDUP_LLM_ENDPOINT),
            llm_model: read_non_empty_env(ENV_MEMORY_DEDUP_LLM_MODEL),
            llm_timeout_ms: read_env_u64(ENV_MEMORY_DEDUP_LLM_TIMEOUT_MS)
                .filter(|value| *value >= 200),
            llm_max_output_tokens: read_env_u32(ENV_MEMORY_DEDUP_LLM_MAX_OUTPUT_TOKENS)
                .filter(|value| *value > 0),
            llm_temperature_milli: read_env_u16(ENV_MEMORY_DEDUP_LLM_TEMPERATURE_MILLI),
            llm_strict: parse_env_bool(std::env::var(ENV_MEMORY_DEDUP_LLM_STRICT).ok().as_deref()),
            llm_max_matches: read_env_usize_optional(ENV_MEMORY_DEDUP_LLM_MAX_MATCHES)
                .filter(|value| *value > 0),
        }
    }
}

impl Default for MemoryDedupConfigSnapshot {
    fn default() -> Self {
        Self {
            mode: None,
            similarity_threshold: f32::from(DEFAULT_MEMORY_DEDUP_SIMILARITY_MILLI) / 1000.0,
            llm_endpoint: None,
            llm_model: None,
            llm_timeout_ms: None,
            llm_max_output_tokens: None,
            llm_temperature_milli: None,
            llm_strict: false,
            llm_max_matches: None,
        }
    }
}

#[must_use]
fn parse_similarity_threshold(raw: Option<&str>) -> f32 {
    let milli = raw
        .and_then(|value| value.trim().parse::<u16>().ok())
        .unwrap_or(DEFAULT_MEMORY_DEDUP_SIMILARITY_MILLI);
    f32::from(milli.min(1000)) / 1000.0
}
