use crate::catalog::{eval_query_set_uri, eval_report_json_uri, eval_report_markdown_uri};
use crate::error::Result;
use crate::models::{EvalBucket, EvalCaseResult, EvalLoopReport, EvalQueryCase};
use crate::quality::format_eval_report_markdown;

use super::AxiomMe;

pub(super) struct EvalReportInput {
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
    pub failures: Vec<EvalCaseResult>,
    pub query_set_uri: String,
}

impl AxiomMe {
    pub(super) fn write_eval_query_set(
        &self,
        run_id: &str,
        query_cases: &[EvalQueryCase],
    ) -> Result<String> {
        let query_set_uri = eval_query_set_uri(run_id)?;
        self.fs.write(
            &query_set_uri,
            &serde_json::to_string_pretty(query_cases)?,
            true,
        )?;
        Ok(query_set_uri.to_string())
    }

    pub(super) fn write_eval_report(&self, input: EvalReportInput) -> Result<EvalLoopReport> {
        let report_uri = eval_report_json_uri(&input.run_id)?;
        let markdown_report_uri = eval_report_markdown_uri(&input.run_id)?;
        let report = EvalLoopReport {
            run_id: input.run_id,
            created_at: input.created_at,
            trace_limit: input.trace_limit,
            query_limit: input.query_limit,
            search_limit: input.search_limit,
            include_golden: input.include_golden,
            golden_only: input.golden_only,
            traces_scanned: input.traces_scanned,
            trace_cases_used: input.trace_cases_used,
            golden_cases_used: input.golden_cases_used,
            executed_cases: input.executed_cases,
            passed: input.passed,
            failed: input.failed,
            top1_accuracy: input.top1_accuracy,
            buckets: input.buckets,
            report_uri: report_uri.to_string(),
            query_set_uri: input.query_set_uri,
            markdown_report_uri: markdown_report_uri.to_string(),
            failures: input.failures,
        };
        self.fs
            .write(&report_uri, &serde_json::to_string_pretty(&report)?, true)?;
        self.fs.write(
            &markdown_report_uri,
            &format_eval_report_markdown(&report),
            true,
        )?;
        Ok(report)
    }
}
