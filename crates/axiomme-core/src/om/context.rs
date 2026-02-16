use std::collections::VecDeque;

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

#[must_use]
pub fn build_bounded_observation_hint(
    active_observations: &str,
    max_lines: usize,
    max_chars: usize,
) -> Option<String> {
    if max_lines == 0 || max_chars == 0 {
        return None;
    }

    let mut tail = VecDeque::<String>::new();
    for line in active_observations.lines() {
        let compact = normalize_whitespace_line(line);
        if compact.is_empty() {
            continue;
        }
        tail.push_back(compact);
        if tail.len() > max_lines {
            let _ = tail.pop_front();
        }
    }
    if tail.is_empty() {
        return None;
    }

    let mut bounded = String::new();
    let mut remaining = max_chars;
    for line in tail {
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
