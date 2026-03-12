use std::collections::BTreeSet;

use crate::model::{
    OmObservationEntryV2, OmObservationOriginKind, OmObservationPriority, OmReflectionResponseV2,
};

fn normalize_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_ids(ids: &[String]) -> Vec<String> {
    let mut normalized = ids
        .iter()
        .filter_map(|id| normalize_text(id))
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

#[must_use]
pub fn apply_reflection_response_v2(
    entries: &[OmObservationEntryV2],
    response: &OmReflectionResponseV2,
    reflection_entry_id: &str,
    scope_key: &str,
    thread_id: &str,
    created_at_rfc3339: &str,
) -> Vec<OmObservationEntryV2> {
    let Some(reflection_id) = normalize_text(reflection_entry_id) else {
        return entries.to_vec();
    };
    let Some(normalized_scope_key) = normalize_text(scope_key) else {
        return entries.to_vec();
    };
    let Some(normalized_thread_id) = normalize_text(thread_id) else {
        return entries.to_vec();
    };
    let Some(normalized_created_at) = normalize_text(created_at_rfc3339) else {
        return entries.to_vec();
    };
    let Some(reflection_text) = normalize_text(&response.reflection_text) else {
        return entries.to_vec();
    };

    let covered_ids = normalize_ids(&response.covers_entry_ids)
        .into_iter()
        .collect::<BTreeSet<_>>();
    if covered_ids.is_empty() {
        return entries.to_vec();
    }

    let mut next = entries.to_vec();
    let mut reflection_source_ids = BTreeSet::<String>::new();
    let mut matched_covered_entry = false;

    for entry in &mut next {
        if !covered_ids.contains(&entry.entry_id) {
            continue;
        }
        matched_covered_entry = true;
        entry.superseded_by = Some(reflection_id.clone());
        for source_id in &entry.source_message_ids {
            if let Some(normalized) = normalize_text(source_id) {
                reflection_source_ids.insert(normalized);
            }
        }
    }
    if !matched_covered_entry {
        return entries.to_vec();
    }

    if let Some(existing) = next
        .iter_mut()
        .find(|entry| entry.entry_id == reflection_id)
    {
        existing.scope_key = normalized_scope_key;
        existing.thread_id = normalized_thread_id;
        existing.priority = OmObservationPriority::Medium;
        existing.text = reflection_text;
        existing.origin_kind = OmObservationOriginKind::Reflection;
        existing.source_message_ids = reflection_source_ids.into_iter().collect::<Vec<_>>();
        existing.created_at_rfc3339 = normalized_created_at;
        existing.superseded_by = None;
        return next;
    }

    next.push(OmObservationEntryV2 {
        entry_id: reflection_id,
        scope_key: normalized_scope_key,
        thread_id: normalized_thread_id,
        priority: OmObservationPriority::Medium,
        text: reflection_text,
        source_message_ids: reflection_source_ids.into_iter().collect::<Vec<_>>(),
        origin_kind: OmObservationOriginKind::Reflection,
        created_at_rfc3339: normalized_created_at,
        superseded_by: None,
    });
    next
}
