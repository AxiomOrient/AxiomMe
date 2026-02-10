use std::path::Path;
use std::time::Instant;

use crate::error::Result;
use crate::models::{
    BenchmarkGateOptions, BenchmarkRunOptions, EvalRunOptions, ReleaseGateDecision,
    ReleaseGatePackOptions, ReleaseGatePackReport,
};
use crate::release_gate::{
    benchmark_release_gate_decision, eval_quality_gate_decision, evaluate_build_quality_gate,
    evaluate_contract_integrity_gate, finalize_release_gate_pack_report,
    operability_evidence_gate_decision, release_gate_pack_report_uri,
    reliability_evidence_gate_decision, resolve_workspace_dir, security_audit_gate_decision,
    session_memory_gate_decision,
};

use super::AxiomMe;

impl AxiomMe {
    pub fn collect_release_gate_pack(
        &self,
        options: ReleaseGatePackOptions,
    ) -> Result<ReleaseGatePackReport> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();

        let output = (|| -> Result<ReleaseGatePackReport> {
            let workspace_path = resolve_workspace_dir(options.workspace_dir.as_deref())?;
            let workspace_dir = workspace_path.display().to_string();
            let decisions =
                self.collect_release_gate_decisions(&workspace_path, &workspace_dir, &options)?;

            let pack_id = uuid::Uuid::new_v4().to_string();
            let report_uri = release_gate_pack_report_uri(&pack_id)?;
            let report = finalize_release_gate_pack_report(
                pack_id,
                workspace_dir,
                decisions,
                report_uri.to_string(),
            );
            self.fs
                .write(&report_uri, &serde_json::to_string_pretty(&report)?, true)?;
            Ok(report)
        })();

        match output {
            Ok(report) => {
                self.log_request_status(
                    request_id,
                    "release.pack",
                    &report.status,
                    started,
                    None,
                    Some(serde_json::json!({
                        "workspace_dir": report.workspace_dir,
                        "passed": report.passed,
                        "unresolved_blockers": report.unresolved_blockers,
                        "report_uri": report.report_uri,
                    })),
                );
                Ok(report)
            }
            Err(err) => {
                self.log_request_error(request_id, "release.pack", started, None, &err, None);
                Err(err)
            }
        }
    }

    fn collect_release_gate_decisions(
        &self,
        workspace_path: &Path,
        workspace_dir: &str,
        options: &ReleaseGatePackOptions,
    ) -> Result<Vec<ReleaseGateDecision>> {
        let mut decisions = Vec::<ReleaseGateDecision>::new();

        decisions.push(evaluate_contract_integrity_gate(workspace_path));
        decisions.push(evaluate_build_quality_gate(workspace_path));

        let reliability = self.collect_reliability_evidence(
            options.replay_limit.max(1),
            options.replay_max_cycles.max(1),
        )?;
        decisions.push(reliability_evidence_gate_decision(&reliability));

        let eval = self.run_eval_loop_with_options(EvalRunOptions {
            trace_limit: options.eval_trace_limit.max(1),
            query_limit: options.eval_query_limit.max(1),
            search_limit: options.eval_search_limit.max(1),
            include_golden: true,
            golden_only: false,
        })?;
        decisions.push(eval_quality_gate_decision(&eval));

        let (g4_passed, memory_category_miss, g4_details) = self.evaluate_session_memory_gate()?;
        decisions.push(session_memory_gate_decision(
            g4_passed,
            memory_category_miss,
            g4_details,
        ));

        let security = self.run_security_audit(Some(workspace_dir))?;
        decisions.push(security_audit_gate_decision(&security));

        if self.list_benchmark_reports(1)?.is_empty() {
            self.ensure_release_benchmark_seed_trace()?;
            let _ = self.run_benchmark_suite(BenchmarkRunOptions {
                query_limit: options.benchmark_query_limit.max(1),
                search_limit: options.benchmark_search_limit.max(1),
                include_golden: false,
                include_trace: true,
                fixture_name: None,
            })?;
        }
        let benchmark_gate = self.benchmark_gate_with_options(BenchmarkGateOptions {
            gate_profile: "rc-release".to_string(),
            threshold_p95_ms: options.benchmark_threshold_p95_ms,
            min_top1_accuracy: options.benchmark_min_top1_accuracy,
            max_p95_regression_pct: options.benchmark_max_p95_regression_pct,
            max_top1_regression_pct: options.benchmark_max_top1_regression_pct,
            window_size: options.benchmark_window_size.max(1),
            required_passes: options.benchmark_required_passes.max(1),
            record: true,
            write_release_check: true,
        })?;
        decisions.push(benchmark_release_gate_decision(&benchmark_gate));

        let operability = self.collect_operability_evidence(
            options.trace_limit.max(1),
            options.request_limit.max(1),
        )?;
        decisions.push(operability_evidence_gate_decision(&operability));

        Ok(decisions)
    }
}
