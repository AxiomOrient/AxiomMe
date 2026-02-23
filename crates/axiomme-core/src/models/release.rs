use serde::{Deserialize, Serialize};

use super::{QueueDiagnostics, ReleaseSecurityAuditMode, ReplayReport};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseCheckDocument {
    pub version: u32,
    pub check_id: String,
    pub created_at: String,
    pub gate_profile: String,
    pub status: ReleaseGateStatus,
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
    pub latest_p95_latency_us: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_p95_latency_us: Option<u128>,
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
    pub mode: ReleaseSecurityAuditMode,
    pub available: bool,
    pub executed: bool,
    pub status: DependencyAuditStatus,
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
    pub status: EvidenceStatus,
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
    pub status: EvidenceStatus,
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
    pub status: EvidenceStatus,
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
pub struct CommandProbeResult {
    pub test_name: String,
    pub command_ok: bool,
    pub matched: bool,
    pub output_excerpt: String,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicSemverProbeResult {
    pub passed: bool,
    pub error: Option<String>,
    pub manifest_req: Option<String>,
    pub manifest_req_ok: Option<bool>,
    pub manifest_uses_path: Option<bool>,
    pub manifest_uses_git: Option<bool>,
    pub manifest_source_ok: Option<bool>,
    pub lock_version: Option<String>,
    pub lock_version_ok: Option<bool>,
    pub lock_source: Option<String>,
    pub lock_source_ok: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicSemverPolicy {
    pub required_major: u64,
    pub required_minor: u64,
    pub required_lock_source_prefix: String,
    pub allowed_manifest_operators: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyContractPolicy {
    pub schema_uri: String,
    pub required_schema_version: u32,
    pub probe_test_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyContractProbeResult {
    pub passed: bool,
    pub error: Option<String>,
    pub command_probe: CommandProbeResult,
    pub schema_uri: String,
    pub schema_version: Option<u32>,
    pub schema_version_ok: bool,
    pub object_type_count: usize,
    pub link_type_count: usize,
    pub action_type_count: usize,
    pub invariant_count: usize,
    pub invariant_check_passed: usize,
    pub invariant_check_failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractIntegrityGateDetails {
    pub policy: EpisodicSemverPolicy,
    pub contract_probe: CommandProbeResult,
    pub episodic_api_probe: CommandProbeResult,
    pub episodic_semver_probe: EpisodicSemverProbeResult,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ontology_policy: Option<OntologyContractPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ontology_probe: Option<OntologyContractProbeResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityGateDetails {
    pub status: EvidenceStatus,
    pub replay_done: usize,
    pub dead_letter: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalQualityGateDetails {
    pub executed_cases: usize,
    pub top1_accuracy: f32,
    pub min_top1_accuracy: f32,
    pub failed: usize,
    pub filter_ignored: usize,
    pub relation_missing: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMemoryGateDetails {
    pub base_details: String,
    pub memory_category_miss: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityAuditGateDetails {
    pub status: EvidenceStatus,
    pub mode: ReleaseSecurityAuditMode,
    pub strict_mode_required: bool,
    pub strict_mode: bool,
    pub audit_status: DependencyAuditStatus,
    pub advisories_found: usize,
    pub packages: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkGateDetails {
    pub passed: bool,
    pub evaluated_runs: usize,
    pub passing_runs: usize,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperabilityGateDetails {
    pub status: EvidenceStatus,
    pub traces_analyzed: usize,
    pub request_logs_scanned: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockerRollupGateDetails {
    pub unresolved_blockers: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildQualityGateDetails {
    pub cargo_check: bool,
    pub cargo_fmt: bool,
    pub cargo_clippy: bool,
    pub check_output: String,
    pub fmt_output: String,
    pub clippy_output: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReleaseGateId {
    #[serde(rename = "G0")]
    ContractIntegrity,
    #[serde(rename = "G1")]
    BuildQuality,
    #[serde(rename = "G2")]
    ReliabilityEvidence,
    #[serde(rename = "G3")]
    EvalQuality,
    #[serde(rename = "G4")]
    SessionMemory,
    #[serde(rename = "G5")]
    SecurityAudit,
    #[serde(rename = "G6")]
    Benchmark,
    #[serde(rename = "G7")]
    OperabilityEvidence,
    #[serde(rename = "G8")]
    BlockerRollup,
}

impl ReleaseGateId {
    pub const ALL: [Self; 9] = [
        Self::ContractIntegrity,
        Self::BuildQuality,
        Self::ReliabilityEvidence,
        Self::EvalQuality,
        Self::SessionMemory,
        Self::SecurityAudit,
        Self::Benchmark,
        Self::OperabilityEvidence,
        Self::BlockerRollup,
    ];

    pub const fn code(self) -> &'static str {
        match self {
            Self::ContractIntegrity => "G0",
            Self::BuildQuality => "G1",
            Self::ReliabilityEvidence => "G2",
            Self::EvalQuality => "G3",
            Self::SessionMemory => "G4",
            Self::SecurityAudit => "G5",
            Self::Benchmark => "G6",
            Self::OperabilityEvidence => "G7",
            Self::BlockerRollup => "G8",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseGateStatus {
    Pass,
    Fail,
}

impl ReleaseGateStatus {
    pub const fn from_passed(passed: bool) -> Self {
        if passed { Self::Pass } else { Self::Fail }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
        }
    }
}

impl std::fmt::Display for ReleaseGateStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

pub type EvidenceStatus = ReleaseGateStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyAuditStatus {
    Passed,
    VulnerabilitiesFound,
    ToolMissing,
    Error,
    HostToolsDisabled,
}

impl std::fmt::Display for DependencyAuditStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Passed => "passed",
            Self::VulnerabilitiesFound => "vulnerabilities_found",
            Self::ToolMissing => "tool_missing",
            Self::Error => "error",
            Self::HostToolsDisabled => "host_tools_disabled",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum ReleaseGateDetails {
    ContractIntegrity(Box<ContractIntegrityGateDetails>),
    BuildQuality(BuildQualityGateDetails),
    ReliabilityEvidence(ReliabilityGateDetails),
    EvalQuality(EvalQualityGateDetails),
    SessionMemory(SessionMemoryGateDetails),
    SecurityAudit(SecurityAuditGateDetails),
    Benchmark(BenchmarkGateDetails),
    OperabilityEvidence(OperabilityGateDetails),
    BlockerRollup(BlockerRollupGateDetails),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseGateDecision {
    pub gate_id: ReleaseGateId,
    pub passed: bool,
    pub status: ReleaseGateStatus,
    pub details: ReleaseGateDetails,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseGatePackReport {
    pub pack_id: String,
    pub created_at: String,
    pub workspace_dir: String,
    pub passed: bool,
    pub status: ReleaseGateStatus,
    pub unresolved_blockers: usize,
    pub decisions: Vec<ReleaseGateDecision>,
    pub report_uri: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_gate_status_serde_contract_is_stable() {
        let pass = serde_json::to_string(&ReleaseGateStatus::Pass).expect("serialize pass");
        let fail = serde_json::to_string(&ReleaseGateStatus::Fail).expect("serialize fail");
        assert_eq!(pass, "\"pass\"");
        assert_eq!(fail, "\"fail\"");

        let parsed_pass: ReleaseGateStatus =
            serde_json::from_str("\"pass\"").expect("deserialize pass");
        let parsed_fail: ReleaseGateStatus =
            serde_json::from_str("\"fail\"").expect("deserialize fail");
        assert_eq!(parsed_pass, ReleaseGateStatus::Pass);
        assert_eq!(parsed_fail, ReleaseGateStatus::Fail);
        assert!(serde_json::from_str::<ReleaseGateStatus>("\"PASS\"").is_err());
    }

    #[test]
    fn dependency_audit_status_serde_contract_is_exhaustive_and_stable() {
        let expected = [
            (DependencyAuditStatus::Passed, "passed"),
            (
                DependencyAuditStatus::VulnerabilitiesFound,
                "vulnerabilities_found",
            ),
            (DependencyAuditStatus::ToolMissing, "tool_missing"),
            (DependencyAuditStatus::Error, "error"),
            (
                DependencyAuditStatus::HostToolsDisabled,
                "host_tools_disabled",
            ),
        ];

        for (status, raw) in expected {
            let serialized = serde_json::to_string(&status).expect("serialize status");
            assert_eq!(serialized, format!("\"{raw}\""));
            let deserialized: DependencyAuditStatus =
                serde_json::from_str(&serialized).expect("deserialize status");
            assert_eq!(deserialized, status);
        }
    }
}
