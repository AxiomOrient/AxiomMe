use super::OmMultiThreadObserverSection;
use super::OmParseMode;
use super::sections::extract_and_remove_tag_sections_return_last;
use super::tokens::{TagKind, is_attr_name_char, is_tag_name_char, parse_tag_tokens};

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
            return Some(value.to_string());
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

    let current_task_tokens = parse_tag_tokens(content);
    let (without_current_task, current_task) = extract_and_remove_tag_sections_return_last(
        content,
        &current_task_tokens,
        "current-task",
        mode,
    );
    let suggested_tokens = parse_tag_tokens(&without_current_task);
    let (without_suggested_response, suggested_response) =
        extract_and_remove_tag_sections_return_last(
            &without_current_task,
            &suggested_tokens,
            "suggested-response",
            mode,
        );

    Some(OmMultiThreadObserverSection {
        thread_id: thread_id.to_string(),
        observations: without_suggested_response.trim().to_string(),
        current_task,
        suggested_response,
    })
}

struct OpenThread {
    thread_id: String,
    open_end: usize,
}

pub(super) fn extract_thread_blocks(text: &str, mode: OmParseMode) -> Vec<(String, String)> {
    let tokens = parse_tag_tokens(text);
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
                    .is_some_and(|segment| !segment.contains('\n'));
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
