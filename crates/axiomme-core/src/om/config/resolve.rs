use crate::om::{OmScope, calculate_dynamic_threshold};

use super::input::{BufferTokensInput, OmConfigInput};
use super::validate::{
    OmConfigError, resolve_block_after, resolve_buffer_tokens, validate_observation_activation,
    validate_observation_max_tokens_per_batch, validate_observation_message_tokens,
    validate_reflection_activation, validate_reflection_observation_tokens,
};
use super::{
    DEFAULT_BLOCK_AFTER_MULTIPLIER, DEFAULT_OBSERVER_BUFFER_ACTIVATION,
    DEFAULT_OBSERVER_BUFFER_TOKENS_RATIO, DEFAULT_OBSERVER_MAX_TOKENS_PER_BATCH,
    DEFAULT_OBSERVER_MESSAGE_TOKENS, DEFAULT_REFLECTOR_BUFFER_ACTIVATION,
    DEFAULT_REFLECTOR_OBSERVATION_TOKENS,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedObservationConfig {
    pub message_tokens_base: u32,
    pub total_budget: Option<u32>,
    pub max_tokens_per_batch: u32,
    pub buffer_tokens: Option<u32>,
    pub buffer_activation: Option<f32>,
    pub block_after: Option<u32>,
}

impl ResolvedObservationConfig {
    #[must_use]
    pub fn dynamic_threshold(&self, current_observation_tokens: u32) -> u32 {
        calculate_dynamic_threshold(
            self.message_tokens_base,
            self.total_budget,
            current_observation_tokens,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedReflectionConfig {
    pub observation_tokens: u32,
    pub buffer_activation: Option<f32>,
    pub block_after: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedOmConfig {
    pub scope: OmScope,
    pub share_token_budget: bool,
    pub async_buffering_disabled: bool,
    pub observation: ResolvedObservationConfig,
    pub reflection: ResolvedReflectionConfig,
}

fn resolve_async_buffering_disabled(input: OmConfigInput) -> Result<bool, OmConfigError> {
    let user_explicitly_configured_async = input.observation.buffer_tokens.is_some()
        || input.observation.buffer_activation.is_some()
        || input.reflection.buffer_activation.is_some();
    let async_buffering_disabled = matches!(
        input.observation.buffer_tokens,
        Some(BufferTokensInput::Disabled)
    ) || (input.scope == OmScope::Resource
        && !user_explicitly_configured_async);
    if input.scope == OmScope::Resource && !async_buffering_disabled {
        return Err(OmConfigError::ResourceScopeAsyncBufferingUnsupported);
    }
    if input.share_token_budget && !async_buffering_disabled {
        return Err(OmConfigError::ShareTokenBudgetRequiresAsyncDisabled);
    }
    Ok(async_buffering_disabled)
}

fn resolve_observation_buffer_tokens(
    input: OmConfigInput,
    async_buffering_disabled: bool,
    message_tokens_base: u32,
) -> Result<Option<u32>, OmConfigError> {
    if async_buffering_disabled {
        return Ok(None);
    }
    let raw = input
        .observation
        .buffer_tokens
        .unwrap_or(BufferTokensInput::Ratio(
            DEFAULT_OBSERVER_BUFFER_TOKENS_RATIO,
        ));
    let resolved = resolve_buffer_tokens(raw, message_tokens_base)?;
    if resolved >= message_tokens_base {
        return Err(OmConfigError::ObservationBufferTokensAtOrAboveThreshold);
    }
    Ok(Some(resolved))
}

fn resolve_observation_block_after_threshold(
    input: OmConfigInput,
    async_buffering_disabled: bool,
    message_tokens_base: u32,
) -> Result<Option<u32>, OmConfigError> {
    if async_buffering_disabled {
        return Ok(None);
    }
    let raw = input
        .observation
        .block_after
        .or(Some(DEFAULT_BLOCK_AFTER_MULTIPLIER));
    let block_after = raw
        .map(|value| {
            resolve_block_after(
                value,
                message_tokens_base,
                OmConfigError::InvalidObservationBlockAfter,
            )
        })
        .transpose()?;
    if block_after.is_some_and(|value| value < message_tokens_base) {
        return Err(OmConfigError::InvalidObservationBlockAfter);
    }
    Ok(block_after)
}

fn resolve_observation_activation_threshold(
    input: OmConfigInput,
    async_buffering_disabled: bool,
) -> Result<Option<f32>, OmConfigError> {
    let activation = if async_buffering_disabled {
        None
    } else {
        Some(
            input
                .observation
                .buffer_activation
                .unwrap_or(DEFAULT_OBSERVER_BUFFER_ACTIVATION),
        )
    };
    validate_observation_activation(activation)?;
    Ok(activation)
}

fn resolve_reflection_activation_threshold(
    input: OmConfigInput,
    async_buffering_disabled: bool,
) -> Result<Option<f32>, OmConfigError> {
    let activation = if async_buffering_disabled {
        None
    } else {
        Some(
            input
                .reflection
                .buffer_activation
                .unwrap_or(DEFAULT_REFLECTOR_BUFFER_ACTIVATION),
        )
    };
    validate_reflection_activation(activation)?;
    Ok(activation)
}

fn resolve_reflection_block_after_threshold(
    input: OmConfigInput,
    async_buffering_disabled: bool,
    observation_tokens: u32,
) -> Result<Option<u32>, OmConfigError> {
    if async_buffering_disabled {
        return Ok(None);
    }
    let raw = input
        .reflection
        .block_after
        .or(Some(DEFAULT_BLOCK_AFTER_MULTIPLIER));
    let block_after = raw
        .map(|value| {
            resolve_block_after(
                value,
                observation_tokens,
                OmConfigError::InvalidReflectionBlockAfter,
            )
        })
        .transpose()?;
    if block_after.is_some_and(|value| value < observation_tokens) {
        return Err(OmConfigError::InvalidReflectionBlockAfter);
    }
    Ok(block_after)
}

pub fn resolve_om_config(input: OmConfigInput) -> Result<ResolvedOmConfig, OmConfigError> {
    let message_tokens_base = validate_observation_message_tokens(
        input
            .observation
            .message_tokens
            .unwrap_or(DEFAULT_OBSERVER_MESSAGE_TOKENS),
    )?;
    let observation_tokens = validate_reflection_observation_tokens(
        input
            .reflection
            .observation_tokens
            .unwrap_or(DEFAULT_REFLECTOR_OBSERVATION_TOKENS),
    )?;
    let async_buffering_disabled = resolve_async_buffering_disabled(input)?;

    let max_tokens_per_batch = validate_observation_max_tokens_per_batch(
        input
            .observation
            .max_tokens_per_batch
            .unwrap_or(DEFAULT_OBSERVER_MAX_TOKENS_PER_BATCH),
    )?;
    let resolved_observation_buffer_tokens =
        resolve_observation_buffer_tokens(input, async_buffering_disabled, message_tokens_base)?;
    let resolved_observation_block_after = resolve_observation_block_after_threshold(
        input,
        async_buffering_disabled,
        message_tokens_base,
    )?;
    let resolved_observation_activation =
        resolve_observation_activation_threshold(input, async_buffering_disabled)?;
    let resolved_reflection_activation =
        resolve_reflection_activation_threshold(input, async_buffering_disabled)?;
    let resolved_reflection_block_after = resolve_reflection_block_after_threshold(
        input,
        async_buffering_disabled,
        observation_tokens,
    )?;

    let total_budget = if input.share_token_budget {
        Some(message_tokens_base.saturating_add(observation_tokens))
    } else {
        None
    };

    Ok(ResolvedOmConfig {
        scope: input.scope,
        share_token_budget: input.share_token_budget,
        async_buffering_disabled,
        observation: ResolvedObservationConfig {
            message_tokens_base,
            total_budget,
            max_tokens_per_batch,
            buffer_tokens: resolved_observation_buffer_tokens,
            buffer_activation: resolved_observation_activation,
            block_after: resolved_observation_block_after,
        },
        reflection: ResolvedReflectionConfig {
            observation_tokens,
            buffer_activation: resolved_reflection_activation,
            block_after: resolved_reflection_block_after,
        },
    })
}
