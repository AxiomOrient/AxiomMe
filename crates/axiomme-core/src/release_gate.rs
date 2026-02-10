use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Utc;

use crate::error::{AxiomError, Result};
use crate::models::{
    BenchmarkGateResult, EvalLoopReport, OperabilityEvidenceReport, ReleaseGateDecision,
    ReleaseGatePackReport, ReliabilityEvidenceReport, SecurityAuditReport,
};
use crate::uri::{AxiomUri, Scope};

const RELEASE_EVAL_MIN_TOP1_ACCURACY: f32 = 0.75;
const CONTRACT_EXECUTION_TEST_NAME: &str =
    "client::tests::contract_execution_probe_validates_core_algorithms";

pub(crate) fn release_gate_pack_report_uri(pack_id: &str) -> Result<AxiomUri> {
    AxiomUri::root(Scope::Queue)
        .join("release")?
        .join("packs")?
        .join(&format!("{pack_id}.json"))
}

pub(crate) fn gate_decision(
    gate_id: &str,
    passed: bool,
    details: String,
    evidence_uri: Option<String>,
) -> ReleaseGateDecision {
    ReleaseGateDecision {
        gate_id: gate_id.to_string(),
        passed,
        status: gate_status(passed),
        details,
        evidence_uri,
    }
}

pub(crate) fn reliability_evidence_gate_decision(
    report: &ReliabilityEvidenceReport,
) -> ReleaseGateDecision {
    gate_decision(
        "G2",
        report.passed,
        format!(
            "status={} replay_done={} dead_letter={}",
            report.status, report.replay_totals.done, report.final_dead_letter
        ),
        Some(report.report_uri.clone()),
    )
}

pub(crate) fn eval_quality_gate_decision(report: &EvalLoopReport) -> ReleaseGateDecision {
    let filter_ignored = eval_bucket_count(report, "filter_ignored");
    let relation_missing = eval_bucket_count(report, "relation_missing");
    let passed = report.executed_cases > 0
        && report.top1_accuracy >= RELEASE_EVAL_MIN_TOP1_ACCURACY
        && filter_ignored == 0
        && relation_missing == 0;
    gate_decision(
        "G3",
        passed,
        format!(
            "executed_cases={} top1_accuracy={:.4} failed={} filter_ignored={} relation_missing={}",
            report.executed_cases,
            report.top1_accuracy,
            report.failed,
            filter_ignored,
            relation_missing
        ),
        Some(report.report_uri.clone()),
    )
}

pub(crate) fn session_memory_gate_decision(
    passed: bool,
    memory_category_miss: usize,
    details: String,
) -> ReleaseGateDecision {
    let gate_passed = passed && memory_category_miss == 0;
    gate_decision(
        "G4",
        gate_passed,
        format!("{details} memory_category_miss={memory_category_miss}"),
        None,
    )
}

pub(crate) fn security_audit_gate_decision(report: &SecurityAuditReport) -> ReleaseGateDecision {
    gate_decision(
        "G5",
        report.passed,
        format!(
            "status={} advisories_found={} packages={}",
            report.status, report.dependency_audit.advisories_found, report.inventory.package_count
        ),
        Some(report.report_uri.clone()),
    )
}

pub(crate) fn benchmark_release_gate_decision(report: &BenchmarkGateResult) -> ReleaseGateDecision {
    let evidence_uri = report
        .release_check_uri
        .clone()
        .or_else(|| report.gate_record_uri.clone());
    gate_decision(
        "G6",
        report.passed,
        format!(
            "passed={} evaluated_runs={} passing_runs={} reasons={}",
            report.passed,
            report.evaluated_runs,
            report.passing_runs,
            report.reasons.join(",")
        ),
        evidence_uri,
    )
}

pub(crate) fn operability_evidence_gate_decision(
    report: &OperabilityEvidenceReport,
) -> ReleaseGateDecision {
    gate_decision(
        "G7",
        report.passed,
        format!(
            "status={} traces_analyzed={} request_logs_scanned={}",
            report.status, report.traces_analyzed, report.request_logs_scanned
        ),
        Some(report.report_uri.clone()),
    )
}

pub(crate) fn unresolved_blockers(decisions: &[ReleaseGateDecision]) -> usize {
    decisions.iter().filter(|d| !d.passed).count()
}

pub(crate) fn blocker_rollup_gate_decision(unresolved_blockers: usize) -> ReleaseGateDecision {
    gate_decision(
        "G8",
        unresolved_blockers == 0,
        format!("unresolved_blockers={unresolved_blockers}"),
        None,
    )
}

pub(crate) fn finalize_release_gate_pack_report(
    pack_id: String,
    workspace_dir: String,
    mut decisions: Vec<ReleaseGateDecision>,
    report_uri: String,
) -> ReleaseGatePackReport {
    let unresolved_blockers = unresolved_blockers(&decisions);
    let g8 = blocker_rollup_gate_decision(unresolved_blockers);
    let passed = g8.passed;
    decisions.push(g8);

    ReleaseGatePackReport {
        pack_id,
        created_at: Utc::now().to_rfc3339(),
        workspace_dir,
        passed,
        status: gate_status(passed),
        unresolved_blockers,
        decisions,
        report_uri,
    }
}

pub(crate) fn resolve_workspace_dir(workspace_dir: Option<&str>) -> Result<PathBuf> {
    let input = workspace_dir.unwrap_or(".");
    let raw = PathBuf::from(input);
    let absolute = if raw.is_absolute() {
        raw
    } else {
        std::env::current_dir()?.join(raw)
    };
    if !absolute.exists() {
        return Err(AxiomError::NotFound(format!(
            "workspace directory not found: {}",
            absolute.display()
        )));
    }
    let workspace = fs::canonicalize(absolute)?;
    if !workspace.join("Cargo.toml").exists() {
        return Err(AxiomError::Validation(format!(
            "workspace missing Cargo.toml: {}",
            workspace.display()
        )));
    }
    Ok(workspace)
}

pub(crate) fn evaluate_contract_integrity_gate(workspace_dir: &Path) -> ReleaseGateDecision {
    let required = [
        workspace_dir.join("docs").join("API_CONTRACT.md"),
        workspace_dir.join("docs").join("FEATURE_SPEC.md"),
        workspace_dir.join("plan").join("TASKS.md"),
        workspace_dir.join("plan").join("QUALITY_GATES.md"),
    ];
    let missing = required
        .iter()
        .filter(|path| !path.exists())
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    let mut marker_errors = Vec::<String>::new();

    let api_contract =
        fs::read_to_string(workspace_dir.join("docs").join("API_CONTRACT.md")).unwrap_or_default();
    for marker in [
        "run_security_audit",
        "collect_operability_evidence",
        "collect_reliability_evidence",
    ] {
        if !api_contract.contains(marker) {
            marker_errors.push(format!("missing_marker:{marker}"));
        }
    }

    let (contract_exec_passed, contract_exec_details) = run_contract_execution_probe(workspace_dir);
    let p1_gap_errors = collect_p1_gap_errors(workspace_dir);
    let passed = missing.is_empty()
        && marker_errors.is_empty()
        && contract_exec_passed
        && p1_gap_errors.is_empty();
    let details = if passed {
        "required docs, extension markers, contract execution probe, and P1 gap checks are present"
            .to_string()
    } else {
        format!(
            "missing_files={} marker_errors={} contract_exec={} p1_gap_errors={}",
            if missing.is_empty() {
                "-".to_string()
            } else {
                missing.join(";")
            },
            if marker_errors.is_empty() {
                "-".to_string()
            } else {
                marker_errors.join(";")
            },
            contract_exec_details,
            if p1_gap_errors.is_empty() {
                "-".to_string()
            } else {
                p1_gap_errors.join(";")
            }
        )
    };

    gate_decision("G0", passed, details, None)
}

pub(crate) fn evaluate_build_quality_gate(workspace_dir: &Path) -> ReleaseGateDecision {
    let check = run_workspace_command(workspace_dir, "cargo", &["check", "--workspace"]);
    let fmt = run_workspace_command(workspace_dir, "cargo", &["fmt", "--all", "--check"]);
    let clippy = run_workspace_command(
        workspace_dir,
        "cargo",
        &["clippy", "--workspace", "--", "-D", "warnings"],
    );
    let passed = check.0 && fmt.0 && clippy.0;
    let details = format!(
        "cargo_check={} cargo_fmt={} cargo_clippy={} check_output={} fmt_output={} clippy_output={}",
        check.0,
        fmt.0,
        clippy.0,
        truncate_text(&check.1, 240),
        truncate_text(&fmt.1, 240),
        truncate_text(&clippy.1, 240)
    );
    gate_decision("G1", passed, details, None)
}

fn run_workspace_command(workspace_dir: &Path, cmd: &str, args: &[&str]) -> (bool, String) {
    match Command::new(cmd)
        .args(args)
        .current_dir(workspace_dir)
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let text = first_non_empty_output(&stdout, &stderr).unwrap_or_default();
            (output.status.success(), text)
        }
        Err(err) => (false, err.to_string()),
    }
}

fn eval_bucket_count(report: &EvalLoopReport, name: &str) -> usize {
    report
        .buckets
        .iter()
        .find(|bucket| bucket.name == name)
        .map(|bucket| bucket.count)
        .unwrap_or(0)
}

fn run_contract_execution_probe(workspace_dir: &Path) -> (bool, String) {
    let core_crate = workspace_dir
        .join("crates")
        .join("axiomme-core")
        .join("Cargo.toml");
    if !core_crate.exists() {
        return (false, "missing_axiomme_core_crate".to_string());
    }

    let (ok, output) = run_workspace_command(
        workspace_dir,
        "cargo",
        &[
            "test",
            "-p",
            "axiomme-core",
            CONTRACT_EXECUTION_TEST_NAME,
            "--",
            "--exact",
        ],
    );
    let matched = output.contains(CONTRACT_EXECUTION_TEST_NAME) && output.contains("ok");
    (
        ok && matched,
        format!(
            "status={} matched={} output={}",
            ok,
            matched,
            truncate_text(&output, 200)
        ),
    )
}

fn collect_p1_gap_errors(workspace_dir: &Path) -> Vec<String> {
    let task_paths = [
        workspace_dir.join("al-plan").join("TASKS.md"),
        workspace_dir.join("plan").join("TASKS.md"),
    ];

    let mut selected = None;
    for path in &task_paths {
        if path.exists() {
            selected = Some(path);
            break;
        }
    }
    let Some(task_path) = selected else {
        return vec!["tasks_file_missing".to_string()];
    };

    let Ok(raw) = fs::read_to_string(task_path) else {
        return vec![format!("tasks_file_unreadable:{}", task_path.display())];
    };

    let mut unresolved = Vec::<String>::new();
    for line in raw.lines() {
        let columns = line
            .split('|')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        let id = columns.first().copied().unwrap_or("ALG-UNKNOWN");
        if !id.starts_with("ALG-T") {
            continue;
        }
        if line.contains("| DONE |") {
            continue;
        }
        unresolved.push(format!("p1_gap_open:{id}"));
    }
    unresolved
}

fn gate_status(passed: bool) -> String {
    if passed {
        "pass".to_string()
    } else {
        "fail".to_string()
    }
}

fn first_non_empty_output(stdout: &str, stderr: &str) -> Option<String> {
    if !stdout.trim().is_empty() {
        return Some(stdout.to_string());
    }
    if !stderr.trim().is_empty() {
        return Some(stderr.to_string());
    }
    None
}

fn truncate_text(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_string()
    } else {
        format!("{}...", &text[..max.min(text.len())])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        BenchmarkGateRunResult, BenchmarkSummary, DependencyAuditSummary,
        DependencyInventorySummary, EvalBucket, EvalCaseResult,
    };

    fn eval_report(executed_cases: usize, top1_accuracy: f32) -> EvalLoopReport {
        EvalLoopReport {
            run_id: "run-1".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            trace_limit: 10,
            query_limit: 10,
            search_limit: 5,
            include_golden: true,
            golden_only: false,
            traces_scanned: 10,
            trace_cases_used: 5,
            golden_cases_used: 5,
            executed_cases,
            passed: 0,
            failed: 0,
            top1_accuracy,
            buckets: Vec::<EvalBucket>::new(),
            report_uri: "axiom://queue/eval/reports/x.json".to_string(),
            query_set_uri: "axiom://queue/eval/query_sets/x.json".to_string(),
            markdown_report_uri: "axiom://queue/eval/reports/x.md".to_string(),
            failures: Vec::<EvalCaseResult>::new(),
        }
    }

    fn benchmark_gate_result(
        release_check_uri: Option<&str>,
        gate_record_uri: Option<&str>,
    ) -> BenchmarkGateResult {
        BenchmarkGateResult {
            passed: true,
            gate_profile: "rc-release".to_string(),
            threshold_p95_ms: 1000,
            min_top1_accuracy: 0.75,
            max_p95_regression_pct: Some(0.1),
            max_top1_regression_pct: Some(2.0),
            window_size: 3,
            required_passes: 1,
            evaluated_runs: 1,
            passing_runs: 1,
            latest: Some(BenchmarkSummary {
                run_id: "run".to_string(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                executed_cases: 10,
                top1_accuracy: 0.9,
                p95_latency_ms: 700,
                report_uri: "axiom://queue/benchmarks/reports/run.json".to_string(),
            }),
            previous: None,
            regression_pct: None,
            top1_regression_pct: None,
            run_results: vec![BenchmarkGateRunResult {
                run_id: "run".to_string(),
                passed: true,
                p95_latency_ms: 700,
                top1_accuracy: 0.9,
                regression_pct: None,
                top1_regression_pct: None,
                reasons: vec!["ok".to_string()],
            }],
            gate_record_uri: gate_record_uri.map(ToString::to_string),
            release_check_uri: release_check_uri.map(ToString::to_string),
            reasons: vec!["ok".to_string()],
        }
    }

    #[test]
    fn eval_quality_gate_decision_respects_threshold_and_case_count() {
        let no_cases = eval_quality_gate_decision(&eval_report(0, 1.0));
        assert!(!no_cases.passed);

        let low_accuracy = eval_quality_gate_decision(&eval_report(10, 0.5));
        assert!(!low_accuracy.passed);

        let passing = eval_quality_gate_decision(&eval_report(10, 0.9));
        assert!(passing.passed);
    }

    #[test]
    fn eval_quality_gate_decision_fails_when_filter_or_relation_buckets_exist() {
        let mut report = eval_report(10, 0.9);
        report.buckets = vec![
            EvalBucket {
                name: "filter_ignored".to_string(),
                count: 1,
            },
            EvalBucket {
                name: "relation_missing".to_string(),
                count: 0,
            },
        ];
        let decision = eval_quality_gate_decision(&report);
        assert!(!decision.passed);
        assert!(decision.details.contains("filter_ignored=1"));
    }

    #[test]
    fn session_memory_gate_decision_fails_when_category_missing() {
        let decision = session_memory_gate_decision(true, 2, "probe".to_string());
        assert!(!decision.passed);
        assert!(decision.details.contains("memory_category_miss=2"));
    }

    #[test]
    fn benchmark_gate_prefers_release_check_evidence_uri() {
        let decision = benchmark_release_gate_decision(&benchmark_gate_result(
            Some("axiom://queue/release/checks/1.json"),
            Some("axiom://queue/release/gates/1.json"),
        ));
        assert_eq!(
            decision.evidence_uri.as_deref(),
            Some("axiom://queue/release/checks/1.json")
        );
    }

    #[test]
    fn finalize_release_gate_pack_adds_g8_and_counts_blockers() {
        let decisions = vec![
            gate_decision("G0", true, "ok".to_string(), None),
            gate_decision("G1", false, "failed".to_string(), None),
        ];
        let report = finalize_release_gate_pack_report(
            "pack-1".to_string(),
            "/tmp/ws".to_string(),
            decisions,
            "axiom://queue/release/packs/pack-1.json".to_string(),
        );
        assert!(!report.passed);
        assert_eq!(report.status, "fail");
        assert_eq!(report.unresolved_blockers, 1);
        let g8 = report.decisions.last().expect("g8");
        assert_eq!(g8.gate_id, "G8");
        assert!(!g8.passed);
        assert!(g8.details.contains("unresolved_blockers=1"));
    }

    #[test]
    fn security_audit_gate_decision_contains_expected_summary() {
        let report = SecurityAuditReport {
            report_id: "sec-1".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            workspace_dir: "/tmp/ws".to_string(),
            passed: true,
            status: "pass".to_string(),
            inventory: DependencyInventorySummary {
                lockfile_present: true,
                package_count: 42,
            },
            dependency_audit: DependencyAuditSummary {
                tool: "cargo-audit".to_string(),
                available: true,
                executed: true,
                status: "passed".to_string(),
                advisories_found: 0,
                tool_version: Some("cargo-audit 1.0".to_string()),
                output_excerpt: None,
            },
            checks: Vec::new(),
            report_uri: "axiom://queue/release/security/sec-1.json".to_string(),
        };
        let decision = security_audit_gate_decision(&report);
        assert!(decision.details.contains("advisories_found=0"));
        assert_eq!(
            decision.evidence_uri.as_deref(),
            Some("axiom://queue/release/security/sec-1.json")
        );
    }
}
