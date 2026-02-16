use serde::{Deserialize, Serialize};

use super::OmScope;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmInferenceModelConfig {
    pub provider: String,
    pub model: String,
    pub max_output_tokens: u32,
    pub temperature_milli: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OmInferenceUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmPendingMessage {
    pub id: String,
    pub role: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at_rfc3339: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmObserverRequest {
    pub scope: OmScope,
    pub scope_key: String,
    pub model: OmInferenceModelConfig,
    pub active_observations: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other_conversations: Option<String>,
    pub pending_messages: Vec<OmPendingMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmObserverResponse {
    pub observations: String,
    pub observation_token_count: u32,
    pub observed_message_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_response: Option<String>,
    pub usage: OmInferenceUsage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmReflectorRequest {
    pub scope: OmScope,
    pub scope_key: String,
    pub model: OmInferenceModelConfig,
    pub generation_count: u32,
    pub active_observations: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmReflectorResponse {
    pub reflection: String,
    pub reflection_token_count: u32,
    pub reflected_observation_line_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_response: Option<String>,
    pub usage: OmInferenceUsage,
}

#[cfg(test)]
mod tests;
