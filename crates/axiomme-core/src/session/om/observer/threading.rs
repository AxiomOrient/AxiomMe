use std::collections::{BTreeMap, HashSet};

use reqwest::Url;
use reqwest::blocking::Client;
use serde_json::Value;

use super::super::{
    AxiomError, MultiThreadObserverRunContext, ObserverBatchResult, ObserverBatchTask,
    ObserverThreadStateUpdate, OmInferenceUsage, OmObserverConfig, OmObserverMessageCandidate,
    OmObserverResponse, OmObserverThreadMessages, OmPendingMessage, OmScope, Result,
    aggregate_multi_thread_observer_sections, build_multi_thread_observer_system_prompt,
    build_multi_thread_observer_user_prompt, estimate_text_tokens, extract_llm_content,
    format_observer_messages_for_prompt, parse_multi_thread_observer_output_accuracy_first,
};
use super::llm::send_observer_llm_request;
use super::parsing::{parse_llm_observer_response, parse_observer_usage_from_value};
use super::record::{normalize_observation_text, normalize_text, truncate_chars};

const MAX_OBSERVER_BATCH_PARALLELISM: usize = 4;
const OBSERVER_BATCH_PARALLELISM_ENV: &str = "AXIOMME_OBSERVER_BATCH_PARALLELISM";

pub(in crate::session::om) fn build_observer_thread_messages_for_scope(
    scope: OmScope,
    bounded_selected: &[OmObserverMessageCandidate],
    scope_key: &str,
    current_session_id: &str,
) -> Vec<OmObserverThreadMessages> {
    if scope == OmScope::Session {
        return Vec::new();
    }
    build_observer_thread_messages(bounded_selected, scope, scope_key, current_session_id)
}

pub(in crate::session::om) fn run_multi_thread_observer_response(
    client: &Client,
    endpoint: &Url,
    config: &OmObserverConfig,
    context: &MultiThreadObserverRunContext<'_>,
) -> Result<Option<(OmObserverResponse, Vec<ObserverThreadStateUpdate>)>> {
    if context.scope != OmScope::Resource || context.thread_messages.is_empty() {
        return Ok(None);
    }
    let thread_known_ids = build_observer_thread_known_ids(
        context.bounded_selected,
        context.scope,
        context.scope_key,
        context.current_session_id,
    );
    let thread_batches =
        chunk_observer_thread_batches(context.thread_messages, context.max_tokens_per_batch);
    let batch_tasks = build_observer_batch_tasks(thread_batches, &thread_known_ids);
    let batch_results = run_observer_batch_tasks(
        client,
        endpoint,
        config,
        &context.request.active_observations,
        context.current_session_id,
        context.skip_continuation_hints,
        batch_tasks,
    )?;
    Ok(combine_multi_thread_batch_results(
        batch_results,
        context.bounded_selected,
        context.current_session_id,
        config.text_budget.observation_max_chars,
    ))
}

pub(in crate::session::om) fn combine_multi_thread_batch_results(
    batch_results: Vec<ObserverBatchResult>,
    bounded_selected: &[OmObserverMessageCandidate],
    current_session_id: &str,
    observation_max_chars: usize,
) -> Option<(OmObserverResponse, Vec<ObserverThreadStateUpdate>)> {
    let mut combined_observations = Vec::<String>::new();
    let mut combined_thread_states = Vec::<ObserverThreadStateUpdate>::new();
    let mut observed_id_set = HashSet::<String>::new();
    let mut usage = OmInferenceUsage::default();

    for batch_result in batch_results {
        let batch_response = batch_result.response;
        if !batch_response.observations.trim().is_empty() {
            combined_observations.push(batch_response.observations);
        }
        for observed_id in batch_response.observed_message_ids {
            observed_id_set.insert(observed_id);
        }
        usage.input_tokens = usage
            .input_tokens
            .saturating_add(batch_response.usage.input_tokens);
        usage.output_tokens = usage
            .output_tokens
            .saturating_add(batch_response.usage.output_tokens);
        combined_thread_states.extend(batch_result.thread_states);
    }

    let observations = truncate_chars(
        &normalize_observation_text(&combined_observations.join("\n\n")),
        observation_max_chars,
    );
    if observations.is_empty() {
        return None;
    }

    let observed_message_ids = bounded_selected
        .iter()
        .filter(|item| observed_id_set.contains(item.id.as_str()))
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let current_task = preferred_thread_text(
        &combined_thread_states,
        current_session_id,
        ObserverThreadField::CurrentTask,
    );
    let suggested_response = preferred_thread_text(
        &combined_thread_states,
        current_session_id,
        ObserverThreadField::SuggestedResponse,
    );
    Some((
        OmObserverResponse {
            observation_token_count: estimate_text_tokens(&observations),
            observations,
            observed_message_ids,
            current_task,
            suggested_response,
            usage,
        },
        combined_thread_states,
    ))
}

#[derive(Clone, Copy)]
enum ObserverThreadField {
    CurrentTask,
    SuggestedResponse,
}

fn preferred_thread_text(
    thread_states: &[ObserverThreadStateUpdate],
    preferred_thread_id: &str,
    field: ObserverThreadField,
) -> Option<String> {
    let pick = |state: &ObserverThreadStateUpdate| match field {
        ObserverThreadField::CurrentTask => state.current_task.clone(),
        ObserverThreadField::SuggestedResponse => state.suggested_response.clone(),
    };
    thread_states
        .iter()
        .find(|state| state.thread_id == preferred_thread_id)
        .and_then(pick)
        .or_else(|| thread_states.iter().find_map(pick))
}
pub(in crate::session::om) fn build_observer_batch_tasks(
    thread_batches: Vec<Vec<OmObserverThreadMessages>>,
    known_ids_by_thread: &BTreeMap<String, Vec<String>>,
) -> Vec<ObserverBatchTask> {
    thread_batches
        .into_iter()
        .enumerate()
        .filter_map(|(index, threads)| {
            let known_ids = collect_known_ids_for_thread_batch(&threads, known_ids_by_thread);
            if known_ids.is_empty() {
                None
            } else {
                Some(ObserverBatchTask {
                    index,
                    threads,
                    known_ids,
                })
            }
        })
        .collect::<Vec<_>>()
}

pub(in crate::session::om) fn execute_observer_batch_task(
    client: &Client,
    endpoint: &Url,
    config: &OmObserverConfig,
    active_observations: &str,
    current_session_id: &str,
    skip_continuation_hints: bool,
    task: ObserverBatchTask,
) -> Result<ObserverBatchResult> {
    let ObserverBatchTask {
        index,
        threads,
        known_ids,
    } = task;
    let system_prompt = build_multi_thread_observer_system_prompt();
    let user_prompt = build_multi_thread_observer_user_prompt(
        Some(active_observations),
        &threads,
        skip_continuation_hints,
    );
    let value = send_observer_llm_request(client, endpoint, config, &system_prompt, &user_prompt)?;
    let (response, thread_states) = if let Some(parsed) = parse_llm_multi_thread_observer_response(
        &value,
        current_session_id,
        &known_ids,
        config.text_budget.observation_max_chars,
    ) {
        (parsed.response, parsed.thread_states)
    } else {
        (
            parse_llm_observer_response(
                &value,
                &known_ids,
                config.text_budget.observation_max_chars,
            )?,
            Vec::new(),
        )
    };
    Ok(ObserverBatchResult {
        index,
        response,
        thread_states,
    })
}

pub(in crate::session::om) fn run_observer_batch_tasks(
    client: &Client,
    endpoint: &Url,
    config: &OmObserverConfig,
    active_observations: &str,
    current_session_id: &str,
    skip_continuation_hints: bool,
    tasks: Vec<ObserverBatchTask>,
) -> Result<Vec<ObserverBatchResult>> {
    let parallelism = observer_batch_parallelism(tasks.len());
    if parallelism <= 1 {
        let mut out = Vec::<ObserverBatchResult>::new();
        for task in tasks {
            out.push(execute_observer_batch_task(
                client,
                endpoint,
                config,
                active_observations,
                current_session_id,
                skip_continuation_hints,
                task,
            )?);
        }
        return Ok(out);
    }

    let endpoint = endpoint.clone();
    let mut results = Vec::<ObserverBatchResult>::with_capacity(tasks.len());
    let mut pending = tasks.into_iter();
    loop {
        let batch = pending.by_ref().take(parallelism).collect::<Vec<_>>();
        if batch.is_empty() {
            break;
        }

        let mut batch_results = std::thread::scope(|scope| {
            let handles = batch
                .into_iter()
                .map(|task| {
                    let client = client.clone();
                    let endpoint = endpoint.clone();
                    let config = config.clone();
                    scope.spawn(move || {
                        execute_observer_batch_task(
                            &client,
                            &endpoint,
                            &config,
                            active_observations,
                            current_session_id,
                            skip_continuation_hints,
                            task,
                        )
                    })
                })
                .collect::<Vec<_>>();

            let mut out = Vec::<ObserverBatchResult>::with_capacity(handles.len());
            for handle in handles {
                let joined = handle.join().map_err(|_| {
                    AxiomError::Internal("observer multi-thread batch worker panicked".to_string())
                })?;
                out.push(joined?);
            }
            Ok::<Vec<ObserverBatchResult>, AxiomError>(out)
        })?;
        results.append(&mut batch_results);
    }

    results.sort_by_key(|item| item.index);
    Ok(results)
}

fn observer_batch_parallelism(task_count: usize) -> usize {
    let available_parallelism = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(MAX_OBSERVER_BATCH_PARALLELISM);
    let env_raw = std::env::var(OBSERVER_BATCH_PARALLELISM_ENV).ok();
    let cap = resolve_observer_batch_parallelism_cap(env_raw.as_deref(), available_parallelism);
    task_count.clamp(1, cap)
}

fn resolve_observer_batch_parallelism_cap(
    env_raw: Option<&str>,
    available_parallelism: usize,
) -> usize {
    let default_cap = available_parallelism.clamp(1, MAX_OBSERVER_BATCH_PARALLELISM);
    let Some(raw) = env_raw else {
        return default_cap;
    };
    let Ok(parsed) = raw.trim().parse::<usize>() else {
        return default_cap;
    };
    if parsed == 0 {
        return default_cap;
    }
    parsed.min(MAX_OBSERVER_BATCH_PARALLELISM)
}

pub(in crate::session::om) fn build_observer_thread_known_ids(
    candidates: &[OmObserverMessageCandidate],
    scope: OmScope,
    scope_key: &str,
    fallback_thread_id: &str,
) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::<String, Vec<String>>::new();
    for candidate in candidates {
        let thread_id = resolve_observer_thread_group_id(
            scope,
            scope_key,
            candidate.source_thread_id.as_deref(),
            candidate.source_session_id.as_deref(),
            fallback_thread_id,
        );
        out.entry(thread_id).or_default().push(candidate.id.clone());
    }
    out
}

pub(in crate::session::om) fn chunk_observer_thread_batches(
    threads: &[OmObserverThreadMessages],
    max_tokens_per_batch: u32,
) -> Vec<Vec<OmObserverThreadMessages>> {
    let limit = max_tokens_per_batch.max(1);
    let mut batches = Vec::<Vec<OmObserverThreadMessages>>::new();
    let mut current = Vec::<OmObserverThreadMessages>::new();
    let mut current_tokens = 0u32;

    for thread in threads {
        let thread_tokens = estimate_observer_thread_prompt_tokens(thread);
        if !current.is_empty() && current_tokens.saturating_add(thread_tokens) > limit {
            batches.push(current);
            current = Vec::new();
            current_tokens = 0;
        }
        current_tokens = current_tokens.saturating_add(thread_tokens);
        current.push(thread.clone());
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

pub(in crate::session::om) fn estimate_observer_thread_prompt_tokens(
    thread: &OmObserverThreadMessages,
) -> u32 {
    estimate_text_tokens(&thread.thread_id)
        .saturating_add(estimate_text_tokens(&thread.message_history))
        .saturating_add(16)
}

pub(in crate::session::om) fn collect_known_ids_for_thread_batch(
    batch: &[OmObserverThreadMessages],
    known_ids_by_thread: &BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    batch
        .iter()
        .flat_map(|thread| {
            known_ids_by_thread
                .get(thread.thread_id.as_str())
                .into_iter()
                .flat_map(|ids| ids.iter().cloned())
        })
        .collect::<Vec<_>>()
}

pub(in crate::session::om) fn build_observer_thread_messages(
    candidates: &[OmObserverMessageCandidate],
    scope: OmScope,
    scope_key: &str,
    fallback_thread_id: &str,
) -> Vec<OmObserverThreadMessages> {
    let mut groups = BTreeMap::<String, Vec<&OmObserverMessageCandidate>>::new();
    for candidate in candidates {
        let thread_id = resolve_observer_thread_group_id(
            scope,
            scope_key,
            candidate.source_thread_id.as_deref(),
            candidate.source_session_id.as_deref(),
            fallback_thread_id,
        );
        groups.entry(thread_id).or_default().push(candidate);
    }

    groups
        .into_iter()
        .filter_map(|(thread_id, mut items)| {
            items.sort_by(|a, b| {
                a.created_at
                    .cmp(&b.created_at)
                    .then_with(|| a.id.cmp(&b.id))
            });
            let pending = items
                .into_iter()
                .map(|item| OmPendingMessage {
                    id: item.id.clone(),
                    role: item.role.clone(),
                    text: normalize_text(&item.text),
                    created_at_rfc3339: Some(item.created_at.to_rfc3339()),
                })
                .collect::<Vec<_>>();
            let message_history = format_observer_messages_for_prompt(&pending);
            if message_history.trim().is_empty() {
                None
            } else {
                Some(OmObserverThreadMessages {
                    thread_id,
                    message_history,
                })
            }
        })
        .collect::<Vec<_>>()
}

pub(in crate::session::om) fn resolve_observer_thread_group_id(
    scope: OmScope,
    scope_key: &str,
    source_thread_id: Option<&str>,
    source_session_id: Option<&str>,
    fallback_thread_id: &str,
) -> String {
    match scope {
        OmScope::Thread => scope_key
            .strip_prefix("thread:")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(fallback_thread_id)
            .to_string(),
        OmScope::Resource => source_thread_id
            .or(source_session_id)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(fallback_thread_id)
            .to_string(),
        OmScope::Session => fallback_thread_id.to_string(),
    }
}

#[derive(Debug, Clone)]
pub(in crate::session::om) struct ParsedMultiThreadObserverResponse {
    pub(in crate::session::om) response: OmObserverResponse,
    pub(in crate::session::om) thread_states: Vec<ObserverThreadStateUpdate>,
}

pub(in crate::session::om) fn parse_llm_multi_thread_observer_response(
    value: &Value,
    primary_thread_id: &str,
    known_ids: &[String],
    observation_max_chars: usize,
) -> Option<ParsedMultiThreadObserverResponse> {
    let content = extract_llm_content(value)?;
    let sections = parse_multi_thread_observer_output_accuracy_first(&content);
    if sections.is_empty() {
        return None;
    }
    let thread_states = sections
        .iter()
        .map(|section| ObserverThreadStateUpdate {
            thread_id: section.thread_id.trim().to_string(),
            current_task: section.current_task.clone(),
            suggested_response: section.suggested_response.clone(),
        })
        .filter(|state| !state.thread_id.is_empty())
        .collect::<Vec<_>>();
    let aggregate = aggregate_multi_thread_observer_sections(&sections, Some(primary_thread_id));
    let observations = truncate_chars(
        &normalize_observation_text(&aggregate.observations),
        observation_max_chars,
    );
    if observations.is_empty() {
        return None;
    }

    Some(ParsedMultiThreadObserverResponse {
        response: OmObserverResponse {
            observation_token_count: estimate_text_tokens(&observations),
            observations,
            observed_message_ids: known_ids.to_vec(),
            current_task: aggregate.current_task,
            suggested_response: aggregate.suggested_response,
            usage: parse_observer_usage_from_value(value),
        },
        thread_states,
    })
}

#[cfg(test)]
mod tests {
    use super::{observer_batch_parallelism, resolve_observer_batch_parallelism_cap};

    #[test]
    fn observer_batch_parallelism_is_at_least_one() {
        assert_eq!(observer_batch_parallelism(0), 1);
        assert_eq!(observer_batch_parallelism(1), 1);
    }

    #[test]
    fn resolve_parallelism_cap_defaults_to_available_with_hard_ceiling() {
        assert_eq!(resolve_observer_batch_parallelism_cap(None, 1), 1);
        assert_eq!(resolve_observer_batch_parallelism_cap(None, 2), 2);
        assert_eq!(resolve_observer_batch_parallelism_cap(None, 8), 4);
    }

    #[test]
    fn resolve_parallelism_cap_honors_valid_env_override() {
        assert_eq!(resolve_observer_batch_parallelism_cap(Some("2"), 8), 2);
        assert_eq!(resolve_observer_batch_parallelism_cap(Some("99"), 8), 4);
    }

    #[test]
    fn resolve_parallelism_cap_ignores_invalid_env_values() {
        assert_eq!(resolve_observer_batch_parallelism_cap(Some("0"), 3), 3);
        assert_eq!(resolve_observer_batch_parallelism_cap(Some("abc"), 3), 3);
    }
}
