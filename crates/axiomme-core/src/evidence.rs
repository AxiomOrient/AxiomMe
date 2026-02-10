use crate::models::{
    OperabilityEvidenceCheck, QueueDiagnostics, ReliabilityEvidenceCheck, ReplayReport,
};

pub(crate) fn accumulate_replay_report(total: &mut ReplayReport, report: &ReplayReport) {
    total.fetched = total.fetched.saturating_add(report.fetched);
    total.processed = total.processed.saturating_add(report.processed);
    total.done = total.done.saturating_add(report.done);
    total.dead_letter = total.dead_letter.saturating_add(report.dead_letter);
    total.requeued = total.requeued.saturating_add(report.requeued);
    total.skipped = total.skipped.saturating_add(report.skipped);
}

pub(crate) fn checkpoint_advanced(baseline: Option<i64>, final_checkpoint: Option<i64>) -> bool {
    match (baseline, final_checkpoint) {
        (Some(before), Some(after)) => after > before,
        (None, Some(_)) => true,
        _ => false,
    }
}

pub(crate) fn build_operability_evidence_checks(
    request_logs_scanned: usize,
    traces_analyzed: usize,
    request_type_count: usize,
    queue: &QueueDiagnostics,
) -> Vec<OperabilityEvidenceCheck> {
    vec![
        OperabilityEvidenceCheck {
            name: "request_logs_present".to_string(),
            passed: request_logs_scanned > 0,
            details: format!("count={request_logs_scanned}"),
        },
        OperabilityEvidenceCheck {
            name: "trace_metrics_present".to_string(),
            passed: traces_analyzed > 0,
            details: format!(
                "traces_analyzed={} request_types={}",
                traces_analyzed, request_type_count
            ),
        },
        OperabilityEvidenceCheck {
            name: "queue_diagnostics_collected".to_string(),
            passed: true,
            details: format!(
                "new_due={} processing={} dead_letter={}",
                queue.counts.new_due, queue.counts.processing, queue.counts.dead_letter
            ),
        },
    ]
}

pub(crate) struct ReliabilityEvidenceInput<'a> {
    pub replay_totals: &'a ReplayReport,
    pub queue_after_replay: &'a QueueDiagnostics,
    pub baseline_dead_letter: u64,
    pub final_dead_letter: u64,
    pub baseline_checkpoint: Option<i64>,
    pub final_checkpoint: Option<i64>,
    pub replay_hit_uri: Option<&'a str>,
    pub restart_hit_uri: Option<&'a str>,
}

pub(crate) fn build_reliability_evidence_checks(
    input: &ReliabilityEvidenceInput<'_>,
) -> Vec<ReliabilityEvidenceCheck> {
    let checkpoint_is_advanced =
        checkpoint_advanced(input.baseline_checkpoint, input.final_checkpoint);
    vec![
        ReliabilityEvidenceCheck {
            name: "replay_processed_events".to_string(),
            passed: input.replay_totals.done > 0,
            details: format!(
                "fetched={} processed={} done={} requeued={} dead_letter={}",
                input.replay_totals.fetched,
                input.replay_totals.processed,
                input.replay_totals.done,
                input.replay_totals.requeued,
                input.replay_totals.dead_letter
            ),
        },
        ReliabilityEvidenceCheck {
            name: "queue_drained_after_replay".to_string(),
            passed: input.queue_after_replay.counts.new_due == 0
                && input.queue_after_replay.counts.processing == 0,
            details: format!(
                "new_due={} processing={} dead_letter={}",
                input.queue_after_replay.counts.new_due,
                input.queue_after_replay.counts.processing,
                input.queue_after_replay.counts.dead_letter
            ),
        },
        ReliabilityEvidenceCheck {
            name: "dead_letter_not_increased".to_string(),
            passed: input.final_dead_letter <= input.baseline_dead_letter,
            details: format!(
                "baseline_dead_letter={} final_dead_letter={}",
                input.baseline_dead_letter, input.final_dead_letter
            ),
        },
        ReliabilityEvidenceCheck {
            name: "replay_checkpoint_advanced".to_string(),
            passed: checkpoint_is_advanced,
            details: format!(
                "baseline_checkpoint={:?} final_checkpoint={:?}",
                input.baseline_checkpoint, input.final_checkpoint
            ),
        },
        ReliabilityEvidenceCheck {
            name: "replay_checkpoint_recorded".to_string(),
            passed: input.final_checkpoint.is_some(),
            details: match input.final_checkpoint {
                Some(value) => format!("checkpoint={value}"),
                None => "checkpoint missing".to_string(),
            },
        },
        ReliabilityEvidenceCheck {
            name: "searchable_after_replay".to_string(),
            passed: input.replay_hit_uri.is_some(),
            details: input
                .replay_hit_uri
                .unwrap_or("no result after replay")
                .to_string(),
        },
        ReliabilityEvidenceCheck {
            name: "searchable_after_restart".to_string(),
            passed: input.restart_hit_uri.is_some(),
            details: input
                .restart_hit_uri
                .unwrap_or("no result after restart")
                .to_string(),
        },
    ]
}

pub(crate) fn evidence_status(passed: bool) -> String {
    if passed {
        "pass".to_string()
    } else {
        "fail".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{QueueCounts, QueueDiagnostics};

    fn queue(new_due: u64, processing: u64, dead_letter: u64) -> QueueDiagnostics {
        QueueDiagnostics {
            counts: QueueCounts {
                new_due,
                processing,
                dead_letter,
                ..QueueCounts::default()
            },
            checkpoints: Vec::new(),
        }
    }

    #[test]
    fn checkpoint_advanced_handles_expected_transitions() {
        assert!(checkpoint_advanced(None, Some(1)));
        assert!(checkpoint_advanced(Some(1), Some(2)));
        assert!(!checkpoint_advanced(Some(2), Some(2)));
        assert!(!checkpoint_advanced(Some(2), Some(1)));
        assert!(!checkpoint_advanced(None, None));
    }

    #[test]
    fn operability_checks_fail_when_logs_and_traces_absent() {
        let checks = build_operability_evidence_checks(0, 0, 0, &queue(3, 1, 0));
        assert_eq!(checks.len(), 3);
        assert_eq!(checks[0].name, "request_logs_present");
        assert!(!checks[0].passed);
        assert_eq!(checks[1].name, "trace_metrics_present");
        assert!(!checks[1].passed);
        assert_eq!(checks[2].name, "queue_diagnostics_collected");
        assert!(checks[2].passed);
    }

    #[test]
    fn reliability_checks_reflect_checkpoint_and_search_failures() {
        let replay_totals = ReplayReport {
            fetched: 10,
            processed: 8,
            done: 0,
            dead_letter: 1,
            requeued: 2,
            skipped: 0,
        };
        let input = ReliabilityEvidenceInput {
            replay_totals: &replay_totals,
            queue_after_replay: &queue(1, 1, 2),
            baseline_dead_letter: 0,
            final_dead_letter: 2,
            baseline_checkpoint: Some(5),
            final_checkpoint: Some(5),
            replay_hit_uri: None,
            restart_hit_uri: None,
        };
        let checks = build_reliability_evidence_checks(&input);
        assert!(checks.iter().any(|check| !check.passed));
        assert!(
            checks
                .iter()
                .any(|check| check.name == "replay_checkpoint_advanced" && !check.passed)
        );
        assert!(
            checks
                .iter()
                .any(|check| check.name == "searchable_after_restart" && !check.passed)
        );
    }

    #[test]
    fn accumulate_replay_report_sums_fields() {
        let mut total = ReplayReport {
            fetched: 1,
            processed: 2,
            done: 3,
            dead_letter: 4,
            requeued: 5,
            skipped: 6,
        };
        let report = ReplayReport {
            fetched: 7,
            processed: 8,
            done: 9,
            dead_letter: 10,
            requeued: 11,
            skipped: 12,
        };
        accumulate_replay_report(&mut total, &report);
        assert_eq!(total.fetched, 8);
        assert_eq!(total.processed, 10);
        assert_eq!(total.done, 12);
        assert_eq!(total.dead_letter, 14);
        assert_eq!(total.requeued, 16);
        assert_eq!(total.skipped, 18);
    }

    #[test]
    fn evidence_status_maps_bool_to_expected_string() {
        assert_eq!(evidence_status(true), "pass");
        assert_eq!(evidence_status(false), "fail");
    }
}
