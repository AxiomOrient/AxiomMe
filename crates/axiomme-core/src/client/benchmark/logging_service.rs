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
                "query_limit": report.query_limit,
                "search_limit": report.search_limit,
                "include_golden": report.include_golden,
                "include_trace": report.include_trace,
                "fixture_name": fixture_name,
                "executed_cases": report.executed_cases,
                "p95_latency_ms": report.p95_latency_ms.to_string(),
                "search_p95_latency_ms": report.search_p95_latency_ms.to_string(),
                "commit_p95_latency_ms": report.commit_p95_latency_ms.to_string(),
                "top1_accuracy": report.top1_accuracy,
                "ndcg_at_10": report.ndcg_at_10,
                "recall_at_10": report.recall_at_10,
                "protocol_passed": report.acceptance.passed,
                "passed": report.passed,
                "failed": report.failed,
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
