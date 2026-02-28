use chrono::{Duration, Utc};

use crate::error::{AxiomError, Result};
use crate::models::{
    MemoryPromotionFact, MemoryPromotionRequest, MemoryPromotionResult, PromotionApplyMode,
};
use crate::state::PromotionCheckpointPhase;

use super::Session;
use super::promotion::{
    promotion_apply_input_from_checkpoint_json, promotion_apply_input_from_request,
};
use super::types::PROMOTION_APPLYING_STALE_SECONDS;

type PromotionApplyFn = fn(&Session, &str, &[MemoryPromotionFact]) -> Result<MemoryPromotionResult>;

pub(super) fn promote_memories(
    session: &Session,
    request: &MemoryPromotionRequest,
    apply_all_or_nothing: PromotionApplyFn,
    apply_best_effort: PromotionApplyFn,
) -> Result<MemoryPromotionResult> {
    if request.session_id.trim() != session.session_id {
        return Err(AxiomError::Validation(format!(
            "promotion session_id mismatch: expected {}, got {}",
            session.session_id, request.session_id
        )));
    }
    if request.checkpoint_id.trim().is_empty() {
        return Err(AxiomError::Validation(
            "checkpoint_id must not be empty".to_string(),
        ));
    }

    let mut apply_input = promotion_apply_input_from_request(request)?;
    let incoming_request_hash = apply_input.request_hash.clone();

    let stale_before =
        (Utc::now() - Duration::seconds(PROMOTION_APPLYING_STALE_SECONDS)).to_rfc3339();
    let _ = session.state.demote_stale_promotion_checkpoint(
        session.session_id.as_str(),
        request.checkpoint_id.as_str(),
        stale_before.as_str(),
    )?;

    if let Some(existing) = session
        .state
        .get_promotion_checkpoint(session.session_id.as_str(), request.checkpoint_id.as_str())?
    {
        if existing.request_hash != incoming_request_hash {
            return Err(AxiomError::Validation(
                "checkpoint_id conflict: request hash mismatch".to_string(),
            ));
        }
        match existing.phase {
            PromotionCheckpointPhase::Applied => {
                let result_json = existing.result_json.ok_or_else(|| {
                    AxiomError::Internal("applied checkpoint missing result_json".to_string())
                })?;
                return Ok(serde_json::from_str(&result_json)?);
            }
            PromotionCheckpointPhase::Applying => {
                return Err(AxiomError::Conflict(
                    "checkpoint_busy: checkpoint is currently applying".to_string(),
                ));
            }
            PromotionCheckpointPhase::Pending => {
                let replay_input = promotion_apply_input_from_checkpoint_json(
                    existing.request_json.as_str(),
                    session.session_id.as_str(),
                    request.checkpoint_id.as_str(),
                )?;
                if replay_input.request_hash != existing.request_hash {
                    return Err(AxiomError::Internal(
                        "checkpoint request_json hash mismatch".to_string(),
                    ));
                }
                apply_input = replay_input;
            }
        }
    } else {
        session.state.insert_promotion_checkpoint_pending(
            session.session_id.as_str(),
            request.checkpoint_id.as_str(),
            apply_input.request_hash.as_str(),
            apply_input.request_json.as_str(),
        )?;
    }

    if !session.state.claim_promotion_checkpoint_applying(
        session.session_id.as_str(),
        request.checkpoint_id.as_str(),
        apply_input.request_hash.as_str(),
    )? {
        if let Some(current) = session
            .state
            .get_promotion_checkpoint(session.session_id.as_str(), request.checkpoint_id.as_str())?
        {
            if current.request_hash != apply_input.request_hash {
                return Err(AxiomError::Validation(
                    "checkpoint_id conflict: request hash mismatch".to_string(),
                ));
            }
            return match current.phase {
                PromotionCheckpointPhase::Applied => {
                    let result_json = current.result_json.ok_or_else(|| {
                        AxiomError::Internal("applied checkpoint missing result_json".to_string())
                    })?;
                    Ok(serde_json::from_str(&result_json)?)
                }
                PromotionCheckpointPhase::Applying | PromotionCheckpointPhase::Pending => Err(
                    AxiomError::Conflict("checkpoint_busy: checkpoint claim lost".to_string()),
                ),
            };
        }
        return Err(AxiomError::Internal(
            "checkpoint claim failed and checkpoint record missing".to_string(),
        ));
    }

    let applied = match apply_input.apply_mode {
        PromotionApplyMode::AllOrNothing => {
            apply_all_or_nothing(session, request.checkpoint_id.as_str(), &apply_input.facts)
        }
        PromotionApplyMode::BestEffort => {
            apply_best_effort(session, request.checkpoint_id.as_str(), &apply_input.facts)
        }
    };

    let result = match applied {
        Ok(result) => result,
        Err(err) => {
            let _ = session.state.set_promotion_checkpoint_pending(
                session.session_id.as_str(),
                request.checkpoint_id.as_str(),
                apply_input.request_hash.as_str(),
            );
            return Err(err);
        }
    };

    let result_json = serde_json::to_string(&result)?;
    if !session.state.finalize_promotion_checkpoint_applied(
        session.session_id.as_str(),
        request.checkpoint_id.as_str(),
        apply_input.request_hash.as_str(),
        result_json.as_str(),
    )? {
        return Err(AxiomError::Conflict(
            "checkpoint finalize failed".to_string(),
        ));
    }
    Ok(result)
}
