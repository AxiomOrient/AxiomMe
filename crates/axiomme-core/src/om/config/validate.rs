use thiserror::Error;

use super::input::BufferTokensInput;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OmConfigError {
    #[error("invalid observation.message_tokens: must be > 0")]
    InvalidObservationMessageTokens,
    #[error("invalid reflection.observation_tokens: must be > 0")]
    InvalidReflectionObservationTokens,
    #[error("invalid observation.bufferTokens ratio: must be in (0, 1)")]
    InvalidObservationBufferTokensRatio,
    #[error("invalid observation.bufferTokens absolute: must be > 0")]
    InvalidObservationBufferTokensAbsolute,
    #[error("observation.bufferTokens must resolve below message threshold")]
    ObservationBufferTokensAtOrAboveThreshold,
    #[error("invalid observation.bufferActivation: must be in (0, 1]")]
    InvalidObservationBufferActivation,
    #[error("invalid reflection.bufferActivation: must be in (0, 1]")]
    InvalidReflectionBufferActivation,
    #[error("invalid observation.blockAfter: must resolve >= message threshold")]
    InvalidObservationBlockAfter,
    #[error("invalid reflection.blockAfter: must resolve >= reflection threshold")]
    InvalidReflectionBlockAfter,
    #[error("invalid observation.maxTokensPerBatch: must be > 0")]
    InvalidObservationMaxTokensPerBatch,
    #[error(
        "resource scope does not support async buffering; disable via observation.bufferTokens=Disabled"
    )]
    ResourceScopeAsyncBufferingUnsupported,
    #[error(
        "shareTokenBudget requires async buffering disabled via observation.bufferTokens=Disabled"
    )]
    ShareTokenBudgetRequiresAsyncDisabled,
}

pub(super) const fn validate_observation_message_tokens(value: u32) -> Result<u32, OmConfigError> {
    if value == 0 {
        Err(OmConfigError::InvalidObservationMessageTokens)
    } else {
        Ok(value)
    }
}

pub(super) const fn validate_reflection_observation_tokens(
    value: u32,
) -> Result<u32, OmConfigError> {
    if value == 0 {
        Err(OmConfigError::InvalidReflectionObservationTokens)
    } else {
        Ok(value)
    }
}

pub(super) const fn validate_observation_max_tokens_per_batch(
    value: u32,
) -> Result<u32, OmConfigError> {
    if value == 0 {
        Err(OmConfigError::InvalidObservationMaxTokensPerBatch)
    } else {
        Ok(value)
    }
}

pub(super) fn resolve_buffer_tokens(
    raw: BufferTokensInput,
    message_tokens: u32,
) -> Result<u32, OmConfigError> {
    match raw {
        BufferTokensInput::Disabled => Ok(0),
        BufferTokensInput::Absolute(value) => {
            if value == 0 {
                return Err(OmConfigError::InvalidObservationBufferTokensAbsolute);
            }
            Ok(value)
        }
        BufferTokensInput::Ratio(value) => {
            if !(value > 0.0 && value < 1.0) {
                return Err(OmConfigError::InvalidObservationBufferTokensRatio);
            }
            let computed = round_f64_to_u32(f64::from(message_tokens) * value)
                .ok_or(OmConfigError::InvalidObservationBufferTokensAbsolute)?;
            if computed == 0 {
                return Err(OmConfigError::InvalidObservationBufferTokensAbsolute);
            }
            Ok(computed)
        }
    }
}

pub(super) fn resolve_block_after(
    raw: f32,
    threshold: u32,
    invalid_error: OmConfigError,
) -> Result<u32, OmConfigError> {
    if !raw.is_finite() {
        return Err(invalid_error);
    }
    if (1.0..2.0).contains(&raw) {
        return round_f64_to_u32(f64::from(threshold) * f64::from(raw)).ok_or(invalid_error);
    }
    if raw >= 2.0 {
        return round_f64_to_u32(f64::from(raw)).ok_or(invalid_error);
    }
    Err(invalid_error)
}

pub(super) fn validate_observation_activation(value: Option<f32>) -> Result<(), OmConfigError> {
    if value.is_some_and(|activation| !(activation > 0.0 && activation <= 1.0)) {
        return Err(OmConfigError::InvalidObservationBufferActivation);
    }
    Ok(())
}

pub(super) fn validate_reflection_activation(value: Option<f32>) -> Result<(), OmConfigError> {
    if value.is_some_and(|activation| !(activation > 0.0 && activation <= 1.0)) {
        return Err(OmConfigError::InvalidReflectionBufferActivation);
    }
    Ok(())
}

fn round_f64_to_u32(value: f64) -> Option<u32> {
    if !value.is_finite() {
        return None;
    }
    let rounded = value.round();
    if !(0.0..=f64::from(u32::MAX)).contains(&rounded) {
        return None;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "value is validated to be finite and within u32 bounds before conversion"
    )]
    {
        Some(rounded as u32)
    }
}
