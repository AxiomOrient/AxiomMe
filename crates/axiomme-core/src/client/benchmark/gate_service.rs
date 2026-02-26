use std::time::Instant;

use crate::catalog::normalize_gate_profile;
use crate::error::Result;
use crate::models::{
    BenchmarkGateArtifacts, BenchmarkGateExecution, BenchmarkGateOptions, BenchmarkGateQuorum,
    BenchmarkGateResult, BenchmarkGateRunResult, BenchmarkGateSnapshot, BenchmarkGateThresholds,
    BenchmarkReport, BenchmarkSummary,
};
use crate::quality::to_benchmark_summary;

use super::AxiomMe;
use super::metrics_service::{percent_delta_u128, percent_drop_f32};

const MAX_SEMANTIC_QUALITY_REGRESSION_PCT: f32 = 3.0;
const REQUIRED_RELEASE_EMBEDDING_PROVIDER: &str = "semantic-model-http";

#[derive(Debug, Clone)]
struct GateConfig {
    gate_profile: String,
    threshold_p95_ms: u128,
    min_top1_accuracy: f32,
    min_stress_top1_accuracy: Option<f32>,
    max_p95_regression_pct: Option<f32>,
    max_top1_regression_pct: Option<f32>,
    window_size: usize,
    required_passes: usize,
    record: bool,
    write_release_check: bool,
    require_release_embedder: bool,
}

impl GateConfig {
    fn from_options(options: BenchmarkGateOptions) -> Self {
        let BenchmarkGateOptions {
            gate_profile,
            threshold_p95_ms,
            min_top1_accuracy,
            min_stress_top1_accuracy,
            max_p95_regression_pct,
            max_top1_regression_pct,
            window_size,
            required_passes,
            record,
            write_release_check,
        } = options;
        let gate_profile = normalize_gate_profile(&gate_profile);
        let window_size = window_size.max(1);
        let required_passes = required_passes.max(1).min(window_size);
        let require_release_embedder =
            write_release_check || gate_profile.to_ascii_lowercase().contains("release");

        Self {
            gate_profile,
            threshold_p95_ms,
            min_top1_accuracy,
            min_stress_top1_accuracy,
            max_p95_regression_pct,
            max_top1_regression_pct,
            window_size,
            required_passes,
            record,
            write_release_check,
            require_release_embedder,
        }
    }

    const fn report_fetch_limit(&self) -> usize {
        self.window_size.saturating_add(1)
    }
}

#[derive(Debug, Clone)]
struct GateSnapshot {
    latest: Option<BenchmarkSummary>,
    previous: Option<BenchmarkSummary>,
    regression_pct: Option<f32>,
    top1_regression_pct: Option<f32>,
    stress_top1_accuracy: Option<f32>,
    semantic_ndcg_regression_pct: Option<f32>,
    semantic_recall_regression_pct: Option<f32>,
    embedding_provider: Option<String>,
    embedding_strict_error: Option<String>,
}

impl GateSnapshot {
    fn from_reports(reports: &[BenchmarkReport]) -> Self {
        let latest = reports.first().cloned().map(to_benchmark_summary);
        let previous = reports.get(1).cloned().map(to_benchmark_summary);

        let regression_pct = match (reports.first(), reports.get(1)) {
            (Some(current), Some(prev)) => percent_delta_u128(
                p95_latency_for_regression(current),
                p95_latency_for_regression(prev),
            ),
            _ => None,
        };
        let top1_regression_pct = match (latest.as_ref(), previous.as_ref()) {
            (Some(current), Some(prev)) => {
                percent_drop_f32(current.top1_accuracy, prev.top1_accuracy)
            }
            _ => None,
        };

        let semantic_ndcg_regression_pct = match (reports.first(), reports.get(1)) {
            (Some(current), Some(prev)) if semantic_quality_regression_eligible(current, prev) => {
                percent_drop_f32(current.quality.ndcg_at_10, prev.quality.ndcg_at_10)
            }
            _ => None,
        };
        let semantic_recall_regression_pct = match (reports.first(), reports.get(1)) {
            (Some(current), Some(prev)) if semantic_quality_regression_eligible(current, prev) => {
                percent_drop_f32(current.quality.recall_at_10, prev.quality.recall_at_10)
            }
            _ => None,
        };

        Self {
            latest,
            previous,
            regression_pct,
            top1_regression_pct,
            stress_top1_accuracy: reports.first().and_then(stress_top1_accuracy),
            semantic_ndcg_regression_pct,
            semantic_recall_regression_pct,
            embedding_provider: reports
                .first()
                .and_then(|report| report.environment.embedding_provider.clone()),
            embedding_strict_error: reports
                .first()
                .and_then(|report| report.environment.embedding_strict_error.clone()),
        }
    }
}

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
            min_stress_top1_accuracy: None,
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
        let config = GateConfig::from_options(options);

        let output = (|| -> Result<BenchmarkGateResult> {
            let reports = self.list_benchmark_reports(config.report_fetch_limit().max(2))?;
            let snapshot = GateSnapshot::from_reports(&reports);

            let mut result = if reports.is_empty() {
                empty_gate_result(&config, &snapshot)
            } else {
                let run_results = reports
                    .iter()
                    .take(config.window_size)
                    .enumerate()
                    .map(|(idx, report)| evaluate_gate_run(report, reports.get(idx + 1), &config))
                    .collect::<Vec<_>>();
                build_gate_result(&config, &snapshot, run_results)
            };

            self.persist_gate_result_artifacts(&mut result, &config)?;
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
                        "threshold_p95_ms": result.thresholds.threshold_p95_ms.to_string(),
                        "min_top1_accuracy": result.thresholds.min_top1_accuracy,
                        "min_stress_top1_accuracy": result.thresholds.min_stress_top1_accuracy,
                        "max_p95_regression_pct": result.thresholds.max_p95_regression_pct,
                        "max_top1_regression_pct": result.thresholds.max_top1_regression_pct,
                        "semantic_regression_pct_max": MAX_SEMANTIC_QUALITY_REGRESSION_PCT,
                        "window_size": result.quorum.window_size,
                        "required_passes": result.quorum.required_passes,
                        "evaluated_runs": result.execution.evaluated_runs,
                        "passing_runs": result.execution.passing_runs,
                        "passed": result.passed,
                        "p95_regression_pct": result.snapshot.regression_pct,
                        "top1_regression_pct": result.snapshot.top1_regression_pct,
                        "stress_top1_accuracy": result.snapshot.stress_top1_accuracy,
                        "reasons": result.execution.reasons,
                        "gate_record_uri": result.artifacts.gate_record_uri,
                        "release_check_uri": result.artifacts.release_check_uri,
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
                        "gate_profile": config.gate_profile,
                        "threshold_p95_ms": config.threshold_p95_ms.to_string(),
                        "min_top1_accuracy": config.min_top1_accuracy,
                        "min_stress_top1_accuracy": config.min_stress_top1_accuracy,
                        "max_p95_regression_pct": config.max_p95_regression_pct,
                        "max_top1_regression_pct": config.max_top1_regression_pct,
                        "semantic_regression_pct_max": MAX_SEMANTIC_QUALITY_REGRESSION_PCT,
                        "window_size": config.window_size,
                        "required_passes": config.required_passes,
                        "record": config.record,
                        "write_release_check": config.write_release_check,
                    })),
                );
                Err(err)
            }
        }
    }

    fn persist_gate_result_artifacts(
        &self,
        result: &mut BenchmarkGateResult,
        config: &GateConfig,
    ) -> Result<()> {
        if config.record {
            result.artifacts.gate_record_uri = Some(self.persist_benchmark_gate_result(result)?);
        }
        if config.write_release_check {
            result.artifacts.release_check_uri = Some(self.persist_release_check_result(result)?);
        }
        Ok(())
    }
}

fn empty_gate_result(config: &GateConfig, snapshot: &GateSnapshot) -> BenchmarkGateResult {
    BenchmarkGateResult {
        passed: false,
        gate_profile: config.gate_profile.clone(),
        thresholds: BenchmarkGateThresholds {
            threshold_p95_ms: config.threshold_p95_ms,
            min_top1_accuracy: config.min_top1_accuracy,
            min_stress_top1_accuracy: config.min_stress_top1_accuracy,
            max_p95_regression_pct: config.max_p95_regression_pct,
            max_top1_regression_pct: config.max_top1_regression_pct,
        },
        quorum: BenchmarkGateQuorum {
            window_size: config.window_size,
            required_passes: config.required_passes,
        },
        snapshot: BenchmarkGateSnapshot {
            latest: snapshot.latest.clone(),
            previous: snapshot.previous.clone(),
            regression_pct: snapshot.regression_pct,
            top1_regression_pct: snapshot.top1_regression_pct,
            stress_top1_accuracy: snapshot.stress_top1_accuracy,
        },
        execution: BenchmarkGateExecution {
            evaluated_runs: 0,
            passing_runs: 0,
            run_results: Vec::new(),
            reasons: vec!["no_benchmark_reports".to_string()],
        },
        artifacts: BenchmarkGateArtifacts {
            gate_record_uri: None,
            release_check_uri: None,
            embedding_provider: None,
            embedding_strict_error: None,
        },
    }
}

fn build_gate_result(
    config: &GateConfig,
    snapshot: &GateSnapshot,
    run_results: Vec<BenchmarkGateRunResult>,
) -> BenchmarkGateResult {
    let evaluated_runs = run_results.len();
    let passing_runs = run_results.iter().filter(|result| result.passed).count();

    let (passed, reasons) = evaluate_gate_outcome(config, snapshot, evaluated_runs, passing_runs);

    BenchmarkGateResult {
        passed,
        gate_profile: config.gate_profile.clone(),
        thresholds: BenchmarkGateThresholds {
            threshold_p95_ms: config.threshold_p95_ms,
            min_top1_accuracy: config.min_top1_accuracy,
            min_stress_top1_accuracy: config.min_stress_top1_accuracy,
            max_p95_regression_pct: config.max_p95_regression_pct,
            max_top1_regression_pct: config.max_top1_regression_pct,
        },
        quorum: BenchmarkGateQuorum {
            window_size: config.window_size,
            required_passes: config.required_passes,
        },
        snapshot: BenchmarkGateSnapshot {
            latest: snapshot.latest.clone(),
            previous: snapshot.previous.clone(),
            regression_pct: snapshot.regression_pct,
            top1_regression_pct: snapshot.top1_regression_pct,
            stress_top1_accuracy: snapshot.stress_top1_accuracy,
        },
        execution: BenchmarkGateExecution {
            evaluated_runs,
            passing_runs,
            run_results,
            reasons,
        },
        artifacts: BenchmarkGateArtifacts {
            gate_record_uri: None,
            release_check_uri: None,
            embedding_provider: snapshot.embedding_provider.clone(),
            embedding_strict_error: snapshot.embedding_strict_error.clone(),
        },
    }
}

fn evaluate_gate_outcome(
    config: &GateConfig,
    snapshot: &GateSnapshot,
    evaluated_runs: usize,
    passing_runs: usize,
) -> (bool, Vec<String>) {
    let mut passed = true;
    let mut reasons = Vec::<String>::new();

    if evaluated_runs < config.required_passes {
        passed = false;
        reasons.push(format!(
            "insufficient_history:{evaluated_runs}<{}",
            config.required_passes
        ));
    }
    if passing_runs < config.required_passes {
        passed = false;
        reasons.push(format!(
            "pass_quorum_not_met:{passing_runs}<{}",
            config.required_passes
        ));
    }

    if config.require_release_embedder {
        let latest_provider = snapshot.embedding_provider.as_deref().unwrap_or("unknown");
        if latest_provider != REQUIRED_RELEASE_EMBEDDING_PROVIDER {
            passed = false;
            reasons.push(format!(
                "release_embedding_provider_required:{latest_provider}!={REQUIRED_RELEASE_EMBEDDING_PROVIDER}"
            ));
        }
        if let Some(strict_err) = snapshot.embedding_strict_error.as_deref() {
            passed = false;
            reasons.push(format!("release_embedding_strict_error:{strict_err}"));
        }
    }

    if let Some(pct) = snapshot.semantic_ndcg_regression_pct
        && pct > MAX_SEMANTIC_QUALITY_REGRESSION_PCT
    {
        passed = false;
        reasons.push(format!(
            "latest_ndcg_regression_exceeded:{pct:.2}%>{MAX_SEMANTIC_QUALITY_REGRESSION_PCT:.2}%"
        ));
    }
    if let Some(pct) = snapshot.semantic_recall_regression_pct
        && pct > MAX_SEMANTIC_QUALITY_REGRESSION_PCT
    {
        passed = false;
        reasons.push(format!(
            "latest_recall_regression_exceeded:{pct:.2}%>{MAX_SEMANTIC_QUALITY_REGRESSION_PCT:.2}%"
        ));
    }

    if passed {
        reasons.push("ok".to_string());
    }

    (passed, reasons)
}

fn evaluate_gate_run(
    report: &BenchmarkReport,
    prev_report: Option<&BenchmarkReport>,
    config: &GateConfig,
) -> BenchmarkGateRunResult {
    let mut passed = true;
    let mut reasons = Vec::<String>::new();

    if report.latency.find.p95_ms > config.threshold_p95_ms {
        passed = false;
        reasons.push(format!(
            "p95_latency_exceeded:{}>{}",
            report.latency.find.p95_ms, config.threshold_p95_ms
        ));
    }
    if report.quality.top1_accuracy < config.min_top1_accuracy {
        passed = false;
        reasons.push(format!(
            "top1_accuracy_below:{:.4}<{:.4}",
            report.quality.top1_accuracy, config.min_top1_accuracy
        ));
    }

    let run_stress_top1_accuracy = stress_top1_accuracy(report);
    if let Some(min_stress_top1_accuracy) = config.min_stress_top1_accuracy {
        if let Some(stress_accuracy) = run_stress_top1_accuracy {
            if stress_accuracy < min_stress_top1_accuracy {
                passed = false;
                reasons.push(format!(
                    "stress_top1_accuracy_below:{stress_accuracy:.4}<{min_stress_top1_accuracy:.4}"
                ));
            }
        } else {
            passed = false;
            reasons.push("stress_queries_missing".to_string());
        }
    }

    let run_regression_pct = prev_report.and_then(|prev| {
        percent_delta_u128(
            p95_latency_for_regression(report),
            p95_latency_for_regression(prev),
        )
    });
    if let (Some(max_regression), Some(pct)) = (config.max_p95_regression_pct, run_regression_pct)
        && pct > max_regression
    {
        passed = false;
        reasons.push(format!(
            "p95_regression_exceeded:{pct:.2}%>{max_regression:.2}%"
        ));
    }

    let run_top1_regression_pct = prev_report.and_then(|prev| {
        percent_drop_f32(report.quality.top1_accuracy, prev.quality.top1_accuracy)
    });
    if let (Some(max_regression), Some(pct)) =
        (config.max_top1_regression_pct, run_top1_regression_pct)
        && pct > max_regression
    {
        passed = false;
        reasons.push(format!(
            "top1_regression_exceeded:{pct:.2}%>{max_regression:.2}%"
        ));
    }

    if let Some(prev) = prev_report
        && semantic_quality_regression_eligible(report, prev)
    {
        if let Some(pct) = percent_drop_f32(report.quality.ndcg_at_10, prev.quality.ndcg_at_10)
            && pct > MAX_SEMANTIC_QUALITY_REGRESSION_PCT
        {
            passed = false;
            reasons.push(format!(
                "ndcg_regression_exceeded:{pct:.2}%>{MAX_SEMANTIC_QUALITY_REGRESSION_PCT:.2}%"
            ));
        }
        if let Some(pct) = percent_drop_f32(report.quality.recall_at_10, prev.quality.recall_at_10)
            && pct > MAX_SEMANTIC_QUALITY_REGRESSION_PCT
        {
            passed = false;
            reasons.push(format!(
                "recall_regression_exceeded:{pct:.2}%>{MAX_SEMANTIC_QUALITY_REGRESSION_PCT:.2}%"
            ));
        }
    }

    if passed {
        reasons.push("ok".to_string());
    }

    BenchmarkGateRunResult {
        run_id: report.run_id.clone(),
        passed,
        p95_latency_ms: report.latency.find.p95_ms,
        p95_latency_us: report.latency.find.p95_us,
        top1_accuracy: report.quality.top1_accuracy,
        stress_top1_accuracy: run_stress_top1_accuracy,
        regression_pct: run_regression_pct,
        top1_regression_pct: run_top1_regression_pct,
        reasons,
    }
}

fn stress_top1_accuracy(report: &BenchmarkReport) -> Option<f32> {
    let mut stress_total = 0.0f32;
    let mut stress_passed = 0.0f32;

    for result in &report.results {
        if !result.source.starts_with("stress:") {
            continue;
        }
        stress_total += 1.0;
        if result.passed {
            stress_passed += 1.0;
        }
    }

    if stress_total > 0.0 {
        Some(stress_passed / stress_total)
    } else {
        None
    }
}

fn p95_latency_for_regression(report: &BenchmarkReport) -> u128 {
    regression_latency_value(report.latency.find.p95_ms, report.latency.find.p95_us)
}

fn regression_latency_value(p95_latency_ms: u128, p95_latency_us: Option<u128>) -> u128 {
    p95_latency_us.unwrap_or(p95_latency_ms.saturating_mul(1_000))
}

const fn semantic_quality_regression_eligible(
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

#[cfg(test)]
mod tests {
    use super::{percent_delta_u128, regression_latency_value};

    #[test]
    fn regression_latency_value_prefers_microseconds_when_present() {
        assert_eq!(regression_latency_value(1, Some(437)), 437);
        assert_eq!(regression_latency_value(1, Some(901)), 901);
    }

    #[test]
    fn regression_latency_value_falls_back_to_millisecond_basis() {
        assert_eq!(regression_latency_value(0, None), 0);
        assert_eq!(regression_latency_value(1, None), 1_000);
    }

    #[test]
    fn percent_delta_uses_microsecond_basis_when_millisecond_values_tie() {
        let current = regression_latency_value(1, Some(437));
        let previous = regression_latency_value(1, Some(465));
        let pct = percent_delta_u128(current, previous).expect("regression pct");
        assert!(pct < 0.0, "expected improvement pct, got {pct}");
    }
}
