#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TagKind {
    Open,
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TagToken {
    pub(super) kind: TagKind,
    pub(super) name: String,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) line_anchored: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TagSectionRange {
    pub(super) open_start: usize,
    pub(super) content_start: usize,
    pub(super) content_end: usize,
    pub(super) close_end: usize,
}

pub(super) const fn is_tag_name_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-'
}

pub(super) const fn is_attr_name_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
}

fn is_line_anchored(text: &str, tag_index: usize) -> bool {
    let line_start = text[..tag_index].rfind('\n').map_or(0, |idx| idx + 1);
    text[line_start..tag_index]
        .chars()
        .all(|ch| ch == ' ' || ch == '\t')
}

fn find_tag_end(bytes: &[u8], mut cursor: usize) -> Option<usize> {
    let mut quote: Option<u8> = None;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if let Some(active_quote) = quote {
            if byte == active_quote {
                quote = None;
            }
            cursor += 1;
            continue;
        }
        if byte == b'\'' || byte == b'"' {
            quote = Some(byte);
            cursor += 1;
            continue;
        }
        if byte == b'>' {
            return Some(cursor);
        }
        cursor += 1;
    }
    None
}

pub(super) fn parse_tag_tokens(text: &str) -> Vec<TagToken> {
    let bytes = text.as_bytes();
    let mut offset = 0usize;
    let mut tokens = Vec::<TagToken>::new();

    while offset < bytes.len() {
        if bytes[offset] != b'<' {
            offset += 1;
            continue;
        }

        let Some(end) = find_tag_end(bytes, offset + 1) else {
            break;
        };
        let inner = &text[offset + 1..end];
        let inner_bytes = inner.as_bytes();

        let mut cursor = 0usize;
        while cursor < inner_bytes.len() && inner_bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= inner_bytes.len() {
            offset = end + 1;
            continue;
        }

        let kind = if inner_bytes[cursor] == b'/' {
            cursor += 1;
            while cursor < inner_bytes.len() && inner_bytes[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            TagKind::Close
        } else {
            TagKind::Open
        };

        let name_start = cursor;
        while cursor < inner_bytes.len() && is_tag_name_char(inner_bytes[cursor]) {
            cursor += 1;
        }
        if cursor == name_start {
            offset = end + 1;
            continue;
        }

        tokens.push(TagToken {
            kind,
            name: inner[name_start..cursor].to_ascii_lowercase(),
            start: offset,
            end: end + 1,
            line_anchored: is_line_anchored(text, offset),
        });

        offset = end + 1;
    }

    tokens
}
