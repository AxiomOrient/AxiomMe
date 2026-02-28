use std::collections::HashSet;

use reqwest::blocking::Client;
use serde_json::Value;

use crate::config::MemoryExtractorConfigSnapshot;
use crate::error::{AxiomError, Result};
use crate::llm_io::{extract_json_fragment, extract_llm_content, parse_local_loopback_endpoint};
use crate::models::Message;

use super::commit::helpers::{
    build_memory_key, extract_memories_heuristically, normalize_memory_text,
};

const DEFAULT_MEMORY_EXTRACTOR_MODE: &str = "auto";
const DEFAULT_MEMORY_LLM_ENDPOINT: &str = "http://127.0.0.1:11434/api/chat";
const DEFAULT_MEMORY_LLM_MODEL: &str = "qwen2.5:7b-instruct";
const DEFAULT_MEMORY_LLM_TIMEOUT_MS: u64 = 4_000;
const DEFAULT_MEMORY_LLM_MAX_OUTPUT_TOKENS: u32 = 1_500;
const DEFAULT_MEMORY_LLM_TEMPERATURE_MILLI: u16 = 0;
const DEFAULT_MEMORY_LLM_MAX_MESSAGES: usize = 40;
const DEFAULT_MEMORY_LLM_MAX_CHARS_PER_MESSAGE: usize = 1_200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryExtractorMode {
    Auto,
    Heuristic,
    Llm,
}

impl MemoryExtractorMode {
    fn parse(raw: Option<&str>) -> Self {
        match raw
            .unwrap_or(DEFAULT_MEMORY_EXTRACTOR_MODE)
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "heuristic" | "rules" | "rule" => Self::Heuristic,
            "llm" | "model" => Self::Llm,
            _ => Self::Auto,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Heuristic => "heuristic",
            Self::Llm => "llm",
        }
    }
}

#[derive(Debug, Clone)]
struct MemoryExtractorConfig {
    mode: MemoryExtractorMode,
    llm_endpoint: String,
    llm_model: String,
    llm_timeout_ms: u64,
    llm_max_output_tokens: u32,
    llm_temperature_milli: u16,
    llm_strict: bool,
    llm_max_messages: usize,
    llm_max_chars_per_message: usize,
}

impl MemoryExtractorConfig {
    fn from_snapshot(snapshot: &MemoryExtractorConfigSnapshot) -> Self {
        Self {
            mode: MemoryExtractorMode::parse(snapshot.mode.as_deref()),
            llm_endpoint: snapshot
                .llm_endpoint
                .clone()
                .unwrap_or_else(|| DEFAULT_MEMORY_LLM_ENDPOINT.to_string()),
            llm_model: snapshot
                .llm_model
                .clone()
                .unwrap_or_else(|| DEFAULT_MEMORY_LLM_MODEL.to_string()),
            llm_timeout_ms: snapshot
                .llm_timeout_ms
                .unwrap_or(DEFAULT_MEMORY_LLM_TIMEOUT_MS),
            llm_max_output_tokens: snapshot
                .llm_max_output_tokens
                .unwrap_or(DEFAULT_MEMORY_LLM_MAX_OUTPUT_TOKENS),
            llm_temperature_milli: snapshot
                .llm_temperature_milli
                .unwrap_or(DEFAULT_MEMORY_LLM_TEMPERATURE_MILLI),
            llm_strict: snapshot.llm_strict,
            llm_max_messages: snapshot
                .llm_max_messages
                .unwrap_or(DEFAULT_MEMORY_LLM_MAX_MESSAGES),
            llm_max_chars_per_message: snapshot
                .llm_max_chars_per_message
                .unwrap_or(DEFAULT_MEMORY_LLM_MAX_CHARS_PER_MESSAGE),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ExtractedMemory {
    pub category: String,
    pub key: String,
    pub text: String,
    pub source_message_ids: Vec<String>,
    pub confidence_milli: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MemoryExtractionResult {
    pub memories: Vec<ExtractedMemory>,
    pub mode_requested: String,
    pub mode_applied: String,
    pub llm_error: Option<String>,
}

pub(super) fn extract_memories_for_commit(
    messages: &[Message],
    config_snapshot: &MemoryExtractorConfigSnapshot,
) -> Result<MemoryExtractionResult> {
    let config = MemoryExtractorConfig::from_snapshot(config_snapshot);
    let mode_requested = config.mode.as_str().to_string();

    match config.mode {
        MemoryExtractorMode::Heuristic => Ok(MemoryExtractionResult {
            memories: heuristic_memories(messages),
            mode_requested,
            mode_applied: "heuristic".to_string(),
            llm_error: None,
        }),
        MemoryExtractorMode::Llm => {
            let memories = llm_memories(messages, &config)?;
            Ok(MemoryExtractionResult {
                memories,
                mode_requested,
                mode_applied: "llm".to_string(),
                llm_error: None,
            })
        }
        MemoryExtractorMode::Auto => match llm_memories(messages, &config) {
            Ok(memories) => Ok(MemoryExtractionResult {
                memories,
                mode_requested,
                mode_applied: "llm".to_string(),
                llm_error: None,
            }),
            Err(err) => {
                if config.llm_strict {
                    Err(err)
                } else {
                    Ok(MemoryExtractionResult {
                        memories: heuristic_memories(messages),
                        mode_requested,
                        mode_applied: "heuristic".to_string(),
                        llm_error: Some(err.to_string()),
                    })
                }
            }
        },
    }
}

pub(crate) fn heuristic_memories(messages: &[Message]) -> Vec<ExtractedMemory> {
    let base = extract_memories_heuristically(messages)
        .into_iter()
        .map(|candidate| ExtractedMemory {
            category: candidate.category,
            key: candidate.key,
            text: normalize_memory_text(&candidate.text),
            source_message_ids: vec![candidate.source_message_id],
            confidence_milli: 550,
        })
        .collect::<Vec<_>>();
    merge_duplicate_memories(base)
}

fn llm_memories(
    messages: &[Message],
    config: &MemoryExtractorConfig,
) -> Result<Vec<ExtractedMemory>> {
    if messages.is_empty() {
        return Ok(Vec::new());
    }
    let endpoint =
        parse_local_loopback_endpoint(&config.llm_endpoint, "memory llm endpoint", "local host")
            .map_err(AxiomError::Validation)?;
    let client = Client::builder()
        .timeout(std::time::Duration::from_millis(config.llm_timeout_ms))
        .build()
        .map_err(|err| AxiomError::Internal(format!("memory llm client build failed: {err}")))?;

    let selected_messages = select_messages_for_llm(
        messages,
        config.llm_max_messages,
        config.llm_max_chars_per_message,
    );
    let message_payload = selected_messages
        .iter()
        .map(|message| {
            serde_json::json!({
                "id": message.id,
                "role": message.role,
                "text": message.text,
            })
        })
        .collect::<Vec<_>>();

    let system_prompt = "Extract durable long-term memories from the conversation. \
Return JSON only. Use categories: profile, preferences, entities, events, cases, patterns. \
Output schema: {\"memories\":[{\"category\":\"...\",\"text\":\"...\",\"source_message_ids\":[\"...\"],\"confidence\":0.0}]}";
    let user_prompt = format!(
        "Conversation messages as JSON:\n{}\n\nRules:\n- keep memory text concise and factual\n- no duplicate memories\n- source_message_ids must reference provided ids",
        serde_json::to_string(&message_payload)?
    );

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
        .post(endpoint)
        .json(&payload)
        .send()
        .map_err(|err| AxiomError::Internal(format!("memory llm request failed: {err}")))?;
    if !response.status().is_success() {
        return Err(AxiomError::Internal(format!(
            "memory llm non-success status: {}",
            response.status()
        )));
    }
    let value = response
        .json::<Value>()
        .map_err(|err| AxiomError::Internal(format!("memory llm invalid json response: {err}")))?;
    parse_llm_memories(&value, &selected_messages)
}

fn parse_llm_memories(value: &Value, messages: &[Message]) -> Result<Vec<ExtractedMemory>> {
    let known_ids = messages
        .iter()
        .map(|message| message.id.as_str())
        .collect::<HashSet<_>>();
    let fallback_source = messages.last().map(|message| message.id.as_str());

    if let Some(parsed) = parse_memories_value(value, &known_ids, fallback_source) {
        if parsed.is_empty() {
            return Err(AxiomError::Validation(
                "memory llm response contained zero valid memories".to_string(),
            ));
        }
        return Ok(parsed);
    }

    let content = extract_llm_content(value)
        .ok_or_else(|| AxiomError::Validation("memory llm response missing content".to_string()))?;
    let json_fragment = extract_json_fragment(&content).ok_or_else(|| {
        AxiomError::Validation("memory llm response does not contain json object/array".to_string())
    })?;
    let parsed_value = serde_json::from_str::<Value>(&json_fragment).map_err(|err| {
        AxiomError::Validation(format!("memory llm content json parse failed: {err}"))
    })?;
    let parsed =
        parse_memories_value(&parsed_value, &known_ids, fallback_source).ok_or_else(|| {
            AxiomError::Validation("memory llm produced unsupported schema".to_string())
        })?;
    if parsed.is_empty() {
        return Err(AxiomError::Validation(
            "memory llm content contained zero valid memories".to_string(),
        ));
    }
    Ok(parsed)
}

fn parse_memories_value(
    value: &Value,
    known_ids: &HashSet<&str>,
    fallback_source: Option<&str>,
) -> Option<Vec<ExtractedMemory>> {
    let root = value
        .get("result")
        .or_else(|| value.get("data"))
        .unwrap_or(value);
    let items = if let Some(array) = root.as_array() {
        array
    } else {
        root.get("memories")?.as_array()?
    };

    let mut out = Vec::<ExtractedMemory>::new();
    for item in items {
        let Some(object) = item.as_object() else {
            continue;
        };
        let Some(category) = object
            .get("category")
            .and_then(|value| value.as_str())
            .map(normalize_or_default_category)
        else {
            continue;
        };
        let raw_text = object
            .get("text")
            .or_else(|| object.get("memory"))
            .or_else(|| object.get("content"))
            .or_else(|| object.get("overview"))
            .or_else(|| object.get("abstract"))
            .and_then(|value| value.as_str());
        let Some(raw_text) = raw_text else {
            continue;
        };
        let text = normalize_memory_text(raw_text);
        if text.is_empty() {
            continue;
        }
        let Some(source_message_ids) =
            normalize_source_message_ids(object, known_ids, fallback_source)
        else {
            continue;
        };
        if source_message_ids.is_empty() {
            continue;
        }
        let confidence_milli = parse_confidence_milli(object.get("confidence"));
        out.push(ExtractedMemory {
            category: category.to_string(),
            key: build_memory_key(category, &text),
            text,
            source_message_ids,
            confidence_milli,
        });
    }

    Some(merge_duplicate_memories(out))
}

fn normalize_source_message_ids(
    object: &serde_json::Map<String, Value>,
    known_ids: &HashSet<&str>,
    fallback_source: Option<&str>,
) -> Option<Vec<String>> {
    let mut out = Vec::<String>::new();
    let has_explicit_sources = object.get("source_message_ids").is_some()
        || object.get("source_ids").is_some()
        || object.get("source_message_id").is_some();

    if let Some(array) = object
        .get("source_message_ids")
        .or_else(|| object.get("source_ids"))
        .and_then(|value| value.as_array())
    {
        for value in array {
            if let Some(id) = value.as_str() {
                let trimmed = id.trim();
                if known_ids.contains(trimmed) {
                    out.push(trimmed.to_string());
                }
            }
        }
    }

    if out.is_empty()
        && let Some(id) = object
            .get("source_message_id")
            .and_then(|value| value.as_str())
    {
        let trimmed = id.trim();
        if known_ids.contains(trimmed) {
            out.push(trimmed.to_string());
        }
    }
    if has_explicit_sources && out.is_empty() {
        return None;
    }

    if out.is_empty()
        && let Some(id) = fallback_source
    {
        out.push(id.to_string());
    }

    out.sort();
    out.dedup();
    Some(out)
}

fn parse_confidence_milli(value: Option<&Value>) -> u16 {
    let Some(value) = value else {
        return 700;
    };
    if let Some(as_float) = value.as_f64()
        && as_float.is_finite()
    {
        let scaled = if as_float <= 1.0 {
            as_float * 1000.0
        } else {
            as_float
        };
        return rounded_f64_to_u16_clamped(scaled.clamp(0.0, 1000.0));
    }
    if let Some(as_int) = value.as_u64() {
        return u16::try_from(as_int.min(1000)).unwrap_or(1000);
    }
    700
}

fn rounded_f64_to_u16_clamped(value: f64) -> u16 {
    let rounded = value.round().clamp(0.0, f64::from(u16::MAX));
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "value is pre-clamped to the representable non-negative u16 range"
    )]
    {
        rounded as u16
    }
}

fn merge_duplicate_memories(memories: Vec<ExtractedMemory>) -> Vec<ExtractedMemory> {
    let mut out = Vec::<ExtractedMemory>::new();
    for memory in memories {
        if let Some(existing) = out
            .iter_mut()
            .find(|item| item.category == memory.category && item.text == memory.text)
        {
            existing.confidence_milli = existing.confidence_milli.max(memory.confidence_milli);
            existing
                .source_message_ids
                .extend(memory.source_message_ids);
            existing.source_message_ids.sort();
            existing.source_message_ids.dedup();
            continue;
        }
        out.push(memory);
    }
    out
}

fn normalize_category(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "profile" | "persona" | "user_profile" => Some("profile"),
        "preference" | "preferences" | "likes" | "dislikes" => Some("preferences"),
        "entity" | "entities" | "fact" | "facts" => Some("entities"),
        "event" | "events" | "timeline" => Some("events"),
        "case" | "cases" | "incident" | "incidents" => Some("cases"),
        "pattern" | "patterns" | "rule" | "rules" | "playbook" => Some("patterns"),
        _ => None,
    }
}

fn normalize_or_default_category(raw: &str) -> &'static str {
    normalize_category(raw).unwrap_or("patterns")
}

fn select_messages_for_llm(
    messages: &[Message],
    max_messages: usize,
    max_chars: usize,
) -> Vec<Message> {
    messages
        .iter()
        .rev()
        .take(max_messages)
        .map(|message| Message {
            id: message.id.clone(),
            role: message.role.clone(),
            text: message.text.chars().take(max_chars).collect::<String>(),
            created_at: message.created_at,
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn msg(id: &str, text: &str) -> Message {
        Message {
            id: id.to_string(),
            role: "user".to_string(),
            text: text.to_string(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn parse_llm_memories_accepts_object_payload() {
        let payload = serde_json::json!({
            "memories": [
                {
                    "category": "preferences",
                    "text": "I prefer concise Rust code",
                    "source_message_ids": ["m1"],
                    "confidence": 0.91
                }
            ]
        });
        let messages = vec![msg("m1", "I prefer concise Rust code")];
        let parsed = parse_llm_memories(&payload, &messages).expect("parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].category, "preferences");
        assert_eq!(parsed[0].source_message_ids, vec!["m1".to_string()]);
        assert!(parsed[0].confidence_milli >= 900);
    }

    #[test]
    fn parse_llm_memories_accepts_embedded_json_content() {
        let payload = serde_json::json!({
            "message": {
                "content": "```json\n[{\"category\":\"entity\",\"text\":\"Project is AxiomMe\",\"source_message_ids\":[\"m2\"]}]\n```"
            }
        });
        let messages = vec![msg("m2", "Project is AxiomMe")];
        let parsed = parse_llm_memories(&payload, &messages).expect("parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].category, "entities");
        assert_eq!(parsed[0].text, "Project is AxiomMe");
    }

    #[test]
    fn parse_llm_memories_accepts_result_wrapper_schema_with_content_priority() {
        let payload = serde_json::json!({
            "result": {
                "memories": [
                    {
                        "category": "unknown-category",
                        "abstract": "User has a reusable workflow",
                        "overview": "Uses checklist before release",
                        "content": "Always run checklist before release",
                        "source_message_ids": ["m3"]
                    }
                ]
            }
        });
        let messages = vec![msg("m3", "Always run checklist before release")];
        let parsed = parse_llm_memories(&payload, &messages).expect("parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].category, "patterns");
        assert_eq!(parsed[0].text, "Always run checklist before release");
        assert!(parsed[0].key.starts_with("pattern-"));
        assert_eq!(parsed[0].source_message_ids, vec!["m3".to_string()]);
        assert_eq!(parsed[0].confidence_milli, 700);
    }

    #[test]
    fn parse_llm_memories_rejects_invalid_explicit_sources_in_result_wrapper_schema() {
        let payload = serde_json::json!({
            "result": {
                "memories": [
                    {
                        "category": "patterns",
                        "content": "Always run checklist before release",
                        "source_message_ids": ["missing-id"]
                    }
                ]
            }
        });
        let messages = vec![msg("m3", "Always run checklist before release")];
        let err = parse_llm_memories(&payload, &messages).expect_err("must fail");
        assert!(err.to_string().contains("zero valid memories"));
    }

    #[test]
    fn parse_llm_memories_rejects_explicit_unknown_source_ids() {
        let payload = serde_json::json!({
            "memories": [
                {
                    "category": "preferences",
                    "text": "I prefer concise Rust code",
                    "source_message_ids": ["missing-id"]
                }
            ]
        });
        let messages = vec![msg("m1", "I prefer concise Rust code")];
        let err = parse_llm_memories(&payload, &messages).expect_err("must fail");
        assert!(err.to_string().contains("zero valid memories"));
    }

    #[test]
    fn merge_duplicate_memories_combines_sources() {
        let merged = merge_duplicate_memories(vec![
            ExtractedMemory {
                category: "preferences".to_string(),
                key: "pref-a".to_string(),
                text: "I prefer concise Rust code".to_string(),
                source_message_ids: vec!["m1".to_string()],
                confidence_milli: 500,
            },
            ExtractedMemory {
                category: "preferences".to_string(),
                key: "pref-b".to_string(),
                text: "I prefer concise Rust code".to_string(),
                source_message_ids: vec!["m2".to_string()],
                confidence_milli: 900,
            },
        ]);
        assert_eq!(merged.len(), 1);
        assert_eq!(
            merged[0].source_message_ids,
            vec!["m1".to_string(), "m2".to_string()]
        );
        assert_eq!(merged[0].confidence_milli, 900);
    }

    #[test]
    fn heuristic_memories_uses_rules_pipeline() {
        let messages = vec![msg("m1", "I prefer concise Rust code.")];
        let extracted = heuristic_memories(&messages);
        assert!(
            !extracted.is_empty(),
            "rules pipeline must emit at least one memory"
        );
        let preference = extracted
            .iter()
            .find(|item| item.category == "preferences")
            .expect("preference memory expected from 'I prefer ...' input");
        assert_eq!(preference.source_message_ids, vec!["m1".to_string()]);
        assert_eq!(preference.confidence_milli, 550);
        assert!(
            preference.key.starts_with("pref-"),
            "preference memories must use pref-* key namespace"
        );
    }
}
