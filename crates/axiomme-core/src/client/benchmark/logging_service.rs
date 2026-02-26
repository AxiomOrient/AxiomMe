use std::time::Instant;

use crate::error::AxiomError;
use crate::models::BenchmarkReport;

use super::AxiomMe;

pub(super) struct BenchmarkRunLogContext {
    pub run_id: String,
    pub query_limit: usize,
    pub search_limit: usize,
    pub include_golden: bool,
    pub include_trace: bool,
    pub fixture_name: Option<String>,
}

impl AxiomMe {
    pub(super) fn log_benchmark_run_success(
        &self,
        request_id: &str,
        started: Instant,
        fixture_name: Option<&str>,
        report: &BenchmarkReport,
    ) {
        self.log_request_status(
            request_id.to_string(),
            "benchmark.run",
            "ok",
            started,
            None,
            Some(serde_json::json!({
                "run_id": report.run_id,
                "query_limit": report.selection.query_limit,
                "search_limit": report.selection.search_limit,
                "include_golden": report.selection.include_golden,
                "include_trace": report.selection.include_trace,
                "fixture_name": fixture_name,
                "executed_cases": report.quality.executed_cases,
                "p95_latency_ms": report.latency.find.p95_ms.to_string(),
                "p95_latency_us": report.latency.find.p95_us.map(|value| value.to_string()),
                "search_p95_latency_ms": report.latency.search.p95_ms.to_string(),
                "search_p95_latency_us": report.latency.search.p95_us.map(|value| value.to_string()),
                "commit_p95_latency_ms": report.latency.commit.p95_ms.to_string(),
                "commit_p95_latency_us": report.latency.commit.p95_us.map(|value| value.to_string()),
                "top1_accuracy": report.quality.top1_accuracy,
                "ndcg_at_10": report.quality.ndcg_at_10,
                "recall_at_10": report.quality.recall_at_10,
                "protocol_passed": report.acceptance.passed,
                "passed": report.quality.passed,
                "failed": report.quality.failed,
            })),
        );
    }

    pub(super) fn log_benchmark_run_error(
        &self,
        request_id: &str,
        started: Instant,
        context: &BenchmarkRunLogContext,
        err: &AxiomError,
    ) {
        self.log_request_error(
            request_id.to_string(),
            "benchmark.run",
            started,
            None,
            err,
            Some(serde_json::json!({
                "run_id": context.run_id,
                "query_limit": context.query_limit,
                "search_limit": context.search_limit,
                "include_golden": context.include_golden,
                "include_trace": context.include_trace,
                "fixture_name": context.fixture_name,
            })),
        );
    }
}
