use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OmScope {
    Session,
    Thread,
    Resource,
}

impl OmScope {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Thread => "thread",
            Self::Resource => "resource",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "session" => Some(Self::Session),
            "thread" => Some(Self::Thread),
            "resource" => Some(Self::Resource),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OmOriginType {
    Initial,
    Reflection,
}

impl OmOriginType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Initial => "initial",
            Self::Reflection => "reflection",
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "initial" => Some(Self::Initial),
            "reflection" => Some(Self::Reflection),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "record persists independent observer/reflector/buffer states as normalized DB columns"
)]
pub struct OmRecord {
    pub id: String,
    pub scope: OmScope,
    pub scope_key: String,
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub resource_id: Option<String>,
    pub generation_count: u32,
    pub last_applied_outbox_event_id: Option<i64>,
    pub origin_type: OmOriginType,
    pub active_observations: String,
    pub observation_token_count: u32,
    pub pending_message_tokens: u32,
    pub last_observed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_response: Option<String>,
    #[serde(default)]
    pub last_activated_message_ids: Vec<String>,
    #[serde(default)]
    pub observer_trigger_count_total: u32,
    #[serde(default)]
    pub reflector_trigger_count_total: u32,
    pub is_observing: bool,
    pub is_reflecting: bool,
    pub is_buffering_observation: bool,
    pub is_buffering_reflection: bool,
    pub last_buffered_at_tokens: u32,
    pub last_buffered_at_time: Option<DateTime<Utc>>,
    pub buffered_reflection: Option<String>,
    pub buffered_reflection_tokens: Option<u32>,
    pub buffered_reflection_input_tokens: Option<u32>,
    pub reflected_observation_line_count: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmObservationChunk {
    pub id: String,
    pub record_id: String,
    pub seq: u32,
    pub cycle_id: String,
    pub observations: String,
    pub token_count: u32,
    pub message_tokens: u32,
    pub message_ids: Vec<String>,
    pub last_observed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests;
