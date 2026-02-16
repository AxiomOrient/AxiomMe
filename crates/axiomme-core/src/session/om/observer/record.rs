use std::collections::HashSet;

use chrono::{DateTime, Utc};
use uuid::Uuid;

#[cfg(test)]
use super::super::scope_binding;
use super::super::{
    OmObservationChunk, OmObserverMessageCandidate, OmOriginType, OmRecord, OmScopeBinding,
    combine_observations_for_buffering, estimate_text_tokens,
};

pub(in crate::session::om) fn new_om_record(
    scope: &OmScopeBinding,
    now: DateTime<Utc>,
) -> OmRecord {
    OmRecord {
        id: Uuid::new_v4().to_string(),
        scope: scope.scope,
        scope_key: scope.scope_key.clone(),
        session_id: scope.session_id.clone(),
        thread_id: scope.thread_id.clone(),
        resource_id: scope.resource_id.clone(),
        generation_count: 0,
        last_applied_outbox_event_id: None,
        origin_type: OmOriginType::Initial,
        active_observations: String::new(),
        observation_token_count: 0,
        pending_message_tokens: 0,
        last_observed_at: None,
        current_task: None,
        suggested_response: None,
        last_activated_message_ids: Vec::new(),
        observer_trigger_count_total: 0,
        reflector_trigger_count_total: 0,
        is_observing: false,
        is_reflecting: false,
        is_buffering_observation: false,
        is_buffering_reflection: false,
        last_buffered_at_tokens: 0,
        last_buffered_at_time: None,
        buffered_reflection: None,
        buffered_reflection_tokens: None,
        buffered_reflection_input_tokens: None,
        reflected_observation_line_count: None,
        created_at: now,
        updated_at: now,
    }
}

#[cfg(test)]
pub(in crate::session::om) fn new_session_om_record(
    session_id: &str,
    scope_key: &str,
    now: DateTime<Utc>,
) -> OmRecord {
    let scope = OmScopeBinding {
        scope: super::super::OmScope::Session,
        scope_key: scope_key.to_string(),
        session_id: Some(session_id.to_string()),
        thread_id: None,
        resource_id: None,
    };
    new_om_record(&scope, now)
}

pub(in crate::session::om) fn observed_message_ids_set(
    activated_ids: &[String],
    buffered_chunks: &[OmObservationChunk],
) -> HashSet<String> {
    let mut out = HashSet::new();
    for id in activated_ids {
        if !id.trim().is_empty() {
            out.insert(id.clone());
        }
    }
    for chunk in buffered_chunks {
        for id in &chunk.message_ids {
            if !id.trim().is_empty() {
                out.insert(id.clone());
            }
        }
    }
    out
}

pub(in crate::session::om) fn buffered_observations_text(
    buffered_chunks: &[OmObservationChunk],
) -> String {
    buffered_chunks
        .iter()
        .map(|chunk| chunk.observations.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(in crate::session::om) fn record_with_buffered_observation_context(
    record: &OmRecord,
    buffered_chunks: &[OmObservationChunk],
    active_observations_max_chars: usize,
) -> OmRecord {
    let buffered_text = buffered_observations_text(buffered_chunks);
    let Some(combined) =
        combine_observations_for_buffering(&record.active_observations, &buffered_text)
    else {
        return record.clone();
    };
    let mut out = record.clone();
    out.active_observations =
        truncate_chars(&combined, active_observations_max_chars.saturating_mul(2));
    out
}

pub(in crate::session::om) fn build_observation_chunk(
    record_id: &str,
    selected: &[OmObserverMessageCandidate],
    buffered_chunks: &[OmObservationChunk],
    now: DateTime<Utc>,
    observations_text: &str,
    observation_max_chars: usize,
) -> Option<OmObservationChunk> {
    if selected.is_empty() {
        return None;
    }

    let observation = truncate_chars(
        &normalize_observation_text(observations_text),
        observation_max_chars,
    );
    if observation.trim().is_empty() {
        return None;
    }

    let message_tokens = selected.iter().fold(0u32, |sum, item| {
        sum.saturating_add(estimate_text_tokens(&item.text))
    });
    if message_tokens == 0 {
        return None;
    }

    let message_ids = selected
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let (cycle_anchor_id, last_observed_at) = selected
        .iter()
        .max_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        })
        .map(|item| (item.id.clone(), item.created_at))?;
    let next_seq = buffered_chunks
        .last()
        .map_or(1, |chunk| chunk.seq.saturating_add(1));

    Some(OmObservationChunk {
        id: Uuid::new_v4().to_string(),
        record_id: record_id.to_string(),
        seq: next_seq,
        cycle_id: format!("observer_sync:{cycle_anchor_id}"),
        observations: observation.clone(),
        token_count: estimate_text_tokens(&observation),
        message_tokens,
        message_ids,
        last_observed_at,
        created_at: now,
    })
}

pub(in crate::session::om) fn normalize_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(in crate::session::om) fn normalize_observation_text(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub(in crate::session::om) fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect::<String>()
}

#[cfg(test)]
pub(in crate::session::om) fn parse_env_enabled_default_true(raw: Option<&str>) -> bool {
    scope_binding::parse_env_enabled_default_true(raw)
}
