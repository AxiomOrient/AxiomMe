use reqwest::Url;
use reqwest::blocking::Client;
use serde_json::Value;

use super::super::{
    ObserverThreadStateUpdate, OmInferenceFailureKind, OmInferenceModelConfig, OmObserverConfig,
    OmObserverMessageCandidate, OmObserverPromptInput, OmObserverRequest, OmObserverResponse,
    OmPendingMessage, OmRecord, Result, build_observer_system_prompt, build_observer_user_prompt,
    build_other_conversation_blocks, estimate_text_tokens, format_observer_messages_for_prompt,
    om_observer_error, om_status_kind, parse_local_loopback_endpoint,
};
use super::parsing::parse_llm_observer_response;
use super::record::{normalize_observation_text, normalize_text, truncate_chars};

pub(in crate::session::om) fn build_observer_endpoint(config: &OmObserverConfig) -> Result<Url> {
    parse_local_loopback_endpoint(&config.llm_endpoint, "om observer endpoint", "local host")
        .map_err(|err| {
            om_observer_error(
                OmInferenceFailureKind::Fatal,
                format!("invalid endpoint: {err}"),
            )
        })
}

pub(in crate::session::om) fn build_observer_client(config: &OmObserverConfig) -> Result<Client> {
    Client::builder()
        .timeout(std::time::Duration::from_millis(config.llm_timeout_ms))
        .build()
        .map_err(|err| {
            om_observer_error(
                OmInferenceFailureKind::Fatal,
                format!("client build failed: {err}"),
            )
        })
}

pub(in crate::session::om) fn build_observer_llm_request(
    record: &OmRecord,
    scope_key: &str,
    config: &OmObserverConfig,
    pending_candidates: &[OmObserverMessageCandidate],
    other_conversation_candidates: &[OmObserverMessageCandidate],
) -> OmObserverRequest {
    OmObserverRequest {
        scope: record.scope,
        scope_key: scope_key.to_string(),
        model: OmInferenceModelConfig {
            provider: "local-http".to_string(),
            model: config.llm_model.clone(),
            max_output_tokens: config.llm_max_output_tokens,
            temperature_milli: config.llm_temperature_milli,
        },
        active_observations: truncate_chars(
            &normalize_observation_text(&record.active_observations),
            config.active_observations_max_chars,
        ),
        other_conversations: build_other_conversation_blocks(
            other_conversation_candidates,
            None,
            config.other_conversation_max_part_chars,
        ),
        pending_messages: pending_candidates
            .iter()
            .map(|item| OmPendingMessage {
                id: item.id.clone(),
                role: item.role.clone(),
                text: normalize_text(&item.text),
                created_at_rfc3339: Some(item.created_at.to_rfc3339()),
            })
            .collect::<Vec<_>>(),
    }
}
pub(in crate::session::om) fn run_single_thread_observer_response(
    client: &Client,
    endpoint: &Url,
    config: &OmObserverConfig,
    request: &OmObserverRequest,
    pending_candidates: &[OmObserverMessageCandidate],
    skip_continuation_hints: bool,
) -> Result<(OmObserverResponse, Vec<ObserverThreadStateUpdate>)> {
    let system_prompt = build_observer_system_prompt();
    let message_history = format_observer_messages_for_prompt(&request.pending_messages);
    let user_prompt = build_observer_user_prompt(OmObserverPromptInput {
        request_json: None,
        existing_observations: Some(&request.active_observations),
        message_history: &message_history,
        other_conversation_context: request.other_conversations.as_deref(),
        skip_continuation_hints,
    });
    let known_ids = pending_candidates
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    let value = send_observer_llm_request(client, endpoint, config, &system_prompt, &user_prompt)?;
    Ok((
        parse_llm_observer_response(&value, &known_ids, config.observation_max_chars)?,
        Vec::new(),
    ))
}

pub(in crate::session::om) fn send_observer_llm_request(
    client: &Client,
    endpoint: &Url,
    config: &OmObserverConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<Value> {
    let payload = serde_json::json!({
        "model": config.llm_model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "stream": false,
        "options": {
            "temperature": (f64::from(config.llm_temperature_milli) / 1000.0),
            "num_predict": config.llm_max_output_tokens
        }
    });
    let response = client
        .post(endpoint.clone())
        .json(&payload)
        .send()
        .map_err(|err| {
            om_observer_error(
                OmInferenceFailureKind::Transient,
                format!("request failed: {err}"),
            )
        })?;
    if !response.status().is_success() {
        let status = response.status();
        return Err(om_observer_error(
            om_status_kind(status),
            format!("non-success status: {status}"),
        ));
    }
    response.json::<Value>().map_err(|err| {
        om_observer_error(
            OmInferenceFailureKind::Schema,
            format!("invalid json response: {err}"),
        )
    })
}
pub(in crate::session::om) fn select_messages_for_observer_llm(
    selected: &[OmObserverMessageCandidate],
    max_chars_per_message: usize,
    max_input_tokens: u32,
) -> Vec<OmObserverMessageCandidate> {
    let mut kept = Vec::<OmObserverMessageCandidate>::new();
    let mut total_tokens = 0u32;

    for item in selected.iter().rev() {
        let bounded_text = truncate_chars(&normalize_text(&item.text), max_chars_per_message);
        if bounded_text.is_empty() {
            continue;
        }
        let bounded = OmObserverMessageCandidate {
            id: item.id.clone(),
            role: item.role.clone(),
            text: bounded_text,
            created_at: item.created_at,
            source_thread_id: item.source_thread_id.clone(),
            source_session_id: item.source_session_id.clone(),
        };
        let item_tokens = estimate_text_tokens(&bounded.id)
            .saturating_add(estimate_text_tokens(&bounded.role))
            .saturating_add(estimate_text_tokens(&bounded.text))
            .saturating_add(8);

        if !kept.is_empty() && total_tokens.saturating_add(item_tokens) > max_input_tokens {
            break;
        }

        kept.push(bounded);
        total_tokens = total_tokens
            .saturating_add(item_tokens)
            .min(max_input_tokens);
    }

    kept.into_iter().rev().collect()
}
