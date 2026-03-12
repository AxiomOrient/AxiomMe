use super::OmMultiThreadObserverSection;
use super::OmParseMode;
use super::sections::section_ranges_for_tag;
use super::tokens::{
    TagKind, TagSectionRange, TagToken, is_attr_name_char, is_tag_name_char, parse_tag_tokens,
};
use crate::xml::unescape_xml_attribute;

fn parse_tag_attribute(open_tag: &str, key: &str) -> Option<String> {
    if !open_tag.starts_with('<') || !open_tag.ends_with('>') {
        return None;
    }
    let inner = &open_tag[1..open_tag.len() - 1];
    let bytes = inner.as_bytes();
    let mut cursor = 0usize;

    while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    while cursor < bytes.len() && is_tag_name_char(bytes[cursor]) {
        cursor += 1;
    }

    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }

        let name_start = cursor;
        while cursor < bytes.len() && is_attr_name_char(bytes[cursor]) {
            cursor += 1;
        }
        if cursor == name_start {
            cursor += 1;
            continue;
        }
        let name = inner[name_start..cursor].to_ascii_lowercase();

        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b'=' {
            continue;
        }
        cursor += 1;

        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }

        let value = if bytes[cursor] == b'"' || bytes[cursor] == b'\'' {
            let quote = bytes[cursor];
            cursor += 1;
            let value_start = cursor;
            while cursor < bytes.len() && bytes[cursor] != quote {
                cursor += 1;
            }
            let value = inner.get(value_start..cursor).unwrap_or_default();
            if cursor < bytes.len() {
                cursor += 1;
            }
            value
        } else {
            let value_start = cursor;
            while cursor < bytes.len() && !bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            inner.get(value_start..cursor).unwrap_or_default()
        };

        if name == key {
            return Some(unescape_xml_attribute(value));
        }
    }

    None
}

pub(super) fn extract_thread_id(thread_open_tag: &str) -> Option<String> {
    parse_tag_attribute(thread_open_tag, "id")
}

pub(super) fn parse_thread_observer_section(
    thread_id: &str,
    content: &str,
    mode: OmParseMode,
) -> Option<OmMultiThreadObserverSection> {
    let thread_id = thread_id.trim();
    if thread_id.is_empty() {
        return None;
    }

    let tokens = parse_tag_tokens(content);
    let current_task_ranges = section_ranges_for_tag(content, &tokens, "current-task", mode);
    let current_task = extract_last_non_empty_content_from_ranges(content, &current_task_ranges);

    let suggested_ranges = section_ranges_for_tag(content, &tokens, "suggested-response", mode)
        .into_iter()
        .filter(|range| {
            current_task_ranges
                .iter()
                .all(|current| !ranges_overlap(range, current))
        })
        .collect::<Vec<_>>();
    let suggested_response = extract_last_non_empty_content_from_ranges(content, &suggested_ranges);

    let mut stripped_ranges = current_task_ranges;
    stripped_ranges.extend(section_ranges_for_tag(
        content,
        &tokens,
        "suggested-response",
        mode,
    ));
    let stripped_observations = strip_tag_ranges(content, stripped_ranges);

    Some(OmMultiThreadObserverSection {
        thread_id: thread_id.to_string(),
        observations: stripped_observations,
        current_task,
        suggested_response,
    })
}

fn ranges_overlap(left: &TagSectionRange, right: &TagSectionRange) -> bool {
    left.open_start < right.close_end && right.open_start < left.close_end
}

fn extract_last_non_empty_content_from_ranges(
    text: &str,
    ranges: &[TagSectionRange],
) -> Option<String> {
    ranges
        .iter()
        .rev()
        .filter_map(|range| text.get(range.content_start..range.content_end))
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn strip_tag_ranges(text: &str, mut ranges: Vec<TagSectionRange>) -> String {
    if ranges.is_empty() {
        return text.trim().to_string();
    }

    ranges.sort_by(|left, right| {
        left.open_start
            .cmp(&right.open_start)
            .then_with(|| left.close_end.cmp(&right.close_end))
    });

    let mut stripped = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for range in ranges {
        if range.open_start > cursor
            && let Some(prefix) = text.get(cursor..range.open_start)
        {
            stripped.push_str(prefix);
        }
        cursor = cursor.max(range.close_end);
    }
    if let Some(suffix) = text.get(cursor..) {
        stripped.push_str(suffix);
    }

    stripped.trim().to_string()
}

struct OpenThread {
    thread_id: String,
    open_end: usize,
}

pub(super) fn extract_thread_blocks_with_tokens(
    text: &str,
    tokens: &[TagToken],
    mode: OmParseMode,
) -> Vec<(String, String)> {
    let mut blocks = Vec::<(String, String)>::new();
    let mut open: Option<OpenThread> = None;
    let mut discard_next_close = false;

    for token in tokens {
        if token.name != "thread" {
            continue;
        }
        match token.kind {
            TagKind::Open => {
                if !token.line_anchored {
                    continue;
                }
                if matches!(mode, OmParseMode::Strict) && open.is_some() {
                    // Ambiguous overlap in strict mode: discard this malformed block.
                    open = None;
                    discard_next_close = true;
                    continue;
                }
                let open_tag = text.get(token.start..token.end).unwrap_or_default();
                let thread_id = extract_thread_id(open_tag).unwrap_or_default();
                open = Some(OpenThread {
                    thread_id,
                    open_end: token.end,
                });
            }
            TagKind::Close => {
                if discard_next_close {
                    discard_next_close = false;
                    continue;
                }
                let Some(current) = open.as_ref() else {
                    continue;
                };
                if token.start < current.open_end {
                    continue;
                }
                let same_line_close = text
                    .get(current.open_end..token.start)
                    .map(|segment| !segment.contains('\n'))
                    .unwrap_or(false);
                if !token.line_anchored && !same_line_close {
                    continue;
                }

                let body = text
                    .get(current.open_end..token.start)
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                blocks.push((current.thread_id.clone(), body));
                open = None;
            }
        }
    }

    blocks
}

pub(super) fn extract_thread_blocks(text: &str, mode: OmParseMode) -> Vec<(String, String)> {
    let tokens = parse_tag_tokens(text);
    extract_thread_blocks_with_tokens(text, &tokens, mode)
}
