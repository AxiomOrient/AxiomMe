mod formatter;
mod system;
mod user;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OmObserverPromptInput<'a> {
    pub request_json: Option<&'a str>,
    pub existing_observations: Option<&'a str>,
    pub message_history: &'a str,
    pub other_conversation_context: Option<&'a str>,
    pub skip_continuation_hints: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OmReflectorPromptInput<'a> {
    pub observations: &'a str,
    pub request_json: Option<&'a str>,
    pub manual_prompt: Option<&'a str>,
    pub compression_level: u8,
    pub skip_continuation_hints: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmObserverThreadMessages {
    pub thread_id: String,
    pub message_history: String,
}

pub use formatter::{
    format_multi_thread_observer_messages_for_prompt, format_observer_messages_for_prompt,
};
pub use system::{
    build_multi_thread_observer_system_prompt, build_observer_system_prompt,
    build_reflector_system_prompt,
};
pub use user::{
    build_multi_thread_observer_user_prompt, build_observer_user_prompt,
    build_reflector_user_prompt,
};

#[cfg(test)]
mod tests;
