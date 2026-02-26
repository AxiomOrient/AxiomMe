use std::time::Instant;

use chrono::Utc;

use crate::error::Result;
use crate::models::{EvalLoopReport, EvalRunOptions};

use super::{
    AxiomMe,
    execution_service::{EvalCaseSelection, EvalExecutionOutcome},
    logging_service::EvalRunLogContext,
    report_service::{
        EvalReportCoverageInput, EvalReportInput, EvalReportMetaInput, EvalReportOutcomeInput,
        EvalReportRunConfigInput,
    },
};

impl AxiomMe {
    pub fn run_eval_loop(
        &self,
        trace_limit: usize,
        query_limit: usize,
        search_limit: usize,
    ) -> Result<EvalLoopReport> {
        self.run_eval_loop_with_options(&EvalRunOptions {
            trace_limit,
            query_limit,
            search_limit,
            include_golden: true,
            golden_only: false,
        })
    }

    pub fn run_eval_loop_with_options(&self, options: &EvalRunOptions) -> Result<EvalLoopReport> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let trace_limit = options.trace_limit.max(1);
        let query_limit = options.query_limit.max(1);
        let search_limit = options.search_limit.max(1);
        let include_golden = options.include_golden || options.golden_only;
        let golden_only = options.golden_only;
        let run_id = uuid::Uuid::new_v4().to_string();
        let log_context = EvalRunLogContext {
            run_id: run_id.clone(),
            trace_limit,
            query_limit,
            search_limit,
            include_golden,
            golden_only,
        };

        let output = (|| -> Result<EvalLoopReport> {
            let created_at = Utc::now().to_rfc3339();
            let EvalCaseSelection {
                query_cases,
                traces_scanned,
                trace_cases_used,
                golden_cases_used,
            } = self.select_eval_query_cases(
                trace_limit,
                query_limit,
                include_golden,
                golden_only,
            )?;
            let query_set_uri = self.write_eval_query_set(&run_id, &query_cases)?;
            let EvalExecutionOutcome {
                passed,
                failed,
                top1_accuracy,
                buckets,
                failures,
            } = self.execute_eval_cases(&query_cases, search_limit)?;
            self.write_eval_report(EvalReportInput {
                meta: EvalReportMetaInput {
                    run_id: run_id.clone(),
                    created_at,
                    query_set_uri,
                },
                run_config: EvalReportRunConfigInput {
                    trace_limit,
                    query_limit,
                    search_limit,
                    include_golden,
                    golden_only,
                },
                coverage: EvalReportCoverageInput {
                    traces_scanned,
                    trace_cases_used,
                    golden_cases_used,
                    executed_cases: query_cases.len(),
                },
                outcome: EvalReportOutcomeInput {
                    passed,
                    failed,
                    top1_accuracy,
                    buckets,
                    failures,
                },
            })
        })();

        match output {
            Ok(report) => {
                self.log_eval_run_success(request_id, started, &report);
                Ok(report)
            }
            Err(err) => {
                self.log_eval_run_error(request_id, started, &log_context, &err);
                Err(err)
            }
        }
    }
}
