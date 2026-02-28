use std::collections::HashMap;

use crate::embedding::embed_text;
use crate::error::{AxiomError, Result};

use super::super::memory::{build_memory_key, normalize_memory_text};
use super::super::memory_extractor::ExtractedMemory;
use super::Session;
use super::dedup::{prefilter_existing_memory_matches, resolve_dedup_selection};
use super::fallbacks::record_memory_dedup_fallback as record_memory_dedup_fallback_event;
use super::promotion::memory_uri_for_category_key;
use super::read_path::list_existing_memory_facts;
use super::types::{
    ExistingMemoryFact, MemoryDedupConfig, MemoryDedupDecision, ResolvedMemoryCandidate,
};

impl Session {
    pub(super) fn resolve_memory_candidates(
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
                let existing = list_existing_memory_facts(self, &candidate.category)?;
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
                record_memory_dedup_fallback_event(self, dedup_config.mode.as_str(), &error);
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
}

pub(super) fn merge_resolved_candidate(
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

pub(super) fn dedup_source_ids(ids: &[String]) -> Vec<String> {
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
