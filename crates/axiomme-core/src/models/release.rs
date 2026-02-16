use serde::{Deserialize, Serialize};

use super::{QueueDiagnostics, ReplayReport};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseCheckDocument {
    pub version: u32,
    pub check_id: String,
    pub created_at: String,
    pub gate_profile: String,
    pub status: String,
    pub passed: bool,
    pub reasons: Vec<String>,
    pub threshold_p95_ms: u128,
    pub min_top1_accuracy: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_stress_top1_accuracy: Option<f32>,
    pub max_p95_regression_pct: Option<f32>,
    pub max_top1_regression_pct: Option<f32>,
    pub window_size: usize,
    pub required_passes: usize,
    pub evaluated_runs: usize,
    pub passing_runs: usize,
    pub latest_report_uri: Option<String>,
    pub previous_report_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_strict_error: Option<String>,
    pub gate_record_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyInventorySummary {
    pub lockfile_present: bool,
    pub package_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyAuditSummary {
    pub tool: String,
    pub mode: String,
    pub available: bool,
    pub executed: bool,
    pub status: String,
    pub advisories_found: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_excerpt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAuditCheck {
    pub name: String,
    pub passed: bool,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAuditReport {
    pub report_id: String,
    pub created_at: String,
    pub workspace_dir: String,
    pub passed: bool,
    pub status: String,
    pub inventory: DependencyInventorySummary,
    pub dependency_audit: DependencyAuditSummary,
    pub checks: Vec<SecurityAuditCheck>,
    pub report_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperabilityEvidenceCheck {
    pub name: String,
    pub passed: bool,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperabilityEvidenceReport {
    pub report_id: String,
    pub created_at: String,
    pub passed: bool,
    pub status: String,
    pub trace_limit: usize,
    pub request_limit: usize,
    pub traces_analyzed: usize,
    pub request_logs_scanned: usize,
    pub trace_metrics_snapshot_uri: String,
    pub queue: QueueDiagnostics,
    pub checks: Vec<OperabilityEvidenceCheck>,
    pub report_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityEvidenceCheck {
    pub name: String,
    pub passed: bool,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityEvidenceReport {
    pub report_id: String,
    pub created_at: String,
    pub passed: bool,
    pub status: String,
    pub replay_limit: usize,
    pub max_cycles: u32,
    pub replay_cycles: u32,
    pub replay_totals: ReplayReport,
    pub baseline_dead_letter: u64,
    pub final_dead_letter: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_checkpoint: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_checkpoint: Option<i64>,
    pub queued_root_uri: String,
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_hit_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart_hit_uri: Option<String>,
    pub queue: QueueDiagnostics,
    pub checks: Vec<ReliabilityEvidenceCheck>,
    pub report_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseGateDecision {
    pub gate_id: String,
    pub passed: bool,
    pub status: String,
    pub details: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseGatePackReport {
    pub pack_id: String,
    pub created_at: String,
    pub workspace_dir: String,
    pub passed: bool,
    pub status: String,
    pub unresolved_blockers: usize,
    pub decisions: Vec<ReleaseGateDecision>,
    pub report_uri: String,
}
