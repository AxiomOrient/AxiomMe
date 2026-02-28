use reqwest::blocking::Client;
use serde_json::Value;

use crate::embedding::embed_text;
use crate::error::{AxiomError, Result};
use crate::llm_io::{extract_json_fragment, extract_llm_content, parse_local_loopback_endpoint};

use super::super::memory_extractor::ExtractedMemory;
use super::helpers::normalize_memory_text;
use super::types::{
    DedupSelection, ExistingMemoryFact, MemoryDedupConfig, MemoryDedupDecision, MemoryDedupMode,
    ParsedLlmDedupDecision, PrefilteredMemoryMatch,
};

pub(super) fn prefilter_existing_memory_matches(
    candidate_text: &str,
    existing: &[ExistingMemoryFact],
    threshold: f32,
) -> Vec<PrefilteredMemoryMatch> {
    let mut out = Vec::<PrefilteredMemoryMatch>::new();
    let normalized_candidate = normalize_memory_text(candidate_text);
    let candidate_vector = embed_text(candidate_text);
    for fact in existing {
        let score = if normalize_memory_text(&fact.text) == normalized_candidate {
            1.0
        } else {
            cosine_similarity(&candidate_vector, &fact.vector)
        };
        if score >= threshold {
            out.push(PrefilteredMemoryMatch {
                uri: fact.uri.clone(),
                text: fact.text.clone(),
                score,
            });
        }
    }
    out.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.uri.to_string().cmp(&right.uri.to_string()))
    });
    out
}

const fn deterministic_dedup_selection(prefiltered: &[PrefilteredMemoryMatch]) -> DedupSelection {
    if prefiltered.is_empty() {
        DedupSelection {
            decision: MemoryDedupDecision::Create,
            selected_index: None,
        }
    } else {
        DedupSelection {
            decision: MemoryDedupDecision::Merge,
            selected_index: Some(0),
        }
    }
}

pub(super) fn resolve_dedup_selection(
    candidate: &ExtractedMemory,
    normalized_text: &str,
    prefiltered: &[PrefilteredMemoryMatch],
    config: &MemoryDedupConfig,
) -> Result<(DedupSelection, Option<String>)> {
    let deterministic = deterministic_dedup_selection(prefiltered);
    let conservative_create = DedupSelection {
        decision: MemoryDedupDecision::Create,
        selected_index: None,
    };
    match config.mode {
        MemoryDedupMode::Deterministic => Ok((deterministic, None)),
        MemoryDedupMode::Llm | MemoryDedupMode::Auto => {
            if prefiltered.is_empty() {
                return Ok((deterministic, None));
            }
            match llm_dedup_selection(candidate, normalized_text, prefiltered, config) {
                Ok(selection) => Ok((selection, None)),
                Err(err) => {
                    if config.mode == MemoryDedupMode::Llm && config.llm_strict {
                        Err(err)
                    } else {
                        Ok((conservative_create, Some(err.to_string())))
                    }
                }
            }
        }
    }
}

fn llm_dedup_selection(
    candidate: &ExtractedMemory,
    normalized_text: &str,
    prefiltered: &[PrefilteredMemoryMatch],
    config: &MemoryDedupConfig,
) -> Result<DedupSelection> {
    let endpoint = parse_local_loopback_endpoint(
        &config.llm_endpoint,
        "memory dedup llm endpoint",
        "local host",
    )
    .map_err(AxiomError::Validation)?;
    let client = Client::builder()
        .timeout(std::time::Duration::from_millis(config.llm_timeout_ms))
        .build()
        .map_err(|err| {
            AxiomError::Internal(format!("memory dedup llm client build failed: {err}"))
        })?;

    let matches_payload = prefiltered
        .iter()
        .take(config.llm_max_matches)
        .enumerate()
        .map(|(index, item)| {
            serde_json::json!({
                "rank": index + 1,
                "uri": item.uri.to_string(),
                "text": item.text,
                "score": item.score,
            })
        })
        .collect::<Vec<_>>();
    let prompt_payload = serde_json::json!({
        "candidate": {
            "category": candidate.category,
            "text": normalized_text,
            "source_message_ids": candidate.source_message_ids,
        },
        "matches": matches_payload,
    });
    let system_prompt = "Decide dedup action for candidate memory against similar memories. \
Return JSON only with schema: {\"decision\":\"create|merge|skip\",\"target_index\":1,\"target_uri\":\"...\",\"reason\":\"...\"}.";
    let user_prompt = format!(
        "Dedup request JSON:\n{}\n\nRules:\n- create: candidate should become new memory\n- merge: candidate matches existing memory\n- skip: candidate is duplicate/no-op\n- if merge, choose either target_index (1-based) or target_uri",
        serde_json::to_string(&prompt_payload)?
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

    let response =
        client.post(endpoint).json(&payload).send().map_err(|err| {
            AxiomError::Internal(format!("memory dedup llm request failed: {err}"))
        })?;
    if !response.status().is_success() {
        return Err(AxiomError::Internal(format!(
            "memory dedup llm non-success status: {}",
            response.status()
        )));
    }

    let value = response.json::<Value>().map_err(|err| {
        AxiomError::Internal(format!("memory dedup llm invalid json response: {err}"))
    })?;
    let parsed = parse_llm_dedup_decision(&value)?;
    let selected_index = match parsed.decision {
        MemoryDedupDecision::Create | MemoryDedupDecision::Skip => None,
        MemoryDedupDecision::Merge => Some(resolve_merge_target_index(&parsed, prefiltered)?),
    };
    Ok(DedupSelection {
        decision: parsed.decision,
        selected_index,
    })
}

pub(super) fn resolve_merge_target_index(
    parsed: &ParsedLlmDedupDecision,
    prefiltered: &[PrefilteredMemoryMatch],
) -> Result<usize> {
    if prefiltered.is_empty() {
        return Err(AxiomError::Validation(
            "memory dedup llm merge decision requires at least one prefiltered match".to_string(),
        ));
    }
    if let Some(target_uri) = parsed.target_uri.as_deref()
        && let Some(index) = prefiltered
            .iter()
            .position(|item| item.uri.to_string() == target_uri)
    {
        return Ok(index);
    }
    if let Some(rank) = parsed.target_index
        && rank > 0
    {
        let idx = rank - 1;
        if idx < prefiltered.len() {
            return Ok(idx);
        }
    }
    Err(AxiomError::Validation(
        "memory dedup llm merge decision missing valid target".to_string(),
    ))
}

pub(super) fn parse_llm_dedup_decision(value: &Value) -> Result<ParsedLlmDedupDecision> {
    if let Some(parsed) = parse_llm_dedup_decision_value(value) {
        return Ok(parsed);
    }

    let content = extract_llm_content(value).ok_or_else(|| {
        AxiomError::Validation("memory dedup llm response missing content".to_string())
    })?;
    let json_fragment = extract_json_fragment(&content).ok_or_else(|| {
        AxiomError::Validation(
            "memory dedup llm response does not contain json object/array".to_string(),
        )
    })?;
    let parsed_value = serde_json::from_str::<Value>(&json_fragment).map_err(|err| {
        AxiomError::Validation(format!("memory dedup llm content json parse failed: {err}"))
    })?;
    parse_llm_dedup_decision_value(&parsed_value).ok_or_else(|| {
        AxiomError::Validation("memory dedup llm response schema is unsupported".to_string())
    })
}

fn parse_llm_dedup_decision_value(value: &Value) -> Option<ParsedLlmDedupDecision> {
    let object = value.as_object()?;
    let object = object
        .get("result")
        .or_else(|| object.get("data"))
        .and_then(|inner| inner.as_object())
        .unwrap_or(object);

    let decision = object
        .get("decision")
        .or_else(|| object.get("action"))
        .or_else(|| object.get("mode"))
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_ascii_lowercase())
        .and_then(|value| match value.as_str() {
            "create" => Some(MemoryDedupDecision::Create),
            "merge" => Some(MemoryDedupDecision::Merge),
            "skip" => Some(MemoryDedupDecision::Skip),
            _ => None,
        })?;
    let target_uri = object
        .get("target_uri")
        .or_else(|| object.get("uri"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let target_index = object
        .get("target_index")
        .or_else(|| object.get("target_rank"))
        .or_else(|| object.get("match_index"))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0);

    Some(ParsedLlmDedupDecision {
        decision,
        target_uri,
        target_index,
    })
}

pub(super) fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let limit = left.len().min(right.len());
    let mut dot = 0.0f32;
    let mut left_norm = 0.0f32;
    let mut right_norm = 0.0f32;
    for idx in 0..limit {
        dot += left[idx] * right[idx];
        left_norm += left[idx] * left[idx];
        right_norm += right[idx] * right[idx];
    }
    if left_norm <= 0.0 || right_norm <= 0.0 {
        return 0.0;
    }
    dot / (left_norm.sqrt() * right_norm.sqrt())
}
