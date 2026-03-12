use crate::embedding::embed_text;
use crate::uri::AxiomUri;

use super::super::memory_extractor::ExtractedMemory;
use super::dedup::{
    cosine_similarity, parse_llm_dedup_decision, prefilter_existing_memory_matches,
    resolve_dedup_selection, resolve_merge_target_index,
};
use super::helpers::build_memory_key;
use super::resolve_path::merge_resolved_candidate;
use super::types::{
    DEFAULT_MEMORY_DEDUP_LLM_MAX_MATCHES, DEFAULT_MEMORY_DEDUP_LLM_MAX_OUTPUT_TOKENS,
    DEFAULT_MEMORY_DEDUP_LLM_TEMPERATURE_MILLI, DEFAULT_MEMORY_DEDUP_LLM_TIMEOUT_MS,
    ExistingMemoryFact, MemoryDedupConfig, MemoryDedupDecision, MemoryDedupMode,
    ParsedLlmDedupDecision, PrefilteredMemoryMatch, ResolvedMemoryCandidate,
};

fn extracted(category: &str, text: &str, source_ids: &[&str]) -> ExtractedMemory {
    ExtractedMemory {
        category: category.to_string(),
        key: build_memory_key(category, text),
        text: text.to_string(),
        source_message_ids: source_ids.iter().copied().map(str::to_string).collect(),
        confidence_milli: 700,
    }
}

fn dedup_config(mode: MemoryDedupMode, strict: bool, endpoint: &str) -> MemoryDedupConfig {
    MemoryDedupConfig {
        mode,
        similarity_threshold: 0.9,
        llm_endpoint: endpoint.to_string(),
        llm_model: "qwen2.5:7b-instruct".to_string(),
        llm_timeout_ms: DEFAULT_MEMORY_DEDUP_LLM_TIMEOUT_MS,
        llm_max_output_tokens: DEFAULT_MEMORY_DEDUP_LLM_MAX_OUTPUT_TOKENS,
        llm_temperature_milli: DEFAULT_MEMORY_DEDUP_LLM_TEMPERATURE_MILLI,
        llm_strict: strict,
        llm_max_matches: DEFAULT_MEMORY_DEDUP_LLM_MAX_MATCHES,
    }
}

#[test]
fn cosine_similarity_returns_expected_value() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![0.5, 0.0, 0.0];
    let c = vec![0.0, 1.0, 0.0];
    assert!(cosine_similarity(&a, &b) > 0.99);
    assert!(cosine_similarity(&a, &c) < 0.01);
}

#[test]
fn memory_dedup_mode_defaults_to_auto() {
    assert_eq!(MemoryDedupMode::parse(None), MemoryDedupMode::Auto);
    assert_eq!(MemoryDedupMode::parse(Some("")), MemoryDedupMode::Auto);
}

#[test]
fn merge_resolved_candidate_combines_source_ids() {
    let mut out = Vec::<ResolvedMemoryCandidate>::new();
    merge_resolved_candidate(
        &mut out,
        ResolvedMemoryCandidate {
            category: "preferences".to_string(),
            key: "pref-1".to_string(),
            text: "I prefer concise Rust code".to_string(),
            source_message_ids: vec!["m2".to_string()],
            target_uri: None,
        },
    );
    merge_resolved_candidate(
        &mut out,
        ResolvedMemoryCandidate {
            category: "preferences".to_string(),
            key: "pref-1".to_string(),
            text: "I prefer concise Rust code".to_string(),
            source_message_ids: vec!["m1".to_string()],
            target_uri: None,
        },
    );
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].source_message_ids,
        vec!["m1".to_string(), "m2".to_string()]
    );
}

#[test]
fn prefilter_existing_memory_matches_keeps_exact_at_threshold_one() {
    let existing = vec![
        ExistingMemoryFact {
            uri: AxiomUri::parse("axiom://user/memories/preferences/pref-a.md").expect("uri"),
            text: "I prefer concise Rust code".to_string(),
            vector: embed_text("I prefer concise Rust code"),
        },
        ExistingMemoryFact {
            uri: AxiomUri::parse("axiom://user/memories/preferences/pref-b.md").expect("uri"),
            text: "Use Kubernetes deployment checklist".to_string(),
            vector: embed_text("Use Kubernetes deployment checklist"),
        },
    ];
    let matches = prefilter_existing_memory_matches("I prefer concise Rust code", &existing, 1.0);
    assert_eq!(matches.len(), 1);
    assert_eq!(
        matches[0].uri.to_string(),
        "axiom://user/memories/preferences/pref-a.md"
    );
    assert!(matches[0].score >= 1.0);
}

#[test]
fn parse_llm_dedup_decision_accepts_object_payload() {
    let payload = serde_json::json!({
        "decision": "merge",
        "target_index": 2,
        "target_uri": "axiom://user/memories/preferences/pref-2.md",
        "reason": "same preference"
    });
    let parsed = parse_llm_dedup_decision(&payload).expect("parse");
    assert_eq!(parsed.decision, MemoryDedupDecision::Merge);
    assert_eq!(parsed.target_index, Some(2));
    assert_eq!(
        parsed.target_uri.as_deref(),
        Some("axiom://user/memories/preferences/pref-2.md")
    );
}

#[test]
fn parse_llm_dedup_decision_accepts_data_wrapper() {
    let payload = serde_json::json!({
        "data": {
            "decision": "merge",
            "target_index": 1
        }
    });
    let parsed = parse_llm_dedup_decision(&payload).expect("parse");
    assert_eq!(parsed.decision, MemoryDedupDecision::Merge);
    assert_eq!(parsed.target_index, Some(1));
    assert_eq!(parsed.target_uri, None);
}

#[test]
fn parse_llm_dedup_decision_accepts_embedded_json_content() {
    let payload = serde_json::json!({
        "message": {
            "content": "```json\n{\"decision\":\"skip\"}\n```"
        }
    });
    let parsed = parse_llm_dedup_decision(&payload).expect("parse");
    assert_eq!(parsed.decision, MemoryDedupDecision::Skip);
    assert_eq!(parsed.target_index, None);
    assert_eq!(parsed.target_uri, None);
}

#[test]
fn resolve_dedup_selection_auto_falls_back_to_create_on_llm_error() {
    let candidate = extracted("preferences", "I prefer concise Rust code", &["m1"]);
    let prefiltered = vec![PrefilteredMemoryMatch {
        uri: AxiomUri::parse("axiom://user/memories/preferences/pref-a.md").expect("uri"),
        text: "I prefer concise Rust code".to_string(),
        score: 1.0,
    }];
    let config = dedup_config(MemoryDedupMode::Auto, false, "http://example.com/api/chat");
    let (selection, llm_error) =
        resolve_dedup_selection(&candidate, &candidate.text, &prefiltered, &config)
            .expect("selection");
    assert_eq!(selection.decision, MemoryDedupDecision::Create);
    assert_eq!(selection.selected_index, None);
    assert!(llm_error.is_some());
}

#[test]
fn resolve_dedup_selection_llm_strict_returns_error_on_llm_failure() {
    let candidate = extracted("preferences", "I prefer concise Rust code", &["m1"]);
    let prefiltered = vec![PrefilteredMemoryMatch {
        uri: AxiomUri::parse("axiom://user/memories/preferences/pref-a.md").expect("uri"),
        text: "I prefer concise Rust code".to_string(),
        score: 1.0,
    }];
    let config = dedup_config(MemoryDedupMode::Llm, true, "http://example.com/api/chat");
    let err = resolve_dedup_selection(&candidate, &candidate.text, &prefiltered, &config)
        .expect_err("must fail");
    assert!(err.to_string().contains("memory dedup llm endpoint"));
}

#[test]
fn resolve_merge_target_index_requires_valid_target() {
    let prefiltered = vec![PrefilteredMemoryMatch {
        uri: AxiomUri::parse("axiom://user/memories/preferences/pref-a.md").expect("uri"),
        text: "I prefer concise Rust code".to_string(),
        score: 1.0,
    }];
    let parsed = ParsedLlmDedupDecision {
        decision: MemoryDedupDecision::Merge,
        target_uri: Some("axiom://user/memories/preferences/unknown.md".to_string()),
        target_index: Some(99),
    };
    let err = resolve_merge_target_index(&parsed, &prefiltered).expect_err("must fail");
    assert!(err.to_string().contains("missing valid target"));
}
