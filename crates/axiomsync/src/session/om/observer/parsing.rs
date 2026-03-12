use std::collections::HashSet;

use crate::om::{OM_PROMPT_CONTRACT_NAME, OM_PROMPT_CONTRACT_VERSION, OM_PROTOCOL_VERSION};
use serde_json::Value;

use super::super::{
    OmInferenceFailureKind, OmInferenceUsage, OmObserverResponse, Result, estimate_text_tokens,
    extract_json_fragment, extract_llm_content, om_observer_error,
    parse_memory_section_xml_accuracy_first, parse_u32_value,
};
use super::record::{normalize_observation_text, truncate_chars};

pub(in crate::session::om) fn parse_observer_usage_from_value(value: &Value) -> OmInferenceUsage {
    let Some(object) = value.as_object() else {
        return OmInferenceUsage::default();
    };
    let object = object
        .get("result")
        .and_then(|inner| inner.as_object())
        .unwrap_or(object);
    object
        .get("usage")
        .and_then(|usage| usage.as_object())
        .map(|usage| OmInferenceUsage {
            input_tokens: usage
                .get("input_tokens")
                .or_else(|| usage.get("inputTokens"))
                .and_then(parse_u32_value)
                .unwrap_or(0),
            output_tokens: usage
                .get("output_tokens")
                .or_else(|| usage.get("outputTokens"))
                .and_then(parse_u32_value)
                .unwrap_or(0),
        })
        .unwrap_or_default()
}

pub(in crate::session::om) fn observer_response_object(
    value: &Value,
) -> Option<&serde_json::Map<String, Value>> {
    let object = value.as_object()?;
    Some(
        object
            .get("result")
            .and_then(|inner| inner.as_object())
            .unwrap_or(object),
    )
}

pub(in crate::session::om) fn parse_observer_observations_text(
    object: &serde_json::Map<String, Value>,
    observation_max_chars: usize,
) -> Option<String> {
    let observations_raw = object
        .get("observations")
        .or_else(|| object.get("observation"))
        .or_else(|| object.get("summary"))
        .or_else(|| object.get("text"))
        .or_else(|| object.get("content"))
        .and_then(|value| value.as_str());
    let has_known_schema = observer_known_json_schema(object);
    if !has_known_schema {
        return None;
    }
    let observations = truncate_chars(
        &normalize_observation_text(observations_raw.unwrap_or("")),
        observation_max_chars,
    );
    (!observations.is_empty()).then_some(observations)
}

fn observer_known_json_schema(object: &serde_json::Map<String, Value>) -> bool {
    object.get("observations").is_some()
        || object.get("observation").is_some()
        || object.get("summary").is_some()
        || object.get("text").is_some()
        || object.get("observed_message_ids").is_some()
        || object.get("observedMessageIds").is_some()
        || object.get("observed_message_id").is_some()
        || object.get("observedMessageId").is_some()
        || object.get("observation_token_count").is_some()
        || object.get("observationTokenCount").is_some()
        || object.get("token_count").is_some()
        || object.get("tokenCount").is_some()
        || object.get("usage").is_some()
}

pub(in crate::session::om) fn parse_observed_message_ids(
    object: &serde_json::Map<String, Value>,
    known_ids: &[String],
) -> Vec<String> {
    let known_id_set = known_ids.iter().map(String::as_str).collect::<HashSet<_>>();
    let mut observed_message_ids = object
        .get("observed_message_ids")
        .or_else(|| object.get("observedMessageIds"))
        .or_else(|| object.get("message_ids"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|id| known_id_set.contains(*id))
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if observed_message_ids.is_empty()
        && let Some(id) = object
            .get("observed_message_id")
            .or_else(|| object.get("observedMessageId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| known_id_set.contains(*id))
    {
        observed_message_ids.push(id.to_string());
    }
    if observed_message_ids.is_empty() {
        observed_message_ids = known_ids.to_vec();
    }
    observed_message_ids.sort();
    observed_message_ids.dedup();
    observed_message_ids
}

pub(in crate::session::om) fn parse_optional_non_empty_text(
    object: &serde_json::Map<String, Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter()
        .find_map(|key| object.get(*key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub(in crate::session::om) fn parse_llm_observer_response(
    value: &Value,
    known_ids: &[String],
    observation_max_chars: usize,
) -> Result<OmObserverResponse> {
    validate_observer_contract_header_for_value(value)?;
    if let Some(parsed) = parse_observer_response_value(value, known_ids, observation_max_chars) {
        return Ok(parsed);
    }
    let content = extract_llm_content(value).ok_or_else(|| {
        om_observer_error(
            OmInferenceFailureKind::Schema,
            "response missing content".to_string(),
        )
    })?;
    if let Some(parsed) =
        parse_observer_response_xml_content(&content, known_ids, observation_max_chars)
    {
        require_observer_contract_marker_in_content(&content)?;
        return Ok(parsed);
    }
    let json_fragment = extract_json_fragment(&content).ok_or_else(|| {
        om_observer_error(
            OmInferenceFailureKind::Schema,
            "response does not contain json object/array".to_string(),
        )
    })?;
    let parsed = serde_json::from_str::<Value>(&json_fragment).map_err(|err| {
        om_observer_error(
            OmInferenceFailureKind::Schema,
            format!("content json parse failed: {err}"),
        )
    })?;
    validate_observer_contract_header_for_value(&parsed)?;
    parse_observer_response_value(&parsed, known_ids, observation_max_chars).ok_or_else(|| {
        om_observer_error(
            OmInferenceFailureKind::Schema,
            "response schema is unsupported".to_string(),
        )
    })
}

pub(in crate::session::om) fn require_observer_contract_marker_in_content(
    content: &str,
) -> Result<()> {
    if content_contains_contract_marker(content) {
        return Ok(());
    }
    Err(om_observer_error(
        OmInferenceFailureKind::Schema,
        "observer response content missing contract marker".to_string(),
    ))
}

fn content_contains_contract_marker(content: &str) -> bool {
    content_contains_json_contract_marker(content) || content_contains_xml_contract_marker(content)
}

fn content_contains_json_contract_marker(content: &str) -> bool {
    let Some(json_fragment) = extract_json_fragment(content) else {
        return false;
    };
    let Ok(parsed) = serde_json::from_str::<Value>(&json_fragment) else {
        return false;
    };
    let Some(object) = observer_response_object(&parsed) else {
        return false;
    };
    let Some(header) = object.get("header").and_then(Value::as_object) else {
        return false;
    };
    matches_contract_header(header)
}

fn content_contains_xml_contract_marker(content: &str) -> bool {
    let lowered = content.to_ascii_lowercase();
    xml_tag_value(&lowered, "contract-name")
        .is_some_and(|value| value == OM_PROMPT_CONTRACT_NAME.to_ascii_lowercase())
        && xml_tag_value(&lowered, "contract-version")
            .is_some_and(|value| value == OM_PROMPT_CONTRACT_VERSION.to_ascii_lowercase())
        && xml_tag_value(&lowered, "protocol-version")
            .is_some_and(|value| value == OM_PROTOCOL_VERSION.to_ascii_lowercase())
}

fn xml_tag_value(content: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = content.find(&open)? + open.len();
    let end = content[start..].find(&close)? + start;
    Some(content[start..end].trim().to_string())
}

fn matches_contract_header(header: &serde_json::Map<String, Value>) -> bool {
    header
        .get("contract_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .is_some_and(|value| value == OM_PROMPT_CONTRACT_NAME)
        && header
            .get("contract_version")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|value| value == OM_PROMPT_CONTRACT_VERSION)
        && header
            .get("protocol_version")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|value| value == OM_PROTOCOL_VERSION)
}

pub(in crate::session::om) fn validate_observer_contract_header_for_value(
    value: &Value,
) -> Result<()> {
    if let Some(object) = observer_response_object(value) {
        validate_observer_contract_header(object, observer_known_json_schema(object))?;
    }
    Ok(())
}

pub(in crate::session::om) fn parse_observer_response_value(
    value: &Value,
    known_ids: &[String],
    observation_max_chars: usize,
) -> Option<OmObserverResponse> {
    let object = observer_response_object(value)?;
    let observations = parse_observer_observations_text(object, observation_max_chars)?;
    let observed_message_ids = parse_observed_message_ids(object, known_ids);

    let usage = object
        .get("usage")
        .and_then(Value::as_object)
        .map(|usage| OmInferenceUsage {
            input_tokens: usage
                .get("input_tokens")
                .or_else(|| usage.get("inputTokens"))
                .and_then(parse_u32_value)
                .unwrap_or(0),
            output_tokens: usage
                .get("output_tokens")
                .or_else(|| usage.get("outputTokens"))
                .and_then(parse_u32_value)
                .unwrap_or(0),
        })
        .unwrap_or_default();
    let observation_token_count = object
        .get("observation_token_count")
        .or_else(|| object.get("observationTokenCount"))
        .or_else(|| object.get("token_count"))
        .or_else(|| object.get("tokenCount"))
        .and_then(parse_u32_value)
        .unwrap_or_else(|| estimate_text_tokens(&observations));

    Some(OmObserverResponse {
        observations,
        observation_token_count,
        observed_message_ids,
        current_task: parse_optional_non_empty_text(object, &["current_task", "currentTask"]),
        suggested_response: parse_optional_non_empty_text(
            object,
            &[
                "suggested_response",
                "suggestedResponse",
                "suggested_continuation",
                "suggestedContinuation",
            ],
        ),
        usage,
    })
}

fn validate_observer_contract_header(
    object: &serde_json::Map<String, Value>,
    require_header: bool,
) -> Result<()> {
    let Some(header) = object.get("header").and_then(Value::as_object) else {
        if require_header {
            return Err(om_observer_error(
                OmInferenceFailureKind::Schema,
                "observer response missing contract header".to_string(),
            ));
        }
        return Ok(());
    };
    let contract_name = header
        .get("contract_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            om_observer_error(
                OmInferenceFailureKind::Schema,
                "observer response header missing contract_name".to_string(),
            )
        })?;
    if contract_name != OM_PROMPT_CONTRACT_NAME {
        return Err(om_observer_error(
            OmInferenceFailureKind::Schema,
            format!(
                "observer response contract_name mismatch: expected {OM_PROMPT_CONTRACT_NAME}, got {contract_name}"
            ),
        ));
    }
    let contract_version = header
        .get("contract_version")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            om_observer_error(
                OmInferenceFailureKind::Schema,
                "observer response header missing contract_version".to_string(),
            )
        })?;
    if contract_version != OM_PROMPT_CONTRACT_VERSION {
        return Err(om_observer_error(
            OmInferenceFailureKind::Schema,
            format!(
                "observer response contract_version mismatch: expected {OM_PROMPT_CONTRACT_VERSION}, got {contract_version}"
            ),
        ));
    }
    let protocol_version = header
        .get("protocol_version")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            om_observer_error(
                OmInferenceFailureKind::Schema,
                "observer response header missing protocol_version".to_string(),
            )
        })?;
    if protocol_version != OM_PROTOCOL_VERSION {
        return Err(om_observer_error(
            OmInferenceFailureKind::Schema,
            format!(
                "observer response protocol_version mismatch: expected {OM_PROTOCOL_VERSION}, got {protocol_version}"
            ),
        ));
    }
    Ok(())
}

pub(in crate::session::om) fn parse_observer_response_xml_content(
    content: &str,
    known_ids: &[String],
    observation_max_chars: usize,
) -> Option<OmObserverResponse> {
    let parsed = parse_memory_section_xml_accuracy_first(content);
    if parsed.observations.trim().is_empty() {
        return None;
    }
    let observations = truncate_chars(
        &normalize_observation_text(&parsed.observations),
        observation_max_chars,
    );
    if observations.is_empty() {
        return None;
    }
    Some(OmObserverResponse {
        observation_token_count: estimate_text_tokens(&observations),
        observations,
        observed_message_ids: known_ids.to_vec(),
        current_task: parsed.current_task,
        suggested_response: parsed.suggested_response,
        usage: OmInferenceUsage::default(),
    })
}
