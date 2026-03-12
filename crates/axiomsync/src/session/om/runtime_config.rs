use crate::config::OmRuntimeEnvConfig;
use crate::error::{AxiomError, Result};
use crate::llm_io::parse_env_bool;
use crate::om::{
    BufferTokensInput, ObservationConfigInput, OmConfigInput, OmScope, ReflectionConfigInput,
    ResolvedOmConfig, resolve_om_config,
};

use super::{
    ENV_OM_ACTIVATION_RATIO, ENV_OM_BUFFER_TOKENS, ENV_OM_MESSAGE_TOKENS,
    ENV_OM_OBSERVER_BLOCK_AFTER, ENV_OM_OBSERVER_MAX_TOKENS_PER_BATCH,
    ENV_OM_REFLECTOR_BLOCK_AFTER, ENV_OM_REFLECTOR_BUFFER_ACTIVATION,
    ENV_OM_REFLECTOR_OBSERVATION_TOKENS,
};

#[derive(Debug, Clone, Default)]
pub(super) struct RuntimeOmEnv {
    pub(super) message_tokens: Option<String>,
    pub(super) observer_max_tokens_per_batch: Option<String>,
    pub(super) reflector_observation_tokens: Option<String>,
    pub(super) activation_ratio: Option<String>,
    pub(super) share_token_budget: Option<String>,
    pub(super) buffer_tokens: Option<String>,
    pub(super) observer_block_after: Option<String>,
    pub(super) reflector_buffer_activation: Option<String>,
    pub(super) reflector_block_after: Option<String>,
}

pub(super) fn runtime_om_env_from_config(config: &OmRuntimeEnvConfig) -> RuntimeOmEnv {
    RuntimeOmEnv {
        message_tokens: config.message_tokens.clone(),
        observer_max_tokens_per_batch: config.observer_max_tokens_per_batch.clone(),
        reflector_observation_tokens: config.reflector_observation_tokens.clone(),
        activation_ratio: config.activation_ratio.clone(),
        share_token_budget: config.share_token_budget.clone(),
        buffer_tokens: config.buffer_tokens.clone(),
        observer_block_after: config.observer_block_after.clone(),
        reflector_buffer_activation: config.reflector_buffer_activation.clone(),
        reflector_block_after: config.reflector_block_after.clone(),
    }
}

pub(super) fn resolve_runtime_om_config(
    env: &RuntimeOmEnv,
    scope: OmScope,
) -> Result<ResolvedOmConfig> {
    let input = OmConfigInput {
        scope,
        share_token_budget: parse_env_bool(env.share_token_budget.as_deref()),
        observation: ObservationConfigInput {
            message_tokens: parse_env_u32_optional(
                env.message_tokens.as_deref(),
                ENV_OM_MESSAGE_TOKENS,
            )?,
            max_tokens_per_batch: parse_env_u32_optional(
                env.observer_max_tokens_per_batch.as_deref(),
                ENV_OM_OBSERVER_MAX_TOKENS_PER_BATCH,
            )?,
            buffer_tokens: parse_env_buffer_tokens_optional(env.buffer_tokens.as_deref())?,
            buffer_activation: parse_env_f32_optional(
                env.activation_ratio.as_deref(),
                ENV_OM_ACTIVATION_RATIO,
            )?,
            block_after: parse_env_f32_optional(
                env.observer_block_after.as_deref(),
                ENV_OM_OBSERVER_BLOCK_AFTER,
            )?,
        },
        reflection: ReflectionConfigInput {
            observation_tokens: parse_env_u32_optional(
                env.reflector_observation_tokens.as_deref(),
                ENV_OM_REFLECTOR_OBSERVATION_TOKENS,
            )?,
            buffer_activation: parse_env_f32_optional(
                env.reflector_buffer_activation.as_deref(),
                ENV_OM_REFLECTOR_BUFFER_ACTIVATION,
            )?,
            block_after: parse_env_f32_optional(
                env.reflector_block_after.as_deref(),
                ENV_OM_REFLECTOR_BLOCK_AFTER,
            )?,
        },
    };

    resolve_om_config(input)
        .map_err(|err| AxiomError::Validation(format!("invalid OM config: {err}")))
}

fn parse_env_u32_optional(raw: Option<&str>, env_name: &str) -> Result<Option<u32>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed = trimmed.parse::<u32>().map_err(|_| {
        AxiomError::Validation(format!(
            "{env_name} must be a positive integer, got: {trimmed}"
        ))
    })?;
    if parsed == 0 {
        return Err(AxiomError::Validation(format!(
            "{env_name} must be > 0, got: {trimmed}"
        )));
    }
    Ok(Some(parsed))
}

fn parse_env_f32_optional(raw: Option<&str>, env_name: &str) -> Result<Option<f32>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed = trimmed.parse::<f32>().map_err(|_| {
        AxiomError::Validation(format!("{env_name} must be a float, got: {trimmed}"))
    })?;
    if !parsed.is_finite() || parsed <= 0.0 {
        return Err(AxiomError::Validation(format!(
            "{env_name} must be > 0, got: {trimmed}"
        )));
    }
    Ok(Some(parsed))
}

fn parse_env_buffer_tokens_optional(raw: Option<&str>) -> Result<Option<BufferTokensInput>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let lowered = trimmed.to_ascii_lowercase();
    if matches!(
        lowered.as_str(),
        "false" | "off" | "disabled" | "none" | "no"
    ) {
        return Ok(Some(BufferTokensInput::Disabled));
    }

    if let Ok(parsed_int) = trimmed.parse::<u32>() {
        if parsed_int == 0 {
            return Ok(Some(BufferTokensInput::Disabled));
        }
        return Ok(Some(BufferTokensInput::Absolute(parsed_int)));
    }

    if let Ok(parsed_float) = trimmed.parse::<f64>() {
        if parsed_float == 0.0 {
            return Ok(Some(BufferTokensInput::Disabled));
        }
        if parsed_float.is_finite() && parsed_float > 0.0 && parsed_float < 1.0 {
            return Ok(Some(BufferTokensInput::Ratio(parsed_float)));
        }
    }

    Err(AxiomError::Validation(format!(
        "{ENV_OM_BUFFER_TOKENS} must be one of: false|off|disabled|<positive-int>|<ratio(0,1)>, got: {trimmed}"
    )))
}
