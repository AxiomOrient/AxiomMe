use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;

use chrono::{Duration, Utc};
use reqwest::blocking::Client;
use serde_json::Value;
use uuid::Uuid;

use crate::config::MemoryDedupConfigSnapshot;
use crate::embedding::embed_text;
use crate::error::{AxiomError, Result};
use crate::llm_io::{extract_json_fragment, extract_llm_content, parse_local_loopback_endpoint};
use crate::models::{
    CommitMode, CommitResult, CommitStats, IndexRecord, MemoryCandidate, MemoryPromotionFact,
    MemoryPromotionRequest, MemoryPromotionResult, PromotionApplyMode,
};
use crate::state::PromotionCheckpointPhase;
use crate::uri::{AxiomUri, Scope};

use super::Session;
use super::archive::{next_archive_number, summarize_messages};
use super::indexing::ensure_directory_record;
use super::memory::{
    MemorySource, build_memory_key, merge_memory_markdown, normalize_memory_text,
    parse_memory_entries, slugify,
};
use super::memory_extractor::{ExtractedMemory, extract_memories_for_commit};

const DEFAULT_MEMORY_DEDUP_MODE: &str = "auto";
const DEFAULT_MEMORY_DEDUP_LLM_ENDPOINT: &str = "http://127.0.0.1:11434/api/chat";
const DEFAULT_MEMORY_DEDUP_LLM_MODEL: &str = "qwen2.5:7b-instruct";
const DEFAULT_MEMORY_DEDUP_LLM_TIMEOUT_MS: u64 = 2_000;
const DEFAULT_MEMORY_DEDUP_LLM_MAX_OUTPUT_TOKENS: u32 = 600;
const DEFAULT_MEMORY_DEDUP_LLM_TEMPERATURE_MILLI: u16 = 0;
const DEFAULT_MEMORY_DEDUP_LLM_MAX_MATCHES: usize = 3;
const PROMOTION_MAX_FACTS: usize = 64;
const PROMOTION_MAX_TEXT_CHARS: usize = 512;
const PROMOTION_MAX_SOURCE_IDS_PER_FACT: usize = 32;
const PROMOTION_MAX_CONFIDENCE_MILLI: u16 = 1_000;
const PROMOTION_APPLYING_STALE_SECONDS: i64 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedMemoryCandidate {
    category: String,
    key: String,
    text: String,
    source_message_ids: Vec<String>,
    target_uri: Option<AxiomUri>,
}

#[derive(Debug, Clone)]
struct ExistingMemoryFact {
    uri: AxiomUri,
    text: String,
    vector: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExistingPromotionFact {
    category: String,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PromotionApplyPlan {
    candidates: Vec<ResolvedMemoryCandidate>,
    skipped_duplicates: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PromotionApplyInput {
    request_hash: String,
    request_json: String,
    apply_mode: PromotionApplyMode,
    facts: Vec<MemoryPromotionFact>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryDedupMode {
    Deterministic,
    Llm,
    Auto,
}

impl MemoryDedupMode {
    fn parse(raw: Option<&str>) -> Self {
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

    const fn as_str(self) -> &'static str {
        match self {
            Self::Deterministic => "deterministic",
            Self::Llm => "llm",
            Self::Auto => "auto",
        }
    }
}

#[derive(Debug, Clone)]
struct MemoryDedupConfig {
    mode: MemoryDedupMode,
    similarity_threshold: f32,
    llm_endpoint: String,
    llm_model: String,
    llm_timeout_ms: u64,
    llm_max_output_tokens: u32,
    llm_temperature_milli: u16,
    llm_strict: bool,
    llm_max_matches: usize,
}

impl MemoryDedupConfig {
    fn from_snapshot(snapshot: &MemoryDedupConfigSnapshot) -> Self {
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
enum MemoryDedupDecision {
    Create,
    Merge,
    Skip,
}

#[derive(Debug, Clone)]
struct PrefilteredMemoryMatch {
    uri: AxiomUri,
    text: String,
    score: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DedupSelection {
    decision: MemoryDedupDecision,
    selected_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedLlmDedupDecision {
    decision: MemoryDedupDecision,
    target_uri: Option<String>,
    target_index: Option<usize>,
}

impl Session {
    pub fn commit(&self) -> Result<CommitResult> {
        self.commit_with_mode(CommitMode::ArchiveAndExtract)
    }

    pub fn commit_with_mode(&self, mode: CommitMode) -> Result<CommitResult> {
        let active_messages = self.read_messages()?;
        let total_turns = active_messages.len();
        let meta = self.read_meta()?;

        if total_turns == 0 {
            return Ok(CommitResult {
                session_id: self.session_id.clone(),
                status: "committed".to_string(),
                memories_extracted: 0,
                active_count_updated: 0,
                archived: false,
                stats: CommitStats {
                    total_turns: 0,
                    contexts_used: meta.context_usage.contexts_used,
                    skills_used: meta.context_usage.skills_used,
                    memories_extracted: 0,
                },
            });
        }

        let archive_num = next_archive_number(self)?;
        let archive_uri = self
            .session_uri()?
            .join(&format!("history/archive_{archive_num:03}"))?;
        self.fs.create_dir_all(&archive_uri, true)?;

        let archive_messages_uri = archive_uri.join("messages.jsonl")?;
        let messages_path = self.messages_path()?;
        let raw_messages = fs::read_to_string(&messages_path)?;
        self.fs.write(&archive_messages_uri, &raw_messages, true)?;
        fs::write(messages_path, "")?;

        let session_summary = summarize_messages(&active_messages);
        self.fs.write_tiers(
            &archive_uri,
            &session_summary,
            &format!("# Archive {archive_num}\n\n{session_summary}"),
        )?;

        let session_uri = self.session_uri()?;
        self.fs.write_tiers(
            &session_uri,
            &format!("Session {} latest commit", self.session_id),
            &format!("# Session Overview\n\nLatest archive: {archive_num}"),
        )?;

        let mut candidates_len = 0usize;
        let mut persisted_uris = Vec::new();
        if matches!(mode, CommitMode::ArchiveAndExtract) {
            let extracted =
                extract_memories_for_commit(&active_messages, &self.config.memory.extractor)?;
            if let Some(error) = extracted.llm_error.as_deref() {
                self.record_memory_extractor_fallback(&extracted.mode_requested, error);
            }

            let candidates = self.resolve_memory_candidates(&extracted.memories)?;
            candidates_len = candidates.len();
            for candidate in &candidates {
                let uri = self.persist_memory(candidate)?;
                persisted_uris.push(uri);
            }
            self.reindex_memory_uris(&persisted_uris)?;
        }

        self.touch_meta(|meta| {
            meta.updated_at = Utc::now();
        })?;

        Ok(CommitResult {
            session_id: self.session_id.clone(),
            status: "committed".to_string(),
            memories_extracted: candidates_len,
            active_count_updated: persisted_uris.len(),
            archived: true,
            stats: CommitStats {
                total_turns,
                contexts_used: meta.context_usage.contexts_used,
                skills_used: meta.context_usage.skills_used,
                memories_extracted: candidates_len,
            },
        })
    }

    pub fn promote_memories(
        &self,
        request: &MemoryPromotionRequest,
    ) -> Result<MemoryPromotionResult> {
        if request.session_id.trim() != self.session_id {
            return Err(AxiomError::Validation(format!(
                "promotion session_id mismatch: expected {}, got {}",
                self.session_id, request.session_id
            )));
        }
        if request.checkpoint_id.trim().is_empty() {
            return Err(AxiomError::Validation(
                "checkpoint_id must not be empty".to_string(),
            ));
        }

        let mut apply_input = promotion_apply_input_from_request(request)?;
        let incoming_request_hash = apply_input.request_hash.clone();

        let stale_before =
            (Utc::now() - Duration::seconds(PROMOTION_APPLYING_STALE_SECONDS)).to_rfc3339();
        let _ = self.state.demote_stale_promotion_checkpoint(
            self.session_id.as_str(),
            request.checkpoint_id.as_str(),
            stale_before.as_str(),
        )?;

        if let Some(existing) = self
            .state
            .get_promotion_checkpoint(self.session_id.as_str(), request.checkpoint_id.as_str())?
        {
            if existing.request_hash != incoming_request_hash {
                return Err(AxiomError::Validation(
                    "checkpoint_id conflict: request hash mismatch".to_string(),
                ));
            }
            match existing.phase {
                PromotionCheckpointPhase::Applied => {
                    let result_json = existing.result_json.ok_or_else(|| {
                        AxiomError::Internal("applied checkpoint missing result_json".to_string())
                    })?;
                    return Ok(serde_json::from_str(&result_json)?);
                }
                PromotionCheckpointPhase::Applying => {
                    return Err(AxiomError::Conflict(
                        "checkpoint_busy: checkpoint is currently applying".to_string(),
                    ));
                }
                PromotionCheckpointPhase::Pending => {
                    let replay_input = promotion_apply_input_from_checkpoint_json(
                        existing.request_json.as_str(),
                        self.session_id.as_str(),
                        request.checkpoint_id.as_str(),
                    )?;
                    if replay_input.request_hash != existing.request_hash {
                        return Err(AxiomError::Internal(
                            "checkpoint request_json hash mismatch".to_string(),
                        ));
                    }
                    apply_input = replay_input;
                }
            }
        } else {
            self.state.insert_promotion_checkpoint_pending(
                self.session_id.as_str(),
                request.checkpoint_id.as_str(),
                apply_input.request_hash.as_str(),
                apply_input.request_json.as_str(),
            )?;
        }

        if !self.state.claim_promotion_checkpoint_applying(
            self.session_id.as_str(),
            request.checkpoint_id.as_str(),
            apply_input.request_hash.as_str(),
        )? {
            if let Some(current) = self.state.get_promotion_checkpoint(
                self.session_id.as_str(),
                request.checkpoint_id.as_str(),
            )? {
                if current.request_hash != apply_input.request_hash {
                    return Err(AxiomError::Validation(
                        "checkpoint_id conflict: request hash mismatch".to_string(),
                    ));
                }
                return match current.phase {
                    PromotionCheckpointPhase::Applied => {
                        let result_json = current.result_json.ok_or_else(|| {
                            AxiomError::Internal(
                                "applied checkpoint missing result_json".to_string(),
                            )
                        })?;
                        Ok(serde_json::from_str(&result_json)?)
                    }
                    PromotionCheckpointPhase::Applying | PromotionCheckpointPhase::Pending => Err(
                        AxiomError::Conflict("checkpoint_busy: checkpoint claim lost".to_string()),
                    ),
                };
            }
            return Err(AxiomError::Internal(
                "checkpoint claim failed and checkpoint record missing".to_string(),
            ));
        }

        let applied = match apply_input.apply_mode {
            PromotionApplyMode::AllOrNothing => self
                .apply_promotion_all_or_nothing(request.checkpoint_id.as_str(), &apply_input.facts),
            PromotionApplyMode::BestEffort => {
                self.apply_promotion_best_effort(request.checkpoint_id.as_str(), &apply_input.facts)
            }
        };

        let result = match applied {
            Ok(result) => result,
            Err(err) => {
                let _ = self.state.set_promotion_checkpoint_pending(
                    self.session_id.as_str(),
                    request.checkpoint_id.as_str(),
                    apply_input.request_hash.as_str(),
                );
                return Err(err);
            }
        };

        let result_json = serde_json::to_string(&result)?;
        if !self.state.finalize_promotion_checkpoint_applied(
            self.session_id.as_str(),
            request.checkpoint_id.as_str(),
            apply_input.request_hash.as_str(),
            result_json.as_str(),
        )? {
            return Err(AxiomError::Conflict(
                "checkpoint finalize failed".to_string(),
            ));
        }
        Ok(result)
    }

    fn apply_promotion_all_or_nothing(
        &self,
        checkpoint_id: &str,
        facts: &[MemoryPromotionFact],
    ) -> Result<MemoryPromotionResult> {
        for fact in facts {
            validate_promotion_fact_semantics(fact)?;
        }

        let existing = self.list_existing_promotion_facts()?;
        let plan = plan_promotion_apply(&existing, facts);
        let mut snapshots = BTreeMap::<String, Option<String>>::new();
        let mut persisted_uris = Vec::<AxiomUri>::new();

        for candidate in &plan.candidates {
            let uri = match self.persist_promotion_candidate(candidate, Some(&mut snapshots)) {
                Ok(uri) => uri,
                Err(err) => {
                    restore_promotion_snapshots(self, &snapshots)?;
                    return Err(err);
                }
            };
            if !persisted_uris.iter().any(|item| item == &uri) {
                persisted_uris.push(uri);
            }
        }

        if let Err(reindex_err) = self.reindex_memory_uris(&persisted_uris) {
            self.record_memory_dedup_fallback("promotion_reindex", &reindex_err.to_string());
            let rollback_err = restore_promotion_snapshots(self, &snapshots).err();
            let rollback_reindex_err = if rollback_err.is_none() {
                self.reindex_memory_uris(&persisted_uris).err()
            } else {
                None
            };
            let rollback_status = rollback_err
                .as_ref()
                .map_or_else(|| "ok".to_string(), |err| format!("err:{err}"));
            let rollback_reindex_status = rollback_reindex_err
                .as_ref()
                .map_or_else(|| "ok_or_skipped".to_string(), |err| format!("err:{err}"));
            return Err(AxiomError::Internal(format!(
                "promotion all_or_nothing reindex failed: {reindex_err}; rollback={rollback_status}; rollback_reindex={rollback_reindex_status}",
            )));
        }

        Ok(MemoryPromotionResult {
            session_id: self.session_id.clone(),
            checkpoint_id: checkpoint_id.to_string(),
            accepted: facts.len(),
            persisted: plan.candidates.len(),
            skipped_duplicates: plan.skipped_duplicates,
            rejected: 0,
        })
    }

    fn apply_promotion_best_effort(
        &self,
        checkpoint_id: &str,
        facts: &[MemoryPromotionFact],
    ) -> Result<MemoryPromotionResult> {
        let mut rejected = 0usize;
        let mut valid = Vec::<MemoryPromotionFact>::new();
        for fact in facts {
            if validate_promotion_fact_semantics(fact).is_ok() {
                valid.push(fact.clone());
            } else {
                rejected = rejected.saturating_add(1);
            }
        }

        let existing = self.list_existing_promotion_facts()?;
        let plan = plan_promotion_apply(&existing, &valid);

        let mut persisted = 0usize;
        let mut persisted_uris = Vec::<AxiomUri>::new();
        let mut snapshots = BTreeMap::<String, Option<String>>::new();
        for candidate in &plan.candidates {
            match self.persist_promotion_candidate(candidate, Some(&mut snapshots)) {
                Ok(uri) => {
                    if !persisted_uris.iter().any(|item| item == &uri) {
                        persisted_uris.push(uri);
                    }
                    persisted = persisted.saturating_add(1);
                }
                Err(_) => {
                    rejected = rejected.saturating_add(1);
                }
            }
        }
        if let Err(reindex_err) = self.reindex_memory_uris(&persisted_uris) {
            self.record_memory_dedup_fallback("promotion_reindex", &reindex_err.to_string());
            let rollback_err = restore_promotion_snapshots(self, &snapshots).err();
            let rollback_reindex_err = if rollback_err.is_none() {
                self.reindex_memory_uris(&persisted_uris).err()
            } else {
                None
            };
            let rollback_status = rollback_err
                .as_ref()
                .map_or_else(|| "ok".to_string(), |err| format!("err:{err}"));
            let rollback_reindex_status = rollback_reindex_err
                .as_ref()
                .map_or_else(|| "ok_or_skipped".to_string(), |err| format!("err:{err}"));
            return Err(AxiomError::Internal(format!(
                "promotion best_effort reindex failed: {reindex_err}; rollback={rollback_status}; rollback_reindex={rollback_reindex_status}",
            )));
        }

        Ok(MemoryPromotionResult {
            session_id: self.session_id.clone(),
            checkpoint_id: checkpoint_id.to_string(),
            accepted: valid.len(),
            persisted,
            skipped_duplicates: plan.skipped_duplicates,
            rejected,
        })
    }

    fn list_existing_promotion_facts(&self) -> Result<Vec<ExistingPromotionFact>> {
        let categories = [
            "profile",
            "preferences",
            "entities",
            "events",
            "cases",
            "patterns",
        ];
        let mut out = Vec::<ExistingPromotionFact>::new();
        for category in categories {
            let uris = self.list_memory_document_uris(category)?;
            for uri in uris {
                let content = self.fs.read(&uri)?;
                let entries = parse_memory_entries(&content);
                for entry in entries {
                    let text = normalize_memory_text(&entry.text);
                    if text.is_empty() {
                        continue;
                    }
                    out.push(ExistingPromotionFact {
                        category: category.to_string(),
                        text,
                    });
                }
            }
        }
        out.sort_by(|left, right| {
            left.category
                .cmp(&right.category)
                .then_with(|| left.text.cmp(&right.text))
        });
        Ok(out)
    }

    fn persist_promotion_candidate(
        &self,
        candidate: &ResolvedMemoryCandidate,
        snapshots: Option<&mut BTreeMap<String, Option<String>>>,
    ) -> Result<AxiomUri> {
        let uri = if let Some(target_uri) = candidate.target_uri.as_ref() {
            target_uri.clone()
        } else {
            memory_uri_for_category_key(&candidate.category, &candidate.key)?
        };

        let path = self.fs.resolve_uri(&uri);
        if let Some(existing_snapshots) = snapshots {
            let key = uri.to_string();
            if let std::collections::btree_map::Entry::Vacant(entry) = existing_snapshots.entry(key)
            {
                let previous = if path.exists() {
                    Some(fs::read_to_string(&path)?)
                } else {
                    None
                };
                entry.insert(previous);
            }
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut merged = if path.exists() {
            fs::read_to_string(&path)?
        } else {
            String::new()
        };
        for source_message_id in dedup_source_ids(&candidate.source_message_ids) {
            let source = MemorySource {
                session_id: self.session_id.clone(),
                message_id: source_message_id.clone(),
            };
            let memory_candidate = MemoryCandidate {
                category: candidate.category.clone(),
                key: candidate.key.clone(),
                text: candidate.text.clone(),
                source_message_id,
            };
            merged = merge_memory_markdown(&merged, &memory_candidate, &source);
        }
        fs::write(path, merged)?;
        Ok(uri)
    }

    fn resolve_memory_candidates(
        &self,
        extracted: &[ExtractedMemory],
    ) -> Result<Vec<ResolvedMemoryCandidate>> {
        let mut by_category = HashMap::<String, Vec<ExistingMemoryFact>>::new();
        let mut resolved = Vec::<ResolvedMemoryCandidate>::new();
        let dedup_config = MemoryDedupConfig::from_snapshot(&self.config.memory.dedup);
        let mut dedup_fallback_logged = false;

        for candidate in extracted {
            let normalized_text = normalize_memory_text(&candidate.text);
            if normalized_text.is_empty() || candidate.source_message_ids.is_empty() {
                continue;
            }

            if !by_category.contains_key(&candidate.category) {
                let existing = self.list_existing_memory_facts(&candidate.category)?;
                by_category.insert(candidate.category.clone(), existing);
            }
            let existing = by_category
                .get_mut(&candidate.category)
                .ok_or_else(|| AxiomError::Internal("memory category cache missing".to_string()))?;

            let prefiltered = prefilter_existing_memory_matches(
                &normalized_text,
                existing,
                dedup_config.similarity_threshold,
            );
            let (selection, llm_error) =
                resolve_dedup_selection(candidate, &normalized_text, &prefiltered, &dedup_config)?;
            if let Some(error) = llm_error
                && !dedup_fallback_logged
            {
                self.record_memory_dedup_fallback(dedup_config.mode.as_str(), &error);
                dedup_fallback_logged = true;
            }

            if selection.decision == MemoryDedupDecision::Skip {
                continue;
            }

            let selected_match = selection
                .selected_index
                .and_then(|index| prefiltered.get(index));
            let (target_uri, canonical_text) = selected_match.map_or_else(
                || (None, normalized_text.clone()),
                |found| (Some(found.uri.clone()), found.text.clone()),
            );
            let key = build_memory_key(&candidate.category, &canonical_text);
            let key_for_future = key.clone();
            let source_message_ids = dedup_source_ids(&candidate.source_message_ids);

            merge_resolved_candidate(
                &mut resolved,
                ResolvedMemoryCandidate {
                    category: candidate.category.clone(),
                    key,
                    text: canonical_text.clone(),
                    source_message_ids,
                    target_uri: target_uri.clone(),
                },
            );

            if target_uri.is_none() {
                let future_uri = memory_uri_for_category_key(&candidate.category, &key_for_future)?;
                existing.push(ExistingMemoryFact {
                    uri: future_uri,
                    text: canonical_text.clone(),
                    vector: embed_text(&canonical_text),
                });
            }
        }

        Ok(resolved)
    }

    fn list_existing_memory_facts(&self, category: &str) -> Result<Vec<ExistingMemoryFact>> {
        let uris = self.list_memory_document_uris(category)?;
        let mut out = Vec::<ExistingMemoryFact>::new();

        for uri in uris {
            let content = self.fs.read(&uri)?;
            let entries = parse_memory_entries(&content);
            for entry in entries {
                let text = normalize_memory_text(&entry.text);
                if text.is_empty() {
                    continue;
                }
                out.push(ExistingMemoryFact {
                    uri: uri.clone(),
                    vector: embed_text(&text),
                    text,
                });
            }
        }

        Ok(out)
    }

    fn list_memory_document_uris(&self, category: &str) -> Result<Vec<AxiomUri>> {
        let (scope, base_path, single_file) = memory_category_path(category)?;
        let base_uri = AxiomUri::root(scope).join(base_path)?;
        if !self.fs.exists(&base_uri) {
            return Ok(Vec::new());
        }
        if single_file {
            return Ok(vec![base_uri]);
        }

        let entries = self.fs.list(&base_uri, true)?;
        let mut out = Vec::<AxiomUri>::new();
        for entry in entries {
            if entry.is_dir || !has_markdown_extension(&entry.uri) {
                continue;
            }
            if entry.uri.ends_with(".abstract.md") || entry.uri.ends_with(".overview.md") {
                continue;
            }
            if let Ok(uri) = AxiomUri::parse(&entry.uri) {
                out.push(uri);
            }
        }
        out.sort_by_key(ToString::to_string);
        Ok(out)
    }

    fn persist_memory(&self, candidate: &ResolvedMemoryCandidate) -> Result<AxiomUri> {
        let uri = if let Some(target_uri) = candidate.target_uri.as_ref() {
            target_uri.clone()
        } else {
            memory_uri_for_category_key(&candidate.category, &candidate.key)?
        };

        let path = self.fs.resolve_uri(&uri);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut merged = if path.exists() {
            fs::read_to_string(&path)?
        } else {
            String::new()
        };
        for source_message_id in dedup_source_ids(&candidate.source_message_ids) {
            let source = MemorySource {
                session_id: self.session_id.clone(),
                message_id: source_message_id.clone(),
            };
            let memory_candidate = MemoryCandidate {
                category: candidate.category.clone(),
                key: candidate.key.clone(),
                text: candidate.text.clone(),
                source_message_id,
            };
            merged = merge_memory_markdown(&merged, &memory_candidate, &source);
        }
        fs::write(path, merged)?;

        self.state.enqueue(
            "upsert",
            &uri.to_string(),
            serde_json::json!({"category": candidate.category}),
        )?;

        Ok(uri)
    }

    fn record_memory_extractor_fallback(&self, mode_requested: &str, error: &str) {
        let uri = format!("axiom://session/{}", self.session_id);
        let _ = self.state.enqueue_dead_letter(
            "memory_extract_fallback",
            &uri,
            serde_json::json!({
                "session_id": self.session_id,
                "mode_requested": mode_requested,
                "error": error,
            }),
        );
    }

    fn record_memory_dedup_fallback(&self, mode_requested: &str, error: &str) {
        let uri = format!("axiom://session/{}", self.session_id);
        let _ = self.state.enqueue_dead_letter(
            "memory_dedup_fallback",
            &uri,
            serde_json::json!({
                "session_id": self.session_id,
                "mode_requested": mode_requested,
                "error": error,
            }),
        );
    }

    fn reindex_memory_uris(&self, uris: &[AxiomUri]) -> Result<()> {
        let mut index = self
            .index
            .write()
            .map_err(|_| AxiomError::lock_poisoned("index"))?;

        for uri in uris {
            if let Some(parent) = uri.parent() {
                ensure_directory_record(&self.fs, &mut index, &parent)?;
                if let Some(record) = index.get(&parent.to_string()).cloned() {
                    self.state.upsert_search_document(&record)?;
                }
            }
            if has_markdown_extension(&uri.to_string()) {
                let text = self.fs.read(uri)?;
                let parent_uri = uri.parent().map(|u| u.to_string());
                let record = IndexRecord {
                    id: Uuid::new_v4().to_string(),
                    uri: uri.to_string(),
                    parent_uri,
                    is_leaf: true,
                    context_type: "memory".to_string(),
                    name: uri.last_segment().unwrap_or("memory").to_string(),
                    abstract_text: text.lines().next().unwrap_or_default().to_string(),
                    content: text,
                    tags: vec!["memory".to_string()],
                    updated_at: Utc::now(),
                    depth: uri.segments().len(),
                };
                index.upsert(record.clone());
                self.state.upsert_search_document(&record)?;
            }
        }

        drop(index);
        Ok(())
    }
}

fn validate_promotion_request_bounds(request: &MemoryPromotionRequest) -> Result<()> {
    if request.facts.len() > PROMOTION_MAX_FACTS {
        return Err(AxiomError::Validation(format!(
            "facts exceeds max limit: {} > {}",
            request.facts.len(),
            PROMOTION_MAX_FACTS
        )));
    }
    for (index, fact) in request.facts.iter().enumerate() {
        if fact.text.chars().count() > PROMOTION_MAX_TEXT_CHARS {
            return Err(AxiomError::Validation(format!(
                "fact[{index}].text exceeds max chars: {} > {}",
                fact.text.chars().count(),
                PROMOTION_MAX_TEXT_CHARS
            )));
        }
        if fact.source_message_ids.len() > PROMOTION_MAX_SOURCE_IDS_PER_FACT {
            return Err(AxiomError::Validation(format!(
                "fact[{index}].source_message_ids exceeds max count: {} > {}",
                fact.source_message_ids.len(),
                PROMOTION_MAX_SOURCE_IDS_PER_FACT
            )));
        }
        if fact.confidence_milli > PROMOTION_MAX_CONFIDENCE_MILLI {
            return Err(AxiomError::Validation(format!(
                "fact[{index}].confidence_milli out of range: {} > {}",
                fact.confidence_milli, PROMOTION_MAX_CONFIDENCE_MILLI
            )));
        }
    }
    Ok(())
}

fn promotion_apply_input_from_request(
    request: &MemoryPromotionRequest,
) -> Result<PromotionApplyInput> {
    validate_promotion_request_bounds(request)?;
    let facts = dedup_promotion_facts(&normalize_promotion_facts(&request.facts));
    let request_json = canonical_promotion_request_json(
        request.session_id.as_str(),
        request.checkpoint_id.as_str(),
        request.apply_mode,
        &facts,
    )?;
    let request_hash = blake3::hash(request_json.as_bytes()).to_hex().to_string();
    Ok(PromotionApplyInput {
        request_hash,
        request_json,
        apply_mode: request.apply_mode,
        facts,
    })
}

fn promotion_apply_input_from_checkpoint_json(
    request_json: &str,
    expected_session_id: &str,
    expected_checkpoint_id: &str,
) -> Result<PromotionApplyInput> {
    let request: MemoryPromotionRequest = serde_json::from_str(request_json).map_err(|error| {
        AxiomError::Validation(format!("invalid checkpoint request_json: {error}"))
    })?;
    if request.session_id.trim() != expected_session_id {
        return Err(AxiomError::Validation(format!(
            "checkpoint request_json session_id mismatch: expected {expected_session_id}, got {}",
            request.session_id
        )));
    }
    if request.checkpoint_id.trim() != expected_checkpoint_id {
        return Err(AxiomError::Validation(format!(
            "checkpoint request_json checkpoint_id mismatch: expected {expected_checkpoint_id}, got {}",
            request.checkpoint_id
        )));
    }
    validate_promotion_request_bounds(&request)?;
    let facts = dedup_promotion_facts(&normalize_promotion_facts(&request.facts));
    Ok(PromotionApplyInput {
        request_hash: blake3::hash(request_json.as_bytes()).to_hex().to_string(),
        request_json: request_json.to_string(),
        apply_mode: request.apply_mode,
        facts,
    })
}

fn validate_promotion_fact_semantics(fact: &MemoryPromotionFact) -> Result<()> {
    if normalize_memory_text(&fact.text).is_empty() {
        return Err(AxiomError::Validation(
            "promotion fact text must not be empty".to_string(),
        ));
    }
    if dedup_source_ids(&fact.source_message_ids).is_empty() {
        return Err(AxiomError::Validation(
            "promotion fact source_message_ids must not be empty".to_string(),
        ));
    }
    Ok(())
}

fn normalize_promotion_facts(facts: &[MemoryPromotionFact]) -> Vec<MemoryPromotionFact> {
    let mut out = facts
        .iter()
        .map(|fact| MemoryPromotionFact {
            category: fact.category,
            text: normalize_memory_text(&fact.text),
            source_message_ids: dedup_source_ids(&fact.source_message_ids),
            source: fact
                .source
                .as_ref()
                .map(|value| normalize_memory_text(value))
                .filter(|value| !value.is_empty()),
            confidence_milli: fact.confidence_milli.min(PROMOTION_MAX_CONFIDENCE_MILLI),
        })
        .collect::<Vec<_>>();
    out.sort_by(|left, right| {
        left.category
            .as_str()
            .cmp(right.category.as_str())
            .then_with(|| left.text.cmp(&right.text))
            .then_with(|| left.source_message_ids.cmp(&right.source_message_ids))
    });
    out
}

fn dedup_promotion_facts(facts: &[MemoryPromotionFact]) -> Vec<MemoryPromotionFact> {
    let mut out = Vec::<MemoryPromotionFact>::new();
    for fact in facts {
        if let Some(existing) = out.iter_mut().find(|item| {
            item.category == fact.category
                && normalize_memory_text(&item.text) == normalize_memory_text(&fact.text)
        }) {
            existing
                .source_message_ids
                .extend(fact.source_message_ids.clone());
            existing.source_message_ids = dedup_source_ids(&existing.source_message_ids);
            if existing.source.is_none() {
                existing.source = fact.source.clone();
            }
            existing.confidence_milli = existing.confidence_milli.max(fact.confidence_milli);
        } else {
            out.push(fact.clone());
        }
    }
    out.sort_by(|left, right| {
        left.category
            .as_str()
            .cmp(right.category.as_str())
            .then_with(|| left.text.cmp(&right.text))
            .then_with(|| left.source_message_ids.cmp(&right.source_message_ids))
    });
    out
}

fn canonical_promotion_request_json(
    session_id: &str,
    checkpoint_id: &str,
    apply_mode: PromotionApplyMode,
    facts: &[MemoryPromotionFact],
) -> Result<String> {
    let facts_json = facts
        .iter()
        .map(|fact| {
            serde_json::json!({
                "category": fact.category.as_str(),
                "text": fact.text,
                "source_message_ids": fact.source_message_ids,
                "source": fact.source,
                "confidence_milli": fact.confidence_milli,
            })
        })
        .collect::<Vec<_>>();
    let payload = serde_json::json!({
        "session_id": session_id,
        "checkpoint_id": checkpoint_id,
        "apply_mode": promotion_apply_mode_label(apply_mode),
        "facts": facts_json,
    });
    Ok(serde_json::to_string(&payload)?)
}

const fn promotion_apply_mode_label(mode: PromotionApplyMode) -> &'static str {
    match mode {
        PromotionApplyMode::AllOrNothing => "all_or_nothing",
        PromotionApplyMode::BestEffort => "best_effort",
    }
}

fn plan_promotion_apply(
    existing: &[ExistingPromotionFact],
    incoming: &[MemoryPromotionFact],
) -> PromotionApplyPlan {
    let mut seen = existing
        .iter()
        .map(|fact| format!("{}|{}", fact.category, normalize_memory_text(&fact.text)))
        .collect::<HashSet<_>>();
    let mut skipped_duplicates = 0usize;
    let mut candidates = Vec::<ResolvedMemoryCandidate>::new();

    for fact in incoming {
        let text = normalize_memory_text(&fact.text);
        let category = fact.category.as_str().to_string();
        let dedup_key = format!("{category}|{text}");
        if !seen.insert(dedup_key) {
            skipped_duplicates = skipped_duplicates.saturating_add(1);
            continue;
        }
        candidates.push(ResolvedMemoryCandidate {
            category: category.clone(),
            key: build_memory_key(&category, &text),
            text,
            source_message_ids: dedup_source_ids(&fact.source_message_ids),
            target_uri: None,
        });
    }

    PromotionApplyPlan {
        candidates,
        skipped_duplicates,
    }
}

fn restore_promotion_snapshots(
    session: &Session,
    snapshots: &BTreeMap<String, Option<String>>,
) -> Result<()> {
    for (uri_raw, content) in snapshots {
        let uri = AxiomUri::parse(uri_raw)?;
        let path = session.fs.resolve_uri(&uri);
        match content {
            Some(previous) => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&path, previous)?;
            }
            None => {
                if path.exists() {
                    fs::remove_file(path)?;
                }
            }
        }
    }
    Ok(())
}

fn memory_category_path(category: &str) -> Result<(Scope, &'static str, bool)> {
    let resolved = match category {
        "profile" => (Scope::User, "memories/profile.md", true),
        "preferences" => (Scope::User, "memories/preferences", false),
        "entities" => (Scope::User, "memories/entities", false),
        "events" => (Scope::User, "memories/events", false),
        "cases" => (Scope::Agent, "memories/cases", false),
        "patterns" => (Scope::Agent, "memories/patterns", false),
        other => {
            return Err(AxiomError::Validation(format!(
                "unsupported memory category: {other}"
            )));
        }
    };
    Ok(resolved)
}

fn memory_uri_for_category_key(category: &str, key: &str) -> Result<AxiomUri> {
    let (scope, base_path, single_file) = memory_category_path(category)?;
    if single_file {
        return AxiomUri::root(scope).join(base_path);
    }
    AxiomUri::root(scope).join(&format!("{base_path}/{}.md", slugify(key)))
}

fn merge_resolved_candidate(
    resolved: &mut Vec<ResolvedMemoryCandidate>,
    mut next: ResolvedMemoryCandidate,
) {
    next.source_message_ids = dedup_source_ids(&next.source_message_ids);
    if let Some(existing) = resolved.iter_mut().find(|item| {
        item.category == next.category
            && item.text == next.text
            && item.target_uri == next.target_uri
    }) {
        existing
            .source_message_ids
            .extend(next.source_message_ids.clone());
        existing.source_message_ids = dedup_source_ids(&existing.source_message_ids);
    } else {
        resolved.push(next);
    }
}

fn dedup_source_ids(ids: &[String]) -> Vec<String> {
    let mut out = ids
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

fn prefilter_existing_memory_matches(
    candidate_text: &str,
    existing: &[ExistingMemoryFact],
    threshold: f32,
) -> Vec<PrefilteredMemoryMatch> {
    let mut out = Vec::<PrefilteredMemoryMatch>::new();
    let normalized_candidate = normalize_memory_text(candidate_text);
    let candidate_vector = embed_text(candidate_text);
    for fact in existing {
        let score = if normalize_memory_text(&fact.text) == normalized_candidate {
            1.0
        } else {
            cosine_similarity(&candidate_vector, &fact.vector)
        };
        if score >= threshold {
            out.push(PrefilteredMemoryMatch {
                uri: fact.uri.clone(),
                text: fact.text.clone(),
                score,
            });
        }
    }
    out.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.uri.to_string().cmp(&right.uri.to_string()))
    });
    out
}

const fn deterministic_dedup_selection(prefiltered: &[PrefilteredMemoryMatch]) -> DedupSelection {
    if prefiltered.is_empty() {
        DedupSelection {
            decision: MemoryDedupDecision::Create,
            selected_index: None,
        }
    } else {
        DedupSelection {
            decision: MemoryDedupDecision::Merge,
            selected_index: Some(0),
        }
    }
}

fn resolve_dedup_selection(
    candidate: &ExtractedMemory,
    normalized_text: &str,
    prefiltered: &[PrefilteredMemoryMatch],
    config: &MemoryDedupConfig,
) -> Result<(DedupSelection, Option<String>)> {
    let deterministic = deterministic_dedup_selection(prefiltered);
    let conservative_create = DedupSelection {
        decision: MemoryDedupDecision::Create,
        selected_index: None,
    };
    match config.mode {
        MemoryDedupMode::Deterministic => Ok((deterministic, None)),
        MemoryDedupMode::Llm | MemoryDedupMode::Auto => {
            if prefiltered.is_empty() {
                return Ok((deterministic, None));
            }
            match llm_dedup_selection(candidate, normalized_text, prefiltered, config) {
                Ok(selection) => Ok((selection, None)),
                Err(err) => {
                    if config.mode == MemoryDedupMode::Llm && config.llm_strict {
                        Err(err)
                    } else {
                        Ok((conservative_create, Some(err.to_string())))
                    }
                }
            }
        }
    }
}

fn llm_dedup_selection(
    candidate: &ExtractedMemory,
    normalized_text: &str,
    prefiltered: &[PrefilteredMemoryMatch],
    config: &MemoryDedupConfig,
) -> Result<DedupSelection> {
    let endpoint = parse_local_loopback_endpoint(
        &config.llm_endpoint,
        "memory dedup llm endpoint",
        "local host",
    )
    .map_err(AxiomError::Validation)?;
    let client = Client::builder()
        .timeout(std::time::Duration::from_millis(config.llm_timeout_ms))
        .build()
        .map_err(|err| {
            AxiomError::Internal(format!("memory dedup llm client build failed: {err}"))
        })?;

    let matches_payload = prefiltered
        .iter()
        .take(config.llm_max_matches)
        .enumerate()
        .map(|(index, item)| {
            serde_json::json!({
                "rank": index + 1,
                "uri": item.uri.to_string(),
                "text": item.text,
                "score": item.score,
            })
        })
        .collect::<Vec<_>>();
    let prompt_payload = serde_json::json!({
        "candidate": {
            "category": candidate.category,
            "text": normalized_text,
            "source_message_ids": candidate.source_message_ids,
        },
        "matches": matches_payload,
    });
    let system_prompt = "Decide dedup action for candidate memory against similar memories. \
Return JSON only with schema: {\"decision\":\"create|merge|skip\",\"target_index\":1,\"target_uri\":\"...\",\"reason\":\"...\"}.";
    let user_prompt = format!(
        "Dedup request JSON:\n{}\n\nRules:\n- create: candidate should become new memory\n- merge: candidate matches existing memory\n- skip: candidate is duplicate/no-op\n- if merge, choose either target_index (1-based) or target_uri",
        serde_json::to_string(&prompt_payload)?
    );
    let payload = serde_json::json!({
        "model": config.llm_model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "stream": false,
        "options": {
            "temperature": (f64::from(config.llm_temperature_milli) / 1000.0),
            "num_predict": config.llm_max_output_tokens
        }
    });

    let response =
        client.post(endpoint).json(&payload).send().map_err(|err| {
            AxiomError::Internal(format!("memory dedup llm request failed: {err}"))
        })?;
    if !response.status().is_success() {
        return Err(AxiomError::Internal(format!(
            "memory dedup llm non-success status: {}",
            response.status()
        )));
    }

    let value = response.json::<Value>().map_err(|err| {
        AxiomError::Internal(format!("memory dedup llm invalid json response: {err}"))
    })?;
    let parsed = parse_llm_dedup_decision(&value)?;
    let selected_index = match parsed.decision {
        MemoryDedupDecision::Create | MemoryDedupDecision::Skip => None,
        MemoryDedupDecision::Merge => Some(resolve_merge_target_index(&parsed, prefiltered)?),
    };
    Ok(DedupSelection {
        decision: parsed.decision,
        selected_index,
    })
}

fn resolve_merge_target_index(
    parsed: &ParsedLlmDedupDecision,
    prefiltered: &[PrefilteredMemoryMatch],
) -> Result<usize> {
    if prefiltered.is_empty() {
        return Err(AxiomError::Validation(
            "memory dedup llm merge decision requires at least one prefiltered match".to_string(),
        ));
    }
    if let Some(target_uri) = parsed.target_uri.as_deref()
        && let Some(index) = prefiltered
            .iter()
            .position(|item| item.uri.to_string() == target_uri)
    {
        return Ok(index);
    }
    if let Some(rank) = parsed.target_index
        && rank > 0
    {
        let idx = rank - 1;
        if idx < prefiltered.len() {
            return Ok(idx);
        }
    }
    Err(AxiomError::Validation(
        "memory dedup llm merge decision missing valid target".to_string(),
    ))
}

fn parse_llm_dedup_decision(value: &Value) -> Result<ParsedLlmDedupDecision> {
    if let Some(parsed) = parse_llm_dedup_decision_value(value) {
        return Ok(parsed);
    }

    let content = extract_llm_content(value).ok_or_else(|| {
        AxiomError::Validation("memory dedup llm response missing content".to_string())
    })?;
    let json_fragment = extract_json_fragment(&content).ok_or_else(|| {
        AxiomError::Validation(
            "memory dedup llm response does not contain json object/array".to_string(),
        )
    })?;
    let parsed_value = serde_json::from_str::<Value>(&json_fragment).map_err(|err| {
        AxiomError::Validation(format!("memory dedup llm content json parse failed: {err}"))
    })?;
    parse_llm_dedup_decision_value(&parsed_value).ok_or_else(|| {
        AxiomError::Validation("memory dedup llm response schema is unsupported".to_string())
    })
}

fn parse_llm_dedup_decision_value(value: &Value) -> Option<ParsedLlmDedupDecision> {
    let object = value.as_object()?;
    let object = object
        .get("result")
        .or_else(|| object.get("data"))
        .and_then(|inner| inner.as_object())
        .unwrap_or(object);

    let decision = object
        .get("decision")
        .or_else(|| object.get("action"))
        .or_else(|| object.get("mode"))
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_ascii_lowercase())
        .and_then(|value| match value.as_str() {
            "create" => Some(MemoryDedupDecision::Create),
            "merge" => Some(MemoryDedupDecision::Merge),
            "skip" => Some(MemoryDedupDecision::Skip),
            _ => None,
        })?;
    let target_uri = object
        .get("target_uri")
        .or_else(|| object.get("uri"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let target_index = object
        .get("target_index")
        .or_else(|| object.get("target_rank"))
        .or_else(|| object.get("match_index"))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0);

    Some(ParsedLlmDedupDecision {
        decision,
        target_uri,
        target_index,
    })
}

fn has_markdown_extension(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let limit = left.len().min(right.len());
    let mut dot = 0.0f32;
    let mut left_norm = 0.0f32;
    let mut right_norm = 0.0f32;
    for idx in 0..limit {
        dot += left[idx] * right[idx];
        left_norm += left[idx] * left[idx];
        right_norm += right[idx] * right[idx];
    }
    if left_norm <= 0.0 || right_norm <= 0.0 {
        return 0.0;
    }
    dot / (left_norm.sqrt() * right_norm.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extracted(category: &str, text: &str, source_ids: &[&str]) -> ExtractedMemory {
        ExtractedMemory {
            category: category.to_string(),
            key: build_memory_key(category, text),
            text: text.to_string(),
            source_message_ids: source_ids.iter().copied().map(str::to_string).collect(),
            confidence_milli: 700,
        }
    }

    fn dedup_config(mode: MemoryDedupMode, strict: bool, endpoint: &str) -> MemoryDedupConfig {
        MemoryDedupConfig {
            mode,
            similarity_threshold: 0.9,
            llm_endpoint: endpoint.to_string(),
            llm_model: "qwen2.5:7b-instruct".to_string(),
            llm_timeout_ms: DEFAULT_MEMORY_DEDUP_LLM_TIMEOUT_MS,
            llm_max_output_tokens: DEFAULT_MEMORY_DEDUP_LLM_MAX_OUTPUT_TOKENS,
            llm_temperature_milli: DEFAULT_MEMORY_DEDUP_LLM_TEMPERATURE_MILLI,
            llm_strict: strict,
            llm_max_matches: DEFAULT_MEMORY_DEDUP_LLM_MAX_MATCHES,
        }
    }

    #[test]
    fn cosine_similarity_returns_expected_value() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.5, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b) > 0.99);
        assert!(cosine_similarity(&a, &c) < 0.01);
    }

    #[test]
    fn memory_dedup_mode_defaults_to_auto() {
        assert_eq!(MemoryDedupMode::parse(None), MemoryDedupMode::Auto);
        assert_eq!(MemoryDedupMode::parse(Some("")), MemoryDedupMode::Auto);
    }

    #[test]
    fn merge_resolved_candidate_combines_source_ids() {
        let mut out = Vec::<ResolvedMemoryCandidate>::new();
        merge_resolved_candidate(
            &mut out,
            ResolvedMemoryCandidate {
                category: "preferences".to_string(),
                key: "pref-1".to_string(),
                text: "I prefer concise Rust code".to_string(),
                source_message_ids: vec!["m2".to_string()],
                target_uri: None,
            },
        );
        merge_resolved_candidate(
            &mut out,
            ResolvedMemoryCandidate {
                category: "preferences".to_string(),
                key: "pref-1".to_string(),
                text: "I prefer concise Rust code".to_string(),
                source_message_ids: vec!["m1".to_string()],
                target_uri: None,
            },
        );
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].source_message_ids,
            vec!["m1".to_string(), "m2".to_string()]
        );
    }

    #[test]
    fn prefilter_existing_memory_matches_keeps_exact_at_threshold_one() {
        let existing = vec![
            ExistingMemoryFact {
                uri: AxiomUri::parse("axiom://user/memories/preferences/pref-a.md").expect("uri"),
                text: "I prefer concise Rust code".to_string(),
                vector: embed_text("I prefer concise Rust code"),
            },
            ExistingMemoryFact {
                uri: AxiomUri::parse("axiom://user/memories/preferences/pref-b.md").expect("uri"),
                text: "Use Kubernetes deployment checklist".to_string(),
                vector: embed_text("Use Kubernetes deployment checklist"),
            },
        ];
        let matches =
            prefilter_existing_memory_matches("I prefer concise Rust code", &existing, 1.0);
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].uri.to_string(),
            "axiom://user/memories/preferences/pref-a.md"
        );
        assert!(matches[0].score >= 1.0);
    }

    #[test]
    fn parse_llm_dedup_decision_accepts_object_payload() {
        let payload = serde_json::json!({
            "decision": "merge",
            "target_index": 2,
            "target_uri": "axiom://user/memories/preferences/pref-2.md",
            "reason": "same preference"
        });
        let parsed = parse_llm_dedup_decision(&payload).expect("parse");
        assert_eq!(parsed.decision, MemoryDedupDecision::Merge);
        assert_eq!(parsed.target_index, Some(2));
        assert_eq!(
            parsed.target_uri.as_deref(),
            Some("axiom://user/memories/preferences/pref-2.md")
        );
    }

    #[test]
    fn parse_llm_dedup_decision_accepts_data_wrapper() {
        let payload = serde_json::json!({
            "data": {
                "decision": "merge",
                "target_index": 1
            }
        });
        let parsed = parse_llm_dedup_decision(&payload).expect("parse");
        assert_eq!(parsed.decision, MemoryDedupDecision::Merge);
        assert_eq!(parsed.target_index, Some(1));
        assert_eq!(parsed.target_uri, None);
    }

    #[test]
    fn parse_llm_dedup_decision_accepts_embedded_json_content() {
        let payload = serde_json::json!({
            "message": {
                "content": "```json\n{\"decision\":\"skip\"}\n```"
            }
        });
        let parsed = parse_llm_dedup_decision(&payload).expect("parse");
        assert_eq!(parsed.decision, MemoryDedupDecision::Skip);
        assert_eq!(parsed.target_index, None);
        assert_eq!(parsed.target_uri, None);
    }

    #[test]
    fn resolve_dedup_selection_auto_falls_back_to_create_on_llm_error() {
        let candidate = extracted("preferences", "I prefer concise Rust code", &["m1"]);
        let prefiltered = vec![PrefilteredMemoryMatch {
            uri: AxiomUri::parse("axiom://user/memories/preferences/pref-a.md").expect("uri"),
            text: "I prefer concise Rust code".to_string(),
            score: 1.0,
        }];
        let config = dedup_config(MemoryDedupMode::Auto, false, "http://example.com/api/chat");
        let (selection, llm_error) =
            resolve_dedup_selection(&candidate, &candidate.text, &prefiltered, &config)
                .expect("selection");
        assert_eq!(selection.decision, MemoryDedupDecision::Create);
        assert_eq!(selection.selected_index, None);
        assert!(llm_error.is_some());
    }

    #[test]
    fn resolve_dedup_selection_llm_strict_returns_error_on_llm_failure() {
        let candidate = extracted("preferences", "I prefer concise Rust code", &["m1"]);
        let prefiltered = vec![PrefilteredMemoryMatch {
            uri: AxiomUri::parse("axiom://user/memories/preferences/pref-a.md").expect("uri"),
            text: "I prefer concise Rust code".to_string(),
            score: 1.0,
        }];
        let config = dedup_config(MemoryDedupMode::Llm, true, "http://example.com/api/chat");
        let err = resolve_dedup_selection(&candidate, &candidate.text, &prefiltered, &config)
            .expect_err("must fail");
        assert!(err.to_string().contains("memory dedup llm endpoint"));
    }

    #[test]
    fn resolve_merge_target_index_requires_valid_target() {
        let prefiltered = vec![PrefilteredMemoryMatch {
            uri: AxiomUri::parse("axiom://user/memories/preferences/pref-a.md").expect("uri"),
            text: "I prefer concise Rust code".to_string(),
            score: 1.0,
        }];
        let parsed = ParsedLlmDedupDecision {
            decision: MemoryDedupDecision::Merge,
            target_uri: Some("axiom://user/memories/preferences/unknown.md".to_string()),
            target_index: Some(99),
        };
        let err = resolve_merge_target_index(&parsed, &prefiltered).expect_err("must fail");
        assert!(err.to_string().contains("missing valid target"));
    }
}
