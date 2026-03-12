use std::collections::HashSet;

const OBSERVATION_HINT_PREFIX: &str = "om:";

fn normalize_whitespace_line(line: &str) -> String {
    let mut out = String::new();
    for part in line.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(part);
    }
    out
}

fn is_continuation_reservation(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("<current-task>")
        || lower.contains("<suggested-response>")
        || lower.starts_with("current-task:")
        || lower.starts_with("suggested-response:")
        || lower.starts_with("next:")
}

fn is_high_priority(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    line.contains('🔴')
        || lower.starts_with("high:")
        || lower.starts_with("[high]")
        || lower.starts_with("priority:high")
        || lower.contains(" priority:high")
}

pub fn build_bounded_observation_hint(
    active_observations: &str,
    max_lines: usize,
    max_chars: usize,
) -> Option<String> {
    if max_lines == 0 || max_chars == 0 {
        return None;
    }

    let all_lines = active_observations
        .lines()
        .map(normalize_whitespace_line)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if all_lines.is_empty() {
        return None;
    }

    let mut selected_indices = Vec::<usize>::new();
    let mut seen_lines = HashSet::<String>::new();

    for idx in (0..all_lines.len()).rev() {
        if selected_indices.len() >= max_lines {
            break;
        }
        let line = &all_lines[idx];
        if is_continuation_reservation(line) && seen_lines.insert(line.clone()) {
            selected_indices.push(idx);
        }
    }

    for idx in (0..all_lines.len()).rev() {
        if selected_indices.len() >= max_lines {
            break;
        }
        let line = &all_lines[idx];
        if is_high_priority(line) && seen_lines.insert(line.clone()) {
            selected_indices.push(idx);
        }
    }

    for idx in (0..all_lines.len()).rev() {
        if selected_indices.len() >= max_lines {
            break;
        }
        let line = &all_lines[idx];
        if seen_lines.insert(line.clone()) {
            selected_indices.push(idx);
        }
    }

    selected_indices.sort_unstable();
    let selected_lines = selected_indices
        .into_iter()
        .map(|idx| all_lines[idx].clone())
        .collect::<Vec<_>>();
    if selected_lines.is_empty() {
        return None;
    }

    let mut bounded = String::new();
    let mut remaining = max_chars;
    for line in selected_lines {
        if remaining == 0 {
            break;
        }
        if !bounded.is_empty() {
            if remaining < 2 {
                break;
            }
            bounded.push(' ');
            remaining -= 1;
        }
        for ch in line.chars() {
            if remaining == 0 {
                break;
            }
            bounded.push(ch);
            remaining -= 1;
        }
    }
    let bounded = bounded.trim().to_string();
    if bounded.is_empty() {
        return None;
    }

    Some(format!("{OBSERVATION_HINT_PREFIX} {bounded}"))
}

#[cfg(test)]
mod tests;
