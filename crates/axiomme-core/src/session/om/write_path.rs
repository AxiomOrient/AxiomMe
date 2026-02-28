use chrono::{DateTime, Utc};

use crate::llm_io::estimate_text_tokens;
use crate::models::Message;
use crate::om::{
    OmObservationChunk, OmRecord, ProcessInputStepOptions, ResolvedOmConfig,
    activate_buffered_observations, compute_pending_tokens, decide_observer_write_action,
    normalize_observation_buffer_boundary, plan_process_input_step,
    should_skip_observer_continuation_hints,
};

use super::{
    DEFAULT_OM_ACTIVATION_RATIO, MessageWriteContext, ObserverRunContext, ObserverRunOptions,
    Session,
};
use crate::error::{AxiomError, Result};

impl Session {
    pub(in super::super) fn update_observational_memory_on_message_write(
        &self,
        message: &Message,
    ) -> Result<()> {
        if !self.om_enabled() {
            return Ok(());
        }
        let scope_binding = self.effective_om_scope_binding()?;
        let runtime_env = super::runtime_om_env_from_config(&self.config.om.runtime_env);
        let runtime_config = super::resolve_runtime_om_config(&runtime_env, scope_binding.scope)?;
        let scope_key = scope_binding.scope_key.clone();
        if scope_binding.scope != crate::om::OmScope::Session {
            self.state
                .upsert_om_scope_session(&scope_key, &self.session_id)?;
        }
        let session_uri = self.session_uri()?.to_string();
        let now = Utc::now();
        let context = MessageWriteContext {
            scope: scope_binding.scope,
            scope_key: &scope_key,
            session_uri: &session_uri,
            now,
            runtime_config,
        };
        let mut record = self
            .state
            .get_om_record_by_scope_key(&scope_key)?
            .unwrap_or_else(|| super::new_om_record(&scope_binding, now));
        Self::apply_message_token_update(&mut record, message, context.runtime_config, context.now);

        let mut observer_decision =
            decide_observer_write_action(&record, context.runtime_config.observation);
        if self.maybe_apply_reflection_without_observer(&mut record, observer_decision, context)? {
            return Ok(());
        }

        // `om_observation_chunks.record_id` references `om_records.id`, so ensure
        // the record exists before appending any buffered chunks.
        self.state.upsert_om_record(&record)?;
        let mut buffered_chunks = self.state.list_om_observation_chunks(&record.id)?;
        let mut step_plan =
            Self::plan_message_write_step(&record, context, !buffered_chunks.is_empty());
        self.activate_buffered_before_observer_if_needed(
            &mut record,
            &mut buffered_chunks,
            &mut observer_decision,
            &mut step_plan,
            context,
        )?;
        self.run_observer_if_needed(
            &mut record,
            &mut buffered_chunks,
            observer_decision,
            &step_plan,
            context,
        )?;
        self.activate_buffered_after_observer_if_needed(
            &mut record,
            &mut buffered_chunks,
            observer_decision,
            &step_plan,
            context,
        )?;
        self.finalize_message_write(&mut record, &buffered_chunks, context)
    }

    fn apply_message_token_update(
        record: &mut OmRecord,
        message: &Message,
        runtime_config: ResolvedOmConfig,
        now: DateTime<Utc>,
    ) {
        let message_tokens = estimate_text_tokens(&message.text);
        record.pending_message_tokens =
            compute_pending_tokens(record.pending_message_tokens, message_tokens);
        if runtime_config.observation.buffer_tokens.is_some() {
            record.last_buffered_at_tokens = normalize_observation_buffer_boundary(
                record.pending_message_tokens,
                record.last_buffered_at_tokens,
            );
        }
        record.updated_at = now;
    }

    fn maybe_apply_reflection_without_observer(
        &self,
        record: &mut OmRecord,
        observer_decision: crate::om::ObserverWriteDecision,
        context: MessageWriteContext<'_>,
    ) -> Result<bool> {
        if observer_decision.threshold_reached || observer_decision.interval_triggered {
            return Ok(false);
        }
        let step_plan = Self::plan_message_write_step(record, context, false);
        if let Some(reflection_decision) = step_plan.reflection_decision {
            self.apply_reflection_decision(reflection_decision, context.session_uri, record)?;
        }
        self.state.upsert_om_record(record)?;

        // Sync with in-memory index
        let mut index = self
            .index
            .write()
            .map_err(|_| AxiomError::lock_poisoned("index"))?;
        index.upsert_om_record(record.clone());

        Ok(true)
    }

    fn plan_message_write_step(
        record: &OmRecord,
        context: MessageWriteContext<'_>,
        has_buffered_observation_chunks: bool,
    ) -> crate::om::ProcessInputStepPlan {
        plan_process_input_step(
            record,
            context.runtime_config.observation,
            context.runtime_config.reflection,
            &context.now.to_rfc3339(),
            ProcessInputStepOptions {
                is_initial_step: true,
                read_only: false,
                has_buffered_observation_chunks,
            },
        )
    }

    fn activate_buffered_before_observer_if_needed(
        &self,
        record: &mut OmRecord,
        buffered_chunks: &mut Vec<OmObservationChunk>,
        observer_decision: &mut crate::om::ObserverWriteDecision,
        step_plan: &mut crate::om::ProcessInputStepPlan,
        context: MessageWriteContext<'_>,
    ) -> Result<()> {
        if !step_plan.should_activate_buffered_before_observer {
            return Ok(());
        }
        // Step-0 equivalent in write-path: activate buffered observations before sync observer.
        if let Some(activation) = activate_buffered_observations(
            record,
            buffered_chunks,
            context
                .runtime_config
                .observation
                .buffer_activation
                .unwrap_or(DEFAULT_OM_ACTIVATION_RATIO),
            observer_decision.threshold,
        ) {
            self.state.clear_om_observation_chunks_through_seq(
                &record.id,
                activation.activated_max_seq,
            )?;
            *observer_decision =
                decide_observer_write_action(record, context.runtime_config.observation);
            *step_plan =
                Self::plan_message_write_step(record, context, !buffered_chunks.is_empty());
        }
        Ok(())
    }

    fn run_observer_if_needed(
        &self,
        record: &mut OmRecord,
        buffered_chunks: &mut Vec<OmObservationChunk>,
        observer_decision: crate::om::ObserverWriteDecision,
        step_plan: &crate::om::ProcessInputStepPlan,
        context: MessageWriteContext<'_>,
    ) -> Result<()> {
        if !step_plan.should_run_observer {
            return Ok(());
        }
        // Interval-triggered observer work is delegated to outbox replay so write-path
        // latency stays bounded. Block-after paths still run synchronously.
        let should_enqueue_async_observer =
            observer_decision.interval_triggered && !observer_decision.block_after_exceeded;
        if should_enqueue_async_observer {
            record.observer_trigger_count_total =
                record.observer_trigger_count_total.saturating_add(1);
            record.last_buffered_at_tokens = record.pending_message_tokens;
            self.enqueue_observer_buffer_request(
                context.session_uri,
                context.scope_key,
                record.generation_count,
                context.now,
            )?;
            return Ok(());
        }
        let skip_continuation_hints = should_skip_observer_continuation_hints(observer_decision);
        let _ = self.run_observer_pass(
            ObserverRunContext {
                scope: context.scope,
                scope_key: context.scope_key,
                now: context.now,
            },
            context.runtime_config.observation.max_tokens_per_batch,
            record,
            buffered_chunks,
            ObserverRunOptions {
                skip_continuation_hints,
                increment_trigger_count: true,
                observe_outbox_event_id: None,
                observe_expected_generation: None,
                observe_cursor_after: None,
                strict_cursor_filtering: false,
            },
        )?;
        Ok(())
    }

    fn activate_buffered_after_observer_if_needed(
        &self,
        record: &mut OmRecord,
        buffered_chunks: &mut Vec<OmObservationChunk>,
        observer_decision: crate::om::ObserverWriteDecision,
        step_plan: &crate::om::ProcessInputStepPlan,
        context: MessageWriteContext<'_>,
    ) -> Result<()> {
        if !step_plan.should_activate_buffered_after_observer || buffered_chunks.is_empty() {
            return Ok(());
        }
        if let Some(activation) = activate_buffered_observations(
            record,
            buffered_chunks,
            context
                .runtime_config
                .observation
                .buffer_activation
                .unwrap_or(DEFAULT_OM_ACTIVATION_RATIO),
            observer_decision.threshold,
        ) {
            self.state.clear_om_observation_chunks_through_seq(
                &record.id,
                activation.activated_max_seq,
            )?;
        }
        Ok(())
    }

    fn finalize_message_write(
        &self,
        record: &mut OmRecord,
        buffered_chunks: &[OmObservationChunk],
        context: MessageWriteContext<'_>,
    ) -> Result<()> {
        let step_plan = Self::plan_message_write_step(record, context, !buffered_chunks.is_empty());
        if let Some(reflection_decision) = step_plan.reflection_decision {
            self.apply_reflection_decision(reflection_decision, context.session_uri, record)?;
        }
        record.updated_at = context.now;
        self.state.upsert_om_record(record)?;

        // Sync with in-memory index
        let mut index = self
            .index
            .write()
            .map_err(|_| AxiomError::lock_poisoned("index"))?;
        index.upsert_om_record(record.clone());

        Ok(())
    }
}
