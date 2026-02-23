use serde::{Deserialize, Serialize};

use super::EvalQueryCase;
use super::defaults::default_true;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "benchmark suite toggles are explicit configuration flags and preserved for JSON contract compatibility"
)]
pub struct BenchmarkRunOptions {
    pub query_limit: usize,
    pub search_limit: usize,
    pub include_golden: bool,
    pub include_trace: bool,
    #[serde(default = "default_true")]
    pub include_stress: bool,
    #[serde(default)]
    pub trace_expectations: bool,
    pub fixture_name: Option<String>,
}

impl Default for BenchmarkRunOptions {
    fn default() -> Self {
        Self {
            query_limit: 100,
            search_limit: 10,
            include_golden: true,
            include_trace: true,
            include_stress: true,
            trace_expectations: false,
            fixture_name: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkAmortizedRunSummary {
    pub iteration: usize,
    pub run_id: String,
    pub created_at: String,
    pub executed_cases: usize,
    pub top1_accuracy: f32,
    pub ndcg_at_10: f32,
    pub recall_at_10: f32,
    pub p95_latency_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p95_latency_us: Option<u128>,
    pub report_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "report shape mirrors run options for stable serialization and downstream tooling"
)]
pub struct BenchmarkAmortizedReport {
    pub mode: String,
    pub iterations: usize,
    pub query_limit: usize,
    pub search_limit: usize,
    pub include_golden: bool,
    pub include_trace: bool,
    pub include_stress: bool,
    pub trace_expectations: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixture_name: Option<String>,
    pub wall_total_ms: u128,
    pub wall_avg_ms: f32,
    pub executed_cases_total: usize,
    pub top1_accuracy_avg: f32,
    pub ndcg_at_10_avg: f32,
    pub recall_at_10_avg: f32,
    pub p95_latency_ms_median: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p95_latency_us_median: Option<u128>,
    pub runs: Vec<BenchmarkAmortizedRunSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkGateOptions {
    pub gate_profile: String,
    pub threshold_p95_ms: u128,
    pub min_top1_accuracy: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_stress_top1_accuracy: Option<f32>,
    pub max_p95_regression_pct: Option<f32>,
    pub max_top1_regression_pct: Option<f32>,
    pub window_size: usize,
    pub required_passes: usize,
    pub record: bool,
    pub write_release_check: bool,
}

impl Default for BenchmarkGateOptions {
    fn default() -> Self {
        Self {
            gate_profile: "custom".to_string(),
            threshold_p95_ms: 600,
            min_top1_accuracy: 0.75,
            min_stress_top1_accuracy: None,
            max_p95_regression_pct: None,
            max_top1_regression_pct: None,
            window_size: 1,
            required_passes: 1,
            record: false,
            write_release_check: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseSecurityAuditMode {
    Offline,
    Strict,
}

impl ReleaseSecurityAuditMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Strict => "strict",
        }
    }
}

impl std::fmt::Display for ReleaseSecurityAuditMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str((*self).as_str())
    }
}

impl Default for ReleaseSecurityAuditMode {
    fn default() -> Self {
        Self::Strict
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseGatePackOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_dir: Option<String>,
    pub replay_limit: usize,
    pub replay_max_cycles: u32,
    pub trace_limit: usize,
    pub request_limit: usize,
    pub eval_trace_limit: usize,
    pub eval_query_limit: usize,
    pub eval_search_limit: usize,
    pub benchmark_query_limit: usize,
    pub benchmark_search_limit: usize,
    pub benchmark_threshold_p95_ms: u128,
    pub benchmark_min_top1_accuracy: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub benchmark_min_stress_top1_accuracy: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark_max_p95_regression_pct: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark_max_top1_regression_pct: Option<f32>,
    pub benchmark_window_size: usize,
    pub benchmark_required_passes: usize,
    #[serde(default)]
    pub security_audit_mode: ReleaseSecurityAuditMode,
}

impl Default for ReleaseGatePackOptions {
    fn default() -> Self {
        Self {
            workspace_dir: None,
            replay_limit: 100,
            replay_max_cycles: 8,
            trace_limit: 200,
            request_limit: 200,
            eval_trace_limit: 200,
            eval_query_limit: 50,
            eval_search_limit: 10,
            benchmark_query_limit: 60,
            benchmark_search_limit: 10,
            benchmark_threshold_p95_ms: 600,
            benchmark_min_top1_accuracy: 0.75,
            benchmark_min_stress_top1_accuracy: None,
            benchmark_max_p95_regression_pct: None,
            benchmark_max_top1_regression_pct: None,
            benchmark_window_size: 1,
            benchmark_required_passes: 1,
            security_audit_mode: ReleaseSecurityAuditMode::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkCaseResult {
    pub query: String,
    pub target_uri: Option<String>,
    pub expected_top_uri: Option<String>,
    pub actual_top_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_rank: Option<usize>,
    pub latency_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_us: Option<u128>,
    pub passed: bool,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkEnvironmentMetadata {
    pub machine_profile: String,
    pub cpu_model: String,
    pub ram_bytes: u64,
    pub os_version: String,
    pub rustc_version: String,
    pub retrieval_backend: String,
    pub reranker_profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_vector_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_strict_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkCorpusMetadata {
    pub profile: String,
    pub snapshot_id: String,
    pub root_uri: String,
    pub file_count: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkQuerySetMetadata {
    pub version: String,
    pub source: String,
    pub total_queries: usize,
    pub semantic_queries: usize,
    pub lexical_queries: usize,
    pub mixed_queries: usize,
    pub warmup_queries: usize,
    pub measured_queries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkAcceptanceThresholds {
    pub find_p95_latency_ms_max: u128,
    pub search_p95_latency_ms_max: u128,
    pub commit_p95_latency_ms_max: u128,
    pub min_ndcg_at_10: f32,
    pub min_recall_at_10: f32,
    pub min_total_queries: usize,
    pub min_semantic_queries: usize,
    pub min_lexical_queries: usize,
    pub min_mixed_queries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkAcceptanceMeasured {
    pub find_p95_latency_ms: u128,
    pub search_p95_latency_ms: u128,
    pub commit_p95_latency_ms: u128,
    pub ndcg_at_10: f32,
    pub recall_at_10: f32,
    pub total_queries: usize,
    pub semantic_queries: usize,
    pub lexical_queries: usize,
    pub mixed_queries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkAcceptanceCheck {
    pub name: String,
    pub passed: bool,
    pub expected: String,
    pub actual: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkAcceptanceResult {
    pub protocol_id: String,
    pub passed: bool,
    pub thresholds: BenchmarkAcceptanceThresholds,
    pub measured: BenchmarkAcceptanceMeasured,
    pub checks: Vec<BenchmarkAcceptanceCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub run_id: String,
    pub created_at: String,
    pub query_limit: usize,
    pub search_limit: usize,
    pub include_golden: bool,
    pub include_trace: bool,
    pub executed_cases: usize,
    pub passed: usize,
    pub failed: usize,
    pub top1_accuracy: f32,
    pub ndcg_at_10: f32,
    pub recall_at_10: f32,
    pub p50_latency_ms: u128,
    pub p95_latency_ms: u128,
    pub p99_latency_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p50_latency_us: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p95_latency_us: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p99_latency_us: Option<u128>,
    pub avg_latency_ms: f32,
    pub search_p50_latency_ms: u128,
    pub search_p95_latency_ms: u128,
    pub search_p99_latency_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_p50_latency_us: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_p95_latency_us: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_p99_latency_us: Option<u128>,
    pub search_avg_latency_ms: f32,
    pub commit_p50_latency_ms: u128,
    pub commit_p95_latency_ms: u128,
    pub commit_p99_latency_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_p50_latency_us: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_p95_latency_us: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_p99_latency_us: Option<u128>,
    pub commit_avg_latency_ms: f32,
    pub error_rate: f32,
    pub environment: BenchmarkEnvironmentMetadata,
    pub corpus: BenchmarkCorpusMetadata,
    pub query_set: BenchmarkQuerySetMetadata,
    pub acceptance: BenchmarkAcceptanceResult,
    pub report_uri: String,
    pub markdown_report_uri: String,
    pub case_set_uri: String,
    pub results: Vec<BenchmarkCaseResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSummary {
    pub run_id: String,
    pub created_at: String,
    pub executed_cases: usize,
    pub top1_accuracy: f32,
    pub p95_latency_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p95_latency_us: Option<u128>,
    pub report_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkFixtureDocument {
    pub version: u32,
    pub created_at: String,
    pub name: String,
    pub cases: Vec<EvalQueryCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkFixtureSummary {
    pub name: String,
    pub uri: String,
    pub case_count: usize,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkTrendReport {
    pub latest: Option<BenchmarkSummary>,
    pub previous: Option<BenchmarkSummary>,
    pub delta_p95_latency_ms: Option<i128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_p95_latency_us: Option<i128>,
    pub delta_top1_accuracy: Option<f32>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkGateRunResult {
    pub run_id: String,
    pub passed: bool,
    pub p95_latency_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p95_latency_us: Option<u128>,
    pub top1_accuracy: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stress_top1_accuracy: Option<f32>,
    pub regression_pct: Option<f32>,
    pub top1_regression_pct: Option<f32>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkGateResult {
    pub passed: bool,
    pub gate_profile: String,
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
    pub latest: Option<BenchmarkSummary>,
    pub previous: Option<BenchmarkSummary>,
    pub regression_pct: Option<f32>,
    pub top1_regression_pct: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stress_top1_accuracy: Option<f32>,
    pub run_results: Vec<BenchmarkGateRunResult>,
    pub gate_record_uri: Option<String>,
    pub release_check_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_strict_error: Option<String>,
    pub reasons: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_security_audit_mode_serde_contract_is_stable() {
        let expected = [
            (ReleaseSecurityAuditMode::Offline, "offline"),
            (ReleaseSecurityAuditMode::Strict, "strict"),
        ];

        for (mode, raw) in expected {
            let serialized = serde_json::to_string(&mode).expect("serialize mode");
            assert_eq!(serialized, format!("\"{raw}\""));
            let deserialized: ReleaseSecurityAuditMode =
                serde_json::from_str(&serialized).expect("deserialize mode");
            assert_eq!(deserialized, mode);
        }
    }
}
