use crate::model::{ContinuationPolicyV2, OmContinuationCandidateV2, OmContinuationStateV2};

use super::super::helpers::normalize_whitespace;

fn normalize_text(value: Option<&str>) -> Option<String> {
    value
        .map(normalize_whitespace)
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

fn normalize_ids(ids: &[String]) -> Vec<String> {
    let mut normalized = ids
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

#[must_use]
pub fn resolve_continuation_update(
    previous: Option<&OmContinuationStateV2>,
    candidate: &OmContinuationCandidateV2,
    policy: ContinuationPolicyV2,
) -> Option<OmContinuationStateV2> {
    let previous_task = previous.and_then(|state| normalize_text(state.current_task.as_deref()));
    let previous_suggested =
        previous.and_then(|state| normalize_text(state.suggested_response.as_deref()));
    let previous_confidence = previous.map(|state| state.confidence_milli).unwrap_or(0);
    let previous_scope_key = previous.map(|state| state.scope_key.as_str());
    let previous_thread_id = previous.map(|state| state.thread_id.as_str());
    let previous_updated_at = previous.map(|state| state.updated_at_rfc3339.as_str());

    let candidate_confidence = candidate.confidence_milli.min(1000);
    let candidate_task = if candidate_confidence >= policy.min_confidence_milli_for_task {
        normalize_text(candidate.current_task.as_deref())
    } else {
        None
    };
    let candidate_suggested =
        if candidate_confidence >= policy.min_confidence_milli_for_suggested_response {
            normalize_text(candidate.suggested_response.as_deref())
        } else {
            None
        };
    let normalized_scope_key = normalize_text(Some(&candidate.scope_key));
    let normalized_thread_id = normalize_text(Some(&candidate.thread_id));
    let normalized_updated_at = normalize_text(Some(&candidate.updated_at_rfc3339));

    let current_task = candidate_task
        .as_deref()
        .or_else(|| {
            policy
                .preserve_existing_task_on_weaker_update
                .then_some(previous_task.as_deref())
                .flatten()
        })
        .map(ToString::to_string);

    let (suggested_response, adopted_candidate_suggested) =
        if let Some(candidate_value) = candidate_suggested.as_deref() {
            if policy.only_improve_suggested_response
                && previous_suggested.is_some()
                && previous_confidence > candidate_confidence
            {
                (
                    previous_suggested.as_deref().map(ToString::to_string),
                    false,
                )
            } else {
                (Some(candidate_value.to_string()), true)
            }
        } else if policy.only_improve_suggested_response {
            (
                previous_suggested.as_deref().map(ToString::to_string),
                false,
            )
        } else {
            (None, false)
        };
    let candidate_contributed = candidate_task.is_some() || adopted_candidate_suggested;

    if current_task.is_none() && suggested_response.is_none() && previous.is_none() {
        return None;
    }

    let scope_key = normalized_scope_key
        .as_deref()
        .or(previous_scope_key)
        .unwrap_or_default()
        .to_string();
    let thread_id = normalized_thread_id
        .as_deref()
        .or(previous_thread_id)
        .unwrap_or_default()
        .to_string();
    let source_message_ids = if !candidate_contributed {
        previous
            .map(|state| state.source_message_ids.clone())
            .unwrap_or_default()
    } else {
        let ids = normalize_ids(&candidate.source_message_ids);
        if ids.is_empty() {
            previous
                .map(|state| state.source_message_ids.clone())
                .unwrap_or_default()
        } else {
            ids
        }
    };
    let updated_at_rfc3339 = if !candidate_contributed {
        previous_updated_at
            .map(ToString::to_string)
            .unwrap_or_default()
    } else {
        normalized_updated_at
            .as_deref()
            .or(previous_updated_at)
            .map(ToString::to_string)
            .unwrap_or_default()
    };
    let staleness_budget_ms = if !candidate_contributed {
        previous.map(|state| state.staleness_budget_ms).unwrap_or(0)
    } else if candidate.staleness_budget_ms > 0 {
        candidate.staleness_budget_ms
    } else {
        previous.map(|state| state.staleness_budget_ms).unwrap_or(0)
    };
    let (confidence_milli, source_kind) = if !candidate_contributed {
        (
            previous
                .map(|state| state.confidence_milli)
                .unwrap_or(candidate_confidence),
            previous
                .map(|state| state.source_kind)
                .unwrap_or(candidate.source_kind),
        )
    } else {
        (candidate_confidence, candidate.source_kind)
    };

    Some(OmContinuationStateV2 {
        scope_key,
        thread_id,
        current_task,
        suggested_response,
        confidence_milli,
        source_kind,
        source_message_ids,
        updated_at_rfc3339,
        staleness_budget_ms,
    })
}
