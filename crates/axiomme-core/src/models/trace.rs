use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceIndexEntry {
    pub trace_id: String,
    pub uri: String,
    pub request_type: String,
    pub query: String,
    pub target_uri: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLogEntry {
    pub request_id: String,
    pub operation: String,
    pub status: String,
    pub latency_ms: u128,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceMetricsSample {
    pub trace_id: String,
    pub request_type: String,
    pub latency_ms: u128,
    pub explored_nodes: usize,
    pub convergence_rounds: u32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceRequestTypeMetrics {
    pub request_type: String,
    pub traces: usize,
    pub p50_latency_ms: u128,
    pub p95_latency_ms: u128,
    pub avg_latency_ms: f32,
    pub avg_explored_nodes: f32,
    pub avg_convergence_rounds: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceMetricsReport {
    pub window_limit: usize,
    pub include_replays: bool,
    pub indexed_traces_scanned: usize,
    pub traces_analyzed: usize,
    pub traces_skipped_missing: usize,
    pub traces_skipped_invalid: usize,
    pub by_request_type: Vec<TraceRequestTypeMetrics>,
    pub slowest_samples: Vec<TraceMetricsSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceMetricsSnapshotDocument {
    pub version: u32,
    pub snapshot_id: String,
    pub created_at: String,
    pub report: TraceMetricsReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceMetricsSnapshotSummary {
    pub snapshot_id: String,
    pub created_at: String,
    pub report_uri: String,
    pub traces_analyzed: usize,
    pub include_replays: bool,
    pub window_limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceMetricsTrendReport {
    pub request_type: String,
    pub latest: Option<TraceMetricsSnapshotSummary>,
    pub previous: Option<TraceMetricsSnapshotSummary>,
    pub latest_p95_latency_ms: Option<u128>,
    pub previous_p95_latency_ms: Option<u128>,
    pub delta_p95_latency_ms: Option<i128>,
    pub latest_avg_explored_nodes: Option<f32>,
    pub previous_avg_explored_nodes: Option<f32>,
    pub delta_avg_explored_nodes: Option<f32>,
    pub status: String,
}
