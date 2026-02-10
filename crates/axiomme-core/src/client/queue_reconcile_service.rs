use std::time::Instant;

use crate::error::{AxiomError, Result};
use crate::models::{ReconcileOptions, ReconcileReport, ReplayReport};
use crate::queue_policy::{
    default_scope_set, push_drift_sample, retry_backoff_seconds, should_retry_event,
};
use crate::uri::AxiomUri;

use super::AxiomMe;

impl AxiomMe {
    pub fn replay_outbox(&self, limit: usize, include_dead_letter: bool) -> Result<ReplayReport> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();

        let output = (|| -> Result<ReplayReport> {
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
                        self.state.mark_outbox_status(event.id, "done", false)?;
                        report.processed += 1;
                        report.done += 1;
                        if !handled {
                            report.skipped += 1;
                        }
                        self.state.set_checkpoint("replay", event.id)?;
                    }
                    Err(_err) => {
                        if should_retry_event(&event.event_type, attempt) {
                            self.state.requeue_outbox_with_delay(
                                event.id,
                                retry_backoff_seconds(&event.event_type, attempt, event.id),
                            )?;
                            report.requeued += 1;
                        } else {
                            self.state
                                .mark_outbox_status(event.id, "dead_letter", false)?;
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
                    })),
                );
                Err(err)
            }
        }
    }

    pub fn reconcile_state(&self) -> Result<ReconcileReport> {
        self.reconcile_state_with_options(ReconcileOptions::default())
    }

    pub fn reconcile_state_with_options(
        &self,
        options: ReconcileOptions,
    ) -> Result<ReconcileReport> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let run_id = uuid::Uuid::new_v4().to_string();
        self.state.start_reconcile_run(&run_id)?;
        let selected_scopes = options.scopes.clone().unwrap_or_else(default_scope_set);
        let scope_names = selected_scopes
            .iter()
            .map(|scope| scope.as_str().to_string())
            .collect::<Vec<_>>();

        let result = (|| -> Result<ReconcileReport> {
            let mut drift_count = 0usize;
            let mut invalid_uri_entries = 0usize;
            let mut missing_uri_entries = 0usize;
            let mut missing_files_pruned = 0usize;
            let mut drift_uris_sample = Vec::<String>::new();

            let uris = self.state.list_index_state_uris()?;
            for uri_str in uris {
                let parsed = match AxiomUri::parse(&uri_str) {
                    Ok(value) => value,
                    Err(_) => {
                        drift_count += 1;
                        invalid_uri_entries += 1;
                        push_drift_sample(
                            &mut drift_uris_sample,
                            &uri_str,
                            options.max_drift_sample,
                        );
                        if !options.dry_run {
                            let _ = self.state.remove_index_state(&uri_str)?;
                            self.state.remove_search_document(&uri_str)?;
                        }
                        continue;
                    }
                };
                if !selected_scopes.contains(&parsed.scope()) {
                    continue;
                }
                if !self.fs.exists(&parsed) {
                    drift_count += 1;
                    missing_uri_entries += 1;
                    push_drift_sample(&mut drift_uris_sample, &uri_str, options.max_drift_sample);
                    if !options.dry_run {
                        missing_files_pruned += 1;
                        let _ = self.state.remove_index_state(&uri_str)?;
                        self.state.remove_search_documents_with_prefix(&uri_str)?;
                        {
                            let mut index = self.index.write().map_err(|_| {
                                AxiomError::Internal("index lock poisoned".to_string())
                            })?;
                            index.remove(&uri_str);
                        }
                        self.try_mirror_delete(&[uri_str], "reconcile_state_prune")?;
                    }
                }
            }

            let reindexed_scopes = if options.dry_run {
                0
            } else {
                self.reindex_scopes(&selected_scopes)?;
                selected_scopes.len()
            };

            Ok(ReconcileReport {
                run_id: run_id.clone(),
                drift_count,
                invalid_uri_entries,
                missing_uri_entries,
                missing_files_pruned,
                reindexed_scopes,
                dry_run: options.dry_run,
                drift_uris_sample,
                status: if options.dry_run {
                    "dry_run".to_string()
                } else {
                    "success".to_string()
                },
            })
        })();

        match &result {
            Ok(report) => {
                self.state
                    .finish_reconcile_run(&run_id, report.drift_count, &report.status)?;
            }
            Err(_) => {
                let _ = self.state.finish_reconcile_run(&run_id, 0, "failed");
            }
        }

        match &result {
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

        result
    }
}
