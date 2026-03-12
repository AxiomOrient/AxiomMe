use std::collections::{BTreeMap, HashSet};
use std::fs;

use crate::error::{AxiomError, Result};
use crate::models::{MemoryPromotionFact, MemoryPromotionRequest, PromotionApplyMode};
use crate::uri::{AxiomUri, Scope};

use super::Session;
use super::helpers::{build_memory_key, normalize_memory_text, slugify};
use super::resolve_path::dedup_source_ids;
use super::types::{
    ExistingPromotionFact, PROMOTION_MAX_CONFIDENCE_MILLI, PROMOTION_MAX_FACTS,
    PROMOTION_MAX_SOURCE_IDS_PER_FACT, PROMOTION_MAX_TEXT_CHARS, PromotionApplyInput,
    PromotionApplyPlan, ResolvedMemoryCandidate,
};

fn validate_promotion_request_bounds(request: &MemoryPromotionRequest) -> Result<()> {
    if request.facts.len() > PROMOTION_MAX_FACTS {
        return Err(AxiomError::Validation(format!(
            "facts exceeds max limit: {} > {}",
            request.facts.len(),
            PROMOTION_MAX_FACTS
        )));
    }
    for (index, fact) in request.facts.iter().enumerate() {
        if fact.text.chars().count() > PROMOTION_MAX_TEXT_CHARS {
            return Err(AxiomError::Validation(format!(
                "fact[{index}].text exceeds max chars: {} > {}",
                fact.text.chars().count(),
                PROMOTION_MAX_TEXT_CHARS
            )));
        }
        if fact.source_message_ids.len() > PROMOTION_MAX_SOURCE_IDS_PER_FACT {
            return Err(AxiomError::Validation(format!(
                "fact[{index}].source_message_ids exceeds max count: {} > {}",
                fact.source_message_ids.len(),
                PROMOTION_MAX_SOURCE_IDS_PER_FACT
            )));
        }
        if fact.confidence_milli > PROMOTION_MAX_CONFIDENCE_MILLI {
            return Err(AxiomError::Validation(format!(
                "fact[{index}].confidence_milli out of range: {} > {}",
                fact.confidence_milli, PROMOTION_MAX_CONFIDENCE_MILLI
            )));
        }
    }
    Ok(())
}

pub(super) fn promotion_apply_input_from_request(
    request: &MemoryPromotionRequest,
) -> Result<PromotionApplyInput> {
    validate_promotion_request_bounds(request)?;
    let facts = dedup_promotion_facts(&normalize_promotion_facts(&request.facts));
    let request_json = canonical_promotion_request_json(
        request.session_id.as_str(),
        request.checkpoint_id.as_str(),
        request.apply_mode,
        &facts,
    )?;
    let request_hash = blake3::hash(request_json.as_bytes()).to_hex().to_string();
    Ok(PromotionApplyInput {
        request_hash,
        request_json,
        apply_mode: request.apply_mode,
        facts,
    })
}

pub(super) fn promotion_apply_input_from_checkpoint_json(
    request_json: &str,
    expected_session_id: &str,
    expected_checkpoint_id: &str,
) -> Result<PromotionApplyInput> {
    let request: MemoryPromotionRequest = serde_json::from_str(request_json).map_err(|error| {
        AxiomError::Validation(format!("invalid checkpoint request_json: {error}"))
    })?;
    if request.session_id.trim() != expected_session_id {
        return Err(AxiomError::Validation(format!(
            "checkpoint request_json session_id mismatch: expected {expected_session_id}, got {}",
            request.session_id
        )));
    }
    if request.checkpoint_id.trim() != expected_checkpoint_id {
        return Err(AxiomError::Validation(format!(
            "checkpoint request_json checkpoint_id mismatch: expected {expected_checkpoint_id}, got {}",
            request.checkpoint_id
        )));
    }
    validate_promotion_request_bounds(&request)?;
    let facts = dedup_promotion_facts(&normalize_promotion_facts(&request.facts));
    Ok(PromotionApplyInput {
        request_hash: blake3::hash(request_json.as_bytes()).to_hex().to_string(),
        request_json: request_json.to_string(),
        apply_mode: request.apply_mode,
        facts,
    })
}

pub(super) fn validate_promotion_fact_semantics(fact: &MemoryPromotionFact) -> Result<()> {
    if normalize_memory_text(&fact.text).is_empty() {
        return Err(AxiomError::Validation(
            "promotion fact text must not be empty".to_string(),
        ));
    }
    if dedup_source_ids(&fact.source_message_ids).is_empty() {
        return Err(AxiomError::Validation(
            "promotion fact source_message_ids must not be empty".to_string(),
        ));
    }
    Ok(())
}

fn normalize_promotion_facts(facts: &[MemoryPromotionFact]) -> Vec<MemoryPromotionFact> {
    let mut out = facts
        .iter()
        .map(|fact| MemoryPromotionFact {
            category: fact.category,
            text: normalize_memory_text(&fact.text),
            source_message_ids: dedup_source_ids(&fact.source_message_ids),
            source: fact
                .source
                .as_ref()
                .map(|value| normalize_memory_text(value))
                .filter(|value| !value.is_empty()),
            confidence_milli: fact.confidence_milli.min(PROMOTION_MAX_CONFIDENCE_MILLI),
        })
        .collect::<Vec<_>>();
    out.sort_by(|left, right| {
        left.category
            .as_str()
            .cmp(right.category.as_str())
            .then_with(|| left.text.cmp(&right.text))
            .then_with(|| left.source_message_ids.cmp(&right.source_message_ids))
    });
    out
}

fn dedup_promotion_facts(facts: &[MemoryPromotionFact]) -> Vec<MemoryPromotionFact> {
    let mut out = Vec::<MemoryPromotionFact>::new();
    for fact in facts {
        if let Some(existing) = out.iter_mut().find(|item| {
            item.category == fact.category
                && normalize_memory_text(&item.text) == normalize_memory_text(&fact.text)
        }) {
            existing
                .source_message_ids
                .extend(fact.source_message_ids.clone());
            existing.source_message_ids = dedup_source_ids(&existing.source_message_ids);
            if existing.source.is_none() {
                existing.source = fact.source.clone();
            }
            existing.confidence_milli = existing.confidence_milli.max(fact.confidence_milli);
        } else {
            out.push(fact.clone());
        }
    }
    out.sort_by(|left, right| {
        left.category
            .as_str()
            .cmp(right.category.as_str())
            .then_with(|| left.text.cmp(&right.text))
            .then_with(|| left.source_message_ids.cmp(&right.source_message_ids))
    });
    out
}

fn canonical_promotion_request_json(
    session_id: &str,
    checkpoint_id: &str,
    apply_mode: PromotionApplyMode,
    facts: &[MemoryPromotionFact],
) -> Result<String> {
    let facts_json = facts
        .iter()
        .map(|fact| {
            serde_json::json!({
                "category": fact.category.as_str(),
                "text": fact.text,
                "source_message_ids": fact.source_message_ids,
                "source": fact.source,
                "confidence_milli": fact.confidence_milli,
            })
        })
        .collect::<Vec<_>>();
    let payload = serde_json::json!({
        "session_id": session_id,
        "checkpoint_id": checkpoint_id,
        "apply_mode": promotion_apply_mode_label(apply_mode),
        "facts": facts_json,
    });
    Ok(serde_json::to_string(&payload)?)
}

const fn promotion_apply_mode_label(mode: PromotionApplyMode) -> &'static str {
    match mode {
        PromotionApplyMode::AllOrNothing => "all_or_nothing",
        PromotionApplyMode::BestEffort => "best_effort",
    }
}

pub(super) fn plan_promotion_apply(
    existing: &[ExistingPromotionFact],
    incoming: &[MemoryPromotionFact],
) -> PromotionApplyPlan {
    let mut seen = existing
        .iter()
        .map(|fact| format!("{}|{}", fact.category, normalize_memory_text(&fact.text)))
        .collect::<HashSet<_>>();
    let mut skipped_duplicates = 0usize;
    let mut candidates = Vec::<ResolvedMemoryCandidate>::new();

    for fact in incoming {
        let text = normalize_memory_text(&fact.text);
        let category = fact.category.as_str().to_string();
        let dedup_key = format!("{category}|{text}");
        if !seen.insert(dedup_key) {
            skipped_duplicates = skipped_duplicates.saturating_add(1);
            continue;
        }
        candidates.push(ResolvedMemoryCandidate {
            category: category.clone(),
            key: build_memory_key(&category, &text),
            text,
            source_message_ids: dedup_source_ids(&fact.source_message_ids),
            target_uri: None,
        });
    }

    PromotionApplyPlan {
        candidates,
        skipped_duplicates,
    }
}

pub(super) fn restore_promotion_snapshots(
    session: &Session,
    snapshots: &BTreeMap<String, Option<String>>,
) -> Result<()> {
    for (uri_raw, content) in snapshots {
        let uri = AxiomUri::parse(uri_raw)?;
        let path = session.fs.resolve_uri(&uri);
        match content {
            Some(previous) => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&path, previous)?;
            }
            None => {
                if path.exists() {
                    fs::remove_file(path)?;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn memory_category_path(category: &str) -> Result<(Scope, &'static str, bool)> {
    let resolved = match category {
        "profile" => (Scope::User, "memories/profile.md", true),
        "preferences" => (Scope::User, "memories/preferences", false),
        "entities" => (Scope::User, "memories/entities", false),
        "events" => (Scope::User, "memories/events", false),
        "cases" => (Scope::Agent, "memories/cases", false),
        "patterns" => (Scope::Agent, "memories/patterns", false),
        other => {
            return Err(AxiomError::Validation(format!(
                "unsupported memory category: {other}"
            )));
        }
    };
    Ok(resolved)
}

pub(super) fn memory_uri_for_category_key(category: &str, key: &str) -> Result<AxiomUri> {
    let (scope, base_path, single_file) = memory_category_path(category)?;
    if single_file {
        return AxiomUri::root(scope).join(base_path);
    }
    AxiomUri::root(scope).join(&format!("{base_path}/{}.md", slugify(key)))
}
