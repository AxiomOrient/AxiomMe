mod llm;
mod parsing;
mod record;
mod response;
mod threading;

#[cfg(test)]
pub(super) use llm::select_messages_for_observer_llm;
#[cfg(test)]
pub(super) use parsing::{parse_llm_observer_response, parse_observer_response_value};
pub(super) use record::{
    build_observation_chunk, new_om_record, observed_message_ids_set,
    record_with_buffered_observation_context,
};
#[cfg(test)]
pub(super) use record::{
    new_session_om_record, normalize_observation_text, parse_env_enabled_default_true,
};
#[cfg(test)]
pub(super) use response::deterministic_observer_response;
pub(super) use response::{
    collect_last_observed_by_thread, merge_observe_after_cursor,
    resolve_observer_response_with_config,
};
pub(super) use threading::resolve_observer_thread_group_id;
#[cfg(test)]
pub(super) use threading::{
    build_observer_batch_tasks, build_observer_thread_messages, chunk_observer_thread_batches,
    collect_known_ids_for_thread_batch, parse_llm_multi_thread_observer_response,
};
