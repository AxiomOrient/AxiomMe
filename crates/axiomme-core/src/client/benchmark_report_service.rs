use crate::catalog::benchmark_case_set_uri;
use crate::error::Result;
use crate::models::{BenchmarkReport, EvalQueryCase};
use crate::quality::format_benchmark_report_markdown;
use crate::uri::AxiomUri;

use super::AxiomMe;

impl AxiomMe {
    pub(super) fn write_benchmark_case_set(
        &self,
        run_id: &str,
        query_cases: &[EvalQueryCase],
    ) -> Result<String> {
        let case_set_uri = benchmark_case_set_uri(run_id)?;
        self.fs.write(
            &case_set_uri,
            &serde_json::to_string_pretty(query_cases)?,
            true,
        )?;
        Ok(case_set_uri.to_string())
    }

    pub(super) fn write_benchmark_report_artifacts(&self, report: &BenchmarkReport) -> Result<()> {
        let report_uri = AxiomUri::parse(&report.report_uri)?;
        self.fs
            .write(&report_uri, &serde_json::to_string_pretty(report)?, true)?;

        let markdown_report_uri = AxiomUri::parse(&report.markdown_report_uri)?;
        self.fs.write(
            &markdown_report_uri,
            &format_benchmark_report_markdown(report),
            true,
        )?;
        Ok(())
    }
}
