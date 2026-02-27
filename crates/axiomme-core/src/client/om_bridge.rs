use std::time::Instant;

use crate::om::build_scope_key;

use crate::error::{AxiomError, Result};
use crate::models::{OutboxEvent, QueueEventStatus, ReplayReport};
use crate::om_bridge::{
    OM_OUTBOX_SCHEMA_VERSION_V1, OmHintReadRequestV1, OmHintReadStateV1, OmMessageAppendRequestV1,
    OmMessageAppendResultV1, OmObserveBufferRequestedV1, OmOutboxEnqueueResultV1,
    OmReflectBufferRequestedV1, OmReflectRequestedV1, OmReplayModeV1, OmReplayRequestV1,
    OmReplayResultV1, OmScopeBindingInputV1, OmScopeV1,
};
use crate::queue_policy::{retry_backoff_seconds, should_retry_event_error};

use super::AxiomMe;

impl AxiomMe {
    pub fn om_bridge_append_message(
        &self,
        request: OmMessageAppendRequestV1,
    ) -> Result<OmMessageAppendResultV1> {
        let session_id = non_empty("session_id", &request.session_id)?;
        let role = non_empty("role", &request.role)?;
        let text = request.text;
        if text.trim().is_empty() {
            return Err(AxiomError::Validation("text must not be empty".to_string()));
        }

        let scope_binding = request.scope_binding.map(normalize_scope_binding);
        let mut session = self.session(Some(session_id.as_str()));
        if let Some(binding) = scope_binding.as_ref() {
            session = session.with_om_scope(
                binding.scope.to_engine_scope(),
                binding.thread_id.as_deref(),
                binding.resource_id.as_deref(),
            )?;
        }

        session.load()?;
        let message = session.add_message(role.as_str(), text)?;
        let scope_key = build_scope_key_from_binding(session_id.as_str(), scope_binding.as_ref())?;

        Ok(OmMessageAppendResultV1 {
            session_id,
            message_id: message.id,
            scope_key,
        })
    }

    pub fn om_bridge_read_hint_state(
        &self,
        request: OmHintReadRequestV1,
    ) -> Result<Option<OmHintReadStateV1>> {
        let session_id = non_empty("session_id", &request.session_id)?;
        let scope_binding = request.scope_binding.map(normalize_scope_binding);
        if let Some(binding) = scope_binding.as_ref() {
            let scope_key = build_scope_key_from_binding(session_id.as_str(), Some(binding))?;
            let preferred_thread_id = match binding.scope {
                OmScopeV1::Session => None,
                OmScopeV1::Thread | OmScopeV1::Resource => binding.thread_id.as_deref(),
            };
            return self.fetch_om_state_by_scope_key(scope_key.as_str(), preferred_thread_id);
        }
        self.fetch_session_om_state(session_id.as_str())
    }

    pub fn om_bridge_enqueue_observe_request(
        &self,
        request: OmObserveBufferRequestedV1,
    ) -> Result<OmOutboxEnqueueResultV1> {
        validate_om_outbox_common(
            "om_observe_buffer_requested",
            request.schema_version,
            request.scope_key.as_str(),
            request.requested_at.as_str(),
        )?;
        let session_id = request
            .session_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .or_else(|| {
                request
                    .scope_key
                    .strip_prefix("session:")
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            })
            .ok_or_else(|| {
                AxiomError::Validation(
                    "om_observe_buffer_requested requires non-empty session_id".to_string(),
                )
            })?;
        let uri = format!("axiom://session/{session_id}");
        let scope_key = request.scope_key.clone();
        let event_id = self.state.enqueue(
            "om_observe_buffer_requested",
            &uri,
            serde_json::to_value(request)?,
        )?;
        Ok(OmOutboxEnqueueResultV1 {
            event_id,
            event_type: "om_observe_buffer_requested".to_string(),
            scope_key,
        })
    }

    pub fn om_bridge_enqueue_reflect_buffer_request(
        &self,
        request: OmReflectBufferRequestedV1,
    ) -> Result<OmOutboxEnqueueResultV1> {
        validate_om_outbox_common(
            "om_reflect_buffer_requested",
            request.schema_version,
            request.scope_key.as_str(),
            request.requested_at.as_str(),
        )?;
        let scope_key = request.scope_key.clone();
        let event_id = self.state.enqueue(
            "om_reflect_buffer_requested",
            OM_BRIDGE_REFLECT_URI,
            serde_json::to_value(request)?,
        )?;
        Ok(OmOutboxEnqueueResultV1 {
            event_id,
            event_type: "om_reflect_buffer_requested".to_string(),
            scope_key,
        })
    }

    pub fn om_bridge_enqueue_reflect_request(
        &self,
        request: OmReflectRequestedV1,
    ) -> Result<OmOutboxEnqueueResultV1> {
        validate_om_outbox_common(
            "om_reflect_requested",
            request.schema_version,
            request.scope_key.as_str(),
            request.requested_at.as_str(),
        )?;
        let scope_key = request.scope_key.clone();
        let event_id = self.state.enqueue(
            "om_reflect_requested",
            OM_BRIDGE_REFLECT_URI,
            serde_json::to_value(request)?,
        )?;
        Ok(OmOutboxEnqueueResultV1 {
            event_id,
            event_type: "om_reflect_requested".to_string(),
            scope_key,
        })
    }

    pub fn om_bridge_replay(&self, request: &OmReplayRequestV1) -> Result<OmReplayResultV1> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let started = Instant::now();
        let limit = request.limit.max(1);

        let output = (|| -> Result<OmReplayResultV1> {
            let (replay, scanned_count, om_candidate_count) = match request.mode {
                OmReplayModeV1::Full => (
                    self.replay_outbox(limit, request.include_dead_letter)?,
                    None,
                    None,
                ),
                OmReplayModeV1::OmOnly => {
                    let outcome = self.replay_outbox_om_only(limit, request.include_dead_letter)?;
                    (
                        outcome.report,
                        Some(outcome.scanned_count),
                        Some(outcome.om_candidate_count),
                    )
                }
            };
            Ok(OmReplayResultV1 {
                fetched: replay.fetched,
                processed: replay.processed,
                done: replay.done,
                dead_letter: replay.dead_letter,
                requeued: replay.requeued,
                skipped: replay.skipped,
                scanned_count,
                om_candidate_count,
            })
        })();

        match output {
            Ok(result) => {
                let mut details = serde_json::json!({
                    "limit": limit,
                    "include_dead_letter": request.include_dead_letter,
                    "mode": request.mode.as_str(),
                    "fetched": result.fetched,
                    "processed": result.processed,
                    "done": result.done,
                    "dead_letter": result.dead_letter,
                    "requeued": result.requeued,
                    "skipped": result.skipped,
                });
                if let Some(object) = details.as_object_mut() {
                    if let Some(scanned_count) = result.scanned_count {
                        object.insert(
                            "scanned_count".to_string(),
                            serde_json::json!(scanned_count),
                        );
                    }
                    if let Some(om_candidate_count) = result.om_candidate_count {
                        object.insert(
                            "om_candidate_count".to_string(),
                            serde_json::json!(om_candidate_count),
                        );
                    }
                }
                self.log_request_status(
                    request_id,
                    "queue.replay.om_bridge",
                    "ok",
                    started,
                    None,
                    Some(details),
                );
                Ok(result)
            }
            Err(err) => {
                self.log_request_error(
                    request_id,
                    "queue.replay.om_bridge",
                    started,
                    None,
                    &err,
                    Some(serde_json::json!({
                        "limit": limit,
                        "include_dead_letter": request.include_dead_letter,
                        "mode": request.mode.as_str(),
                    })),
                );
                Err(err)
            }
        }
    }

    fn replay_outbox_om_only(
        &self,
        limit: usize,
        include_dead_letter: bool,
    ) -> Result<OmOnlyReplayOutcome> {
        let scan_limit = limit.saturating_mul(OM_REPLAY_SCAN_FACTOR).max(limit);
        let selected_new = fetch_outbox_om_events(self, QueueEventStatus::New, scan_limit, limit)?;
        let mut events = selected_new.events;
        let mut scanned_count = selected_new.scanned_count;
        let mut om_candidate_count = selected_new.om_candidate_count;
        if include_dead_letter && events.len() < limit {
            let remaining = limit - events.len();
            let mut selected_dead =
                fetch_outbox_om_events(self, QueueEventStatus::DeadLetter, scan_limit, remaining)?;
            scanned_count = scanned_count.saturating_add(selected_dead.scanned_count);
            om_candidate_count =
                om_candidate_count.saturating_add(selected_dead.om_candidate_count);
            events.append(&mut selected_dead.events);
        }
        Ok(OmOnlyReplayOutcome {
            report: process_replay_events(self, events, OM_BRIDGE_REPLAY_CHECKPOINT)?,
            scanned_count,
            om_candidate_count,
        })
    }
}

const OM_BRIDGE_REFLECT_URI: &str = "axiom://session/__om_bridge__";
const OM_BRIDGE_REPLAY_CHECKPOINT: &str = "replay_om_bridge";
const OM_REPLAY_SCAN_FACTOR: usize = 4;

fn non_empty(field: &str, raw: &str) -> Result<String> {
    let value = raw.trim().to_string();
    if value.is_empty() {
        return Err(AxiomError::Validation(format!("{field} must not be empty")));
    }
    Ok(value)
}

fn normalize_scope_binding(input: OmScopeBindingInputV1) -> OmScopeBindingInputV1 {
    OmScopeBindingInputV1 {
        scope: input.scope,
        thread_id: trim_optional(input.thread_id),
        resource_id: trim_optional(input.resource_id),
    }
}

fn trim_optional(raw: Option<String>) -> Option<String> {
    raw.map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn build_scope_key_from_binding(
    session_id: &str,
    scope_binding: Option<&OmScopeBindingInputV1>,
) -> Result<String> {
    let (scope, thread_id, resource_id) =
        scope_binding.map_or((OmScopeV1::Session, None, None), |binding| {
            (
                binding.scope,
                binding.thread_id.as_deref(),
                binding.resource_id.as_deref(),
            )
        });
    build_scope_key(
        scope.to_engine_scope(),
        Some(session_id),
        thread_id,
        resource_id,
    )
    .map_err(|err| AxiomError::Validation(err.to_string()))
}

fn validate_om_outbox_common(
    event_type: &str,
    schema_version: u8,
    scope_key: &str,
    requested_at: &str,
) -> Result<()> {
    if schema_version != OM_OUTBOX_SCHEMA_VERSION_V1 {
        return Err(AxiomError::Validation(format!(
            "{event_type} unsupported schema_version: {schema_version}"
        )));
    }
    if scope_key.trim().is_empty() {
        return Err(AxiomError::Validation(format!(
            "{event_type} missing scope_key"
        )));
    }
    if requested_at.trim().is_empty() || chrono::DateTime::parse_from_rfc3339(requested_at).is_err()
    {
        return Err(AxiomError::Validation(format!(
            "{event_type} invalid requested_at"
        )));
    }
    Ok(())
}

fn is_om_event_type(event_type: &str) -> bool {
    matches!(
        event_type,
        "om_observe_buffer_requested" | "om_reflect_buffer_requested" | "om_reflect_requested"
    )
}

fn fetch_outbox_om_events(
    app: &AxiomMe,
    status: QueueEventStatus,
    scan_limit: usize,
    take_limit: usize,
) -> Result<OmReplaySelection> {
    if take_limit == 0 {
        return Ok(OmReplaySelection::default());
    }
    let mut events = app.state.fetch_outbox(status, scan_limit)?;
    let scanned_count = events.len();
    events.retain(|event| is_om_event_type(event.event_type.as_str()));
    let om_candidate_count = events.len();
    events.truncate(take_limit);
    Ok(OmReplaySelection {
        events,
        scanned_count,
        om_candidate_count,
    })
}

fn process_replay_events(
    app: &AxiomMe,
    events: Vec<OutboxEvent>,
    checkpoint_name: &str,
) -> Result<ReplayReport> {
    let mut report = ReplayReport {
        fetched: events.len(),
        ..ReplayReport::default()
    };

    for event in events {
        app.state
            .mark_outbox_status(event.id, QueueEventStatus::Processing, true)?;
        let attempt = event.attempt_count.saturating_add(1);
        match app.handle_outbox_event(&event) {
            Ok(handled) => {
                app.state
                    .mark_outbox_status(event.id, QueueEventStatus::Done, false)?;
                report.processed += 1;
                report.done += 1;
                if !handled {
                    report.skipped += 1;
                }
                app.state.set_checkpoint(checkpoint_name, event.id)?;
            }
            Err(err) => {
                if should_retry_event_error(event.event_type.as_str(), attempt, &err) {
                    app.state.requeue_outbox_with_delay(
                        event.id,
                        retry_backoff_seconds(event.event_type.as_str(), attempt, event.id),
                    )?;
                    report.requeued += 1;
                } else {
                    app.state
                        .mark_outbox_status(event.id, QueueEventStatus::DeadLetter, false)?;
                    app.try_cleanup_om_reflection_flags_after_terminal_failure(&event)?;
                    report.dead_letter += 1;
                }
                app.state.set_checkpoint(checkpoint_name, event.id)?;
            }
        }
    }

    Ok(report)
}

#[derive(Debug, Default)]
struct OmReplaySelection {
    events: Vec<OutboxEvent>,
    scanned_count: usize,
    om_candidate_count: usize,
}

#[derive(Debug)]
struct OmOnlyReplayOutcome {
    report: ReplayReport,
    scanned_count: usize,
    om_candidate_count: usize,
}
