use std::collections::HashSet;
use std::hash::BuildHasher;

use chrono::{DateTime, Utc};

use super::super::types::OmObserverMessageCandidate;

#[must_use]
pub fn select_observer_message_candidates(
    candidates: &[OmObserverMessageCandidate],
    observed_message_ids: &HashSet<String, impl BuildHasher>,
    max_messages: usize,
) -> Vec<OmObserverMessageCandidate> {
    if max_messages == 0 {
        return Vec::new();
    }

    let mut filtered = candidates
        .iter()
        .filter(|candidate| {
            !candidate.id.trim().is_empty() && !observed_message_ids.contains(&candidate.id)
        })
        .cloned()
        .collect::<Vec<_>>();

    filtered.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.source_session_id.cmp(&b.source_session_id))
            .then_with(|| a.id.cmp(&b.id))
    });

    if filtered.len() > max_messages {
        filtered = filtered[filtered.len() - max_messages..].to_vec();
    }
    filtered
}

#[must_use]
pub fn filter_observer_candidates_by_last_observed_at(
    candidates: &[OmObserverMessageCandidate],
    last_observed_at: Option<DateTime<Utc>>,
) -> Vec<OmObserverMessageCandidate> {
    let Some(cutoff) = last_observed_at else {
        return candidates.to_vec();
    };
    candidates
        .iter()
        .filter(|candidate| candidate.created_at > cutoff)
        .cloned()
        .collect::<Vec<_>>()
}

#[must_use]
pub fn split_pending_and_other_conversation_candidates(
    candidates: &[OmObserverMessageCandidate],
    current_session_id: Option<&str>,
) -> (
    Vec<OmObserverMessageCandidate>,
    Vec<OmObserverMessageCandidate>,
) {
    let mut pending = Vec::<OmObserverMessageCandidate>::new();
    let mut other_conversations = Vec::<OmObserverMessageCandidate>::new();

    for candidate in candidates {
        let is_pending = current_session_id.is_none_or(|current| {
            candidate
                .source_session_id
                .as_deref()
                .is_none_or(|source| source == current)
        });
        if is_pending {
            pending.push(candidate.clone());
        } else {
            other_conversations.push(candidate.clone());
        }
    }

    (pending, other_conversations)
}

#[must_use]
pub fn select_observed_message_candidates(
    candidates: &[OmObserverMessageCandidate],
    observed_message_ids: &[String],
) -> Vec<OmObserverMessageCandidate> {
    if observed_message_ids.is_empty() {
        return candidates.to_vec();
    }
    let observed = observed_message_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    candidates
        .iter()
        .filter(|item| observed.contains(item.id.as_str()))
        .cloned()
        .collect::<Vec<_>>()
}
