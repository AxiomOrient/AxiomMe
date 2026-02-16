use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::om::addon::OmCommand;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationBoundary {
    pub chunks_activated: usize,
    pub message_tokens_activated: u32,
    pub observation_tokens_activated: u32,
    pub activated_message_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationResult {
    pub activated_max_seq: u32,
    pub chunks_activated: usize,
    pub message_tokens_activated: u32,
    pub observation_tokens_activated: u32,
    pub activated_message_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmObserverMessageCandidate {
    pub id: String,
    pub role: String,
    pub text: String,
    pub created_at: DateTime<Utc>,
    pub source_thread_id: Option<String>,
    pub source_session_id: Option<String>,
}

pub const BUFFERED_OBSERVATIONS_SEPARATOR: &str = "--- BUFFERED (pending activation) ---";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReflectionDraft {
    pub reflection: String,
    pub reflection_token_count: u32,
    pub reflected_observation_line_count: u32,
    pub reflection_input_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferedReflectionSlicePlan {
    pub sliced_observations: String,
    pub reflected_observation_line_count: u32,
    pub slice_token_estimate: u32,
    pub compression_target_tokens: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReflectionAction {
    None,
    Buffer,
    Reflect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReflectionEnqueueDecision {
    pub action: ReflectionAction,
    pub command: Option<OmCommand>,
    pub should_increment_trigger_count: bool,
    pub next_is_reflecting: bool,
    pub next_is_buffering_reflection: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "decision output is an explicit truth table used by pipeline tests"
)]
pub struct ObserverWriteDecision {
    pub threshold: u32,
    pub threshold_reached: bool,
    pub interval_triggered: bool,
    pub block_after_exceeded: bool,
    pub should_run_observer: bool,
    pub should_activate_after_observer: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsyncObservationIntervalState {
    pub interval_tokens: Option<u32>,
    pub crossed_interval_boundary: bool,
    pub new_tokens_since_last_boundary: u32,
    pub min_new_tokens_required: u32,
    pub debounce_passed: bool,
    pub should_trigger: bool,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OmTransformError {
    #[error("missing required identifier for scope: {0}")]
    MissingScopeIdentifier(&'static str),
}
