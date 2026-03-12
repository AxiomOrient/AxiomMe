use std::collections::HashMap;

use crate::om::{
    OmContinuationSourceKind, OmContinuationStateV2, OmObservationChunk, OmObservationEntryV2,
    OmObservationOriginKind, OmObservationPriority,
};
use crate::state::OmActiveEntry;

use super::{OM_HINT_SNAPSHOT_BUFFERED_TAIL_LIMIT, non_empty_trimmed};

#[derive(Debug, Clone, Default)]
pub(super) struct SnapshotVisibleEntrySelection {
    pub(super) selected_entry_ids: Vec<String>,
    pub(super) activated_visible_entry_ids: Vec<String>,
    pub(super) buffered_visible_entry_ids: Vec<String>,
}

pub(super) fn build_snapshot_activated_entries(
    scope_key: &str,
    active_entries: &[OmActiveEntry],
) -> Vec<OmObservationEntryV2> {
    active_entries
        .iter()
        .filter_map(|entry| {
            non_empty_trimmed(&entry.text).map(|text| OmObservationEntryV2 {
                entry_id: entry.entry_id.clone(),
                scope_key: scope_key.to_string(),
                thread_id: entry.canonical_thread_id.clone(),
                priority: parse_observation_priority(entry.priority.as_str()),
                text: text.to_string(),
                source_message_ids: Vec::new(),
                origin_kind: parse_observation_origin(entry.origin_kind.as_str()),
                created_at_rfc3339: entry.created_at.to_rfc3339(),
                superseded_by: None,
            })
        })
        .collect::<Vec<_>>()
}

pub(super) fn build_snapshot_buffered_entries(
    scope_key: &str,
    fallback_thread_id: &str,
    buffered_chunks: &[OmObservationChunk],
) -> (Vec<OmObservationEntryV2>, Vec<String>) {
    let selected_buffered = buffered_chunks
        .iter()
        .rev()
        .filter_map(|chunk| {
            let normalized = chunk.observations.trim();
            if normalized.is_empty() {
                None
            } else {
                Some((
                    chunk.id.clone(),
                    normalized.to_string(),
                    chunk.message_ids.clone(),
                    chunk.created_at.to_rfc3339(),
                ))
            }
        })
        .take(OM_HINT_SNAPSHOT_BUFFERED_TAIL_LIMIT)
        .collect::<Vec<_>>();
    let buffered_chunk_ids = selected_buffered
        .iter()
        .map(|(chunk_id, _, _, _)| chunk_id.clone())
        .collect::<Vec<_>>();
    let buffered_entries = selected_buffered
        .into_iter()
        .map(
            |(chunk_id, text, message_ids, created_at_rfc3339)| OmObservationEntryV2 {
                entry_id: format!("buffered:{scope_key}:{chunk_id}"),
                scope_key: scope_key.to_string(),
                thread_id: fallback_thread_id.to_string(),
                priority: infer_buffered_entry_priority(&text),
                text,
                source_message_ids: message_ids,
                origin_kind: OmObservationOriginKind::Chunk,
                created_at_rfc3339,
                superseded_by: None,
            },
        )
        .collect::<Vec<_>>();
    (buffered_entries, buffered_chunk_ids)
}

pub(super) fn infer_buffered_entry_priority(text: &str) -> OmObservationPriority {
    let lowered = text.to_ascii_lowercase();
    if text.contains('🔴')
        || lowered.starts_with("priority:high")
        || lowered.contains(" priority:high")
        || lowered.starts_with("high:")
        || lowered.starts_with("[high]")
        || lowered.contains("\npriority:high")
        || lowered.contains("\nhigh:")
        || lowered.contains("\n[high]")
    {
        OmObservationPriority::High
    } else {
        OmObservationPriority::Medium
    }
}

pub(super) fn build_snapshot_continuation_state(
    record: &crate::om::OmRecord,
    fallback_thread_id: &str,
    current_task: &Option<String>,
    suggested_response: &Option<String>,
    updated_at_rfc3339: &str,
) -> Option<OmContinuationStateV2> {
    if current_task.is_none() && suggested_response.is_none() {
        return None;
    }
    Some(OmContinuationStateV2 {
        scope_key: record.scope_key.clone(),
        thread_id: fallback_thread_id.to_string(),
        current_task: current_task.clone(),
        suggested_response: suggested_response.clone(),
        confidence_milli: 1000,
        source_kind: OmContinuationSourceKind::ObserverDeterministic,
        source_message_ids: Vec::new(),
        updated_at_rfc3339: updated_at_rfc3339.to_string(),
        staleness_budget_ms: 0,
    })
}

pub(super) fn snapshot_visible_observation_text(entries: &[OmObservationEntryV2]) -> String {
    entries
        .iter()
        .filter_map(|entry| non_empty_trimmed(&entry.text).map(ToString::to_string))
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(super) fn snapshot_visible_entry_selection(
    entries: &[OmObservationEntryV2],
) -> SnapshotVisibleEntrySelection {
    let mut selection = SnapshotVisibleEntrySelection::default();
    let mut ordered_sources = Vec::<String>::new();
    let mut selected_by_source = HashMap::<String, (String, bool)>::new();
    for entry in entries {
        let Some(entry_id) = non_empty_trimmed(&entry.entry_id) else {
            continue;
        };
        let selected_entry_id = entry_id.to_string();
        let is_buffered = selected_entry_id.starts_with("buffered:");
        let source_key = snapshot_visible_entry_source_key(entry_id);
        if let Some(existing) = selected_by_source.get_mut(&source_key) {
            if existing.1 && !is_buffered {
                *existing = (selected_entry_id, false);
            }
            continue;
        }
        ordered_sources.push(source_key.clone());
        selected_by_source.insert(source_key, (selected_entry_id, is_buffered));
    }
    for source_key in ordered_sources {
        if let Some((selected_entry_id, is_buffered)) = selected_by_source.remove(&source_key) {
            if is_buffered {
                selection
                    .buffered_visible_entry_ids
                    .push(selected_entry_id.clone());
            } else {
                selection
                    .activated_visible_entry_ids
                    .push(selected_entry_id.clone());
            }
            selection.selected_entry_ids.push(selected_entry_id);
        }
    }
    selection
}

pub(super) fn snapshot_visible_entry_source_key(entry_id: &str) -> String {
    if let Some(chunk_id) = entry_id.strip_prefix("observation:") {
        return format!("chunk:{}", chunk_id.trim());
    }
    if let Some(buffered_tail) = entry_id.strip_prefix("buffered:")
        && let Some(chunk_id) = buffered_tail.rsplit(':').next()
    {
        return format!("chunk:{}", chunk_id.trim());
    }
    format!("entry:{}", entry_id.trim())
}

fn parse_observation_priority(value: &str) -> OmObservationPriority {
    match value.trim().to_ascii_lowercase().as_str() {
        "high" => OmObservationPriority::High,
        "low" => OmObservationPriority::Low,
        _ => OmObservationPriority::Medium,
    }
}

fn parse_observation_origin(value: &str) -> OmObservationOriginKind {
    match value.trim().to_ascii_lowercase().as_str() {
        "reflection" => OmObservationOriginKind::Reflection,
        "chunk" => OmObservationOriginKind::Chunk,
        "summary" => OmObservationOriginKind::Summary,
        _ => OmObservationOriginKind::Observation,
    }
}
