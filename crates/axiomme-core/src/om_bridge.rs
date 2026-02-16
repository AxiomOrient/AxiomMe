use serde::{Deserialize, Serialize};

pub const OM_OUTBOX_SCHEMA_VERSION_V1: u8 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OmScopeV1 {
    Session,
    Thread,
    Resource,
}

impl OmScopeV1 {
    #[must_use]
    pub const fn to_engine_scope(self) -> crate::om::OmScope {
        match self {
            Self::Session => crate::om::OmScope::Session,
            Self::Thread => crate::om::OmScope::Thread,
            Self::Resource => crate::om::OmScope::Resource,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmObserveBufferRequestedV1 {
    pub schema_version: u8,
    pub scope_key: String,
    pub expected_generation: u32,
    pub requested_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

impl OmObserveBufferRequestedV1 {
    pub fn new(
        scope_key: &str,
        expected_generation: u32,
        requested_at: String,
        session_id: Option<&str>,
    ) -> Self {
        Self {
            schema_version: OM_OUTBOX_SCHEMA_VERSION_V1,
            scope_key: scope_key.to_string(),
            expected_generation,
            requested_at,
            session_id: session_id.map(ToString::to_string),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmReflectBufferRequestedV1 {
    pub schema_version: u8,
    pub scope_key: String,
    pub expected_generation: u32,
    pub requested_at: String,
}

impl OmReflectBufferRequestedV1 {
    #[must_use]
    pub fn new(scope_key: &str, expected_generation: u32, requested_at: String) -> Self {
        Self {
            schema_version: OM_OUTBOX_SCHEMA_VERSION_V1,
            scope_key: scope_key.to_string(),
            expected_generation,
            requested_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmReflectRequestedV1 {
    pub schema_version: u8,
    pub scope_key: String,
    pub expected_generation: u32,
    pub requested_at: String,
}

impl OmReflectRequestedV1 {
    #[must_use]
    pub fn new(scope_key: &str, expected_generation: u32, requested_at: String) -> Self {
        Self {
            schema_version: OM_OUTBOX_SCHEMA_VERSION_V1,
            scope_key: scope_key.to_string(),
            expected_generation,
            requested_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OmHintReadStateV1 {
    pub scope_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    pub activated_message_ids: Vec<String>,
    pub observation_tokens_active: u32,
    pub observer_trigger_count_total: u32,
    pub reflector_trigger_count_total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmScopeBindingInputV1 {
    pub scope: OmScopeV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmMessageAppendRequestV1 {
    pub session_id: String,
    pub role: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_binding: Option<OmScopeBindingInputV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmMessageAppendResultV1 {
    pub session_id: String,
    pub message_id: String,
    pub scope_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmHintReadRequestV1 {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_binding: Option<OmScopeBindingInputV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmOutboxEnqueueResultV1 {
    pub event_id: i64,
    pub event_type: String,
    pub scope_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OmReplayRequestV1 {
    pub limit: usize,
    pub include_dead_letter: bool,
    #[serde(default)]
    pub mode: OmReplayModeV1,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OmReplayResultV1 {
    pub fetched: usize,
    pub processed: usize,
    pub done: usize,
    pub dead_letter: usize,
    pub requeued: usize,
    pub skipped: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scanned_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub om_candidate_count: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OmReplayModeV1 {
    #[default]
    Full,
    OmOnly,
}

impl OmReplayModeV1 {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::OmOnly => "om_only",
        }
    }
}
