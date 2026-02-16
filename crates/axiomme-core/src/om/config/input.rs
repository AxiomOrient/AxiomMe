use serde::{Deserialize, Serialize};

use crate::om::OmScope;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum BufferTokensInput {
    Disabled,
    Absolute(u32),
    Ratio(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ObservationConfigInput {
    pub message_tokens: Option<u32>,
    pub max_tokens_per_batch: Option<u32>,
    pub buffer_tokens: Option<BufferTokensInput>,
    pub buffer_activation: Option<f32>,
    pub block_after: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReflectionConfigInput {
    pub observation_tokens: Option<u32>,
    pub buffer_activation: Option<f32>,
    pub block_after: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OmConfigInput {
    pub scope: OmScope,
    pub share_token_budget: bool,
    pub observation: ObservationConfigInput,
    pub reflection: ReflectionConfigInput,
}

impl Default for OmConfigInput {
    fn default() -> Self {
        Self {
            scope: OmScope::Thread,
            share_token_budget: false,
            observation: ObservationConfigInput::default(),
            reflection: ReflectionConfigInput::default(),
        }
    }
}
