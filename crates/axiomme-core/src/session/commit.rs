use std::collections::HashMap;
use std::fs;
use std::path::Path;

use chrono::Utc;
use reqwest::blocking::Client;
use serde_json::Value;
use uuid::Uuid;

use crate::config::MemoryDedupConfigSnapshot;
use crate::embedding::embed_text;
use crate::error::{AxiomError, Result};
use crate::llm_io::{extract_json_fragment, extract_llm_content, parse_local_loopback_endpoint};
use crate::models::{CommitResult, CommitStats, IndexRecord, MemoryCandidate};
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

        let extracted =
            extract_memories_for_commit(&active_messages, &self.config.memory.extractor)?;
        if let Some(error) = extracted.llm_error.as_deref() {
            self.record_memory_extractor_fallback(&extracted.mode_requested, error);
        }

        let candidates = self.resolve_memory_candidates(&extracted.memories)?;
        let mut persisted_uris = Vec::new();
        for candidate in &candidates {
            let uri = self.persist_memory(candidate)?;
            persisted_uris.push(uri);
        }

        self.reindex_memory_uris(&persisted_uris)?;

        self.touch_meta(|meta| {
            meta.updated_at = Utc::now();
        })?;

        Ok(CommitResult {
            session_id: self.session_id.clone(),
            status: "committed".to_string(),
            memories_extracted: candidates.len(),
            active_count_updated: persisted_uris.len(),
            archived: true,
            stats: CommitStats {
                total_turns,
                contexts_used: meta.context_usage.contexts_used,
                skills_used: meta.context_usage.skills_used,
                memories_extracted: candidates.len(),
            },
        })
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
        if let Ok(event_id) = self.state.enqueue(
            "memory_extract_fallback",
            &uri,
            serde_json::json!({
                "session_id": self.session_id,
                "mode_requested": mode_requested,
                "error": error,
            }),
        ) {
            let _ = self.state.mark_outbox_status(event_id, "dead_letter", true);
        }
    }

    fn record_memory_dedup_fallback(&self, mode_requested: &str, error: &str) {
        let uri = format!("axiom://session/{}", self.session_id);
        if let Ok(event_id) = self.state.enqueue(
            "memory_dedup_fallback",
            &uri,
            serde_json::json!({
                "session_id": self.session_id,
                "mode_requested": mode_requested,
                "error": error,
            }),
        ) {
            let _ = self.state.mark_outbox_status(event_id, "dead_letter", true);
        }
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
