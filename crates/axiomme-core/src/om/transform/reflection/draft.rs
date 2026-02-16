use super::super::types::ReflectionDraft;
use crate::llm_io::estimate_text_tokens;

#[must_use]
pub fn merge_buffered_reflection(
    active_lines: &[String],
    reflected_line_count: usize,
    buffered_reflection: &str,
) -> String {
    let reflection = buffered_reflection.trim();
    if reflection.is_empty() {
        return active_lines.join("\n").trim().to_string();
    }

    let split_at = reflected_line_count.min(active_lines.len());
    let unreflected = active_lines[split_at..].join("\n");
    let unreflected = unreflected.trim();

    if unreflected.is_empty() {
        reflection.to_string()
    } else {
        format!("{reflection}\n\n{unreflected}")
    }
}

fn saturating_usize_to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[must_use]
pub fn build_reflection_draft(
    active_observations: &str,
    max_chars: usize,
) -> Option<ReflectionDraft> {
    let max_chars = max_chars.max(1);
    let lines = active_observations
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return None;
    }

    let reflection_input = lines.join(" ");
    let reflection_input = reflection_input
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if reflection_input.is_empty() {
        return None;
    }

    let reflection = reflection_input.chars().take(max_chars).collect::<String>();
    if reflection.is_empty() {
        return None;
    }

    Some(ReflectionDraft {
        reflection_token_count: estimate_text_tokens(&reflection),
        reflected_observation_line_count: saturating_usize_to_u32(lines.len()),
        reflection_input_tokens: estimate_text_tokens(&reflection_input),
        reflection,
    })
}
