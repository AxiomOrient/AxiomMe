use crate::om::model::{OmObservationChunk, OmRecord};

use super::helpers::merge_activated_message_ids;
use super::{ActivationBoundary, ActivationResult};

#[must_use]
pub fn calculate_dynamic_threshold(
    base_threshold: u32,
    maybe_total_budget: Option<u32>,
    current_observation_tokens: u32,
) -> u32 {
    let Some(total_budget) = maybe_total_budget else {
        return base_threshold;
    };
    total_budget
        .saturating_sub(current_observation_tokens)
        .max(base_threshold)
}

#[must_use]
pub fn select_activation_boundary(
    chunks: &[OmObservationChunk],
    activation_ratio: f32,
    message_threshold: u32,
    current_pending_tokens: u32,
) -> ActivationBoundary {
    if chunks.is_empty() {
        return ActivationBoundary {
            chunks_activated: 0,
            message_tokens_activated: 0,
            observation_tokens_activated: 0,
            activated_message_ids: Vec::new(),
        };
    }

    let ratio = activation_ratio.clamp(0.0, 1.0);
    let retention_floor = f64::from(message_threshold) * f64::from(1.0 - ratio);
    let target_message_tokens =
        floor_f64_to_u64_clamped((f64::from(current_pending_tokens) - retention_floor).max(0.0));

    let mut cumulative_message_tokens: u64 = 0;
    let mut best_boundary = 0usize;
    let mut best_boundary_message_tokens: u64 = 0;

    for (idx, chunk) in chunks.iter().enumerate() {
        cumulative_message_tokens =
            cumulative_message_tokens.saturating_add(u64::from(chunk.message_tokens));
        let boundary = idx + 1;

        let is_under = cumulative_message_tokens <= target_message_tokens;
        let best_is_under = best_boundary_message_tokens <= target_message_tokens;

        let should_replace = if best_boundary == 0 {
            true
        } else if is_under {
            !best_is_under || cumulative_message_tokens > best_boundary_message_tokens
        } else {
            !best_is_under && cumulative_message_tokens < best_boundary_message_tokens
        };
        if should_replace {
            best_boundary = boundary;
            best_boundary_message_tokens = cumulative_message_tokens;
        }
    }

    let chunks_to_activate = if best_boundary == 0 { 1 } else { best_boundary };
    let activated_chunks = &chunks[..chunks_to_activate];

    let message_tokens_activated =
        saturating_u64_to_u32(activated_chunks.iter().fold(0u64, |sum, c| {
            sum.saturating_add(u64::from(c.message_tokens))
        }));
    let observation_tokens_activated = activated_chunks
        .iter()
        .fold(0u64, |sum, c| sum.saturating_add(u64::from(c.token_count)));
    let observation_tokens_activated = saturating_u64_to_u32(observation_tokens_activated);
    let activated_message_ids = activated_chunks
        .iter()
        .flat_map(|chunk| chunk.message_ids.iter().cloned())
        .collect::<Vec<_>>();

    ActivationBoundary {
        chunks_activated: activated_chunks.len(),
        message_tokens_activated,
        observation_tokens_activated,
        activated_message_ids,
    }
}

#[must_use]
pub fn merge_activated_observations(
    active: &str,
    activated_chunks: &[OmObservationChunk],
) -> String {
    let mut parts = Vec::<String>::new();

    if !active.trim().is_empty() {
        parts.push(active.trim().to_string());
    }

    for chunk in activated_chunks {
        let text = chunk.observations.trim();
        if !text.is_empty() {
            parts.push(text.to_string());
        }
    }

    parts.join("\n\n")
}

#[must_use]
pub fn normalize_observation_buffer_boundary(
    current_tokens: u32,
    last_buffered_at_tokens: u32,
) -> u32 {
    last_buffered_at_tokens.min(current_tokens)
}

pub fn activate_buffered_observations(
    record: &mut OmRecord,
    buffered_chunks: &mut Vec<OmObservationChunk>,
    activation_ratio: f32,
    threshold: u32,
) -> Option<ActivationResult> {
    if buffered_chunks.is_empty() {
        return None;
    }

    let boundary = select_activation_boundary(
        buffered_chunks,
        activation_ratio,
        threshold,
        record.pending_message_tokens,
    );
    if boundary.chunks_activated == 0 {
        return None;
    }

    let activated_max_seq = {
        let activated_chunks = &buffered_chunks[..boundary.chunks_activated];
        record.active_observations =
            merge_activated_observations(&record.active_observations, activated_chunks);
        record.observation_token_count = record
            .observation_token_count
            .saturating_add(boundary.observation_tokens_activated);
        record.pending_message_tokens = record
            .pending_message_tokens
            .saturating_sub(boundary.message_tokens_activated);
        record.last_observed_at = activated_chunks.last().map(|chunk| chunk.last_observed_at);
        record.last_activated_message_ids = merge_activated_message_ids(
            &record.last_activated_message_ids,
            &boundary.activated_message_ids,
        );
        activated_chunks
            .last()
            .map(|chunk| chunk.seq)
            .unwrap_or_default()
    };

    buffered_chunks.drain(..boundary.chunks_activated);
    record.is_buffering_observation = !buffered_chunks.is_empty();
    if !record.is_buffering_observation {
        record.last_buffered_at_tokens = 0;
        record.last_buffered_at_time = None;
    }

    Some(ActivationResult {
        activated_max_seq,
        chunks_activated: boundary.chunks_activated,
        message_tokens_activated: boundary.message_tokens_activated,
        observation_tokens_activated: boundary.observation_tokens_activated,
        activated_message_ids: boundary.activated_message_ids,
    })
}

fn floor_f64_to_u64_clamped(value: f64) -> u64 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    let value = value.floor();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "activation input is non-negative finite value and this path intentionally floors to integer tokens"
    )]
    {
        value as u64
    }
}

fn saturating_u64_to_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
