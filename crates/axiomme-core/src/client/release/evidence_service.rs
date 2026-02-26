use std::time::Instant;

use chrono::Utc;

use crate::catalog::operability_evidence_report_uri;
use crate::error::Result;
use crate::evidence::{build_operability_evidence_checks, evidence_status};
use crate::models::{
    OperabilityCoverage, OperabilityEvidenceReport, OperabilitySampleWindow, QueueDiagnostics,
};

use super::AxiomMe;

struct OperabilityRuntimeState {
    traces_analyzed: usize,
    request_logs_scanned: usize,
    request_type_count: usize,
    trace_metrics_snapshot_uri: String,
    queue: QueueDiagnostics,
}

impl AxiomMe {
    pub fn collect_operability_evidence(
        &self,
        trace_limit: usize,
        request_limit: usize,
    ) -> Result<OperabilityEvidenceReport> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let trace_limit = trace_limit.max(1);
        let request_limit = request_limit.max(1);

        let output = (|| -> Result<OperabilityEvidenceReport> {
            let runtime = self.collect_operability_runtime_state(trace_limit, request_limit)?;
            let checks = build_operability_evidence_checks(
                runtime.request_logs_scanned,
                runtime.traces_analyzed,
                runtime.request_type_count,
                &runtime.queue,
            );
            let passed = checks.iter().all(|check| check.passed);
            let status = evidence_status(passed);

            let report_id = uuid::Uuid::new_v4().to_string();
            let report_uri = operability_evidence_report_uri(&report_id)?;
            let report = OperabilityEvidenceReport {
                report_id,
                created_at: Utc::now().to_rfc3339(),
                passed,
                status,
                sample_window: OperabilitySampleWindow {
                    trace_limit,
                    request_limit,
                },
                coverage: OperabilityCoverage {
                    traces_analyzed: runtime.traces_analyzed,
                    request_logs_scanned: runtime.request_logs_scanned,
                    trace_metrics_snapshot_uri: runtime.trace_metrics_snapshot_uri,
                },
                queue: runtime.queue,
                checks,
                report_uri: report_uri.to_string(),
            };
            self.fs
                .write(&report_uri, &serde_json::to_string_pretty(&report)?, true)?;
            Ok(report)
        })();

        match output {
            Ok(report) => {
                self.log_request_status(
                    request_id,
                    "operability.evidence",
                    report.status.as_str(),
                    started,
                    None,
                    Some(serde_json::json!({
                        "trace_limit": report.sample_window.trace_limit,
                        "request_limit": report.sample_window.request_limit,
                        "passed": report.passed,
                        "traces_analyzed": report.coverage.traces_analyzed,
                        "request_logs_scanned": report.coverage.request_logs_scanned,
                        "trace_metrics_snapshot_uri": report.coverage.trace_metrics_snapshot_uri,
                        "report_uri": report.report_uri,
                    })),
                );
                Ok(report)
            }
            Err(err) => {
                self.log_request_error(
                    request_id,
                    "operability.evidence",
                    started,
                    None,
                    &err,
                    Some(serde_json::json!({
                        "trace_limit": trace_limit,
                        "request_limit": request_limit,
                    })),
                );
                Err(err)
            }
        }
    }

    fn collect_operability_runtime_state(
        &self,
        trace_limit: usize,
        request_limit: usize,
    ) -> Result<OperabilityRuntimeState> {
        let trace_snapshot = self.create_trace_metrics_snapshot(trace_limit, false)?;
        let trace_metrics = self.trace_metrics(trace_limit, false)?;
        let request_logs = self.list_request_logs(request_limit)?;
        let queue = self.queue_diagnostics()?;

        Ok(OperabilityRuntimeState {
            traces_analyzed: trace_metrics.traces_analyzed,
            request_logs_scanned: request_logs.len(),
            request_type_count: trace_metrics.by_request_type.len(),
            trace_metrics_snapshot_uri: trace_snapshot.report_uri,
            queue,
        })
    }
}
