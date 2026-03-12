use super::OmParseMode;
use super::tokens::{TagKind, TagSectionRange, TagToken};

fn normalize_tag_name(tag: &str) -> Option<String> {
    let normalized = tag.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn section_content<'a>(text: &'a str, range: &TagSectionRange) -> Option<&'a str> {
    text.get(range.content_start..range.content_end)
}

pub(super) fn section_ranges_for_tag(
    text: &str,
    tokens: &[TagToken],
    tag: &str,
    mode: OmParseMode,
) -> Vec<TagSectionRange> {
    let Some(tag) = normalize_tag_name(tag) else {
        return Vec::new();
    };

    let mut ranges = Vec::<TagSectionRange>::new();
    let mut current_open: Option<&TagToken> = None;
    let mut discard_next_close = false;

    for token in tokens {
        if token.name != tag {
            continue;
        }
        match token.kind {
            TagKind::Open => {
                // Lenient mode prefers latest anchored open for malformed overlap recovery.
                // Strict mode rejects ambiguous overlap instead of recovering.
                if !token.line_anchored {
                    continue;
                }
                if matches!(mode, OmParseMode::Strict) && current_open.is_some() {
                    // Ambiguous overlap in strict mode: discard this malformed block.
                    current_open = None;
                    discard_next_close = true;
                    continue;
                }
                if matches!(mode, OmParseMode::Lenient) || current_open.is_none() {
                    current_open = Some(token);
                }
            }
            TagKind::Close => {
                if discard_next_close {
                    discard_next_close = false;
                    continue;
                }
                let Some(open) = current_open else {
                    continue;
                };
                if token.start < open.end {
                    continue;
                }

                let same_line_close = text
                    .get(open.end..token.start)
                    .map(|segment| !segment.contains('\n'))
                    .unwrap_or(false);
                if !token.line_anchored && !same_line_close {
                    continue;
                }

                ranges.push(TagSectionRange {
                    open_start: open.start,
                    content_start: open.end,
                    content_end: token.start,
                    close_end: token.end,
                });
                current_open = None;
            }
        }
    }

    ranges
}

pub(super) fn extract_tag_sections_from_tokens(
    text: &str,
    tokens: &[TagToken],
    tag: &str,
    mode: OmParseMode,
) -> Vec<String> {
    section_ranges_for_tag(text, tokens, tag, mode)
        .into_iter()
        .filter_map(|range| section_content(text, &range).map(str::trim))
        .filter(|content| !content.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
}

pub(super) fn extract_tag_content_from_tokens(
    text: &str,
    tokens: &[TagToken],
    tag: &str,
    mode: OmParseMode,
) -> Option<String> {
    extract_tag_sections_from_tokens(text, tokens, tag, mode)
        .into_iter()
        .rev()
        .find(|content| !content.is_empty())
}
