use std::time::Instant;

use crate::catalog::normalize_gate_profile;
use crate::error::Result;
use crate::models::{
    BenchmarkGateOptions, BenchmarkGateResult, BenchmarkGateRunResult, BenchmarkReport,
};
use crate::quality::to_benchmark_summary;

use super::AxiomMe;
use super::benchmark_metrics_service::{percent_delta_u128, percent_drop_f32};

const MAX_SEMANTIC_QUALITY_REGRESSION_PCT: f32 = 3.0;

impl AxiomMe {
    pub fn benchmark_gate(
        &self,
        threshold_p95_ms: u128,
        min_top1_accuracy: f32,
        max_p95_regression_pct: Option<f32>,
        max_top1_regression_pct: Option<f32>,
    ) -> Result<BenchmarkGateResult> {
        self.benchmark_gate_with_options(BenchmarkGateOptions {
            gate_profile: "default".to_string(),
            threshold_p95_ms,
            min_top1_accuracy,
            max_p95_regression_pct,
            max_top1_regression_pct,
            ..BenchmarkGateOptions::default()
        })
    }

    pub fn benchmark_gate_with_policy(
        &self,
        threshold_p95_ms: u128,
        min_top1_accuracy: f32,
        max_p95_regression_pct: Option<f32>,
        window_size: usize,
        required_passes: usize,
        record: bool,
    ) -> Result<BenchmarkGateResult> {
        self.benchmark_gate_with_options(BenchmarkGateOptions {
            gate_profile: "custom".to_string(),
            threshold_p95_ms,
            min_top1_accuracy,
            max_p95_regression_pct,
            max_top1_regression_pct: None,
            window_size,
            required_passes,
            record,
            write_release_check: false,
        })
    }

    pub fn benchmark_gate_with_options(
        &self,
        options: BenchmarkGateOptions,
    ) -> Result<BenchmarkGateResult> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let gate_profile = normalize_gate_profile(&options.gate_profile);
        let window_size = options.window_size.max(1);
        let required_passes = options.required_passes.max(1).min(window_size);
        let threshold_p95_ms = options.threshold_p95_ms;
        let min_top1_accuracy = options.min_top1_accuracy;
        let max_p95_regression_pct = options.max_p95_regression_pct;
        let max_top1_regression_pct = options.max_top1_regression_pct;
        let record = options.record;
        let write_release_check = options.write_release_check;

        let output = (|| -> Result<BenchmarkGateResult> {
            let fetch_limit = window_size.saturating_add(1).max(2);
            let reports = self.list_benchmark_reports(fetch_limit)?;
            let latest = reports.first().cloned().map(to_benchmark_summary);
            let previous = reports.get(1).cloned().map(to_benchmark_summary);
            let regression_pct = match (latest.as_ref(), previous.as_ref()) {
                (Some(l), Some(p)) => percent_delta_u128(l.p95_latency_ms, p.p95_latency_ms),
                _ => None,
            };
            let top1_regression_pct = match (latest.as_ref(), previous.as_ref()) {
                (Some(l), Some(p)) => percent_drop_f32(l.top1_accuracy, p.top1_accuracy),
                _ => None,
            };
            let semantic_ndcg_regression_pct = match (reports.first(), reports.get(1)) {
                (Some(latest), Some(previous))
                    if semantic_quality_regression_eligible(latest, previous) =>
                {
                    percent_drop_f32(latest.ndcg_at_10, previous.ndcg_at_10)
                }
                _ => None,
            };
            let semantic_recall_regression_pct = match (reports.first(), reports.get(1)) {
                (Some(latest), Some(previous))
                    if semantic_quality_regression_eligible(latest, previous) =>
                {
                    percent_drop_f32(latest.recall_at_10, previous.recall_at_10)
                }
                _ => None,
            };

            if reports.is_empty() {
                let mut result = BenchmarkGateResult {
                    passed: false,
                    gate_profile: gate_profile.clone(),
                    threshold_p95_ms,
                    min_top1_accuracy,
                    max_p95_regression_pct,
                    max_top1_regression_pct,
                    window_size,
                    required_passes,
                    evaluated_runs: 0,
                    passing_runs: 0,
                    latest,
                    previous,
                    regression_pct,
                    top1_regression_pct,
                    run_results: Vec::new(),
                    gate_record_uri: None,
                    release_check_uri: None,
                    reasons: vec!["no_benchmark_reports".to_string()],
                };
                if record {
                    let uri = self.persist_benchmark_gate_result(&result)?;
                    result.gate_record_uri = Some(uri);
                }
                if write_release_check {
                    let uri = self.persist_release_check_result(&result)?;
                    result.release_check_uri = Some(uri);
                }
                return Ok(result);
            }

            let mut run_results = Vec::<BenchmarkGateRunResult>::new();
            let mut passing_runs = 0usize;
            for (idx, report) in reports.iter().take(window_size).enumerate() {
                let prev = reports.get(idx + 1);
                let mut passed = true;
                let mut reasons = Vec::<String>::new();

                if report.p95_latency_ms > threshold_p95_ms {
                    passed = false;
                    reasons.push(format!(
                        "p95_latency_exceeded:{}>{}",
                        report.p95_latency_ms, threshold_p95_ms
                    ));
                }
                if report.top1_accuracy < min_top1_accuracy {
                    passed = false;
                    reasons.push(format!(
                        "top1_accuracy_below:{:.4}<{:.4}",
                        report.top1_accuracy, min_top1_accuracy
                    ));
                }

                let mut run_regression_pct = None::<f32>;
                if let (Some(max_regression), Some(prev_report)) = (max_p95_regression_pct, prev) {
                    run_regression_pct =
                        percent_delta_u128(report.p95_latency_ms, prev_report.p95_latency_ms);
                    if let Some(pct) = run_regression_pct
                        && pct > max_regression
                    {
                        passed = false;
                        reasons.push(format!(
                            "p95_regression_exceeded:{:.2}%>{:.2}%",
                            pct, max_regression
                        ));
                    }
                }

                let mut run_top1_regression_pct = None::<f32>;
                if let (Some(max_regression), Some(prev_report)) = (max_top1_regression_pct, prev) {
                    run_top1_regression_pct =
                        percent_drop_f32(report.top1_accuracy, prev_report.top1_accuracy);
                    if let Some(pct) = run_top1_regression_pct
                        && pct > max_regression
                    {
                        passed = false;
                        reasons.push(format!(
                            "top1_regression_exceeded:{:.2}%>{:.2}%",
                            pct, max_regression
                        ));
                    }
                }

                if let Some(prev_report) = prev
                    && semantic_quality_regression_eligible(report, prev_report)
                {
                    let run_ndcg_regression_pct =
                        percent_drop_f32(report.ndcg_at_10, prev_report.ndcg_at_10);
                    if let Some(pct) = run_ndcg_regression_pct
                        && pct > MAX_SEMANTIC_QUALITY_REGRESSION_PCT
                    {
                        passed = false;
                        reasons.push(format!(
                            "ndcg_regression_exceeded:{:.2}%>{:.2}%",
                            pct, MAX_SEMANTIC_QUALITY_REGRESSION_PCT
                        ));
                    }

                    let run_recall_regression_pct =
                        percent_drop_f32(report.recall_at_10, prev_report.recall_at_10);
                    if let Some(pct) = run_recall_regression_pct
                        && pct > MAX_SEMANTIC_QUALITY_REGRESSION_PCT
                    {
                        passed = false;
                        reasons.push(format!(
                            "recall_regression_exceeded:{:.2}%>{:.2}%",
                            pct, MAX_SEMANTIC_QUALITY_REGRESSION_PCT
                        ));
                    }
                }
                if passed {
                    passing_runs += 1;
                    reasons.push("ok".to_string());
                }

                run_results.push(BenchmarkGateRunResult {
                    run_id: report.run_id.clone(),
                    passed,
                    p95_latency_ms: report.p95_latency_ms,
                    top1_accuracy: report.top1_accuracy,
                    regression_pct: run_regression_pct,
                    top1_regression_pct: run_top1_regression_pct,
                    reasons,
                });
            }

            let evaluated_runs = run_results.len();
            let mut reasons = Vec::<String>::new();
            let mut passed = true;
            if evaluated_runs < required_passes {
                passed = false;
                reasons.push(format!(
                    "insufficient_history:{}<{}",
                    evaluated_runs, required_passes
                ));
            }
            if passing_runs < required_passes {
                passed = false;
                reasons.push(format!(
                    "pass_quorum_not_met:{}<{}",
                    passing_runs, required_passes
                ));
            }
            if let Some(pct) = semantic_ndcg_regression_pct
                && pct > MAX_SEMANTIC_QUALITY_REGRESSION_PCT
            {
                passed = false;
                reasons.push(format!(
                    "latest_ndcg_regression_exceeded:{:.2}%>{:.2}%",
                    pct, MAX_SEMANTIC_QUALITY_REGRESSION_PCT
                ));
            }
            if let Some(pct) = semantic_recall_regression_pct
                && pct > MAX_SEMANTIC_QUALITY_REGRESSION_PCT
            {
                passed = false;
                reasons.push(format!(
                    "latest_recall_regression_exceeded:{:.2}%>{:.2}%",
                    pct, MAX_SEMANTIC_QUALITY_REGRESSION_PCT
                ));
            }
            if passed {
                reasons.push("ok".to_string());
            }

            let mut result = BenchmarkGateResult {
                passed,
                gate_profile: gate_profile.clone(),
                threshold_p95_ms,
                min_top1_accuracy,
                max_p95_regression_pct,
                max_top1_regression_pct,
                window_size,
                required_passes,
                evaluated_runs,
                passing_runs,
                latest,
                previous,
                regression_pct,
                top1_regression_pct,
                run_results,
                gate_record_uri: None,
                release_check_uri: None,
                reasons,
            };
            if record {
                let uri = self.persist_benchmark_gate_result(&result)?;
                result.gate_record_uri = Some(uri);
            }
            if write_release_check {
                let uri = self.persist_release_check_result(&result)?;
                result.release_check_uri = Some(uri);
            }
            Ok(result)
        })();

        match output {
            Ok(result) => {
                self.log_request_status(
                    request_id,
                    "benchmark.gate",
                    "ok",
                    started,
                    None,
                    Some(serde_json::json!({
                        "gate_profile": result.gate_profile,
                        "threshold_p95_ms": result.threshold_p95_ms.to_string(),
                        "min_top1_accuracy": result.min_top1_accuracy,
                        "max_p95_regression_pct": result.max_p95_regression_pct,
                        "max_top1_regression_pct": result.max_top1_regression_pct,
                        "semantic_regression_pct_max": MAX_SEMANTIC_QUALITY_REGRESSION_PCT,
                        "window_size": result.window_size,
                        "required_passes": result.required_passes,
                        "evaluated_runs": result.evaluated_runs,
                        "passing_runs": result.passing_runs,
                        "passed": result.passed,
                        "p95_regression_pct": result.regression_pct,
                        "top1_regression_pct": result.top1_regression_pct,
                        "reasons": result.reasons,
                        "gate_record_uri": result.gate_record_uri,
                        "release_check_uri": result.release_check_uri,
                    })),
                );
                Ok(result)
            }
            Err(err) => {
                self.log_request_error(
                    request_id,
                    "benchmark.gate",
                    started,
                    None,
                    &err,
                    Some(serde_json::json!({
                        "gate_profile": gate_profile,
                        "threshold_p95_ms": threshold_p95_ms.to_string(),
                        "min_top1_accuracy": min_top1_accuracy,
                        "max_p95_regression_pct": max_p95_regression_pct,
                        "max_top1_regression_pct": max_top1_regression_pct,
                        "semantic_regression_pct_max": MAX_SEMANTIC_QUALITY_REGRESSION_PCT,
                        "window_size": window_size,
                        "required_passes": required_passes,
                        "record": record,
                        "write_release_check": write_release_check,
                    })),
                );
                Err(err)
            }
        }
    }
}

fn semantic_quality_regression_eligible(
    current: &BenchmarkReport,
    previous: &BenchmarkReport,
) -> bool {
    let current_thresholds = &current.acceptance.thresholds;
    let previous_thresholds = &previous.acceptance.thresholds;
    current.query_set.total_queries >= current_thresholds.min_total_queries
        && current.query_set.semantic_queries >= current_thresholds.min_semantic_queries
        && current.query_set.lexical_queries >= current_thresholds.min_lexical_queries
        && current.query_set.mixed_queries >= current_thresholds.min_mixed_queries
        && previous.query_set.total_queries >= previous_thresholds.min_total_queries
        && previous.query_set.semantic_queries >= previous_thresholds.min_semantic_queries
        && previous.query_set.lexical_queries >= previous_thresholds.min_lexical_queries
        && previous.query_set.mixed_queries >= previous_thresholds.min_mixed_queries
}
