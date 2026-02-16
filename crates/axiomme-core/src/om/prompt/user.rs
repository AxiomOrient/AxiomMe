use crate::om::reflector_compression_guidance;

use super::formatter::format_multi_thread_observer_messages_for_prompt;
use super::{OmObserverPromptInput, OmObserverThreadMessages, OmReflectorPromptInput};

const NO_CONTINUATION_HINT_SECTIONS: &str = "IMPORTANT: Do NOT include <current-task> or <suggested-response> sections in your output. Only output <observations>.";
const PREVIOUS_OBSERVATIONS_NOTE: &str =
    "\n\n---\n\nDo not repeat these existing observations. New observations will be appended.\n\n";

pub fn build_observer_user_prompt(input: OmObserverPromptInput<'_>) -> String {
    let mut prompt = String::new();

    if let Some(existing) = input
        .existing_observations
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.push_str("## Previous Observations\n\n");
        prompt.push_str(existing);
        prompt.push_str(PREVIOUS_OBSERVATIONS_NOTE);
    }

    prompt.push_str("## New Message History to Observe\n\n");
    prompt.push_str(input.message_history.trim());
    prompt.push_str("\n\n---\n\n");

    if let Some(other_context) = input
        .other_conversation_context
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.push_str("## Other Conversation Context\n\n");
        prompt.push_str(other_context);
        prompt.push_str("\n\n---\n\n");
    }

    if let Some(request_json) = input
        .request_json
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.push_str("## Observer Request JSON\n\n");
        prompt.push_str(request_json);
        prompt.push_str("\n\n---\n\n");
    }

    prompt.push_str("## Your Task\n\n");
    prompt.push_str(
        "Extract new observations from the message history. Keep observations factual and concise. Do not duplicate previous observations. observed_message_ids must use only provided ids.",
    );
    if input.skip_continuation_hints {
        prompt.push_str("\n\n");
        prompt.push_str(NO_CONTINUATION_HINT_SECTIONS);
    }

    prompt
}

pub fn build_multi_thread_observer_user_prompt(
    existing_observations: Option<&str>,
    threads: &[OmObserverThreadMessages],
    skip_continuation_hints: bool,
) -> String {
    let mut prompt = String::new();

    if let Some(existing) = existing_observations
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.push_str("## Previous Observations\n\n");
        prompt.push_str(existing);
        prompt.push_str(PREVIOUS_OBSERVATIONS_NOTE);
    }

    let formatted_messages = format_multi_thread_observer_messages_for_prompt(threads);
    prompt.push_str("## New Message History to Observe\n\n");
    if formatted_messages.is_empty() {
        prompt.push_str("No thread messages provided.");
    } else {
        prompt.push_str("The following messages are from multiple conversation threads. Each thread is wrapped in a <thread id=\"...\"> tag.\n\n");
        prompt.push_str(&formatted_messages);
    }
    prompt.push_str("\n\n---\n\n");
    prompt.push_str("## Your Task\n\n");
    prompt.push_str("Extract new observations for each thread. Output observations grouped by thread using <thread id=\"...\"> blocks inside <observations>.\n\n");
    prompt.push_str("Example output format:\n");
    prompt.push_str("<observations>\n");
    prompt.push_str("<thread id=\"thread-1\">\n");
    prompt.push_str("Date: Dec 4, 2025\n");
    prompt.push_str("* 🔴 (14:30) User prefers direct answers\n");
    prompt.push_str("<current-task>Working on feature X</current-task>\n");
    prompt.push_str("<suggested-response>Continue with implementation</suggested-response>\n");
    prompt.push_str("</thread>\n");
    prompt.push_str("</observations>");
    if skip_continuation_hints {
        prompt.push_str("\n\n");
        prompt.push_str(NO_CONTINUATION_HINT_SECTIONS);
    }

    prompt
}

pub fn build_reflector_user_prompt(input: OmReflectorPromptInput<'_>) -> String {
    let mut prompt = format!(
        "## OBSERVATIONS TO REFLECT ON\n\n{}\n\n---\n\nPlease analyze these observations and produce a refined, condensed version that will become the assistant's entire memory going forward.",
        input.observations.trim()
    );

    if let Some(manual_prompt) = input
        .manual_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.push_str("\n\n## SPECIFIC GUIDANCE\n\n");
        prompt.push_str(manual_prompt);
    }

    let compression_guidance = reflector_compression_guidance(input.compression_level);
    if !compression_guidance.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(compression_guidance);
    }
    if let Some(request_json) = input
        .request_json
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        prompt.push_str("\n\n## Reflector Request JSON\n\n");
        prompt.push_str(request_json);
    }
    if input.skip_continuation_hints {
        prompt.push_str("\n\n");
        prompt.push_str(NO_CONTINUATION_HINT_SECTIONS);
    }
    prompt
}
