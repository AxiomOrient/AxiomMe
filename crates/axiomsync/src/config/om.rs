use crate::llm_io::parse_env_bool;

use super::env::{
    parse_enabled_default_true, read_env_f32, read_env_u16, read_env_u32, read_env_u64,
    read_env_usize, read_env_usize_optional, read_non_empty_env, read_raw_env,
};

const ENV_OM_ENABLED: &str = "AXIOMSYNC_OM_ENABLED";
const ENV_OM_HINT_READER: &str = "AXIOMSYNC_OM_HINT_READER";
const ENV_OM_SCOPE: &str = "AXIOMSYNC_OM_SCOPE";
const ENV_OM_SCOPE_THREAD_ID: &str = "AXIOMSYNC_OM_SCOPE_THREAD_ID";
const ENV_OM_SCOPE_RESOURCE_ID: &str = "AXIOMSYNC_OM_SCOPE_RESOURCE_ID";
const ENV_OM_OBSERVER_MAX_MESSAGES: &str = "AXIOMSYNC_OM_OBSERVER_MAX_MESSAGES";
const ENV_OM_RESOURCE_SCOPE_CROSS_SESSION_LIMIT: &str =
    "AXIOMSYNC_OM_RESOURCE_SCOPE_CROSS_SESSION_LIMIT";
const ENV_OM_OBSERVATION_MAX_CHARS: &str = "AXIOMSYNC_OM_OBSERVATION_MAX_CHARS";
const ENV_OM_OBSERVER_OTHER_CONVERSATION_MAX_PART_CHARS: &str =
    "AXIOMSYNC_OM_OBSERVER_OTHER_CONVERSATION_MAX_PART_CHARS";
const ENV_OM_OBSERVER_ACTIVE_OBSERVATIONS_MAX_CHARS: &str =
    "AXIOMSYNC_OM_OBSERVER_ACTIVE_OBSERVATIONS_MAX_CHARS";
const ENV_OM_ROLLOUT_PROFILE: &str = "AXIOMSYNC_OM_ROLLOUT_PROFILE";
const ENV_OM_OBSERVER_MODE: &str = "AXIOMSYNC_OM_OBSERVER_MODE";
const ENV_OM_OBSERVER_MODEL_ENABLED: &str = "AXIOMSYNC_OM_OBSERVER_MODEL_ENABLED";
const ENV_OM_OBSERVER_LLM_ENDPOINT: &str = "AXIOMSYNC_OM_OBSERVER_LLM_ENDPOINT";
const ENV_OM_OBSERVER_LLM_MODEL: &str = "AXIOMSYNC_OM_OBSERVER_LLM_MODEL";
const ENV_OM_OBSERVER_LLM_TIMEOUT_MS: &str = "AXIOMSYNC_OM_OBSERVER_LLM_TIMEOUT_MS";
const ENV_OM_OBSERVER_LLM_MAX_OUTPUT_TOKENS: &str = "AXIOMSYNC_OM_OBSERVER_LLM_MAX_OUTPUT_TOKENS";
const ENV_OM_OBSERVER_LLM_TEMPERATURE_MILLI: &str = "AXIOMSYNC_OM_OBSERVER_LLM_TEMPERATURE_MILLI";
const ENV_OM_OBSERVER_LLM_STRICT: &str = "AXIOMSYNC_OM_OBSERVER_LLM_STRICT";
const ENV_OM_OBSERVER_LLM_MAX_CHARS_PER_MESSAGE: &str =
    "AXIOMSYNC_OM_OBSERVER_LLM_MAX_CHARS_PER_MESSAGE";
const ENV_OM_OBSERVER_LLM_MAX_INPUT_TOKENS: &str = "AXIOMSYNC_OM_OBSERVER_LLM_MAX_INPUT_TOKENS";
const ENV_OM_REFLECTOR_MODE: &str = "AXIOMSYNC_OM_REFLECTOR_MODE";
const ENV_OM_REFLECTOR_MODEL_ENABLED: &str = "AXIOMSYNC_OM_REFLECTOR_MODEL_ENABLED";
const ENV_OM_REFLECTOR_LLM_ENDPOINT: &str = "AXIOMSYNC_OM_REFLECTOR_LLM_ENDPOINT";
const ENV_OM_REFLECTOR_LLM_MODEL: &str = "AXIOMSYNC_OM_REFLECTOR_LLM_MODEL";
const ENV_OM_REFLECTOR_LLM_TIMEOUT_MS: &str = "AXIOMSYNC_OM_REFLECTOR_LLM_TIMEOUT_MS";
const ENV_OM_REFLECTOR_LLM_MAX_OUTPUT_TOKENS: &str = "AXIOMSYNC_OM_REFLECTOR_LLM_MAX_OUTPUT_TOKENS";
const ENV_OM_REFLECTOR_LLM_TEMPERATURE_MILLI: &str = "AXIOMSYNC_OM_REFLECTOR_LLM_TEMPERATURE_MILLI";
const ENV_OM_REFLECTOR_LLM_STRICT: &str = "AXIOMSYNC_OM_REFLECTOR_LLM_STRICT";
const ENV_OM_REFLECTOR_OBSERVATION_TOKENS: &str = "AXIOMSYNC_OM_REFLECTOR_OBSERVATION_TOKENS";
const ENV_OM_REFLECTOR_BUFFER_ACTIVATION: &str = "AXIOMSYNC_OM_REFLECTOR_BUFFER_ACTIVATION";
const ENV_OM_REFLECTOR_MAX_CHARS: &str = "AXIOMSYNC_OM_REFLECTOR_MAX_CHARS";
const ENV_OM_MESSAGE_TOKENS: &str = "AXIOMSYNC_OM_MESSAGE_TOKENS";
const ENV_OM_OBSERVER_MAX_TOKENS_PER_BATCH: &str = "AXIOMSYNC_OM_OBSERVER_MAX_TOKENS_PER_BATCH";
const ENV_OM_OBSERVER_LLM_MAX_TOKENS_PER_BATCH_LEGACY: &str =
    "AXIOMSYNC_OM_OBSERVER_LLM_MAX_TOKENS_PER_BATCH";
const ENV_OM_ACTIVATION_RATIO: &str = "AXIOMSYNC_OM_ACTIVATION_RATIO";
const ENV_OM_SHARE_TOKEN_BUDGET: &str = "AXIOMSYNC_OM_SHARE_TOKEN_BUDGET";
const ENV_OM_BUFFER_TOKENS: &str = "AXIOMSYNC_OM_BUFFER_TOKENS";
const ENV_OM_OBSERVER_BLOCK_AFTER: &str = "AXIOMSYNC_OM_OBSERVER_BLOCK_AFTER";
const ENV_OM_REFLECTOR_BLOCK_AFTER: &str = "AXIOMSYNC_OM_REFLECTOR_BLOCK_AFTER";

const DEFAULT_OM_OBSERVER_MAX_MESSAGES: usize = 8;
const DEFAULT_OM_RESOURCE_SCOPE_CROSS_SESSION_LIMIT: usize = 4;
const DEFAULT_OM_OBSERVATION_MAX_CHARS: usize = 4_000;
const DEFAULT_OM_OBSERVER_OTHER_CONVERSATION_MAX_PART_CHARS: usize = 500;
const DEFAULT_OM_REFLECTOR_MAX_CHARS: usize = 1_200;
const DEFAULT_OM_HINT_READER: &str = "snapshot_v2";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OmHintReaderMode {
    None,
    SnapshotV2,
}

impl OmHintReaderMode {
    #[must_use]
    fn from_env(raw: Option<&str>) -> Self {
        match raw
            .unwrap_or(DEFAULT_OM_HINT_READER)
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "none" | "off" | "false" | "0" | "disabled" => Self::None,
            "snapshot_v2" | "v2" | "on" | "true" | "1" | "enabled" => Self::SnapshotV2,
            _ => Self::SnapshotV2,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OmConfig {
    pub(crate) enabled: bool,
    pub(crate) hint_reader: OmHintReaderMode,
    pub(crate) scope: OmScopeConfig,
    pub(crate) limits: OmRuntimeLimitsConfig,
    pub(crate) runtime_env: OmRuntimeEnvConfig,
    pub(crate) observer: OmObserverConfigSnapshot,
    pub(crate) reflector: OmReflectorConfigSnapshot,
}

impl OmConfig {
    #[must_use]
    pub(super) fn from_env() -> Self {
        Self {
            enabled: parse_enabled_default_true(std::env::var(ENV_OM_ENABLED).ok().as_deref()),
            hint_reader: OmHintReaderMode::from_env(
                std::env::var(ENV_OM_HINT_READER).ok().as_deref(),
            ),
            scope: OmScopeConfig::from_env(),
            limits: OmRuntimeLimitsConfig::from_env(),
            runtime_env: OmRuntimeEnvConfig::from_env(),
            observer: OmObserverConfigSnapshot::from_env(),
            reflector: OmReflectorConfigSnapshot::from_env(),
        }
    }
}

impl Default for OmConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            hint_reader: OmHintReaderMode::SnapshotV2,
            scope: OmScopeConfig::default(),
            limits: OmRuntimeLimitsConfig::default(),
            runtime_env: OmRuntimeEnvConfig::default(),
            observer: OmObserverConfigSnapshot::default(),
            reflector: OmReflectorConfigSnapshot::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct OmScopeConfig {
    pub(crate) scope: Option<String>,
    pub(crate) thread_id: Option<String>,
    pub(crate) resource_id: Option<String>,
}

impl OmScopeConfig {
    #[must_use]
    fn from_env() -> Self {
        Self {
            scope: read_non_empty_env(ENV_OM_SCOPE),
            thread_id: read_non_empty_env(ENV_OM_SCOPE_THREAD_ID),
            resource_id: read_non_empty_env(ENV_OM_SCOPE_RESOURCE_ID),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct OmRuntimeLimitsConfig {
    pub(crate) observer_max_messages: usize,
    pub(crate) resource_scope_cross_session_limit: usize,
    pub(crate) observation_max_chars: usize,
    pub(crate) observer_other_conversation_max_part_chars: usize,
    pub(crate) observer_active_observations_max_chars: usize,
}

impl OmRuntimeLimitsConfig {
    #[must_use]
    fn from_env() -> Self {
        let observation_max_chars = read_env_usize(
            ENV_OM_OBSERVATION_MAX_CHARS,
            DEFAULT_OM_OBSERVATION_MAX_CHARS,
            1,
        );
        Self {
            observer_max_messages: read_env_usize(
                ENV_OM_OBSERVER_MAX_MESSAGES,
                DEFAULT_OM_OBSERVER_MAX_MESSAGES,
                1,
            ),
            resource_scope_cross_session_limit: read_env_usize(
                ENV_OM_RESOURCE_SCOPE_CROSS_SESSION_LIMIT,
                DEFAULT_OM_RESOURCE_SCOPE_CROSS_SESSION_LIMIT,
                1,
            ),
            observation_max_chars,
            observer_other_conversation_max_part_chars: read_env_usize(
                ENV_OM_OBSERVER_OTHER_CONVERSATION_MAX_PART_CHARS,
                DEFAULT_OM_OBSERVER_OTHER_CONVERSATION_MAX_PART_CHARS,
                1,
            ),
            observer_active_observations_max_chars: read_env_usize(
                ENV_OM_OBSERVER_ACTIVE_OBSERVATIONS_MAX_CHARS,
                observation_max_chars.saturating_mul(2),
                1,
            ),
        }
    }
}

impl Default for OmRuntimeLimitsConfig {
    fn default() -> Self {
        Self {
            observer_max_messages: DEFAULT_OM_OBSERVER_MAX_MESSAGES,
            resource_scope_cross_session_limit: DEFAULT_OM_RESOURCE_SCOPE_CROSS_SESSION_LIMIT,
            observation_max_chars: DEFAULT_OM_OBSERVATION_MAX_CHARS,
            observer_other_conversation_max_part_chars:
                DEFAULT_OM_OBSERVER_OTHER_CONVERSATION_MAX_PART_CHARS,
            observer_active_observations_max_chars: DEFAULT_OM_OBSERVATION_MAX_CHARS
                .saturating_mul(2),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct OmRuntimeEnvConfig {
    pub(crate) message_tokens: Option<String>,
    pub(crate) observer_max_tokens_per_batch: Option<String>,
    pub(crate) reflector_observation_tokens: Option<String>,
    pub(crate) activation_ratio: Option<String>,
    pub(crate) share_token_budget: Option<String>,
    pub(crate) buffer_tokens: Option<String>,
    pub(crate) observer_block_after: Option<String>,
    pub(crate) reflector_buffer_activation: Option<String>,
    pub(crate) reflector_block_after: Option<String>,
}

impl OmRuntimeEnvConfig {
    #[must_use]
    fn from_env() -> Self {
        Self {
            message_tokens: read_raw_env(ENV_OM_MESSAGE_TOKENS),
            observer_max_tokens_per_batch: read_raw_env(ENV_OM_OBSERVER_MAX_TOKENS_PER_BATCH)
                .or_else(|| read_raw_env(ENV_OM_OBSERVER_LLM_MAX_TOKENS_PER_BATCH_LEGACY)),
            reflector_observation_tokens: read_raw_env(ENV_OM_REFLECTOR_OBSERVATION_TOKENS),
            activation_ratio: read_raw_env(ENV_OM_ACTIVATION_RATIO),
            share_token_budget: read_raw_env(ENV_OM_SHARE_TOKEN_BUDGET),
            buffer_tokens: read_raw_env(ENV_OM_BUFFER_TOKENS),
            observer_block_after: read_raw_env(ENV_OM_OBSERVER_BLOCK_AFTER),
            reflector_buffer_activation: read_raw_env(ENV_OM_REFLECTOR_BUFFER_ACTIVATION),
            reflector_block_after: read_raw_env(ENV_OM_REFLECTOR_BLOCK_AFTER),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct OmObserverConfigSnapshot {
    pub(crate) mode: Option<String>,
    pub(crate) explicit_model_enabled: bool,
    pub(crate) rollout_profile: Option<String>,
    pub(crate) llm_endpoint: Option<String>,
    pub(crate) llm_model: Option<String>,
    pub(crate) llm_timeout_ms: Option<u64>,
    pub(crate) llm_max_output_tokens: Option<u32>,
    pub(crate) llm_temperature_milli: Option<u16>,
    pub(crate) llm_strict: bool,
    pub(crate) llm_max_chars_per_message: Option<usize>,
    pub(crate) llm_max_input_tokens: Option<u32>,
}

impl OmObserverConfigSnapshot {
    #[must_use]
    fn from_env() -> Self {
        Self {
            mode: read_non_empty_env(ENV_OM_OBSERVER_MODE),
            explicit_model_enabled: parse_env_bool(
                std::env::var(ENV_OM_OBSERVER_MODEL_ENABLED).ok().as_deref(),
            ),
            rollout_profile: read_non_empty_env(ENV_OM_ROLLOUT_PROFILE),
            llm_endpoint: read_non_empty_env(ENV_OM_OBSERVER_LLM_ENDPOINT),
            llm_model: read_non_empty_env(ENV_OM_OBSERVER_LLM_MODEL),
            llm_timeout_ms: read_env_u64(ENV_OM_OBSERVER_LLM_TIMEOUT_MS)
                .filter(|value| *value >= 200),
            llm_max_output_tokens: read_env_u32(ENV_OM_OBSERVER_LLM_MAX_OUTPUT_TOKENS)
                .filter(|value| *value > 0),
            llm_temperature_milli: read_env_u16(ENV_OM_OBSERVER_LLM_TEMPERATURE_MILLI),
            llm_strict: parse_env_bool(std::env::var(ENV_OM_OBSERVER_LLM_STRICT).ok().as_deref()),
            llm_max_chars_per_message: read_env_usize_optional(
                ENV_OM_OBSERVER_LLM_MAX_CHARS_PER_MESSAGE,
            )
            .filter(|value| *value > 0),
            llm_max_input_tokens: read_env_u32(ENV_OM_OBSERVER_LLM_MAX_INPUT_TOKENS)
                .filter(|value| *value > 0),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OmReflectorConfigSnapshot {
    pub(crate) mode: Option<String>,
    pub(crate) explicit_model_enabled: bool,
    pub(crate) rollout_profile: Option<String>,
    pub(crate) llm_endpoint: Option<String>,
    pub(crate) llm_model: Option<String>,
    pub(crate) llm_timeout_ms: Option<u64>,
    pub(crate) llm_max_output_tokens: Option<u32>,
    pub(crate) llm_temperature_milli: Option<u16>,
    pub(crate) llm_strict: bool,
    pub(crate) llm_target_observation_tokens: Option<u32>,
    pub(crate) llm_buffer_activation: Option<f32>,
    pub(crate) max_chars: usize,
}

impl OmReflectorConfigSnapshot {
    #[must_use]
    fn from_env() -> Self {
        Self {
            mode: read_non_empty_env(ENV_OM_REFLECTOR_MODE),
            explicit_model_enabled: parse_env_bool(
                std::env::var(ENV_OM_REFLECTOR_MODEL_ENABLED)
                    .ok()
                    .as_deref(),
            ),
            rollout_profile: read_non_empty_env(ENV_OM_ROLLOUT_PROFILE),
            llm_endpoint: read_non_empty_env(ENV_OM_REFLECTOR_LLM_ENDPOINT),
            llm_model: read_non_empty_env(ENV_OM_REFLECTOR_LLM_MODEL),
            llm_timeout_ms: read_env_u64(ENV_OM_REFLECTOR_LLM_TIMEOUT_MS)
                .filter(|value| *value >= 200),
            llm_max_output_tokens: read_env_u32(ENV_OM_REFLECTOR_LLM_MAX_OUTPUT_TOKENS)
                .filter(|value| *value > 0),
            llm_temperature_milli: read_env_u16(ENV_OM_REFLECTOR_LLM_TEMPERATURE_MILLI),
            llm_strict: parse_env_bool(std::env::var(ENV_OM_REFLECTOR_LLM_STRICT).ok().as_deref()),
            llm_target_observation_tokens: read_env_u32(ENV_OM_REFLECTOR_OBSERVATION_TOKENS)
                .filter(|value| *value > 0),
            llm_buffer_activation: read_env_f32(ENV_OM_REFLECTOR_BUFFER_ACTIVATION)
                .filter(|value| *value > 0.0 && *value <= 1.0),
            max_chars: read_env_usize(
                ENV_OM_REFLECTOR_MAX_CHARS,
                DEFAULT_OM_REFLECTOR_MAX_CHARS,
                1,
            ),
        }
    }
}

impl Default for OmReflectorConfigSnapshot {
    fn default() -> Self {
        Self {
            mode: None,
            explicit_model_enabled: false,
            rollout_profile: None,
            llm_endpoint: None,
            llm_model: None,
            llm_timeout_ms: None,
            llm_max_output_tokens: None,
            llm_temperature_milli: None,
            llm_strict: false,
            llm_target_observation_tokens: None,
            llm_buffer_activation: None,
            max_chars: DEFAULT_OM_REFLECTOR_MAX_CHARS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OmHintReaderMode;

    #[test]
    fn om_hint_reader_mode_defaults_to_snapshot_v2() {
        assert_eq!(
            OmHintReaderMode::from_env(None),
            OmHintReaderMode::SnapshotV2
        );
        assert_eq!(
            OmHintReaderMode::from_env(Some("unknown")),
            OmHintReaderMode::SnapshotV2
        );
    }

    #[test]
    fn om_hint_reader_mode_parses_none_and_snapshot_v2_tokens() {
        assert_eq!(
            OmHintReaderMode::from_env(Some("none")),
            OmHintReaderMode::None
        );
        assert_eq!(
            OmHintReaderMode::from_env(Some("off")),
            OmHintReaderMode::None
        );
        assert_eq!(
            OmHintReaderMode::from_env(Some("snapshot_v2")),
            OmHintReaderMode::SnapshotV2
        );
        assert_eq!(
            OmHintReaderMode::from_env(Some("v2")),
            OmHintReaderMode::SnapshotV2
        );
    }
}
