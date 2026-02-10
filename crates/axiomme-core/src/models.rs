use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::uri::{AxiomUri, Scope};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub uri: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobResult {
    pub matches: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddResourceResult {
    pub root_uri: String,
    pub queued: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownDocument {
    pub uri: String,
    pub content: String,
    pub etag: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownSaveResult {
    pub uri: String,
    pub etag: String,
    pub updated_at: String,
    pub reindexed_root: String,
    pub save_ms: u128,
    pub reindex_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueueLaneStatus {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationSummary {
    pub uri: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RelationLink {
    pub id: String,
    pub uris: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextHit {
    pub uri: String,
    pub score: f32,
    #[serde(rename = "abstract")]
    pub abstract_text: String,
    pub context_type: String,
    pub relations: Vec<RelationSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindResult {
    pub memories: Vec<ContextHit>,
    pub resources: Vec<ContextHit>,
    pub skills: Vec<ContextHit>,
    pub query_plan: serde_json::Value,
    pub query_results: Vec<ContextHit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<RetrievalTrace>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchFilter {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub mime: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalStep {
    pub round: u32,
    pub current_uri: String,
    pub children_examined: usize,
    pub children_selected: usize,
    pub queue_size_after: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceStats {
    pub latency_ms: u128,
    pub explored_nodes: usize,
    pub convergence_rounds: u32,
    #[serde(default)]
    pub typed_query_count: usize,
    #[serde(default)]
    pub relation_enriched_hits: usize,
    #[serde(default)]
    pub relation_enriched_links: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalTrace {
    pub trace_id: String,
    pub request_type: String,
    pub query: String,
    pub target_uri: Option<String>,
    pub start_points: Vec<TracePoint>,
    pub steps: Vec<RetrievalStep>,
    pub final_topk: Vec<TracePoint>,
    pub stop_reason: String,
    pub metrics: TraceStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracePoint {
    pub uri: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: String,
    pub text: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub uri: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitStats {
    pub total_turns: usize,
    pub contexts_used: usize,
    pub skills_used: usize,
    pub memories_extracted: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitResult {
    pub session_id: String,
    pub status: String,
    pub memories_extracted: usize,
    pub active_count_updated: usize,
    pub archived: bool,
    pub stats: CommitStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchContext {
    pub session_id: String,
    pub recent_messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub uri: String,
    pub is_dir: bool,
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeResult {
    pub root: TreeNode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexRecord {
    pub id: String,
    pub uri: String,
    pub parent_uri: Option<String>,
    pub is_leaf: bool,
    pub context_type: String,
    pub name: String,
    pub abstract_text: String,
    pub content: String,
    pub tags: Vec<String>,
    pub updated_at: DateTime<Utc>,
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOptions {
    pub query: String,
    pub target_uri: Option<AxiomUri>,
    pub session: Option<String>,
    #[serde(default)]
    pub session_hints: Vec<String>,
    #[serde(default)]
    pub budget: Option<SearchBudget>,
    pub limit: usize,
    pub score_threshold: Option<f32>,
    pub filter: Option<SearchFilter>,
    pub request_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchBudget {
    pub max_ms: Option<u64>,
    pub max_nodes: Option<usize>,
    pub max_depth: Option<usize>,
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
pub struct ContextUsage {
    pub contexts_used: usize,
    pub skills_used: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub context_usage: ContextUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueryPlan {
    pub scopes: Vec<String>,
    pub keywords: Vec<String>,
    #[serde(default)]
    pub typed_queries: Vec<TypedQueryPlan>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedQueryPlan {
    pub kind: String,
    pub query: String,
    pub scopes: Vec<String>,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCandidate {
    pub category: String,
    pub key: String,
    pub text: String,
    pub source_message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataFilter {
    pub fields: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantBackendStatus {
    pub enabled: bool,
    pub base_url: String,
    pub collection: String,
    pub healthy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingBackendStatus {
    pub provider: String,
    pub vector_version: String,
    pub dim: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendStatus {
    pub local_records: usize,
    pub embedding: EmbeddingBackendStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qdrant: Option<QdrantBackendStatus>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileReport {
    pub run_id: String,
    pub drift_count: usize,
    pub invalid_uri_entries: usize,
    pub missing_uri_entries: usize,
    pub missing_files_pruned: usize,
    pub reindexed_scopes: usize,
    pub dry_run: bool,
    pub drift_uris_sample: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileOptions {
    pub dry_run: bool,
    pub scopes: Option<Vec<Scope>>,
    pub max_drift_sample: usize,
}

impl Default for ReconcileOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            scopes: None,
            max_drift_sample: 50,
        }
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalQueryCase {
    pub source_trace_id: String,
    pub query: String,
    pub target_uri: Option<String>,
    pub expected_top_uri: Option<String>,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCaseResult {
    pub source_trace_id: String,
    pub query: String,
    pub target_uri: Option<String>,
    pub expected_top_uri: Option<String>,
    pub actual_top_uri: Option<String>,
    pub passed: bool,
    pub bucket: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub replay_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalBucket {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalLoopReport {
    pub run_id: String,
    pub created_at: String,
    pub trace_limit: usize,
    pub query_limit: usize,
    pub search_limit: usize,
    pub include_golden: bool,
    pub golden_only: bool,
    pub traces_scanned: usize,
    pub trace_cases_used: usize,
    pub golden_cases_used: usize,
    pub executed_cases: usize,
    pub passed: usize,
    pub failed: usize,
    pub top1_accuracy: f32,
    pub buckets: Vec<EvalBucket>,
    pub report_uri: String,
    pub query_set_uri: String,
    pub markdown_report_uri: String,
    pub failures: Vec<EvalCaseResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRunOptions {
    pub trace_limit: usize,
    pub query_limit: usize,
    pub search_limit: usize,
    pub include_golden: bool,
    pub golden_only: bool,
}

impl Default for EvalRunOptions {
    fn default() -> Self {
        Self {
            trace_limit: 100,
            query_limit: 50,
            search_limit: 10,
            include_golden: true,
            golden_only: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalGoldenAddResult {
    pub golden_uri: String,
    pub added: bool,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalGoldenMergeReport {
    pub golden_uri: String,
    pub before_count: usize,
    pub added_count: usize,
    pub after_count: usize,
    pub trace_limit: usize,
    pub max_add: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalGoldenDocument {
    pub version: u32,
    pub updated_at: String,
    pub cases: Vec<EvalQueryCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRunOptions {
    pub query_limit: usize,
    pub search_limit: usize,
    pub include_golden: bool,
    pub include_trace: bool,
    pub fixture_name: Option<String>,
}

impl Default for BenchmarkRunOptions {
    fn default() -> Self {
        Self {
            query_limit: 100,
            search_limit: 10,
            include_golden: true,
            include_trace: true,
            fixture_name: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkGateOptions {
    pub gate_profile: String,
    pub threshold_p95_ms: u128,
    pub min_top1_accuracy: f32,
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
            max_p95_regression_pct: None,
            max_top1_regression_pct: None,
            window_size: 1,
            required_passes: 1,
            record: false,
            write_release_check: false,
        }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark_max_p95_regression_pct: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark_max_top1_regression_pct: Option<f32>,
    pub benchmark_window_size: usize,
    pub benchmark_required_passes: usize,
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
            benchmark_max_p95_regression_pct: None,
            benchmark_max_top1_regression_pct: None,
            benchmark_window_size: 1,
            benchmark_required_passes: 1,
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
    pub qdrant_version: String,
    pub qdrant_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qdrant_base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qdrant_collection: Option<String>,
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
    pub avg_latency_ms: f32,
    pub search_p50_latency_ms: u128,
    pub search_p95_latency_ms: u128,
    pub search_p99_latency_ms: u128,
    pub search_avg_latency_ms: f32,
    pub commit_p50_latency_ms: u128,
    pub commit_p95_latency_ms: u128,
    pub commit_p99_latency_ms: u128,
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
    pub delta_top1_accuracy: Option<f32>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkGateRunResult {
    pub run_id: String,
    pub passed: bool,
    pub p95_latency_ms: u128,
    pub top1_accuracy: f32,
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
    pub run_results: Vec<BenchmarkGateRunResult>,
    pub gate_record_uri: Option<String>,
    pub release_check_uri: Option<String>,
    pub reasons: Vec<String>,
}

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
    pub max_p95_regression_pct: Option<f32>,
    pub max_top1_regression_pct: Option<f32>,
    pub window_size: usize,
    pub required_passes: usize,
    pub evaluated_runs: usize,
    pub passing_runs: usize,
    pub latest_report_uri: Option<String>,
    pub previous_report_uri: Option<String>,
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
