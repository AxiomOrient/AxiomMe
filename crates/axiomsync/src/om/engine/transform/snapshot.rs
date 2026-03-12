use std::collections::HashSet;

use crate::model::{
    OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION, OmContinuationStateV2, OmHintPolicyV2,
    OmObservationEntryV2, OmObservationPriority, OmSearchVisibleSnapshotV2,
};

const SEARCH_HINT_PREFIX: &str = "om:";

fn normalize_text(value: &str) -> Option<String> {
    let mut out = String::new();
    for part in value.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(part);
    }
    if out.is_empty() { None } else { Some(out) }
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value.and_then(normalize_text)
}

fn entry_created_at(entry: &OmObservationEntryV2) -> &str {
    entry.created_at_rfc3339.as_str()
}

fn dedup_entries_by_id(entries: &[OmObservationEntryV2]) -> Vec<OmObservationEntryV2> {
    let mut seen = HashSet::<String>::new();
    let mut deduped = Vec::<OmObservationEntryV2>::new();
    for entry in entries {
        if normalize_text(&entry.entry_id)
            .map(|id| seen.insert(id))
            .unwrap_or(false)
        {
            deduped.push(entry.clone());
        }
    }
    deduped
}

fn truncate_by_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect::<String>()
}

#[must_use]
pub fn render_search_hint(
    snapshot: &OmSearchVisibleSnapshotV2,
    policy: OmHintPolicyV2,
) -> Option<String> {
    if policy.max_lines == 0 || policy.max_chars == 0 {
        return None;
    }

    let mut selected = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();
    let push_line =
        |line: String, selected: &mut Vec<String>, seen: &mut HashSet<String>| -> bool {
            if selected.len() >= policy.max_lines {
                return false;
            }
            if seen.insert(line.clone()) {
                selected.push(line);
                return true;
            }
            false
        };

    if policy.reserve_current_task_line
        && let Some(task) = normalize_optional(snapshot.current_task.as_deref())
    {
        let _ = push_line(format!("current-task: {task}"), &mut selected, &mut seen);
    }
    if policy.reserve_suggested_response_line
        && let Some(suggested) = normalize_optional(snapshot.suggested_response.as_deref())
    {
        let _ = push_line(
            format!("suggested-response: {suggested}"),
            &mut selected,
            &mut seen,
        );
    }

    let mut candidates = snapshot
        .visible_entries
        .iter()
        .filter(|entry| {
            entry
                .superseded_by
                .as_deref()
                .and_then(normalize_text)
                .is_none()
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        entry_created_at(right)
            .cmp(entry_created_at(left))
            .then_with(|| left.entry_id.cmp(&right.entry_id))
    });

    let mut added_high = 0usize;
    for entry in candidates
        .iter()
        .copied()
        .filter(|entry| entry.priority == OmObservationPriority::High)
    {
        if selected.len() >= policy.max_lines || added_high >= policy.high_priority_slots {
            break;
        }
        if let Some(text) = normalize_text(&entry.text)
            && push_line(text, &mut selected, &mut seen)
        {
            added_high += 1;
        }
    }

    for entry in candidates {
        if selected.len() >= policy.max_lines {
            break;
        }
        if entry.priority == OmObservationPriority::High && added_high > 0 {
            continue;
        }
        if let Some(text) = normalize_text(&entry.text) {
            let _ = push_line(text, &mut selected, &mut seen);
        }
    }

    if selected.is_empty() {
        return snapshot
            .rendered_hint
            .as_deref()
            .and_then(normalize_text)
            .map(|hint| {
                let content = truncate_by_chars(&hint, policy.max_chars);
                if content.starts_with(SEARCH_HINT_PREFIX) {
                    content
                } else {
                    format!("{SEARCH_HINT_PREFIX} {content}")
                }
            });
    }

    let content = selected.join(" | ");
    let content = truncate_by_chars(&content, policy.max_chars);
    if content.is_empty() {
        return None;
    }
    Some(format!("{SEARCH_HINT_PREFIX} {content}"))
}

#[must_use]
pub fn materialize_search_visible_snapshot(
    scope_key: &str,
    activated_entries: &[OmObservationEntryV2],
    buffered_entries: &[OmObservationEntryV2],
    continuation: Option<&OmContinuationStateV2>,
    materialized_at_rfc3339: &str,
    policy: OmHintPolicyV2,
) -> OmSearchVisibleSnapshotV2 {
    let normalized_scope_key = normalize_text(scope_key).unwrap_or_default();
    let normalized_materialized_at = normalize_text(materialized_at_rfc3339).unwrap_or_default();

    let mut activated = dedup_entries_by_id(activated_entries);
    activated.sort_by(|left, right| {
        entry_created_at(left)
            .cmp(entry_created_at(right))
            .then_with(|| left.entry_id.cmp(&right.entry_id))
    });
    let activated_entry_ids = activated
        .iter()
        .filter_map(|entry| normalize_text(&entry.entry_id))
        .collect::<Vec<_>>();

    let mut buffered = dedup_entries_by_id(buffered_entries);
    buffered.sort_by(|left, right| {
        entry_created_at(left)
            .cmp(entry_created_at(right))
            .then_with(|| left.entry_id.cmp(&right.entry_id))
    });
    let buffered_entry_ids = buffered
        .iter()
        .filter_map(|entry| normalize_text(&entry.entry_id))
        .collect::<Vec<_>>();

    let mut visible_entries = activated;
    if policy.include_buffered_entries {
        let mut seen_ids = visible_entries
            .iter()
            .filter_map(|entry| normalize_text(&entry.entry_id))
            .collect::<HashSet<_>>();
        for entry in buffered {
            if let Some(id) = normalize_text(&entry.entry_id)
                && seen_ids.insert(id)
            {
                visible_entries.push(entry);
            }
        }
    }
    visible_entries.sort_by(|left, right| {
        entry_created_at(left)
            .cmp(entry_created_at(right))
            .then_with(|| left.entry_id.cmp(&right.entry_id))
    });

    let mut snapshot = OmSearchVisibleSnapshotV2 {
        scope_key: normalized_scope_key,
        activated_entry_ids,
        buffered_entry_ids,
        current_task: continuation
            .and_then(|state| normalize_optional(state.current_task.as_deref())),
        suggested_response: continuation
            .and_then(|state| normalize_optional(state.suggested_response.as_deref())),
        rendered_hint: None,
        materialized_at_rfc3339: normalized_materialized_at,
        snapshot_version: OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION.to_string(),
        visible_entries,
    };
    snapshot.rendered_hint = render_search_hint(&snapshot, policy);
    snapshot
}
