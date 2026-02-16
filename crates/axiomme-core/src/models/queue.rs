use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueueLaneStatus {
    pub new_total: u64,
    pub new_due: u64,
    pub processing: u64,
    pub processed: u64,
    pub error_count: u64,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueueStatus {
    pub semantic: QueueLaneStatus,
    pub embedding: QueueLaneStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueueCounts {
    pub new_total: u64,
    pub new_due: u64,
    pub processing: u64,
    pub done: u64,
    pub dead_letter: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub earliest_next_attempt_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueCheckpoint {
    pub worker_name: String,
    pub last_event_id: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueueDiagnostics {
    pub counts: QueueCounts,
    pub checkpoints: Vec<QueueCheckpoint>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queue_dead_letter_rate: Vec<QueueDeadLetterRate>,
    #[serde(default)]
    pub om_status: OmQueueStatus,
    #[serde(default)]
    pub om_reflection_apply_metrics: OmReflectionApplyMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueueDeadLetterRate {
    pub event_type: String,
    pub total: u64,
    pub dead_letter: u64,
    pub dead_letter_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct OmQueueStatus {
    pub records_total: u64,
    pub observing_count: u64,
    pub reflecting_count: u64,
    pub buffering_observation_count: u64,
    pub buffering_reflection_count: u64,
    pub observation_tokens_active: u64,
    pub pending_message_tokens: u64,
    pub observer_trigger_count_total: u64,
    pub reflector_trigger_count_total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct OmReflectionApplyMetrics {
    pub attempts_total: u64,
    pub applied_total: u64,
    pub stale_generation_total: u64,
    pub idempotent_total: u64,
    pub stale_generation_ratio: f64,
    pub avg_latency_ms: f64,
    pub max_latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueueOverview {
    pub counts: QueueCounts,
    pub checkpoints: Vec<QueueCheckpoint>,
    pub lanes: QueueStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queue_dead_letter_rate: Vec<QueueDeadLetterRate>,
    #[serde(default)]
    pub om_status: OmQueueStatus,
    #[serde(default)]
    pub om_reflection_apply_metrics: OmReflectionApplyMetrics,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxEvent {
    pub id: i64,
    pub event_type: String,
    pub uri: String,
    pub payload_json: serde_json::Value,
    pub status: String,
    pub attempt_count: u32,
    pub next_attempt_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReplayReport {
    pub fetched: usize,
    pub processed: usize,
    pub done: usize,
    pub dead_letter: usize,
    pub requeued: usize,
    pub skipped: usize,
}
