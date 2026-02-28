use chrono::Utc;

use crate::models::{
    BenchmarkGateDetails, BenchmarkGateResult, BlockerRollupGateDetails, EvalLoopReport,
    EvalQualityGateDetails, OperabilityEvidenceReport, OperabilityGateDetails, ReleaseGateDecision,
    ReleaseGateDetails, ReleaseGateId, ReleaseGatePackReport, ReleaseGateStatus,
    ReleaseSecurityAuditMode, ReliabilityEvidenceReport, ReliabilityGateDetails,
    SecurityAuditGateDetails, SecurityAuditReport, SessionMemoryGateDetails,
};

const RELEASE_EVAL_MIN_TOP1_ACCURACY: f32 = 0.75;

pub(super) fn gate_decision(
    gate_id: ReleaseGateId,
    passed: bool,
    details: ReleaseGateDetails,
    evidence_uri: Option<String>,
) -> ReleaseGateDecision {
    ReleaseGateDecision {
        gate_id,
        passed,
        status: ReleaseGateStatus::from_passed(passed),
        details,
        evidence_uri,
    }
}

pub(super) fn reliability_evidence_gate_decision(
    report: &ReliabilityEvidenceReport,
) -> ReleaseGateDecision {
    gate_decision(
        ReleaseGateId::ReliabilityEvidence,
        report.passed,
        ReleaseGateDetails::ReliabilityEvidence(ReliabilityGateDetails {
            status: report.status,
            replay_done: report.replay_progress.replay_totals.done,
            dead_letter: report.queue_delta.final_dead_letter,
        }),
        Some(report.report_uri.clone()),
    )
}

pub(super) fn eval_quality_gate_decision(report: &EvalLoopReport) -> ReleaseGateDecision {
    let filter_ignored = eval_bucket_count(report, "filter_ignored");
    let relation_missing = eval_bucket_count(report, "relation_missing");
    let passed = report.coverage.executed_cases > 0
        && report.quality.top1_accuracy >= RELEASE_EVAL_MIN_TOP1_ACCURACY
        && filter_ignored == 0
        && relation_missing == 0;
    gate_decision(
        ReleaseGateId::EvalQuality,
        passed,
        ReleaseGateDetails::EvalQuality(EvalQualityGateDetails {
            executed_cases: report.coverage.executed_cases,
            top1_accuracy: report.quality.top1_accuracy,
            min_top1_accuracy: RELEASE_EVAL_MIN_TOP1_ACCURACY,
            failed: report.quality.failed,
            filter_ignored,
            relation_missing,
        }),
        Some(report.artifacts.report_uri.clone()),
    )
}

pub(super) fn session_memory_gate_decision(
    passed: bool,
    memory_category_miss: usize,
    details: &str,
) -> ReleaseGateDecision {
    let gate_passed = passed && memory_category_miss == 0;
    gate_decision(
        ReleaseGateId::SessionMemory,
        gate_passed,
        ReleaseGateDetails::SessionMemory(SessionMemoryGateDetails {
            base_details: details.to_string(),
            memory_category_miss,
        }),
        None,
    )
}

pub(super) fn security_audit_gate_decision(report: &SecurityAuditReport) -> ReleaseGateDecision {
    let strict_mode = report.dependency_audit.mode == ReleaseSecurityAuditMode::Strict;
    let passed = report.passed && strict_mode;
    gate_decision(
        ReleaseGateId::SecurityAudit,
        passed,
        ReleaseGateDetails::SecurityAudit(SecurityAuditGateDetails {
            status: report.status,
            mode: report.dependency_audit.mode,
            strict_mode_required: true,
            strict_mode,
            audit_status: report.dependency_audit.status,
            advisories_found: report.dependency_audit.advisories_found,
            packages: report.inventory.package_count,
        }),
        Some(report.report_uri.clone()),
    )
}

pub(super) fn benchmark_release_gate_decision(report: &BenchmarkGateResult) -> ReleaseGateDecision {
    let evidence_uri = report
        .artifacts
        .release_check_uri
        .clone()
        .or_else(|| report.artifacts.gate_record_uri.clone());
    gate_decision(
        ReleaseGateId::Benchmark,
        report.passed,
        ReleaseGateDetails::Benchmark(BenchmarkGateDetails {
            passed: report.passed,
            evaluated_runs: report.execution.evaluated_runs,
            passing_runs: report.execution.passing_runs,
            reasons: report.execution.reasons.clone(),
        }),
        evidence_uri,
    )
}

pub(super) fn operability_evidence_gate_decision(
    report: &OperabilityEvidenceReport,
) -> ReleaseGateDecision {
    gate_decision(
        ReleaseGateId::OperabilityEvidence,
        report.passed,
        ReleaseGateDetails::OperabilityEvidence(OperabilityGateDetails {
            status: report.status,
            traces_analyzed: report.coverage.traces_analyzed,
            request_logs_scanned: report.coverage.request_logs_scanned,
        }),
        Some(report.report_uri.clone()),
    )
}

pub(super) fn unresolved_blockers(decisions: &[ReleaseGateDecision]) -> usize {
    decisions.iter().filter(|decision| !decision.passed).count()
}

pub(super) fn blocker_rollup_gate_decision(unresolved_blockers: usize) -> ReleaseGateDecision {
    gate_decision(
        ReleaseGateId::BlockerRollup,
        unresolved_blockers == 0,
        ReleaseGateDetails::BlockerRollup(BlockerRollupGateDetails {
            unresolved_blockers,
        }),
        None,
    )
}

pub(super) fn finalize_release_gate_pack_report(
    pack_id: String,
    workspace_dir: String,
    mut decisions: Vec<ReleaseGateDecision>,
    report_uri: String,
) -> ReleaseGatePackReport {
    let unresolved_blockers = super::unresolved_blockers(&decisions);
    let g8 = super::blocker_rollup_gate_decision(unresolved_blockers);
    let passed = g8.passed;
    decisions.push(g8);

    ReleaseGatePackReport {
        pack_id,
        created_at: Utc::now().to_rfc3339(),
        workspace_dir,
        passed,
        status: ReleaseGateStatus::from_passed(passed),
        unresolved_blockers,
        decisions,
        report_uri,
    }
}

fn eval_bucket_count(report: &EvalLoopReport, name: &str) -> usize {
    report
        .quality
        .buckets
        .iter()
        .find(|bucket| bucket.name == name)
        .map_or(0, |bucket| bucket.count)
}
