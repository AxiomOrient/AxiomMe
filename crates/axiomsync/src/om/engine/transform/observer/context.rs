use std::collections::BTreeMap;

use crate::xml::{escape_xml_attribute, escape_xml_text};

use super::super::helpers::normalize_whitespace;
use super::super::types::{BUFFERED_OBSERVATIONS_SEPARATOR, OmObserverMessageCandidate};

pub fn build_other_conversation_blocks(
    candidates: &[OmObserverMessageCandidate],
    current_session_id: Option<&str>,
    max_part_chars: usize,
) -> Option<String> {
    if max_part_chars == 0 {
        return None;
    }

    let normalized_current_session_id = current_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let mut groups = BTreeMap::<String, Vec<&OmObserverMessageCandidate>>::new();
    for candidate in candidates {
        let Some(source_session_id) = candidate
            .source_session_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if normalized_current_session_id.is_some_and(|current| current == source_session_id) {
            continue;
        }
        groups
            .entry(source_session_id.to_string())
            .or_default()
            .push(candidate);
    }

    if groups.is_empty() {
        return None;
    }

    let mut blocks = Vec::<String>::new();
    for (source_session_id, mut messages) in groups {
        messages.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        let lines = messages
            .into_iter()
            .filter_map(|message| {
                let role = normalize_whitespace(&message.role);
                let text = normalize_whitespace(&message.text);
                if text.is_empty() {
                    return None;
                }
                let bounded = text.chars().take(max_part_chars).collect::<String>();
                if bounded.is_empty() {
                    return None;
                }
                let escaped_text = escape_xml_text(&bounded);
                Some(if role.is_empty() {
                    escaped_text
                } else {
                    format!("[{}] {escaped_text}", escape_xml_text(&role))
                })
            })
            .collect::<Vec<_>>();
        if lines.is_empty() {
            continue;
        }
        let source_session_id = escape_xml_attribute(&source_session_id);
        blocks.push(format!(
            "<other-conversation id=\"{source_session_id}\">\n{}\n</other-conversation>",
            lines.join("\n")
        ));
    }

    if blocks.is_empty() {
        None
    } else {
        Some(blocks.join("\n\n"))
    }
}

pub fn combine_observations_for_buffering(
    active_observations: &str,
    buffered_observations: &str,
) -> Option<String> {
    let active = active_observations.trim();
    let buffered = buffered_observations.trim();
    if active.is_empty() && buffered.is_empty() {
        return None;
    }
    if active.is_empty() {
        return Some(buffered.to_string());
    }
    if buffered.is_empty() {
        return Some(active.to_string());
    }
    Some(format!(
        "{active}\n\n{BUFFERED_OBSERVATIONS_SEPARATOR}\n\n{buffered}"
    ))
}
