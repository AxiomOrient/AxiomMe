use crate::catalog::{eval_query_set_uri, eval_report_json_uri, eval_report_markdown_uri};
use crate::error::Result;
use crate::models::{
    EvalArtifacts, EvalBucket, EvalCaseResult, EvalCoverageSummary, EvalLoopReport,
    EvalQualitySummary, EvalQueryCase, EvalRunSelection,
};
use crate::quality::format_eval_report_markdown;

use super::AxiomMe;

pub(super) struct EvalReportMetaInput {
    pub run_id: String,
    pub created_at: String,
    pub query_set_uri: String,
}

pub(super) struct EvalReportRunConfigInput {
    pub trace_limit: usize,
    pub query_limit: usize,
    pub search_limit: usize,
    pub include_golden: bool,
    pub golden_only: bool,
}

pub(super) struct EvalReportCoverageInput {
    pub traces_scanned: usize,
    pub trace_cases_used: usize,
    pub golden_cases_used: usize,
    pub executed_cases: usize,
}

pub(super) struct EvalReportOutcomeInput {
    pub passed: usize,
    pub failed: usize,
    pub top1_accuracy: f32,
    pub buckets: Vec<EvalBucket>,
    pub failures: Vec<EvalCaseResult>,
}

pub(super) struct EvalReportInput {
    pub meta: EvalReportMetaInput,
    pub run_config: EvalReportRunConfigInput,
    pub coverage: EvalReportCoverageInput,
    pub outcome: EvalReportOutcomeInput,
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
        let report_uri = eval_report_json_uri(&input.meta.run_id)?;
        let markdown_report_uri = eval_report_markdown_uri(&input.meta.run_id)?;
        let report = EvalLoopReport {
            run_id: input.meta.run_id,
            created_at: input.meta.created_at,
            selection: EvalRunSelection {
                trace_limit: input.run_config.trace_limit,
                query_limit: input.run_config.query_limit,
                search_limit: input.run_config.search_limit,
                include_golden: input.run_config.include_golden,
                golden_only: input.run_config.golden_only,
            },
            coverage: EvalCoverageSummary {
                traces_scanned: input.coverage.traces_scanned,
                trace_cases_used: input.coverage.trace_cases_used,
                golden_cases_used: input.coverage.golden_cases_used,
                executed_cases: input.coverage.executed_cases,
            },
            quality: EvalQualitySummary {
                passed: input.outcome.passed,
                failed: input.outcome.failed,
                top1_accuracy: input.outcome.top1_accuracy,
                buckets: input.outcome.buckets,
                failures: input.outcome.failures,
            },
            artifacts: EvalArtifacts {
                report_uri: report_uri.to_string(),
                query_set_uri: input.meta.query_set_uri,
                markdown_report_uri: markdown_report_uri.to_string(),
            },
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
