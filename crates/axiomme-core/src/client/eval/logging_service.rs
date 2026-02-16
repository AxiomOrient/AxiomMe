use std::time::Instant;

use crate::error::AxiomError;
use crate::models::EvalLoopReport;

use super::AxiomMe;

pub(super) struct EvalRunLogContext {
    pub run_id: String,
    pub trace_limit: usize,
    pub query_limit: usize,
    pub search_limit: usize,
    pub include_golden: bool,
    pub golden_only: bool,
}

impl AxiomMe {
    pub(super) fn log_eval_run_success(
        &self,
        request_id: String,
        started: Instant,
        report: &EvalLoopReport,
    ) {
        self.log_request_status(
            request_id,
            "eval.run",
            "ok",
            started,
            None,
            Some(serde_json::json!({
                "run_id": report.run_id,
                "trace_limit": report.trace_limit,
                "query_limit": report.query_limit,
                "search_limit": report.search_limit,
                "include_golden": report.include_golden,
                "golden_only": report.golden_only,
                "executed_cases": report.executed_cases,
                "passed": report.passed,
                "failed": report.failed,
                "top1_accuracy": report.top1_accuracy,
            })),
        );
    }

    pub(super) fn log_eval_run_error(
        &self,
        request_id: String,
        started: Instant,
        context: &EvalRunLogContext,
        err: &AxiomError,
    ) {
        self.log_request_error(
            request_id,
            "eval.run",
            started,
            None,
            err,
            Some(serde_json::json!({
                "run_id": context.run_id,
                "trace_limit": context.trace_limit,
                "query_limit": context.query_limit,
                "search_limit": context.search_limit,
                "include_golden": context.include_golden,
                "golden_only": context.golden_only,
            })),
        );
    }
}
