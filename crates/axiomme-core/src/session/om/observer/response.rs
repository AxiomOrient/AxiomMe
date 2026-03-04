use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::super::{
    MultiThreadObserverRunContext, OmInferenceUsage, OmObserverConfig, OmObserverMessageCandidate,
    OmObserverMode, OmObserverResponse, OmPendingMessage, OmRecord, OmScope,
    ResolvedObserverOutput, Result, estimate_text_tokens, resolve_canonical_thread_id,
    select_observed_message_candidates, split_pending_and_other_conversation_candidates,
    synthesize_observer_observations,
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

const DETERMINISTIC_CONTINUATION_MAX_CHARS: usize = 220;
const DETERMINISTIC_SUGGESTED_RESPONSE_MIN_CONFIDENCE: f32 = 0.78;

#[derive(Debug, Clone, Default)]
struct DeterministicContinuationHints {
    current_task: Option<String>,
    suggested_response: Option<String>,
}

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
            selected,
            config.text_budget.observation_max_chars,
        ));
    }
    match config.mode {
        OmObserverMode::Deterministic => Ok(deterministic_observer_output(
            record,
            selected,
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
                            selected,
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
    selected: &[OmObserverMessageCandidate],
    observation_max_chars: usize,
) -> ResolvedObserverOutput {
    ResolvedObserverOutput {
        selected_messages: selected.to_vec(),
        response: deterministic_observer_response(record, selected, observation_max_chars),
        thread_states: Vec::new(),
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
    let observations = synthesize_observer_observations(
        &record.active_observations,
        &pending_messages,
        observation_max_chars,
    );
    let continuation = infer_deterministic_continuation(&pending_messages);
    OmObserverResponse {
        observation_token_count: estimate_text_tokens(&observations),
        observations,
        observed_message_ids: selected.iter().map(|item| item.id.clone()).collect(),
        current_task: continuation.current_task,
        suggested_response: continuation.suggested_response,
        usage: OmInferenceUsage::default(),
    }
}

fn infer_deterministic_continuation(
    pending_messages: &[OmPendingMessage],
) -> DeterministicContinuationHints {
    let last_user = pending_messages
        .iter()
        .rev()
        .find(|message| role_eq(&message.role, "user"));
    let last_blocking = pending_messages.iter().rev().find(|message| {
        (role_eq(&message.role, "assistant") || role_eq(&message.role, "tool"))
            && contains_error_signal(&message.text)
    });

    let task_candidate = last_user.and_then(|message| infer_task_from_user_message(&message.text));
    let current_task = task_candidate
        .as_ref()
        .map(|candidate| candidate.normalized.as_str())
        .and_then(|value| bounded_hint(value, DETERMINISTIC_CONTINUATION_MAX_CHARS))
        .map(|value| format!("Primary: {value}"));

    let suggested_response = infer_suggested_response(
        task_candidate.as_ref(),
        last_user.map(|message| message.text.as_str()),
        last_blocking.map(|message| message.text.as_str()),
    );

    DeterministicContinuationHints {
        current_task,
        suggested_response,
    }
}

#[derive(Debug, Clone)]
struct TaskCandidate {
    normalized: String,
    confidence: f32,
    has_identifier: bool,
}

fn infer_task_from_user_message(text: &str) -> Option<TaskCandidate> {
    let normalized = normalize_sentence_like(text)?;
    let actionable = contains_action_verb(&normalized);
    let has_identifier = !extract_identifier_tokens(&normalized, 1).is_empty();
    let confidence = task_confidence(actionable, has_identifier, &normalized);
    if confidence < 0.62 {
        return None;
    }
    Some(TaskCandidate {
        normalized,
        confidence,
        has_identifier,
    })
}

fn infer_suggested_response(
    task_candidate: Option<&TaskCandidate>,
    last_user_text: Option<&str>,
    last_blocking_text: Option<&str>,
) -> Option<String> {
    let Some(task) = task_candidate else {
        return None;
    };
    let blocking_identifiers = last_blocking_text
        .map(|text| extract_identifier_tokens(text, 3))
        .unwrap_or_default();
    let user_identifiers = last_user_text
        .map(|text| extract_identifier_tokens(text, 2))
        .unwrap_or_default();
    let has_blocking_signal = last_blocking_text.is_some();
    let has_identifier = !blocking_identifiers.is_empty() || task.has_identifier;
    let confidence =
        suggested_response_confidence(task.confidence, has_blocking_signal, has_identifier);
    if confidence < DETERMINISTIC_SUGGESTED_RESPONSE_MIN_CONFIDENCE {
        return None;
    }

    let response = if has_blocking_signal {
        let detail = if blocking_identifiers.is_empty() {
            "the reported error".to_string()
        } else {
            blocking_identifiers.join(", ")
        };
        format!(
            "Resolve {detail} and continue: {}",
            task.normalized.trim_end_matches('.')
        )
    } else if user_identifiers.is_empty() {
        format!(
            "Proceed with {} and report verification evidence.",
            task.normalized.trim_end_matches('.')
        )
    } else {
        format!(
            "Proceed with {} while preserving {}.",
            task.normalized.trim_end_matches('.'),
            user_identifiers.join(", ")
        )
    };

    bounded_hint(&response, DETERMINISTIC_CONTINUATION_MAX_CHARS)
}

fn role_eq(role: &str, expected: &str) -> bool {
    role.trim().eq_ignore_ascii_case(expected)
}

fn normalize_sentence_like(text: &str) -> Option<String> {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    let candidate = normalized
        .split(['\n', '!', '?', ';'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .find(|line| contains_action_verb(line) || !extract_identifier_tokens(line, 1).is_empty())
        .unwrap_or_else(|| normalized.as_str());
    bounded_hint(candidate, DETERMINISTIC_CONTINUATION_MAX_CHARS)
}

fn contains_action_verb(text: &str) -> bool {
    const ACTION_VERBS: [&str; 30] = [
        "fix",
        "add",
        "update",
        "implement",
        "investigate",
        "debug",
        "refactor",
        "remove",
        "create",
        "write",
        "test",
        "verify",
        "review",
        "analyze",
        "find",
        "search",
        "configure",
        "setup",
        "migrate",
        "optimize",
        "clean",
        "수정",
        "구현",
        "검토",
        "확인",
        "분석",
        "찾",
        "조사",
        "개선",
        "테스트",
    ];
    let lowered = text.to_ascii_lowercase();
    ACTION_VERBS.iter().any(|verb| lowered.contains(verb))
}

fn contains_error_signal(text: &str) -> bool {
    const ERROR_SIGNALS: [&str; 18] = [
        "error",
        "failed",
        "failure",
        "exception",
        "panic",
        "traceback",
        "timeout",
        "denied",
        "invalid",
        "not found",
        "429",
        "500",
        "503",
        "오류",
        "실패",
        "예외",
        "타임아웃",
        "에러",
    ];
    let lowered = text.to_ascii_lowercase();
    ERROR_SIGNALS.iter().any(|signal| lowered.contains(signal))
}

fn task_confidence(actionable: bool, has_identifier: bool, normalized: &str) -> f32 {
    let mut confidence: f32 = 0.48;
    if actionable {
        confidence += 0.23;
    }
    if has_identifier {
        confidence += 0.14;
    }
    let word_count = normalized.split_whitespace().count();
    if (3..=18).contains(&word_count) {
        confidence += 0.08;
    }
    if !normalized.ends_with('?') {
        confidence += 0.07;
    }
    confidence.min(0.95)
}

fn suggested_response_confidence(
    task_confidence: f32,
    has_blocking_signal: bool,
    has_identifier: bool,
) -> f32 {
    let mut confidence: f32 = 0.42 + (task_confidence * 0.4);
    if has_blocking_signal {
        confidence += 0.18;
    }
    if has_identifier {
        confidence += 0.09;
    }
    confidence.min(0.96)
}

fn extract_identifier_tokens(text: &str, max_items: usize) -> Vec<String> {
    let mut out = Vec::<String>::new();
    let mut seen = std::collections::BTreeSet::<String>::new();
    for raw in text.split_whitespace() {
        let candidate = raw.trim_matches(|ch: char| {
            !ch.is_ascii_alphanumeric() && !matches!(ch, '_' | '-' | '.' | '/' | ':')
        });
        if candidate.len() < 3 || candidate.len() > 96 {
            continue;
        }
        if !looks_like_identifier(candidate) {
            continue;
        }
        let dedupe_key = candidate.to_ascii_lowercase();
        if seen.insert(dedupe_key) {
            out.push(candidate.to_string());
        }
        if out.len() >= max_items {
            break;
        }
    }
    out
}

fn looks_like_identifier(token: &str) -> bool {
    if token.contains("::")
        || token.contains('/')
        || token.contains('.')
        || token.contains('_')
        || token.contains('-')
        || token.chars().any(|ch| ch.is_ascii_digit())
    {
        return true;
    }
    let uppercase_count = token.chars().filter(|ch| ch.is_ascii_uppercase()).count();
    let has_lowercase = token.chars().any(|ch| ch.is_ascii_lowercase());
    uppercase_count >= 2 && !has_lowercase
}

fn bounded_hint(text: &str, max_chars: usize) -> Option<String> {
    let normalized = text.trim();
    if normalized.is_empty() {
        return None;
    }
    Some(normalized.chars().take(max_chars).collect::<String>())
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
            selected,
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
            selected,
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
            selected,
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
