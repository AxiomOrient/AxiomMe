use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::super::{
    MultiThreadObserverRunContext, ObserverThreadStateUpdate, OmInferenceUsage, OmObserverConfig,
    OmObserverMessageCandidate, OmObserverMode, OmObserverResponse, OmPendingMessage, OmRecord,
    OmScope, ResolvedObserverOutput, Result, resolve_canonical_thread_id,
    select_observed_message_candidates, split_pending_and_other_conversation_candidates,
};
use super::llm::{
    build_observer_client, build_observer_endpoint, build_observer_llm_request,
    run_single_thread_observer_response, select_messages_for_observer_llm,
};
use super::record::normalize_text;
use super::threading::{
    build_observer_thread_messages_for_scope, resolve_observer_thread_group_id,
    run_multi_thread_observer_response,
};

pub(in crate::session::om) fn merge_observe_after_cursor(
    record_last_observed_at: Option<DateTime<Utc>>,
    observe_cursor_after: Option<DateTime<Utc>>,
) -> Option<DateTime<Utc>> {
    match (record_last_observed_at, observe_cursor_after) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

pub(in crate::session::om) fn collect_last_observed_by_thread(
    scope: OmScope,
    scope_key: &str,
    session_id: &str,
    selected_messages: &[OmObserverMessageCandidate],
) -> BTreeMap<String, DateTime<Utc>> {
    let mut out = BTreeMap::<String, DateTime<Utc>>::new();
    for item in selected_messages {
        let thread_id = resolve_observer_thread_group_id(
            scope,
            scope_key,
            item.source_thread_id.as_deref(),
            item.source_session_id.as_deref(),
            session_id,
        );
        out.entry(thread_id)
            .and_modify(|current| {
                if item.created_at > *current {
                    *current = item.created_at;
                }
            })
            .or_insert(item.created_at);
    }
    out
}

pub(in crate::session::om) fn resolve_observer_response_with_config(
    record: &OmRecord,
    scope_key: &str,
    selected: &[OmObserverMessageCandidate],
    current_session_id: &str,
    max_tokens_per_batch: u32,
    skip_continuation_hints: bool,
    config: &OmObserverConfig,
) -> Result<ResolvedObserverOutput> {
    if !config.model_enabled {
        return Ok(deterministic_observer_output(
            record,
            scope_key,
            selected,
            current_session_id,
            config.text_budget.observation_max_chars,
        ));
    }
    match config.mode {
        OmObserverMode::Deterministic => Ok(deterministic_observer_output(
            record,
            scope_key,
            selected,
            current_session_id,
            config.text_budget.observation_max_chars,
        )),
        OmObserverMode::Llm => llm_observer_response(
            record,
            scope_key,
            selected,
            current_session_id,
            max_tokens_per_batch,
            skip_continuation_hints,
            config,
        ),
        OmObserverMode::Auto => {
            match llm_observer_response(
                record,
                scope_key,
                selected,
                current_session_id,
                max_tokens_per_batch,
                skip_continuation_hints,
                config,
            ) {
                Ok(output) => Ok(output),
                Err(err) => {
                    if config.llm.strict {
                        Err(err)
                    } else {
                        Ok(deterministic_observer_output(
                            record,
                            scope_key,
                            selected,
                            current_session_id,
                            config.text_budget.observation_max_chars,
                        ))
                    }
                }
            }
        }
    }
}

pub(in crate::session::om) fn deterministic_observer_output(
    record: &OmRecord,
    scope_key: &str,
    selected: &[OmObserverMessageCandidate],
    current_session_id: &str,
    observation_max_chars: usize,
) -> ResolvedObserverOutput {
    ResolvedObserverOutput {
        selected_messages: selected.to_vec(),
        response: deterministic_observer_response(record, selected, observation_max_chars),
        thread_states: deterministic_thread_state_updates(
            record,
            scope_key,
            selected,
            current_session_id,
            observation_max_chars,
        ),
    }
}

pub(in crate::session::om) fn deterministic_observer_response(
    record: &OmRecord,
    selected: &[OmObserverMessageCandidate],
    observation_max_chars: usize,
) -> OmObserverResponse {
    let pending_messages = selected
        .iter()
        .map(|item| OmPendingMessage {
            id: item.id.clone(),
            role: normalize_text(&item.role),
            text: normalize_text(&item.text),
            created_at_rfc3339: Some(item.created_at.to_rfc3339()),
        })
        .collect::<Vec<_>>();
    let inferred = crate::om::infer_deterministic_observer_response(
        &record.active_observations,
        &pending_messages,
        observation_max_chars,
    );
    OmObserverResponse {
        observation_token_count: inferred.observation_token_count,
        observations: inferred.observations,
        observed_message_ids: inferred.observed_message_ids,
        current_task: normalize_deterministic_current_task(inferred.current_task),
        suggested_response: inferred.suggested_response,
        usage: OmInferenceUsage::default(),
    }
}

fn normalize_deterministic_current_task(current_task: Option<String>) -> Option<String> {
    current_task.and_then(|task| {
        let trimmed = task.trim();
        if trimmed.is_empty() {
            None
        } else if trimmed.starts_with("Primary:") {
            Some(trimmed.to_string())
        } else {
            Some(format!("Primary: {trimmed}"))
        }
    })
}

fn normalize_deterministic_suggested_response(
    suggested_response: Option<String>,
) -> Option<String> {
    suggested_response.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn deterministic_thread_state_updates(
    record: &OmRecord,
    scope_key: &str,
    selected: &[OmObserverMessageCandidate],
    current_session_id: &str,
    observation_max_chars: usize,
) -> Vec<ObserverThreadStateUpdate> {
    if record.scope == OmScope::Session {
        return Vec::new();
    }
    let mut pending_by_thread = BTreeMap::<String, Vec<OmPendingMessage>>::new();
    for item in selected {
        let thread_id = resolve_observer_thread_group_id(
            record.scope,
            scope_key,
            item.source_thread_id.as_deref(),
            item.source_session_id.as_deref(),
            current_session_id,
        );
        pending_by_thread
            .entry(thread_id)
            .or_default()
            .push(OmPendingMessage {
                id: item.id.clone(),
                role: normalize_text(&item.role),
                text: normalize_text(&item.text),
                created_at_rfc3339: Some(item.created_at.to_rfc3339()),
            });
    }
    let mut out = Vec::<ObserverThreadStateUpdate>::new();
    for (thread_id, pending_messages) in pending_by_thread {
        let inferred = crate::om::infer_deterministic_observer_response(
            &record.active_observations,
            &pending_messages,
            observation_max_chars,
        );
        let current_task = normalize_deterministic_current_task(inferred.current_task);
        let suggested_response =
            normalize_deterministic_suggested_response(inferred.suggested_response);
        if current_task.is_none() && suggested_response.is_none() {
            continue;
        }
        out.push(ObserverThreadStateUpdate {
            thread_id,
            current_task,
            suggested_response,
        });
    }
    out
}

pub(in crate::session::om) fn llm_observer_response(
    record: &OmRecord,
    scope_key: &str,
    selected: &[OmObserverMessageCandidate],
    current_session_id: &str,
    max_tokens_per_batch: u32,
    skip_continuation_hints: bool,
    config: &OmObserverConfig,
) -> Result<ResolvedObserverOutput> {
    if selected.is_empty() {
        return Ok(deterministic_observer_output(
            record,
            scope_key,
            selected,
            current_session_id,
            config.text_budget.observation_max_chars,
        ));
    }

    let endpoint = build_observer_endpoint(config)?;
    let client = build_observer_client(config)?;

    let bounded_selected = select_messages_for_observer_llm(
        selected,
        config.llm.max_chars_per_message,
        config.llm.max_input_tokens,
    );
    if bounded_selected.is_empty() {
        return Ok(deterministic_observer_output(
            record,
            scope_key,
            selected,
            current_session_id,
            config.text_budget.observation_max_chars,
        ));
    }

    let (pending_candidates, other_conversation_candidates) =
        split_pending_and_other_conversation_candidates(
            &bounded_selected,
            Some(current_session_id),
        );
    let request = build_observer_llm_request(
        record,
        scope_key,
        config,
        &pending_candidates,
        &other_conversation_candidates,
    );
    let thread_messages = build_observer_thread_messages_for_scope(
        record.scope,
        &bounded_selected,
        scope_key,
        current_session_id,
    );
    let preferred_thread_id = resolve_canonical_thread_id(
        record.scope,
        scope_key,
        record.thread_id.as_deref(),
        Some(current_session_id),
        current_session_id,
    );
    let multi_thread_context = MultiThreadObserverRunContext {
        request: &request,
        bounded_selected: &bounded_selected,
        thread_messages: &thread_messages,
        scope: record.scope,
        scope_key,
        current_session_id,
        preferred_thread_id: &preferred_thread_id,
        max_tokens_per_batch,
        skip_continuation_hints,
    };
    let (response, thread_states) = if let Some(value) =
        run_multi_thread_observer_response(&client, &endpoint, config, &multi_thread_context)?
    {
        value
    } else {
        run_single_thread_observer_response(
            &client,
            &endpoint,
            config,
            &request,
            &pending_candidates,
            skip_continuation_hints,
        )?
    };
    if response.observations.trim().is_empty() {
        return Ok(deterministic_observer_output(
            record,
            scope_key,
            selected,
            current_session_id,
            config.text_budget.observation_max_chars,
        ));
    }
    let selected_messages =
        select_observed_message_candidates(&bounded_selected, &response.observed_message_ids);
    Ok(ResolvedObserverOutput {
        selected_messages,
        response,
        thread_states,
    })
}
