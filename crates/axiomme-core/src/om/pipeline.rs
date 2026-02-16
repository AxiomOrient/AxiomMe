use super::{
    OmRecord, ReflectionEnqueueDecision, ResolvedObservationConfig, ResolvedReflectionConfig,
    decide_observer_write_action, decide_reflection_enqueue,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessInputStepOptions {
    pub is_initial_step: bool,
    pub read_only: bool,
    pub has_buffered_observation_chunks: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInputStepPlan {
    pub should_activate_buffered_before_observer: bool,
    pub should_run_observer: bool,
    pub should_activate_buffered_after_observer: bool,
    pub reflection_decision: Option<ReflectionEnqueueDecision>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessOutputResultPlan {
    pub should_save_unsaved_messages: bool,
}

#[must_use]
pub fn plan_process_input_step(
    record: &OmRecord,
    observation_config: ResolvedObservationConfig,
    reflection_config: ResolvedReflectionConfig,
    requested_at_rfc3339: &str,
    options: ProcessInputStepOptions,
) -> ProcessInputStepPlan {
    let observer = decide_observer_write_action(record, observation_config);
    let should_activate_buffered_before_observer = !options.read_only
        && options.is_initial_step
        && options.has_buffered_observation_chunks
        && observation_config.buffer_tokens.is_some()
        && observer.threshold_reached;
    let should_run_observer = !options.read_only && observer.should_run_observer;
    let should_activate_buffered_after_observer =
        !options.read_only && observer.should_activate_after_observer;
    let reflection_decision = if options.read_only {
        None
    } else {
        Some(decide_reflection_enqueue(
            record,
            reflection_config,
            requested_at_rfc3339,
        ))
    };
    ProcessInputStepPlan {
        should_activate_buffered_before_observer,
        should_run_observer,
        should_activate_buffered_after_observer,
        reflection_decision,
    }
}

#[must_use]
pub const fn plan_process_output_result(
    read_only: bool,
    unsaved_message_count: usize,
) -> ProcessOutputResultPlan {
    ProcessOutputResultPlan {
        should_save_unsaved_messages: !read_only && unsaved_message_count > 0,
    }
}

#[cfg(test)]
mod tests;
