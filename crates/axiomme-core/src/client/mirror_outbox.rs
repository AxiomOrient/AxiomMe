use crate::error::Result;
use crate::models::OutboxEvent;
use crate::state::{OmReflectionApplyContext, OmReflectionBufferPayload};
use crate::uri::AxiomUri;

use super::AxiomMe;

mod reflector;

use reflector::{
    OmReflectorCallOptions, buffered_or_resolved_reflector_response, parse_observe_session_id,
    parse_om_observe_buffer_requested_payload, parse_om_reflect_buffer_requested_payload,
    parse_om_reflect_requested_payload, resolve_reflector_response,
};

impl AxiomMe {
    pub(super) fn handle_outbox_event(&self, event: &OutboxEvent) -> Result<bool> {
        match event.event_type.as_str() {
            "semantic_scan" => {
                let target = AxiomUri::parse(&event.uri)?;
                self.prune_index_prefix_from_memory(&target)?;
                self.state
                    .remove_search_documents_with_prefix(&target.to_string())?;
                self.state
                    .remove_index_state_with_prefix(&target.to_string())?;
                self.ensure_tiers_recursive(&target)?;
                self.reindex_uri_tree(&target)?;
                Ok(true)
            }
            "upsert" | "reindex" | "delete" => Ok(true),
            "om_reflect_buffer_requested" => {
                if !self.config.om.enabled {
                    return Ok(true);
                }
                self.handle_om_reflect_buffer_requested(event)
            }
            "om_observe_buffer_requested" => {
                if !self.config.om.enabled {
                    return Ok(true);
                }
                self.handle_om_observe_buffer_requested(event)
            }
            "om_reflect_requested" => {
                if !self.config.om.enabled {
                    return Ok(true);
                }
                self.handle_om_reflect_requested(event)
            }
            _ => Ok(false),
        }
    }

    fn handle_om_reflect_buffer_requested(&self, event: &OutboxEvent) -> Result<bool> {
        let payload = parse_om_reflect_buffer_requested_payload(&event.payload_json)?;
        let scope_key = payload.scope_key.as_str();
        let expected_generation = payload.expected_generation;

        let Some(record) = self.state.get_om_record_by_scope_key(scope_key)? else {
            return Ok(true);
        };
        if record.generation_count != expected_generation {
            return Ok(true);
        }
        if record
            .buffered_reflection
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Ok(true);
        }

        let reflection = resolve_reflector_response(
            &record,
            scope_key,
            expected_generation,
            OmReflectorCallOptions::BUFFERED,
            &self.config.om.reflector,
        )?;
        let _buffered = self.state.buffer_om_reflection_with_cas(
            scope_key,
            expected_generation,
            OmReflectionBufferPayload {
                reflection: &reflection.reflection,
                reflection_token_count: reflection.reflection_token_count,
                reflection_input_tokens: reflection.usage.input_tokens,
                reflected_observation_line_count: reflection.reflected_observation_line_count,
                current_task: reflection.current_task.as_deref(),
                suggested_response: reflection.suggested_response.as_deref(),
            },
        )?;
        Ok(true)
    }

    fn handle_om_observe_buffer_requested(&self, event: &OutboxEvent) -> Result<bool> {
        let payload = parse_om_observe_buffer_requested_payload(&event.payload_json)?;
        let scope_key = payload.scope_key.as_str();
        let expected_generation = payload.expected_generation;
        let session_id = parse_observe_session_id(payload.session_id.as_deref(), &event.uri)?;

        let session = self.session(Some(&session_id));
        let _ = session.process_om_observe_buffer_requested(
            scope_key,
            expected_generation,
            event.id,
        )?;
        Ok(true)
    }

    fn handle_om_reflect_requested(&self, event: &OutboxEvent) -> Result<bool> {
        let payload = parse_om_reflect_requested_payload(&event.payload_json)?;
        let scope_key = payload.scope_key.as_str();
        let expected_generation = payload.expected_generation;

        let Some(record) = self.state.get_om_record_by_scope_key(scope_key)? else {
            return Ok(true);
        };
        if record.generation_count != expected_generation {
            return Ok(true);
        }

        let reflection = buffered_or_resolved_reflector_response(
            &record,
            scope_key,
            expected_generation,
            OmReflectorCallOptions::DEFAULT,
            &self.config.om.reflector,
        )?;
        let _applied = self.state.apply_om_reflection_with_cas(
            scope_key,
            expected_generation,
            event.id,
            &reflection.reflection,
            reflection.reflected_observation_line_count,
            OmReflectionApplyContext {
                current_task: reflection.current_task.as_deref(),
                suggested_response: reflection.suggested_response.as_deref(),
            },
        )?;
        Ok(true)
    }
}
