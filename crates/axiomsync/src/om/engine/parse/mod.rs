mod sections;
mod thread;
mod tokens;

use crate::xml::{escape_xml_attribute, escape_xml_text};
use sections::{extract_tag_content_from_tokens, section_ranges_for_tag};
use thread::{
    extract_thread_blocks, extract_thread_blocks_with_tokens, parse_thread_observer_section,
};
use tokens::parse_tag_tokens;
use tokens::{TagKind, TagSectionRange, TagToken};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmParseMode {
    // Reject recovery heuristics where possible.
    Strict,
    // Recover from common malformed overlaps produced by model output.
    Lenient,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OmMemorySection {
    pub observations: String,
    pub current_task: Option<String>,
    pub suggested_response: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmMultiThreadObserverSection {
    pub thread_id: String,
    pub observations: String,
    pub current_task: Option<String>,
    pub suggested_response: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OmMultiThreadObserverAggregate {
    pub observations: String,
    pub current_task: Option<String>,
    pub suggested_response: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct MemoryParseQuality {
    observation_chars: usize,
    metadata_fields: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct MultiThreadParseQuality {
    section_count: usize,
    sections_with_observations: usize,
    sections_with_metadata: usize,
}

fn is_numbered_list_item(trimmed: &str) -> bool {
    let digit_count = trimmed.chars().take_while(|ch| ch.is_ascii_digit()).count();
    digit_count > 0 && trimmed[digit_count..].starts_with(". ")
}

fn memory_parse_quality(section: &OmMemorySection) -> MemoryParseQuality {
    let current_task_present = section
        .current_task
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    let suggested_response_present = section
        .suggested_response
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    MemoryParseQuality {
        observation_chars: section.observations.trim().len(),
        metadata_fields: u8::from(current_task_present) + u8::from(suggested_response_present),
    }
}

fn multi_thread_parse_quality(
    sections: &[OmMultiThreadObserverSection],
) -> MultiThreadParseQuality {
    let mut quality = MultiThreadParseQuality {
        section_count: sections.len(),
        sections_with_observations: 0,
        sections_with_metadata: 0,
    };
    for section in sections {
        if !section.observations.trim().is_empty() {
            quality.sections_with_observations += 1;
        }
        let current_task_present = section
            .current_task
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        let suggested_response_present = section
            .suggested_response
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        if current_task_present || suggested_response_present {
            quality.sections_with_metadata += 1;
        }
    }
    quality
}

fn decide_memory_parse(strict: MemoryParseQuality, lenient: MemoryParseQuality) -> OmParseMode {
    if strict.observation_chars > 0 {
        OmParseMode::Strict
    } else if lenient.observation_chars > 0 {
        OmParseMode::Lenient
    } else if strict.metadata_fields >= lenient.metadata_fields {
        OmParseMode::Strict
    } else {
        OmParseMode::Lenient
    }
}

fn decide_multi_thread_parse(
    strict: MultiThreadParseQuality,
    lenient: MultiThreadParseQuality,
) -> OmParseMode {
    if strict.sections_with_observations > 0 {
        OmParseMode::Strict
    } else if lenient.sections_with_observations > 0 {
        OmParseMode::Lenient
    } else if strict.sections_with_metadata > lenient.sections_with_metadata {
        OmParseMode::Strict
    } else if lenient.sections_with_metadata > strict.sections_with_metadata {
        OmParseMode::Lenient
    } else if strict.section_count >= lenient.section_count {
        OmParseMode::Strict
    } else {
        OmParseMode::Lenient
    }
}

fn has_overlap_recovery_candidate(tokens: &[TagToken], tag: &str) -> bool {
    let mut open_seen = false;
    for token in tokens {
        if token.name != tag {
            continue;
        }
        match token.kind {
            TagKind::Open if token.line_anchored => {
                if open_seen {
                    return true;
                }
                open_seen = true;
            }
            TagKind::Close => {
                open_seen = false;
            }
            TagKind::Open => {}
        }
    }
    false
}

fn should_attempt_lenient_memory_parse(
    tokens: &[TagToken],
    strict_quality: MemoryParseQuality,
) -> bool {
    // Hot path: strict already extracted full signal.
    if strict_quality.observation_chars > 0 || strict_quality.metadata_fields == 2 {
        return false;
    }

    // Lenient mode only adds value for malformed overlap recovery.
    has_overlap_recovery_candidate(tokens, "observations")
        || has_overlap_recovery_candidate(tokens, "current-task")
        || has_overlap_recovery_candidate(tokens, "suggested-response")
}

fn should_attempt_lenient_multi_thread_parse(
    tokens: &[TagToken],
    strict_quality: MultiThreadParseQuality,
) -> bool {
    // Hot path: strict already retained at least one usable thread observation.
    if strict_quality.sections_with_observations > 0 {
        return false;
    }

    // Lenient mode only changes results when nested/overlapping tags exist.
    has_overlap_recovery_candidate(tokens, "observations")
        || has_overlap_recovery_candidate(tokens, "thread")
}

fn parse_thread_sections(
    scope: &str,
    mode: OmParseMode,
    tokens: Option<&[TagToken]>,
) -> Vec<OmMultiThreadObserverSection> {
    let thread_blocks = match tokens {
        Some(tokens) => extract_thread_blocks_with_tokens(scope, tokens, mode),
        None => extract_thread_blocks(scope, mode),
    };

    thread_blocks
        .into_iter()
        .filter_map(|(thread_id, body)| {
            parse_thread_observer_section(&thread_id, &body, mode).and_then(|section| {
                if section.thread_id.trim().is_empty() {
                    None
                } else {
                    Some(section)
                }
            })
        })
        .collect::<Vec<_>>()
}

fn join_section_ranges(text: &str, ranges: &[TagSectionRange]) -> String {
    let mut joined = String::new();
    for range in ranges {
        let Some(section) = text.get(range.content_start..range.content_end) else {
            continue;
        };
        let section = section.trim();
        if section.is_empty() {
            continue;
        }
        if !joined.is_empty() {
            joined.push('\n');
        }
        joined.push_str(section);
    }
    joined
}

fn parse_memory_section_xml_with_tokens(
    content: &str,
    tokens: &[TagToken],
    mode: OmParseMode,
) -> OmMemorySection {
    let observations = {
        let ranges = section_ranges_for_tag(content, tokens, "observations", mode);
        if ranges.is_empty() {
            extract_list_items_only(content)
        } else {
            join_section_ranges(content, &ranges)
        }
    }
    .trim()
    .to_string();

    let current_task = extract_tag_content_from_tokens(content, tokens, "current-task", mode)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let suggested_response =
        extract_tag_content_from_tokens(content, tokens, "suggested-response", mode)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

    OmMemorySection {
        observations,
        current_task,
        suggested_response,
    }
}

pub fn parse_memory_section_xml(content: &str, mode: OmParseMode) -> OmMemorySection {
    let tokens = parse_tag_tokens(content);
    parse_memory_section_xml_with_tokens(content, &tokens, mode)
}

pub fn parse_memory_section_xml_accuracy_first(content: &str) -> OmMemorySection {
    let tokens = parse_tag_tokens(content);
    let strict = parse_memory_section_xml_with_tokens(content, &tokens, OmParseMode::Strict);
    let strict_quality = memory_parse_quality(&strict);
    if !should_attempt_lenient_memory_parse(&tokens, strict_quality) {
        strict
    } else {
        let lenient = parse_memory_section_xml_with_tokens(content, &tokens, OmParseMode::Lenient);
        let lenient_quality = memory_parse_quality(&lenient);
        match decide_memory_parse(strict_quality, lenient_quality) {
            OmParseMode::Strict => strict,
            OmParseMode::Lenient => lenient,
        }
    }
}

fn parse_multi_thread_observer_output_with_tokens(
    content: &str,
    tokens: &[TagToken],
    mode: OmParseMode,
) -> Vec<OmMultiThreadObserverSection> {
    let observation_ranges = section_ranges_for_tag(content, tokens, "observations", mode);
    if observation_ranges.is_empty() {
        return parse_thread_sections(content, mode, Some(tokens));
    }

    let mut out = Vec::<OmMultiThreadObserverSection>::new();
    for range in observation_ranges {
        let Some(section) = content.get(range.content_start..range.content_end) else {
            continue;
        };
        let section = section.trim();
        if section.is_empty() {
            continue;
        }
        out.extend(parse_thread_sections(section, mode, None));
    }
    out
}

pub fn parse_multi_thread_observer_output(
    content: &str,
    mode: OmParseMode,
) -> Vec<OmMultiThreadObserverSection> {
    let tokens = parse_tag_tokens(content);
    parse_multi_thread_observer_output_with_tokens(content, &tokens, mode)
}

pub fn parse_multi_thread_observer_output_accuracy_first(
    content: &str,
) -> Vec<OmMultiThreadObserverSection> {
    let tokens = parse_tag_tokens(content);
    let strict =
        parse_multi_thread_observer_output_with_tokens(content, &tokens, OmParseMode::Strict);
    let strict_quality = multi_thread_parse_quality(&strict);
    if !should_attempt_lenient_multi_thread_parse(&tokens, strict_quality) {
        strict
    } else {
        let lenient =
            parse_multi_thread_observer_output_with_tokens(content, &tokens, OmParseMode::Lenient);
        let lenient_quality = multi_thread_parse_quality(&lenient);
        match decide_multi_thread_parse(strict_quality, lenient_quality) {
            OmParseMode::Strict => strict,
            OmParseMode::Lenient => lenient,
        }
    }
}

pub fn aggregate_multi_thread_observer_sections(
    sections: &[OmMultiThreadObserverSection],
    primary_thread_id: Option<&str>,
) -> OmMultiThreadObserverAggregate {
    let observations = sections
        .iter()
        .filter_map(|section| {
            let thread_id = section.thread_id.trim();
            let observations = section.observations.trim();
            if thread_id.is_empty() || observations.is_empty() {
                return None;
            }
            let thread_id = escape_xml_attribute(thread_id);
            let observations = escape_xml_text(observations);
            Some(format!(
                "<thread id=\"{thread_id}\">\n{observations}\n</thread>"
            ))
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let primary_thread_id = primary_thread_id
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let primary = primary_thread_id.and_then(|id| {
        sections
            .iter()
            .rev()
            .find(|section| section.thread_id.trim() == id)
    });

    let current_task = primary
        .and_then(|section| {
            section
                .current_task
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            sections.iter().rev().find_map(|section| {
                section
                    .current_task
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
        })
        .map(ToString::to_string);
    let suggested_response = primary
        .and_then(|section| {
            section
                .suggested_response
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            sections.iter().rev().find_map(|section| {
                section
                    .suggested_response
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
        })
        .map(ToString::to_string);

    OmMultiThreadObserverAggregate {
        observations,
        current_task,
        suggested_response,
    }
}

#[cfg(test)]
fn extract_observations_sections(text: &str, mode: OmParseMode) -> Option<String> {
    let tokens = parse_tag_tokens(text);
    let ranges = section_ranges_for_tag(text, &tokens, "observations", mode);
    if ranges.is_empty() {
        None
    } else {
        let joined = join_section_ranges(text, &ranges);
        if joined.is_empty() {
            None
        } else {
            Some(joined)
        }
    }
}

#[cfg(test)]
fn extract_tag_content(text: &str, tag: &str, mode: OmParseMode) -> Option<String> {
    let tokens = parse_tag_tokens(text);
    extract_tag_content_from_tokens(text, &tokens, tag, mode)
}

pub fn extract_list_items_only(text: &str) -> String {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("- ") || trimmed.starts_with("* ") || is_numbered_list_item(trimmed)
        })
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests;
