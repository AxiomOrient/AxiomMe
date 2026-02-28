use crate::config::MemoryDedupConfigSnapshot;
use crate::models::{MemoryPromotionFact, PromotionApplyMode};
use crate::uri::AxiomUri;

pub(super) const DEFAULT_MEMORY_DEDUP_MODE: &str = "auto";
pub(super) const DEFAULT_MEMORY_DEDUP_LLM_ENDPOINT: &str = "http://127.0.0.1:11434/api/chat";
pub(super) const DEFAULT_MEMORY_DEDUP_LLM_MODEL: &str = "qwen2.5:7b-instruct";
pub(super) const DEFAULT_MEMORY_DEDUP_LLM_TIMEOUT_MS: u64 = 2_000;
pub(super) const DEFAULT_MEMORY_DEDUP_LLM_MAX_OUTPUT_TOKENS: u32 = 600;
pub(super) const DEFAULT_MEMORY_DEDUP_LLM_TEMPERATURE_MILLI: u16 = 0;
pub(super) const DEFAULT_MEMORY_DEDUP_LLM_MAX_MATCHES: usize = 3;
pub(super) const PROMOTION_MAX_FACTS: usize = 64;
pub(super) const PROMOTION_MAX_TEXT_CHARS: usize = 512;
pub(super) const PROMOTION_MAX_SOURCE_IDS_PER_FACT: usize = 32;
pub(super) const PROMOTION_MAX_CONFIDENCE_MILLI: u16 = 1_000;
pub(super) const PROMOTION_APPLYING_STALE_SECONDS: i64 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedMemoryCandidate {
    pub(super) category: String,
    pub(super) key: String,
    pub(super) text: String,
    pub(super) source_message_ids: Vec<String>,
    pub(super) target_uri: Option<AxiomUri>,
}

#[derive(Debug, Clone)]
pub(super) struct ExistingMemoryFact {
    pub(super) uri: AxiomUri,
    pub(super) text: String,
    pub(super) vector: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ExistingPromotionFact {
    pub(super) category: String,
    pub(super) text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PromotionApplyPlan {
    pub(super) candidates: Vec<ResolvedMemoryCandidate>,
    pub(super) skipped_duplicates: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PromotionApplyInput {
    pub(super) request_hash: String,
    pub(super) request_json: String,
    pub(super) apply_mode: PromotionApplyMode,
    pub(super) facts: Vec<MemoryPromotionFact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MemoryDedupMode {
    Deterministic,
    Llm,
    Auto,
}

impl MemoryDedupMode {
    pub(super) fn parse(raw: Option<&str>) -> Self {
        let normalized = raw
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_MEMORY_DEDUP_MODE)
            .to_ascii_lowercase();
        match normalized.as_str() {
            "llm" | "model" => Self::Llm,
            "auto" => Self::Auto,
            _ => Self::Deterministic,
        }
    }

    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Deterministic => "deterministic",
            Self::Llm => "llm",
            Self::Auto => "auto",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct MemoryDedupConfig {
    pub(super) mode: MemoryDedupMode,
    pub(super) similarity_threshold: f32,
    pub(super) llm_endpoint: String,
    pub(super) llm_model: String,
    pub(super) llm_timeout_ms: u64,
    pub(super) llm_max_output_tokens: u32,
    pub(super) llm_temperature_milli: u16,
    pub(super) llm_strict: bool,
    pub(super) llm_max_matches: usize,
}

impl MemoryDedupConfig {
    pub(super) fn from_snapshot(snapshot: &MemoryDedupConfigSnapshot) -> Self {
        Self {
            mode: MemoryDedupMode::parse(snapshot.mode.as_deref()),
            similarity_threshold: snapshot.similarity_threshold,
            llm_endpoint: snapshot
                .llm_endpoint
                .clone()
                .unwrap_or_else(|| DEFAULT_MEMORY_DEDUP_LLM_ENDPOINT.to_string()),
            llm_model: snapshot
                .llm_model
                .clone()
                .unwrap_or_else(|| DEFAULT_MEMORY_DEDUP_LLM_MODEL.to_string()),
            llm_timeout_ms: snapshot
                .llm_timeout_ms
                .unwrap_or(DEFAULT_MEMORY_DEDUP_LLM_TIMEOUT_MS),
            llm_max_output_tokens: snapshot
                .llm_max_output_tokens
                .unwrap_or(DEFAULT_MEMORY_DEDUP_LLM_MAX_OUTPUT_TOKENS),
            llm_temperature_milli: snapshot
                .llm_temperature_milli
                .unwrap_or(DEFAULT_MEMORY_DEDUP_LLM_TEMPERATURE_MILLI),
            llm_strict: snapshot.llm_strict,
            llm_max_matches: snapshot
                .llm_max_matches
                .unwrap_or(DEFAULT_MEMORY_DEDUP_LLM_MAX_MATCHES),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MemoryDedupDecision {
    Create,
    Merge,
    Skip,
}

#[derive(Debug, Clone)]
pub(super) struct PrefilteredMemoryMatch {
    pub(super) uri: AxiomUri,
    pub(super) text: String,
    pub(super) score: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DedupSelection {
    pub(super) decision: MemoryDedupDecision,
    pub(super) selected_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParsedLlmDedupDecision {
    pub(super) decision: MemoryDedupDecision,
    pub(super) target_uri: Option<String>,
    pub(super) target_index: Option<usize>,
}
