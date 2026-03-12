use std::collections::BTreeMap;

use crate::error::{AxiomError, Result};
use crate::models::{MemoryPromotionFact, MemoryPromotionResult};
use crate::uri::AxiomUri;

use super::Session;
use super::promotion::{
    plan_promotion_apply, restore_promotion_snapshots, validate_promotion_fact_semantics,
};
use super::read_path::list_existing_promotion_facts;
use super::write_path::{
    persist_promotion_candidate as persist_promotion_candidate_write_path,
    reindex_memory_uris as reindex_memory_uris_write_path,
};

type RecordDedupFallbackFn = fn(&Session, &str, &str);

pub(super) fn apply_promotion_all_or_nothing(
    session: &Session,
    checkpoint_id: &str,
    facts: &[MemoryPromotionFact],
    record_dedup_fallback: RecordDedupFallbackFn,
) -> Result<MemoryPromotionResult> {
    for fact in facts {
        validate_promotion_fact_semantics(fact)?;
    }

    let existing = list_existing_promotion_facts(session)?;
    let plan = plan_promotion_apply(&existing, facts);
    let mut snapshots = BTreeMap::<String, Option<String>>::new();
    let mut persisted_uris = Vec::<AxiomUri>::new();

    for candidate in &plan.candidates {
        let uri = match persist_promotion_candidate_write_path(
            session,
            candidate,
            Some(&mut snapshots),
        ) {
            Ok(uri) => uri,
            Err(err) => {
                restore_promotion_snapshots(session, &snapshots)?;
                return Err(err);
            }
        };
        if !persisted_uris.iter().any(|item| item == &uri) {
            persisted_uris.push(uri);
        }
    }

    if let Err(reindex_err) = reindex_memory_uris_write_path(session, &persisted_uris) {
        record_dedup_fallback(session, "promotion_reindex", &reindex_err.to_string());
        let rollback_err = restore_promotion_snapshots(session, &snapshots).err();
        let rollback_reindex_err = if rollback_err.is_none() {
            reindex_memory_uris_write_path(session, &persisted_uris).err()
        } else {
            None
        };
        let rollback_status = rollback_err
            .as_ref()
            .map_or_else(|| "ok".to_string(), |err| format!("err:{err}"));
        let rollback_reindex_status = rollback_reindex_err
            .as_ref()
            .map_or_else(|| "ok_or_skipped".to_string(), |err| format!("err:{err}"));
        return Err(AxiomError::Internal(format!(
            "promotion all_or_nothing reindex failed: {reindex_err}; rollback={rollback_status}; rollback_reindex={rollback_reindex_status}",
        )));
    }

    Ok(MemoryPromotionResult {
        session_id: session.session_id.clone(),
        checkpoint_id: checkpoint_id.to_string(),
        accepted: facts.len(),
        persisted: plan.candidates.len(),
        skipped_duplicates: plan.skipped_duplicates,
        rejected: 0,
    })
}

pub(super) fn apply_promotion_best_effort(
    session: &Session,
    checkpoint_id: &str,
    facts: &[MemoryPromotionFact],
    record_dedup_fallback: RecordDedupFallbackFn,
) -> Result<MemoryPromotionResult> {
    let mut rejected = 0usize;
    let mut valid = Vec::<MemoryPromotionFact>::new();
    for fact in facts {
        if validate_promotion_fact_semantics(fact).is_ok() {
            valid.push(fact.clone());
        } else {
            rejected = rejected.saturating_add(1);
        }
    }

    let existing = list_existing_promotion_facts(session)?;
    let plan = plan_promotion_apply(&existing, &valid);

    let mut persisted = 0usize;
    let mut persisted_uris = Vec::<AxiomUri>::new();
    let mut snapshots = BTreeMap::<String, Option<String>>::new();
    for candidate in &plan.candidates {
        match persist_promotion_candidate_write_path(session, candidate, Some(&mut snapshots)) {
            Ok(uri) => {
                if !persisted_uris.iter().any(|item| item == &uri) {
                    persisted_uris.push(uri);
                }
                persisted = persisted.saturating_add(1);
            }
            Err(_) => {
                rejected = rejected.saturating_add(1);
            }
        }
    }
    if let Err(reindex_err) = reindex_memory_uris_write_path(session, &persisted_uris) {
        record_dedup_fallback(session, "promotion_reindex", &reindex_err.to_string());
        let rollback_err = restore_promotion_snapshots(session, &snapshots).err();
        let rollback_reindex_err = if rollback_err.is_none() {
            reindex_memory_uris_write_path(session, &persisted_uris).err()
        } else {
            None
        };
        let rollback_status = rollback_err
            .as_ref()
            .map_or_else(|| "ok".to_string(), |err| format!("err:{err}"));
        let rollback_reindex_status = rollback_reindex_err
            .as_ref()
            .map_or_else(|| "ok_or_skipped".to_string(), |err| format!("err:{err}"));
        return Err(AxiomError::Internal(format!(
            "promotion best_effort reindex failed: {reindex_err}; rollback={rollback_status}; rollback_reindex={rollback_reindex_status}",
        )));
    }

    Ok(MemoryPromotionResult {
        session_id: session.session_id.clone(),
        checkpoint_id: checkpoint_id.to_string(),
        accepted: valid.len(),
        persisted,
        skipped_duplicates: plan.skipped_duplicates,
        rejected,
    })
}
