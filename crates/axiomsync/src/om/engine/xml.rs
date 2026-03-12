fn is_xml_char_allowed(ch: char) -> bool {
    matches!(
        ch,
        '\u{9}'
            | '\u{A}'
            | '\u{D}'
            | '\u{20}'..='\u{D7FF}'
            | '\u{E000}'..='\u{FFFD}'
            | '\u{10000}'..='\u{10FFFF}'
    )
}

fn escape_xml(text: &str, attribute: bool) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if !is_xml_char_allowed(ch) {
            continue;
        }
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            '\'' if attribute => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

pub(crate) fn escape_xml_text(text: &str) -> String {
    escape_xml(text, false)
}

pub(crate) fn escape_xml_attribute(text: &str) -> String {
    escape_xml(text, true)
}

fn decode_xml_entity(entity: &str) -> Option<char> {
    match entity {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" | "#39" => Some('\''),
        _ if entity.starts_with("#x") || entity.starts_with("#X") => {
            u32::from_str_radix(&entity[2..], 16)
                .ok()
                .and_then(char::from_u32)
        }
        _ if entity.starts_with('#') => entity[1..].parse::<u32>().ok().and_then(char::from_u32),
        _ => None,
    }
}

fn unescape_xml_entities(text: &str) -> String {
    if text.is_empty() || !text.contains('&') {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    while cursor < text.len() {
        if text.as_bytes()[cursor] == b'&'
            && cursor + 1 < text.len()
            && let Some(end_rel) = text[cursor + 1..].find(';')
        {
            let end = cursor + 1 + end_rel;
            let entity = &text[cursor + 1..end];
            if let Some(ch) = decode_xml_entity(entity) {
                out.push(ch);
                cursor = end + 1;
                continue;
            }
        }

        let Some(ch) = text[cursor..].chars().next() else {
            break;
        };
        out.push(ch);
        cursor += ch.len_utf8();
    }
    out
}

pub(crate) fn unescape_xml_attribute(text: &str) -> String {
    unescape_xml_entities(text)
}

#[cfg(test)]
mod tests {
    use super::{escape_xml_attribute, escape_xml_text, unescape_xml_attribute};

    #[test]
    fn escape_xml_text_replaces_core_entities() {
        let raw = "a&b<c>d\"'x";
        assert_eq!(escape_xml_text(raw), "a&amp;b&lt;c&gt;d\"'x");
    }

    #[test]
    fn escape_xml_attribute_replaces_attribute_entities() {
        let raw = "a&b<c>d\"'x";
        assert_eq!(escape_xml_attribute(raw), "a&amp;b&lt;c&gt;d&quot;&#39;x");
    }

    #[test]
    fn escape_xml_is_identity_for_plain_text() {
        let raw = "plain_text_123";
        assert_eq!(escape_xml_text(raw), raw);
        assert_eq!(escape_xml_attribute(raw), raw);
    }

    #[test]
    fn escape_xml_drops_disallowed_control_characters() {
        let raw = format!("a{}b{}c\t\n\r", '\u{0}', '\u{1F}');
        assert_eq!(escape_xml_text(&raw), "abc\t\n\r");
        assert_eq!(escape_xml_attribute(&raw), "abc\t\n\r");
    }

    #[test]
    fn unescape_xml_attribute_decodes_named_and_numeric_entities() {
        let raw = "a&amp;b&lt;c&gt;d&quot;&#39;e&#x41;&#65;";
        assert_eq!(unescape_xml_attribute(raw), "a&b<c>d\"'eAA");
    }
}
