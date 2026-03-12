use reqwest::Url;
use reqwest::blocking::Client;
use serde_json::Value;

use crate::config::OmReflectorConfigSnapshot;
use crate::error::{AxiomError, OmInferenceFailureKind, OmInferenceSource, Result};
use crate::llm_io::{
    estimate_text_tokens, extract_json_fragment, extract_llm_content,
    parse_local_loopback_endpoint, parse_u32_value,
};
use crate::om::{
    DEFAULT_REFLECTOR_BUFFER_ACTIVATION, DEFAULT_REFLECTOR_OBSERVATION_TOKENS,
    OM_PROMPT_CONTRACT_NAME, OM_PROMPT_CONTRACT_VERSION, OM_PROTOCOL_VERSION,
    OmInferenceModelConfig, OmInferenceUsage, OmReflectorPromptInput, OmReflectorRequest,
    OmReflectorResponse, build_reflection_draft, build_reflector_prompt_contract_v2,
    build_reflector_system_prompt, build_reflector_user_prompt, om_observer_error,
    om_reflector_error, om_status_kind, parse_memory_section_xml_accuracy_first,
    resolve_reflector_model_enabled, validate_reflection_compression,
};
use crate::om_bridge::{
    OM_OUTBOX_SCHEMA_VERSION_V1, OmObserveBufferRequestedV1, OmReflectBufferRequestedV1,
    OmReflectRequestedV1,
};
use crate::state::OmActiveEntry;
use crate::uri::{AxiomUri, Scope};

const DEFAULT_OM_REFLECTOR_MODE: &str = "auto";
const DEFAULT_OM_REFLECTOR_LLM_ENDPOINT: &str = "http://127.0.0.1:11434/api/chat";
const DEFAULT_OM_REFLECTOR_LLM_MODEL: &str = "qwen2.5:7b-instruct";
const DEFAULT_OM_REFLECTOR_LLM_TIMEOUT_MS: u64 = 2_000;
const DEFAULT_OM_REFLECTOR_LLM_MAX_OUTPUT_TOKENS: u32 = 1_200;
const DEFAULT_OM_REFLECTOR_LLM_TEMPERATURE_MILLI: u16 = 0;
#[cfg(test)]
const DEFAULT_OM_REFLECTOR_MAX_CHARS: usize = 1_200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OmReflectorMode {
    Auto,
    Deterministic,
    Llm,
}

impl OmReflectorMode {
    fn parse(raw: Option<&str>) -> Self {
        match raw
            .unwrap_or(DEFAULT_OM_REFLECTOR_MODE)
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "deterministic" | "local" | "draft" => Self::Deterministic,
            "llm" | "model" => Self::Llm,
            _ => Self::Auto,
        }
    }
}

#[derive(Debug, Clone)]
struct OmReflectorConfig {
    mode: OmReflectorMode,
    model_enabled: bool,
    llm_endpoint: String,
    llm_model: String,
    llm_timeout_ms: u64,
    llm_max_output_tokens: u32,
    llm_temperature_milli: u16,
    llm_strict: bool,
    llm_target_observation_tokens: u32,
    llm_buffer_activation: f32,
    max_chars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OmReflectorCallOptions {
    skip_continuation_hints: bool,
    compression_start_level: u8,
}

impl OmReflectorCallOptions {
    pub(super) const DEFAULT: Self = Self {
        skip_continuation_hints: false,
        compression_start_level: 0,
    };

    pub(super) const BUFFERED: Self = Self {
        skip_continuation_hints: true,
        compression_start_level: 1,
    };

    fn first_level(self) -> u8 {
        self.compression_start_level.min(2)
    }
}

impl OmReflectorConfig {
    fn from_snapshot(snapshot: &OmReflectorConfigSnapshot) -> Self {
        Self {
            mode: OmReflectorMode::parse(snapshot.mode.as_deref()),
            model_enabled: resolve_reflector_model_enabled(
                snapshot.explicit_model_enabled,
                snapshot.rollout_profile.as_deref(),
            ),
            llm_endpoint: snapshot
                .llm_endpoint
                .clone()
                .unwrap_or_else(|| DEFAULT_OM_REFLECTOR_LLM_ENDPOINT.to_string()),
            llm_model: snapshot
                .llm_model
                .clone()
                .unwrap_or_else(|| DEFAULT_OM_REFLECTOR_LLM_MODEL.to_string()),
            llm_timeout_ms: snapshot
                .llm_timeout_ms
                .unwrap_or(DEFAULT_OM_REFLECTOR_LLM_TIMEOUT_MS),
            llm_max_output_tokens: snapshot
                .llm_max_output_tokens
                .unwrap_or(DEFAULT_OM_REFLECTOR_LLM_MAX_OUTPUT_TOKENS),
            llm_temperature_milli: snapshot
                .llm_temperature_milli
                .unwrap_or(DEFAULT_OM_REFLECTOR_LLM_TEMPERATURE_MILLI),
            llm_strict: snapshot.llm_strict,
            llm_target_observation_tokens: snapshot
                .llm_target_observation_tokens
                .unwrap_or(DEFAULT_REFLECTOR_OBSERVATION_TOKENS),
            llm_buffer_activation: snapshot
                .llm_buffer_activation
                .unwrap_or(DEFAULT_REFLECTOR_BUFFER_ACTIVATION),
            max_chars: snapshot.max_chars,
        }
    }
}
pub(super) fn resolve_reflector_response(
    record: &crate::om::OmRecord,
    scope_key: &str,
    expected_generation: u32,
    options: OmReflectorCallOptions,
    config_snapshot: &OmReflectorConfigSnapshot,
    active_entries: &[OmActiveEntry],
) -> Result<OmReflectorResponse> {
    let config = OmReflectorConfig::from_snapshot(config_snapshot);
    resolve_reflector_response_with_config(
        record,
        scope_key,
        expected_generation,
        options,
        &config,
        active_entries,
    )
}

pub(super) fn buffered_or_resolved_reflector_response(
    record: &crate::om::OmRecord,
    scope_key: &str,
    expected_generation: u32,
    options: OmReflectorCallOptions,
    config_snapshot: &OmReflectorConfigSnapshot,
    active_entries: &[OmActiveEntry],
) -> Result<OmReflectorResponse> {
    if let Some(buffered) = record
        .buffered_reflection
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(OmReflectorResponse {
            reflection: buffered.to_string(),
            reflection_token_count: record
                .buffered_reflection_tokens
                .unwrap_or_else(|| estimate_text_tokens(buffered)),
            usage: OmInferenceUsage {
                input_tokens: record.buffered_reflection_input_tokens.unwrap_or(0),
                output_tokens: record.buffered_reflection_tokens.unwrap_or(0),
            },
            current_task: None,
            suggested_response: None,
        });
    }
    resolve_reflector_response(
        record,
        scope_key,
        expected_generation,
        options,
        config_snapshot,
        active_entries,
    )
}

fn resolve_reflector_response_with_config(
    record: &crate::om::OmRecord,
    scope_key: &str,
    expected_generation: u32,
    options: OmReflectorCallOptions,
    config: &OmReflectorConfig,
    active_entries: &[OmActiveEntry],
) -> Result<OmReflectorResponse> {
    let deterministic =
        deterministic_reflector_response(&record.active_observations, config.max_chars);
    if !config.model_enabled {
        return Ok(deterministic);
    }
    match config.mode {
        OmReflectorMode::Deterministic => Ok(deterministic),
        OmReflectorMode::Llm => llm_reflector_response(
            record,
            scope_key,
            expected_generation,
            options,
            config,
            active_entries,
        ),
        OmReflectorMode::Auto => {
            match llm_reflector_response(
                record,
                scope_key,
                expected_generation,
                options,
                config,
                active_entries,
            ) {
                Ok(response) => Ok(response),
                Err(err) => {
                    if config.llm_strict {
                        Err(err)
                    } else {
                        Ok(deterministic)
                    }
                }
            }
        }
    }
}

fn deterministic_reflector_response(
    active_observations: &str,
    max_chars: usize,
) -> OmReflectorResponse {
    let draft = build_reflection_draft(active_observations, max_chars);
    OmReflectorResponse {
        reflection: draft
            .as_ref()
            .map(|value| value.reflection.clone())
            .unwrap_or_default(),
        reflection_token_count: draft
            .as_ref()
            .map_or(0, |value| value.reflection_token_count),
        current_task: None,
        suggested_response: None,
        usage: OmInferenceUsage::default(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OmReflectorAttemptInput {
    active_observations: String,
    target_threshold_tokens: u32,
    reflection_input_tokens_override: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BufferedReflectionSelection {
    selected_entry_ids: Vec<String>,
    sliced_observations: String,
    slice_token_estimate: u32,
    compression_target_tokens: u32,
}

fn prepare_reflector_attempt_input(
    record: &crate::om::OmRecord,
    options: OmReflectorCallOptions,
    config: &OmReflectorConfig,
    active_entries: &[OmActiveEntry],
) -> OmReflectorAttemptInput {
    if options == OmReflectorCallOptions::BUFFERED {
        let selection = select_buffered_reflection_entries(
            active_entries,
            record.observation_token_count,
            config.llm_target_observation_tokens,
            config.llm_buffer_activation,
        );
        return OmReflectorAttemptInput {
            active_observations: selection.sliced_observations,
            target_threshold_tokens: selection.compression_target_tokens,
            reflection_input_tokens_override: Some(selection.slice_token_estimate),
        };
    }

    OmReflectorAttemptInput {
        active_observations: record.active_observations.clone(),
        target_threshold_tokens: config.llm_target_observation_tokens,
        reflection_input_tokens_override: None,
    }
}

fn llm_reflector_response(
    record: &crate::om::OmRecord,
    scope_key: &str,
    expected_generation: u32,
    options: OmReflectorCallOptions,
    config: &OmReflectorConfig,
    active_entries: &[OmActiveEntry],
) -> Result<OmReflectorResponse> {
    if record.active_observations.trim().is_empty() {
        return Ok(deterministic_reflector_response(
            &record.active_observations,
            config.max_chars,
        ));
    }

    let endpoint =
        parse_local_loopback_endpoint(&config.llm_endpoint, "om reflector endpoint", "local host")
            .map_err(|err| {
                om_reflector_error(
                    OmInferenceFailureKind::Fatal,
                    format!("invalid endpoint: {err}"),
                )
            })?;
    let client = Client::builder()
        .timeout(std::time::Duration::from_millis(config.llm_timeout_ms))
        .build()
        .map_err(|err| {
            om_reflector_error(
                OmInferenceFailureKind::Fatal,
                format!("client build failed: {err}"),
            )
        })?;

    let request = OmReflectorRequest {
        scope: record.scope,
        scope_key: scope_key.to_string(),
        model: OmInferenceModelConfig {
            provider: "local-http".to_string(),
            model: config.llm_model.clone(),
            max_output_tokens: config.llm_max_output_tokens,
            temperature_milli: config.llm_temperature_milli,
        },
        generation_count: expected_generation,
        active_observations: String::new(),
    };
    let attempt_input = prepare_reflector_attempt_input(record, options, config, active_entries);
    let request = OmReflectorRequest {
        active_observations: attempt_input.active_observations.clone(),
        ..request
    };
    let first_level = options.first_level();
    let retry_level = first_level.saturating_add(1).min(2);

    let mut total_usage = OmInferenceUsage::default();
    let mut parsed = request_llm_reflector_attempt(
        &client,
        &endpoint,
        config,
        &request,
        first_level,
        options.skip_continuation_hints,
    )?;
    accumulate_usage(&mut total_usage, &parsed.usage);

    let reflected_tokens = parsed
        .reflection_token_count
        .max(estimate_text_tokens(&parsed.reflection));
    if !validate_reflection_compression(reflected_tokens, attempt_input.target_threshold_tokens) {
        let retry = request_llm_reflector_attempt(
            &client,
            &endpoint,
            config,
            &request,
            retry_level,
            options.skip_continuation_hints,
        )?;
        accumulate_usage(&mut total_usage, &retry.usage);
        parsed = retry;
    }

    if total_usage != OmInferenceUsage::default() {
        parsed.usage = total_usage;
    }
    if let Some(input_tokens) = attempt_input.reflection_input_tokens_override {
        parsed.usage.input_tokens = input_tokens;
    }
    Ok(parsed)
}

fn request_llm_reflector_attempt(
    client: &Client,
    endpoint: &Url,
    config: &OmReflectorConfig,
    request: &OmReflectorRequest,
    compression_level: u8,
    skip_continuation_hints: bool,
) -> Result<OmReflectorResponse> {
    let system_prompt = build_reflector_system_prompt();
    let request_json = reflector_prompt_contract_json(
        request,
        compression_level,
        skip_continuation_hints,
        config.max_chars,
    )?;
    let user_prompt = build_reflector_user_prompt(OmReflectorPromptInput {
        observations: &request.active_observations,
        request_json: Some(request_json.as_str()),
        manual_prompt: None,
        compression_level,
        skip_continuation_hints,
    });
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
            om_reflector_error(
                OmInferenceFailureKind::Transient,
                format!("request failed: {err}"),
            )
        })?;
    if !response.status().is_success() {
        let status = response.status();
        return Err(om_reflector_error(
            om_status_kind(status),
            format!("non-success status: {status}"),
        ));
    }

    let value = response.json::<Value>().map_err(|err| {
        om_reflector_error(
            OmInferenceFailureKind::Schema,
            format!("invalid json response: {err}"),
        )
    })?;
    parse_llm_reflector_response(&value, &request.active_observations, config.max_chars)
}

fn reflector_prompt_contract_json(
    request: &OmReflectorRequest,
    compression_level: u8,
    skip_continuation_hints: bool,
    reflection_max_chars: usize,
) -> Result<String> {
    let request_contract = build_reflector_prompt_contract_v2(
        request,
        compression_level,
        skip_continuation_hints,
        reflection_max_chars,
    );
    serde_json::to_string_pretty(&request_contract).map_err(|err| {
        om_reflector_error(
            OmInferenceFailureKind::Schema,
            format!("failed to encode reflector prompt contract json: {err}"),
        )
    })
}

const fn accumulate_usage(total: &mut OmInferenceUsage, usage: &OmInferenceUsage) {
    total.input_tokens = total.input_tokens.saturating_add(usage.input_tokens);
    total.output_tokens = total.output_tokens.saturating_add(usage.output_tokens);
}

fn parse_llm_reflector_response(
    value: &Value,
    active_observations: &str,
    max_chars: usize,
) -> Result<OmReflectorResponse> {
    validate_reflector_contract_header_for_value(value)?;
    if let Some(parsed) = parse_reflector_response_value(value, active_observations, max_chars) {
        return Ok(parsed);
    }
    let content = extract_llm_content(value).ok_or_else(|| {
        om_reflector_error(
            OmInferenceFailureKind::Schema,
            "response missing content".to_string(),
        )
    })?;
    if let Some(parsed) =
        parse_reflector_response_xml_content(&content, active_observations, max_chars)
    {
        require_reflector_contract_marker_in_content(&content)?;
        return Ok(parsed);
    }
    if let Some(json_fragment) = extract_json_fragment(&content)
        && let Ok(parsed_json) = serde_json::from_str::<Value>(&json_fragment)
    {
        validate_reflector_contract_header_for_value(&parsed_json)?;
        if let Some(parsed) =
            parse_reflector_response_value(&parsed_json, active_observations, max_chars)
        {
            return Ok(parsed);
        }
    }
    if let Some(parsed) =
        parse_reflector_response_text_content(&content, active_observations, max_chars)
    {
        require_reflector_contract_marker_in_content(&content)?;
        return Ok(parsed);
    }
    Err(om_reflector_error(
        OmInferenceFailureKind::Schema,
        "response schema is unsupported".to_string(),
    ))
}

fn require_reflector_contract_marker_in_content(content: &str) -> Result<()> {
    if content_contains_contract_marker(content) {
        return Ok(());
    }
    Err(om_reflector_error(
        OmInferenceFailureKind::Schema,
        "reflector response content missing contract marker".to_string(),
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
    let Some(object) = reflector_response_object(&parsed) else {
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

fn validate_reflector_contract_header_for_value(value: &Value) -> Result<()> {
    let Some(object) = reflector_response_object(value) else {
        return Ok(());
    };
    validate_reflector_contract_header(object, reflector_known_json_schema(object))
}

fn reflector_response_object(value: &Value) -> Option<&serde_json::Map<String, Value>> {
    let object = value.as_object()?;
    Some(
        object
            .get("result")
            .and_then(|inner| inner.as_object())
            .unwrap_or(object),
    )
}

fn reflector_known_json_schema(object: &serde_json::Map<String, Value>) -> bool {
    object.get("reflection").is_some()
        || object.get("observations").is_some()
        || object.get("summary").is_some()
        || object.get("text").is_some()
        || object.get("content").is_some()
        || object.get("reflected_observation_line_count").is_some()
        || object.get("reflectedObservationLineCount").is_some()
        || object.get("line_count").is_some()
        || object.get("lineCount").is_some()
        || object.get("reflection_token_count").is_some()
        || object.get("reflectionTokenCount").is_some()
        || object.get("token_count").is_some()
        || object.get("tokenCount").is_some()
        || object.get("usage").is_some()
        || object.get("current_task").is_some()
        || object.get("currentTask").is_some()
        || object.get("suggested_response").is_some()
        || object.get("suggestedResponse").is_some()
        || object.get("suggested_continuation").is_some()
        || object.get("suggestedContinuation").is_some()
}

fn validate_reflector_contract_header(
    object: &serde_json::Map<String, Value>,
    require_header: bool,
) -> Result<()> {
    let Some(header) = object.get("header").and_then(Value::as_object) else {
        if require_header {
            return Err(om_reflector_error(
                OmInferenceFailureKind::Schema,
                "reflector response missing contract header".to_string(),
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
            om_reflector_error(
                OmInferenceFailureKind::Schema,
                "reflector response header missing contract_name".to_string(),
            )
        })?;
    if contract_name != OM_PROMPT_CONTRACT_NAME {
        return Err(om_reflector_error(
            OmInferenceFailureKind::Schema,
            format!(
                "reflector response contract_name mismatch: expected {OM_PROMPT_CONTRACT_NAME}, got {contract_name}"
            ),
        ));
    }
    let contract_version = header
        .get("contract_version")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            om_reflector_error(
                OmInferenceFailureKind::Schema,
                "reflector response header missing contract_version".to_string(),
            )
        })?;
    if contract_version != OM_PROMPT_CONTRACT_VERSION {
        return Err(om_reflector_error(
            OmInferenceFailureKind::Schema,
            format!(
                "reflector response contract_version mismatch: expected {OM_PROMPT_CONTRACT_VERSION}, got {contract_version}"
            ),
        ));
    }
    let protocol_version = header
        .get("protocol_version")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            om_reflector_error(
                OmInferenceFailureKind::Schema,
                "reflector response header missing protocol_version".to_string(),
            )
        })?;
    if protocol_version != OM_PROTOCOL_VERSION {
        return Err(om_reflector_error(
            OmInferenceFailureKind::Schema,
            format!(
                "reflector response protocol_version mismatch: expected {OM_PROTOCOL_VERSION}, got {protocol_version}"
            ),
        ));
    }
    Ok(())
}

fn parse_reflector_response_value(
    value: &Value,
    _active_observations: &str,
    max_chars: usize,
) -> Option<OmReflectorResponse> {
    let object = reflector_response_object(value)?;

    let reflection_raw = object
        .get("reflection")
        .or_else(|| object.get("observations"))
        .or_else(|| object.get("summary"))
        .or_else(|| object.get("text"))
        .or_else(|| object.get("content"))
        .and_then(|value| value.as_str());
    let has_known_schema = reflector_known_json_schema(object);
    if !has_known_schema {
        return None;
    }
    let reflection = normalize_reflection_text(reflection_raw.unwrap_or(""), max_chars);
    let current_task = object
        .get("current_task")
        .or_else(|| object.get("currentTask"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let suggested_response = object
        .get("suggested_response")
        .or_else(|| object.get("suggestedResponse"))
        .or_else(|| object.get("suggested_continuation"))
        .or_else(|| object.get("suggestedContinuation"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    if reflection.is_empty() && current_task.is_none() && suggested_response.is_none() {
        return None;
    }

    let usage = object
        .get("usage")
        .and_then(|value| value.as_object())
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
    let reflection_token_count = object
        .get("reflection_token_count")
        .or_else(|| object.get("reflectionTokenCount"))
        .or_else(|| object.get("token_count"))
        .or_else(|| object.get("tokenCount"))
        .and_then(parse_u32_value)
        .unwrap_or_else(|| estimate_text_tokens(&reflection));

    Some(OmReflectorResponse {
        reflection,
        reflection_token_count,
        current_task,
        suggested_response,
        usage,
    })
}

fn parse_reflector_response_xml_content(
    content: &str,
    _active_observations: &str,
    max_chars: usize,
) -> Option<OmReflectorResponse> {
    let parsed = parse_memory_section_xml_accuracy_first(content);
    if parsed.observations.trim().is_empty() {
        return None;
    }
    let observations = parsed.observations;
    let reflection = normalize_reflection_text(&observations, max_chars);
    if reflection.is_empty() {
        return None;
    }
    Some(OmReflectorResponse {
        reflection_token_count: estimate_text_tokens(&reflection),
        reflection,
        current_task: parsed.current_task,
        suggested_response: parsed.suggested_response,
        usage: OmInferenceUsage::default(),
    })
}

fn parse_reflector_response_text_content(
    content: &str,
    _active_observations: &str,
    max_chars: usize,
) -> Option<OmReflectorResponse> {
    let reflection = normalize_reflection_text(&strip_contract_marker_lines(content), max_chars);
    if reflection.is_empty() {
        return None;
    }
    let parsed = parse_memory_section_xml_accuracy_first(content);
    Some(OmReflectorResponse {
        reflection_token_count: estimate_text_tokens(&reflection),
        reflection,
        current_task: parsed.current_task,
        suggested_response: parsed.suggested_response,
        usage: OmInferenceUsage::default(),
    })
}

fn strip_contract_marker_lines(content: &str) -> String {
    content
        .lines()
        .filter(|line| {
            let lowered = line.to_ascii_lowercase();
            !(lowered.contains("contract_name")
                || lowered.contains("contract_version")
                || lowered.contains("protocol_version")
                || lowered.contains("<contract-name>")
                || lowered.contains("</contract-name>")
                || lowered.contains("<contract-version>")
                || lowered.contains("</contract-version>")
                || lowered.contains("<protocol-version>")
                || lowered.contains("</protocol-version>"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn resolve_reflection_cover_entry_ids(
    record: &crate::om::OmRecord,
    options: OmReflectorCallOptions,
    config_snapshot: &OmReflectorConfigSnapshot,
    active_entries: &[OmActiveEntry],
) -> Vec<String> {
    if active_entries.is_empty() {
        return Vec::new();
    }

    let has_buffered_reflection = record
        .buffered_reflection
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    let effective_options = if has_buffered_reflection {
        OmReflectorCallOptions::BUFFERED
    } else {
        options
    };
    if effective_options == OmReflectorCallOptions::DEFAULT {
        return sorted_active_entries(active_entries)
            .into_iter()
            .map(|entry| entry.entry_id)
            .collect();
    }

    let config = OmReflectorConfig::from_snapshot(config_snapshot);
    select_buffered_reflection_entries(
        active_entries,
        record.observation_token_count,
        config.llm_target_observation_tokens,
        config.llm_buffer_activation,
    )
    .selected_entry_ids
}

pub(super) fn parse_om_reflect_buffer_requested_payload(
    payload: &serde_json::Value,
) -> Result<OmReflectBufferRequestedV1> {
    let parsed =
        serde_json::from_value::<OmReflectBufferRequestedV1>(payload.clone()).map_err(|err| {
            om_reflector_error(
                OmInferenceFailureKind::Schema,
                format!("om_reflect_buffer_requested invalid payload schema: {err}"),
            )
        })?;
    validate_om_event_payload_common(
        "om_reflect_buffer_requested",
        parsed.schema_version,
        &parsed.scope_key,
        &parsed.requested_at,
        OmInferenceSource::Reflector,
    )?;
    Ok(parsed)
}

pub(super) fn parse_om_observe_buffer_requested_payload(
    payload: &serde_json::Value,
) -> Result<OmObserveBufferRequestedV1> {
    let parsed =
        serde_json::from_value::<OmObserveBufferRequestedV1>(payload.clone()).map_err(|err| {
            om_observer_error(
                OmInferenceFailureKind::Schema,
                format!("om_observe_buffer_requested invalid payload schema: {err}"),
            )
        })?;
    validate_om_event_payload_common(
        "om_observe_buffer_requested",
        parsed.schema_version,
        &parsed.scope_key,
        &parsed.requested_at,
        OmInferenceSource::Observer,
    )?;
    Ok(parsed)
}

pub(super) fn parse_om_reflect_requested_payload(
    payload: &serde_json::Value,
) -> Result<OmReflectRequestedV1> {
    let parsed =
        serde_json::from_value::<OmReflectRequestedV1>(payload.clone()).map_err(|err| {
            om_reflector_error(
                OmInferenceFailureKind::Schema,
                format!("om_reflect_requested invalid payload schema: {err}"),
            )
        })?;
    validate_om_event_payload_common(
        "om_reflect_requested",
        parsed.schema_version,
        &parsed.scope_key,
        &parsed.requested_at,
        OmInferenceSource::Reflector,
    )?;
    Ok(parsed)
}

fn validate_om_event_payload_common(
    event_type: &str,
    schema_version: u8,
    scope_key: &str,
    requested_at: &str,
    source: OmInferenceSource,
) -> Result<()> {
    if schema_version != OM_OUTBOX_SCHEMA_VERSION_V1 {
        let message = format!(
            "{event_type} unsupported schema_version: {schema_version} (expected: {OM_OUTBOX_SCHEMA_VERSION_V1})"
        );
        return Err(match source {
            OmInferenceSource::Observer => {
                om_observer_error(OmInferenceFailureKind::Schema, message)
            }
            OmInferenceSource::Reflector => {
                om_reflector_error(OmInferenceFailureKind::Schema, message)
            }
        });
    }

    if scope_key.trim().is_empty() {
        let message = format!("{event_type} missing scope_key");
        return Err(match source {
            OmInferenceSource::Observer => {
                om_observer_error(OmInferenceFailureKind::Schema, message)
            }
            OmInferenceSource::Reflector => {
                om_reflector_error(OmInferenceFailureKind::Schema, message)
            }
        });
    }

    if requested_at.trim().is_empty() || chrono::DateTime::parse_from_rfc3339(requested_at).is_err()
    {
        let message = format!("{event_type} invalid requested_at");
        return Err(match source {
            OmInferenceSource::Observer => {
                om_observer_error(OmInferenceFailureKind::Schema, message)
            }
            OmInferenceSource::Reflector => {
                om_reflector_error(OmInferenceFailureKind::Schema, message)
            }
        });
    }
    Ok(())
}

pub(super) fn parse_observe_session_id(
    session_id_payload: Option<&str>,
    uri: &str,
) -> Result<String> {
    if let Some(session_id) = session_id_payload
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(session_id.to_string());
    }

    let parsed_uri = AxiomUri::parse(uri)?;
    if parsed_uri.scope() != Scope::Session {
        return Err(AxiomError::Validation(format!(
            "om_observe_buffer_requested uri must use session scope, got: {uri}"
        )));
    }
    let session_id = parsed_uri
        .segments()
        .first()
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AxiomError::Validation(
                "om_observe_buffer_requested missing session_id in payload and uri".to_string(),
            )
        })?;
    Ok(session_id.to_string())
}

fn sorted_active_entries(active_entries: &[OmActiveEntry]) -> Vec<OmActiveEntry> {
    let mut ordered_entries = active_entries.to_vec();
    ordered_entries.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.entry_id.cmp(&right.entry_id))
    });
    ordered_entries
}

fn select_buffered_reflection_entries(
    active_entries: &[OmActiveEntry],
    observation_token_count: u32,
    reflection_threshold: u32,
    buffer_activation: f32,
) -> BufferedReflectionSelection {
    let target_tokens = (f64::from(reflection_threshold) * f64::from(buffer_activation))
        .floor()
        .clamp(0.0, f64::from(u32::MAX)) as u32;
    if target_tokens == 0 {
        return BufferedReflectionSelection {
            selected_entry_ids: Vec::new(),
            sliced_observations: String::new(),
            slice_token_estimate: 0,
            compression_target_tokens: 0,
        };
    }

    let ordered_entries = sorted_active_entries(active_entries);
    let total_chars = ordered_entries
        .iter()
        .map(|entry| entry.text.trim().chars().count())
        .sum::<usize>();
    if total_chars == 0 {
        return BufferedReflectionSelection {
            selected_entry_ids: Vec::new(),
            sliced_observations: String::new(),
            slice_token_estimate: 0,
            compression_target_tokens: target_tokens,
        };
    }
    let total_chars_f64 = total_chars as f64;

    let mut selected = Vec::<String>::new();
    let mut covered_tokens = 0u32;
    let mut selected_texts = Vec::<&str>::new();
    for entry in &ordered_entries {
        let entry_text = entry.text.trim();
        if entry_text.is_empty() {
            continue;
        }
        let entry_chars = entry_text.chars().count();
        let entry_tokens = if observation_token_count > 0 {
            (f64::from(observation_token_count) * (entry_chars as f64 / total_chars_f64))
                .round()
                .clamp(1.0, f64::from(u32::MAX)) as u32
        } else {
            estimate_text_tokens(entry_text).max(1)
        };
        if covered_tokens.saturating_add(entry_tokens) > target_tokens {
            if selected.is_empty() {
                covered_tokens = covered_tokens.saturating_add(entry_tokens);
                selected.push(entry.entry_id.clone());
                selected_texts.push(entry_text);
            }
            break;
        }
        covered_tokens = covered_tokens.saturating_add(entry_tokens);
        selected.push(entry.entry_id.clone());
        selected_texts.push(entry_text);
    }
    BufferedReflectionSelection {
        selected_entry_ids: selected,
        sliced_observations: selected_texts.join("\n"),
        slice_token_estimate: covered_tokens,
        compression_target_tokens: target_tokens,
    }
}

fn normalize_reflection_text(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let compact = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();
    compact.chars().take(max_chars).collect::<String>()
}

#[cfg(test)]
mod tests;
