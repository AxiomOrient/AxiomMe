fn escape_xml(text: &str, attribute: bool) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
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

pub fn escape_xml_text(text: &str) -> String {
    escape_xml(text, false)
}

pub fn escape_xml_attribute(text: &str) -> String {
    escape_xml(text, true)
}

#[cfg(test)]
mod tests {
    use super::{escape_xml_attribute, escape_xml_text};

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
}
