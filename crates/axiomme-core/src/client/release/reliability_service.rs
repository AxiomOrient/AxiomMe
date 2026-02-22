use std::fs;
use std::time::Instant;

use chrono::Utc;

use crate::catalog::reliability_evidence_report_uri;
use crate::error::Result;
use crate::evidence::{
    ReliabilityEvidenceInput, accumulate_replay_report, build_reliability_evidence_checks,
    evidence_status,
};
use crate::models::{MetadataFilter, QueueDiagnostics, ReliabilityEvidenceReport, ReplayReport};
use crate::uri::{AxiomUri, Scope};

use super::AxiomMe;

struct ReliabilityRuntimeState {
    replay_totals: ReplayReport,
    replay_cycles: u32,
    queue_after_replay: QueueDiagnostics,
    queue_after_restart: QueueDiagnostics,
    replay_hit_uri: Option<String>,
    restart_hit_uri: Option<String>,
    final_checkpoint: Option<i64>,
    final_dead_letter: u64,
}

impl AxiomMe {
    pub fn collect_reliability_evidence(
        &self,
        replay_limit: usize,
        max_cycles: u32,
    ) -> Result<ReliabilityEvidenceReport> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let replay_limit = replay_limit.max(1);
        let max_cycles = max_cycles.max(1);

        let mut seeded_probe_uri: Option<String> = None;
        let output =
            self.build_reliability_evidence_report(replay_limit, max_cycles, &mut seeded_probe_uri);
        if let Some(uri) = seeded_probe_uri.as_deref()
            && let Err(cleanup_err) = self.cleanup_reliability_probe(uri)
        {
            self.log_request_warning(
                request_id.clone(),
                "reliability.evidence.cleanup",
                started,
                Some(uri.to_string()),
                "reliability probe cleanup failed",
                Some(serde_json::json!({
                    "error": cleanup_err.to_string(),
                })),
            );
        }

        match output {
            Ok(report) => {
                self.log_request_status(
                    request_id,
                    "reliability.evidence",
                    &report.status,
                    started,
                    Some(report.queued_root_uri.clone()),
                    Some(serde_json::json!({
                        "replay_limit": report.replay_limit,
                        "max_cycles": report.max_cycles,
                        "replay_cycles": report.replay_cycles,
                        "passed": report.passed,
                        "report_uri": report.report_uri,
                        "replay_done": report.replay_totals.done,
                        "replay_dead_letter": report.replay_totals.dead_letter,
                    })),
                );
                Ok(report)
            }
            Err(err) => {
                self.log_request_error(
                    request_id,
                    "reliability.evidence",
                    started,
                    None,
                    &err,
                    Some(serde_json::json!({
                        "replay_limit": replay_limit,
                        "max_cycles": max_cycles,
                    })),
                );
                Err(err)
            }
        }
    }

    fn build_reliability_evidence_report(
        &self,
        replay_limit: usize,
        max_cycles: u32,
        seeded_probe_uri: &mut Option<String>,
    ) -> Result<ReliabilityEvidenceReport> {
        let baseline_queue = self.queue_diagnostics()?;
        let baseline_dead_letter = baseline_queue.counts.dead_letter;
        let baseline_checkpoint = self.state.get_checkpoint("replay")?;

        let report_id = uuid::Uuid::new_v4().to_string();
        let query = format!("axiomme reliability evidence {report_id}");
        let queued_root_uri_str = self.seed_reliability_probe(&report_id, &query)?;
        *seeded_probe_uri = Some(queued_root_uri_str.clone());
        let runtime = self.execute_reliability_runtime_state(
            &query,
            &queued_root_uri_str,
            replay_limit,
            max_cycles,
        )?;
        let checks = build_reliability_evidence_checks(&ReliabilityEvidenceInput {
            replay_totals: &runtime.replay_totals,
            queue_after_replay: &runtime.queue_after_replay,
            baseline_dead_letter,
            final_dead_letter: runtime.final_dead_letter,
            baseline_checkpoint,
            final_checkpoint: runtime.final_checkpoint,
            replay_hit_uri: runtime.replay_hit_uri.as_deref(),
            restart_hit_uri: runtime.restart_hit_uri.as_deref(),
        });
        let passed = checks.iter().all(|check| check.passed);
        let status = evidence_status(passed);

        let report_uri = reliability_evidence_report_uri(&report_id)?;
        let report = ReliabilityEvidenceReport {
            report_id,
            created_at: Utc::now().to_rfc3339(),
            passed,
            status,
            replay_limit,
            max_cycles,
            replay_cycles: runtime.replay_cycles,
            replay_totals: runtime.replay_totals,
            baseline_dead_letter,
            final_dead_letter: runtime.final_dead_letter,
            baseline_checkpoint,
            final_checkpoint: runtime.final_checkpoint,
            queued_root_uri: queued_root_uri_str,
            query,
            replay_hit_uri: runtime.replay_hit_uri,
            restart_hit_uri: runtime.restart_hit_uri,
            queue: runtime.queue_after_restart,
            checks,
            report_uri: report_uri.to_string(),
        };
        self.fs
            .write(&report_uri, &serde_json::to_string_pretty(&report)?, true)?;
        Ok(report)
    }

    fn seed_reliability_probe(&self, report_id: &str, query: &str) -> Result<String> {
        let source_path = std::env::temp_dir().join(format!("axiomme_reliability_{report_id}.txt"));
        let source_path_str = source_path.to_string_lossy().to_string();
        fs::write(&source_path, format!("{query}\n"))?;

        // Reliability evidence requires replay/restart search visibility, so probe data must
        // live in an indexed scope. It is still cleaned up immediately after report generation.
        let queued_root_uri = AxiomUri::root(Scope::Resources)
            .join("reliability")?
            .join("evidence")?
            .join(report_id)?;
        let queued_root_uri_str = queued_root_uri.to_string();
        let add_result = self.add_resource(
            &source_path_str,
            Some(&queued_root_uri_str),
            None,
            None,
            false,
            None,
        );
        let _ = fs::remove_file(&source_path);
        add_result?;
        Ok(queued_root_uri_str)
    }

    fn cleanup_reliability_probe(&self, probe_root_uri: &str) -> Result<()> {
        let probe_uri = AxiomUri::parse(probe_root_uri)?;
        if !self.fs.exists(&probe_uri) {
            return Ok(());
        }
        self.fs.rm(&probe_uri, true, true)?;
        self.prune_index_prefix_from_memory(&probe_uri)?;
        self.state
            .remove_search_documents_with_prefix(probe_root_uri)?;
        self.state.remove_index_state_with_prefix(probe_root_uri)?;
        Ok(())
    }

    fn execute_reliability_runtime_state(
        &self,
        query: &str,
        queued_root_uri_str: &str,
        replay_limit: usize,
        max_cycles: u32,
    ) -> Result<ReliabilityRuntimeState> {
        let replay_app = Self::new(self.fs.root())?;
        let mut replay_totals = ReplayReport::default();
        let mut replay_cycles = 0u32;
        for _ in 0..max_cycles {
            replay_cycles = replay_cycles.saturating_add(1);
            let report = replay_app.replay_outbox(replay_limit, false)?;
            accumulate_replay_report(&mut replay_totals, &report);
            let queue_state = replay_app.queue_diagnostics()?;
            if queue_state.counts.new_due == 0 && queue_state.counts.processing == 0 {
                break;
            }
        }
        let queue_after_replay = replay_app.queue_diagnostics()?;

        let replay_find = replay_app.find(
            query,
            Some(queued_root_uri_str),
            Some(5),
            None,
            None::<MetadataFilter>,
        )?;
        let replay_hit_uri = replay_find.query_results.first().map(|hit| hit.uri.clone());

        let restarted_app = Self::new(self.fs.root())?;
        restarted_app.initialize()?;
        let restart_find = restarted_app.find(
            query,
            Some(queued_root_uri_str),
            Some(5),
            None,
            None::<MetadataFilter>,
        )?;
        let restart_hit_uri = restart_find
            .query_results
            .first()
            .map(|hit| hit.uri.clone());

        let queue_after_restart = restarted_app.queue_diagnostics()?;
        let final_checkpoint = restarted_app.state.get_checkpoint("replay")?;
        let final_dead_letter = queue_after_replay.counts.dead_letter;

        Ok(ReliabilityRuntimeState {
            replay_totals,
            replay_cycles,
            queue_after_replay,
            queue_after_restart,
            replay_hit_uri,
            restart_hit_uri,
            final_checkpoint,
            final_dead_letter,
        })
    }
}
