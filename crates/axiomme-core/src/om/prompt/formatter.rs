use crate::om::OmPendingMessage;
use crate::om::xml::{escape_xml_attribute, escape_xml_text};

use super::OmObserverThreadMessages;

fn normalize_role(role: &str) -> String {
    let mut chars = role.trim().chars();
    let Some(first) = chars.next() else {
        return "Unknown".to_string();
    };
    let mut normalized = first.to_uppercase().collect::<String>();
    normalized.push_str(chars.as_str());
    normalized
}

fn format_timestamp_for_observer(raw: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|value| value.with_timezone(&chrono::Utc))
        .map(|value| value.format("%b %-d, %Y, %-I:%M %p").to_string())
}

#[must_use]
pub fn format_observer_messages_for_prompt(messages: &[OmPendingMessage]) -> String {
    messages
        .iter()
        .map(|message| {
            let role = normalize_role(&message.role);
            let text = message.text.trim();
            let timestamp = message
                .created_at_rfc3339
                .as_deref()
                .and_then(format_timestamp_for_observer);
            let timestamp_suffix = timestamp
                .map(|value| format!(" ({value})"))
                .unwrap_or_default();
            format!("**{role}{timestamp_suffix}:**\n{text}")
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

#[must_use]
pub fn format_multi_thread_observer_messages_for_prompt(
    threads: &[OmObserverThreadMessages],
) -> String {
    threads
        .iter()
        .filter_map(|thread| {
            let thread_id = thread.thread_id.trim();
            let message_history = thread.message_history.trim();
            if thread_id.is_empty() || message_history.is_empty() {
                return None;
            }
            let thread_id = escape_xml_attribute(thread_id);
            let message_history = escape_xml_text(message_history);
            Some(format!(
                "<thread id=\"{thread_id}\">\n{message_history}\n</thread>"
            ))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
