use std::collections::HashSet;

use crate::om::inference::OmPendingMessage;

use super::super::helpers::normalize_whitespace;

pub fn synthesize_observer_observations(
    active_observations: &str,
    pending_messages: &[OmPendingMessage],
    max_chars: usize,
) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let existing = active_observations
        .lines()
        .map(normalize_whitespace)
        .filter(|line| !line.is_empty())
        .collect::<HashSet<_>>();

    let mut seen = HashSet::<String>::new();
    let mut lines = Vec::<String>::new();

    for item in pending_messages {
        let role = normalize_whitespace(&item.role);
        let text = normalize_whitespace(&item.text);
        if text.is_empty() {
            continue;
        }
        let line = if role.is_empty() {
            text
        } else {
            format!("[{role}] {text}")
        };
        let normalized = normalize_whitespace(&line);
        if normalized.is_empty() || existing.contains(&normalized) || !seen.insert(normalized) {
            continue;
        }
        lines.push(line);
    }

    // Keep forward progress even when all candidates were deduplicated.
    if lines.is_empty()
        && let Some(fallback) = pending_messages.iter().find_map(|item| {
            let role = normalize_whitespace(&item.role);
            let text = normalize_whitespace(&item.text);
            if text.is_empty() {
                return None;
            }
            Some(if role.is_empty() {
                text
            } else {
                format!("[{role}] {text}")
            })
        })
    {
        lines.push(fallback);
    }

    lines.join("\n").chars().take(max_chars).collect::<String>()
}
