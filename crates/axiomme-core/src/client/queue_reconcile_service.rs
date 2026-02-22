use std::time::Instant;

use crate::error::{AxiomError, Result};
use crate::models::{OutboxEvent, ReconcileOptions, ReconcileReport, ReplayReport};
use crate::queue_policy::{
    default_scope_set, push_drift_sample, retry_backoff_seconds, should_retry_event_error,
};
use crate::uri::{AxiomUri, Scope};

use super::AxiomMe;

const PROCESSING_TIMEOUT_RECOVERY_SECS: i64 = 300;

impl AxiomMe {
    pub fn replay_outbox(&self, limit: usize, include_dead_letter: bool) -> Result<ReplayReport> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let mut recovered_processing = 0u64;

        let output = (|| -> Result<ReplayReport> {
            recovered_processing = self
                .state
                .recover_timed_out_processing_events(PROCESSING_TIMEOUT_RECOVERY_SECS)?;
            let mut events = self.state.fetch_outbox("new", limit)?;
            if include_dead_letter && events.len() < limit {
                let remaining = limit - events.len();
                let mut dead = self.state.fetch_outbox("dead_letter", remaining)?;
                events.append(&mut dead);
            }

            let mut report = ReplayReport {
                fetched: events.len(),
                ..ReplayReport::default()
            };

            for event in events {
                self.state
                    .mark_outbox_status(event.id, "processing", true)?;
                let attempt = event.attempt_count.saturating_add(1);
                match self.handle_outbox_event(&event) {
                    Ok(handled) => {
                        report.processed += 1;
                        if handled {
                            self.state.mark_outbox_status(event.id, "done", false)?;
                            report.done += 1;
                        } else {
                            self.state
                                .mark_outbox_status(event.id, "dead_letter", false)?;
                            report.dead_letter += 1;
                        }
                        self.state.set_checkpoint("replay", event.id)?;
                    }
                    Err(err) => {
                        if should_retry_event_error(&event.event_type, attempt, &err) {
                            self.state.requeue_outbox_with_delay(
                                event.id,
                                retry_backoff_seconds(&event.event_type, attempt, event.id),
                            )?;
                            report.requeued += 1;
                        } else {
                            self.state
                                .mark_outbox_status(event.id, "dead_letter", false)?;
                            self.try_cleanup_om_reflection_flags_after_terminal_failure(&event)?;
                            report.dead_letter += 1;
                        }
                        self.state.set_checkpoint("replay", event.id)?;
                    }
                }
            }

            Ok(report)
        })();

        match output {
            Ok(report) => {
                self.log_request_status(
                    request_id,
                    "queue.replay",
                    "ok",
                    started,
                    None,
                    Some(serde_json::json!({
                        "limit": limit,
                        "include_dead_letter": include_dead_letter,
                        "fetched": report.fetched,
                        "processed": report.processed,
                        "done": report.done,
                        "dead_letter": report.dead_letter,
                        "requeued": report.requeued,
                        "skipped": report.skipped,
                        "recovered_processing": recovered_processing,
                    })),
                );
                Ok(report)
            }
            Err(err) => {
                self.log_request_error(
                    request_id,
                    "queue.replay",
                    started,
                    None,
                    &err,
                    Some(serde_json::json!({
                        "limit": limit,
                        "include_dead_letter": include_dead_letter,
                        "recovered_processing": recovered_processing,
                    })),
                );
                Err(err)
            }
        }
    }

    pub fn reconcile_state(&self) -> Result<ReconcileReport> {
        self.reconcile_state_with_options(&ReconcileOptions::default())
    }

    pub fn reconcile_state_with_options(
        &self,
        options: &ReconcileOptions,
    ) -> Result<ReconcileReport> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let run_id = uuid::Uuid::new_v4().to_string();
        self.state.start_reconcile_run(&run_id)?;
        let scope_selection = resolve_reconcile_scope_selection(options);
        let result = self.execute_reconcile_run(&run_id, options, &scope_selection.selected_scopes);
        self.finish_reconcile_run_status(&run_id, &result)?;
        self.log_reconcile_run_result(
            request_id,
            started,
            &run_id,
            options,
            &scope_selection.scope_names,
            &result,
        );

        result
    }

    fn execute_reconcile_run(
        &self,
        run_id: &str,
        options: &ReconcileOptions,
        selected_scopes: &[Scope],
    ) -> Result<ReconcileReport> {
        let stats = self.collect_reconcile_drift_stats(options, selected_scopes)?;
        let reindexed_scopes = self.reindex_reconcile_scopes(options, selected_scopes)?;
        Ok(ReconcileReport {
            run_id: run_id.to_string(),
            drift_count: stats.drift_count,
            invalid_uri_entries: stats.invalid_uri_entries,
            missing_uri_entries: stats.missing_uri_entries,
            missing_files_pruned: stats.missing_files_pruned,
            reindexed_scopes,
            dry_run: options.dry_run,
            drift_uris_sample: stats.drift_uris_sample,
            status: reconcile_status(options.dry_run).to_string(),
        })
    }

    fn collect_reconcile_drift_stats(
        &self,
        options: &ReconcileOptions,
        selected_scopes: &[Scope],
    ) -> Result<ReconcileDriftStats> {
        let mut stats = ReconcileDriftStats::default();
        for uri_str in self.state.list_index_state_uris()? {
            self.process_reconcile_uri(&uri_str, options, selected_scopes, &mut stats)?;
        }
        Ok(stats)
    }

    fn process_reconcile_uri(
        &self,
        uri_str: &str,
        options: &ReconcileOptions,
        selected_scopes: &[Scope],
        stats: &mut ReconcileDriftStats,
    ) -> Result<()> {
        let Ok(parsed) = AxiomUri::parse(uri_str) else {
            self.record_invalid_reconcile_uri(uri_str, options, stats)?;
            return Ok(());
        };
        if !selected_scopes.contains(&parsed.scope()) {
            return Ok(());
        }
        if !self.fs.exists(&parsed) {
            self.record_missing_reconcile_uri(uri_str, options, stats)?;
        }
        Ok(())
    }

    fn record_invalid_reconcile_uri(
        &self,
        uri_str: &str,
        options: &ReconcileOptions,
        stats: &mut ReconcileDriftStats,
    ) -> Result<()> {
        stats.drift_count = stats.drift_count.saturating_add(1);
        stats.invalid_uri_entries = stats.invalid_uri_entries.saturating_add(1);
        push_drift_sample(
            &mut stats.drift_uris_sample,
            uri_str,
            options.max_drift_sample,
        );
        if !options.dry_run {
            let _ = self.state.remove_index_state(uri_str)?;
            self.state.remove_search_document(uri_str)?;
        }
        Ok(())
    }

    fn record_missing_reconcile_uri(
        &self,
        uri_str: &str,
        options: &ReconcileOptions,
        stats: &mut ReconcileDriftStats,
    ) -> Result<()> {
        stats.drift_count = stats.drift_count.saturating_add(1);
        stats.missing_uri_entries = stats.missing_uri_entries.saturating_add(1);
        push_drift_sample(
            &mut stats.drift_uris_sample,
            uri_str,
            options.max_drift_sample,
        );
        if options.dry_run {
            return Ok(());
        }

        stats.missing_files_pruned = stats.missing_files_pruned.saturating_add(1);
        let _ = self.state.remove_index_state(uri_str)?;
        self.state.remove_search_documents_with_prefix(uri_str)?;
        {
            let mut index = self
                .index
                .write()
                .map_err(|_| AxiomError::lock_poisoned("index"))?;
            index.remove(uri_str);
        }
        Ok(())
    }

    fn reindex_reconcile_scopes(
        &self,
        options: &ReconcileOptions,
        selected_scopes: &[Scope],
    ) -> Result<usize> {
        if options.dry_run {
            return Ok(0);
        }
        self.reindex_scopes(selected_scopes)?;
        Ok(selected_scopes.len())
    }

    fn finish_reconcile_run_status(
        &self,
        run_id: &str,
        result: &Result<ReconcileReport>,
    ) -> Result<()> {
        match result {
            Ok(report) => {
                self.state
                    .finish_reconcile_run(run_id, report.drift_count, &report.status)?;
            }
            Err(_) => {
                let _ = self.state.finish_reconcile_run(run_id, 0, "failed");
            }
        }
        Ok(())
    }

    fn log_reconcile_run_result(
        &self,
        request_id: String,
        started: Instant,
        run_id: &str,
        options: &ReconcileOptions,
        scope_names: &[String],
        result: &Result<ReconcileReport>,
    ) {
        match result {
            Ok(report) => {
                self.log_request_status(
                    request_id,
                    "reconcile.run",
                    &report.status,
                    started,
                    None,
                    Some(serde_json::json!({
                        "run_id": report.run_id,
                        "dry_run": report.dry_run,
                        "scopes": scope_names,
                        "drift_count": report.drift_count,
                        "invalid_uri_entries": report.invalid_uri_entries,
                        "missing_uri_entries": report.missing_uri_entries,
                        "missing_files_pruned": report.missing_files_pruned,
                        "reindexed_scopes": report.reindexed_scopes,
                    })),
                );
            }
            Err(err) => {
                self.log_request_error(
                    request_id,
                    "reconcile.run",
                    started,
                    None,
                    err,
                    Some(serde_json::json!({
                        "run_id": run_id,
                        "dry_run": options.dry_run,
                        "scopes": scope_names,
                        "max_drift_sample": options.max_drift_sample,
                    })),
                );
            }
        }
    }

    pub(crate) fn try_cleanup_om_reflection_flags_after_terminal_failure(
        &self,
        event: &OutboxEvent,
    ) -> Result<()> {
        let Some(target) = parse_om_reflection_cleanup_target(event) else {
            return Ok(());
        };
        let _ = self
            .state
            .clear_om_reflection_flags_with_cas(target.scope_key, target.expected_generation)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OmReflectionCleanupTarget<'a> {
    scope_key: &'a str,
    expected_generation: u32,
}

#[derive(Debug, Clone, Default)]
struct ReconcileDriftStats {
    drift_count: usize,
    invalid_uri_entries: usize,
    missing_uri_entries: usize,
    missing_files_pruned: usize,
    drift_uris_sample: Vec<String>,
}

#[derive(Debug, Clone)]
struct ReconcileScopeSelection {
    selected_scopes: Vec<Scope>,
    scope_names: Vec<String>,
}

fn resolve_reconcile_scope_selection(options: &ReconcileOptions) -> ReconcileScopeSelection {
    let selected_scopes = options.scopes.clone().unwrap_or_else(default_scope_set);
    let scope_names = selected_scopes
        .iter()
        .map(|scope| scope.as_str().to_string())
        .collect::<Vec<_>>();
    ReconcileScopeSelection {
        selected_scopes,
        scope_names,
    }
}

const fn reconcile_status(dry_run: bool) -> &'static str {
    if dry_run { "dry_run" } else { "success" }
}

fn parse_om_reflection_cleanup_target(
    event: &OutboxEvent,
) -> Option<OmReflectionCleanupTarget<'_>> {
    if !matches!(
        event.event_type.as_str(),
        "om_reflect_requested" | "om_reflect_buffer_requested"
    ) {
        return None;
    }

    let payload = &event.payload_json;
    let scope_key = payload.get("scope_key")?.as_str()?.trim();
    if scope_key.is_empty() {
        return None;
    }
    let expected_generation = parse_payload_generation(payload.get("expected_generation")?)?;

    Some(OmReflectionCleanupTarget {
        scope_key,
        expected_generation,
    })
}

fn parse_payload_generation(value: &serde_json::Value) -> Option<u32> {
    if let Some(num) = value.as_u64() {
        return u32::try_from(num).ok();
    }
    value
        .as_str()
        .map(str::trim)
        .filter(|raw| !raw.is_empty())
        .and_then(|raw| raw.parse::<u32>().ok())
}
