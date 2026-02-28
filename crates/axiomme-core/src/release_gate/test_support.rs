use std::fs;
use std::path::Path;

use crate::models::{
    BenchmarkGateArtifacts, BenchmarkGateExecution, BenchmarkGateQuorum, BenchmarkGateResult,
    BenchmarkGateRunResult, BenchmarkGateSnapshot, BenchmarkGateThresholds, BenchmarkSummary,
    EvalBucket, EvalCaseResult, EvalLoopReport,
};

pub(super) fn eval_report(executed_cases: usize, top1_accuracy: f32) -> EvalLoopReport {
    EvalLoopReport {
        run_id: "run-1".to_string(),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        selection: crate::models::EvalRunSelection {
            trace_limit: 10,
            query_limit: 10,
            search_limit: 5,
            include_golden: true,
            golden_only: false,
        },
        coverage: crate::models::EvalCoverageSummary {
            traces_scanned: 10,
            trace_cases_used: 5,
            golden_cases_used: 5,
            executed_cases,
        },
        quality: crate::models::EvalQualitySummary {
            passed: 0,
            failed: 0,
            top1_accuracy,
            buckets: Vec::<EvalBucket>::new(),
            failures: Vec::<EvalCaseResult>::new(),
        },
        artifacts: crate::models::EvalArtifacts {
            report_uri: "axiom://queue/eval/reports/x.json".to_string(),
            query_set_uri: "axiom://queue/eval/query_sets/x.json".to_string(),
            markdown_report_uri: "axiom://queue/eval/reports/x.md".to_string(),
        },
    }
}

pub(super) fn benchmark_gate_result(
    release_check_uri: Option<&str>,
    gate_record_uri: Option<&str>,
) -> BenchmarkGateResult {
    BenchmarkGateResult {
        passed: true,
        gate_profile: "rc-release".to_string(),
        thresholds: BenchmarkGateThresholds {
            threshold_p95_ms: 1000,
            min_top1_accuracy: 0.75,
            min_stress_top1_accuracy: None,
            max_p95_regression_pct: Some(0.1),
            max_top1_regression_pct: Some(2.0),
        },
        quorum: BenchmarkGateQuorum {
            window_size: 3,
            required_passes: 1,
        },
        snapshot: BenchmarkGateSnapshot {
            latest: Some(BenchmarkSummary {
                run_id: "run".to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                executed_cases: 10,
                top1_accuracy: 0.9,
                p95_latency_ms: 700,
                p95_latency_us: Some(699_420),
                report_uri: "axiom://queue/benchmarks/reports/run.json".to_string(),
            }),
            previous: None,
            regression_pct: None,
            top1_regression_pct: None,
            stress_top1_accuracy: None,
        },
        execution: BenchmarkGateExecution {
            evaluated_runs: 1,
            passing_runs: 1,
            run_results: vec![BenchmarkGateRunResult {
                run_id: "run".to_string(),
                passed: true,
                p95_latency_ms: 700,
                p95_latency_us: Some(699_420),
                top1_accuracy: 0.9,
                stress_top1_accuracy: None,
                regression_pct: None,
                top1_regression_pct: None,
                reasons: vec!["ok".to_string()],
            }],
            reasons: vec!["ok".to_string()],
        },
        artifacts: BenchmarkGateArtifacts {
            gate_record_uri: gate_record_uri.map(ToString::to_string),
            release_check_uri: release_check_uri.map(ToString::to_string),
            embedding_provider: Some("semantic-model-http".to_string()),
            embedding_strict_error: None,
        },
    }
}

pub(super) fn write_contract_gate_workspace_fixture(
    root: &Path,
    episodic_dep: &str,
    lock_source: Option<&str>,
) {
    let core = root.join("crates").join("axiomme-core");
    fs::create_dir_all(&core).expect("mkdir core");

    fs::write(
        core.join("Cargo.toml"),
        format!(
            "[package]\nname=\"axiomme-core\"\nversion=\"0.1.0\"\n\n[dependencies]\n{episodic_dep}\n"
        ),
    )
    .expect("write core cargo");

    let lock_source_line = lock_source
        .map(|value| format!("source = \"{value}\"\n"))
        .unwrap_or_default();
    fs::write(
        root.join("Cargo.lock"),
        format!("[[package]]\nname = \"episodic\"\nversion = \"0.1.0\"\n{lock_source_line}\n"),
    )
    .expect("write lockfile");
}
