use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedDocument {
    pub parser: String,
    pub is_text: bool,
    pub title: Option<String>,
    pub text_preview: String,
    #[serde(skip)]
    pub normalized_text: Option<String>,
    pub line_count: usize,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ParserRegistry;

impl ParserRegistry {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    #[must_use]
    pub fn parse_file(&self, path: &Path, bytes: &[u8]) -> ParsedDocument {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        match ext.as_str() {
            "md" | "markdown" => parse_markdown(bytes),
            "json" => parse_json(bytes),
            "yaml" | "yml" => parse_yaml(bytes),
            "toml" => parse_toml(bytes),
            "jsonl" => parse_jsonl(bytes),
            "xml" => parse_xml(bytes),
            _ => {
                if let Ok(text) = std::str::from_utf8(bytes) {
                    return parse_plain_text(text);
                }
                parse_binary(bytes)
            }
        }
    }
}

fn parse_markdown(bytes: &[u8]) -> ParsedDocument {
    let text = String::from_utf8_lossy(bytes);
    let normalized = normalize_markdown_for_indexing(&text);
    let mut title = None;
    for line in normalized.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("# ") {
            title = Some(rest.trim().to_string());
            break;
        }
        if !trimmed.is_empty() && title.is_none() && !is_markdown_rule_line(trimmed) {
            title = Some(trimmed.to_string());
        }
    }

    let preview = normalized.chars().take(240).collect::<String>();
    ParsedDocument {
        parser: "markdown".to_string(),
        is_text: true,
        title,
        text_preview: preview,
        normalized_text: Some(normalized.clone()),
        line_count: normalized.lines().count(),
        tags: vec!["markdown".to_string()],
    }
}

fn parse_plain_text(text: &str) -> ParsedDocument {
    let preview = text.chars().take(240).collect::<String>();
    let title = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToString::to_string);

    ParsedDocument {
        parser: "text".to_string(),
        is_text: true,
        title,
        text_preview: preview,
        normalized_text: Some(text.to_string()),
        line_count: text.lines().count(),
        tags: vec!["text".to_string()],
    }
}

fn parse_json(bytes: &[u8]) -> ParsedDocument {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return parse_binary(bytes);
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return parse_plain_text(text);
    };

    let mut lines = Vec::new();
    flatten_json_value(&value, "$", &mut lines);
    if lines.is_empty() {
        lines.push("$={}".to_string());
    }
    structured_document(
        "json",
        infer_title_from_json_value(&value),
        lines,
        vec!["json".to_string(), "config".to_string()],
    )
}

fn parse_yaml(bytes: &[u8]) -> ParsedDocument {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return parse_binary(bytes);
    };
    let Ok(value) = serde_norway::from_str::<serde_norway::Value>(text) else {
        return parse_plain_text(text);
    };
    let Ok(json_value) = serde_json::to_value(value) else {
        return parse_plain_text(text);
    };

    let mut lines = Vec::new();
    flatten_json_value(&json_value, "$", &mut lines);
    if lines.is_empty() {
        lines.push("$={}".to_string());
    }
    structured_document(
        "yaml",
        infer_title_from_json_value(&json_value),
        lines,
        vec!["yaml".to_string(), "config".to_string()],
    )
}

fn parse_toml(bytes: &[u8]) -> ParsedDocument {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return parse_binary(bytes);
    };
    let Ok(value) = text.parse::<toml::Value>() else {
        return parse_plain_text(text);
    };
    let Ok(json_value) = serde_json::to_value(value) else {
        return parse_plain_text(text);
    };

    let mut lines = Vec::new();
    flatten_json_value(&json_value, "$", &mut lines);
    if lines.is_empty() {
        lines.push("$={}".to_string());
    }
    structured_document(
        "toml",
        infer_title_from_json_value(&json_value),
        lines,
        vec!["toml".to_string(), "config".to_string()],
    )
}

fn parse_jsonl(bytes: &[u8]) -> ParsedDocument {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return parse_binary(bytes);
    };

    let mut lines = Vec::new();
    let mut key_histogram = HashMap::<String, usize>::new();
    let mut parsed_count = 0usize;
    let mut inferred_title = None::<String>;

    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        parsed_count = parsed_count.saturating_add(1);
        if inferred_title.is_none() {
            inferred_title = infer_title_from_json_value(&value);
        }
        update_top_level_key_histogram(&value, &mut key_histogram);
        flatten_json_value(
            &value,
            &format!("$[{}]", line_idx.saturating_add(1)),
            &mut lines,
        );
    }

    if parsed_count == 0 {
        return parse_plain_text(text);
    }

    let mut normalized = Vec::new();
    normalized.push(format!("jsonl.records={parsed_count}"));
    let dominant_keys = dominant_keys(&key_histogram, 8);
    if !dominant_keys.is_empty() {
        normalized.push(format!("jsonl.keys={}", dominant_keys.join(",")));
    }
    normalized.extend(lines);

    structured_document(
        "jsonl",
        inferred_title,
        normalized,
        vec!["jsonl".to_string(), "data".to_string()],
    )
}

fn parse_xml(bytes: &[u8]) -> ParsedDocument {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return parse_binary(bytes);
    };
    let lines = flatten_xml(text);
    if lines.is_empty() {
        return parse_plain_text(text);
    }

    let title = lines
        .iter()
        .find_map(|line| {
            let (path, value) = line.split_once('=')?;
            if path.ends_with("/title") || path.ends_with("/name") || path.ends_with("/id") {
                return Some(value.to_string());
            }
            None
        })
        .filter(|value| !value.trim().is_empty());

    structured_document(
        "xml",
        title,
        lines,
        vec!["xml".to_string(), "data".to_string()],
    )
}

fn structured_document(
    parser: &str,
    title: Option<String>,
    lines: Vec<String>,
    tags: Vec<String>,
) -> ParsedDocument {
    let normalized = lines.join("\n");
    ParsedDocument {
        parser: parser.to_string(),
        is_text: true,
        title,
        text_preview: preview_from_lines(&lines),
        normalized_text: Some(normalized),
        line_count: lines.len(),
        tags,
    }
}

fn preview_from_lines(lines: &[String]) -> String {
    let joined = lines
        .iter()
        .take(12)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    joined.chars().take(240).collect::<String>()
}

fn infer_title_from_json_value(value: &serde_json::Value) -> Option<String> {
    let object = value.as_object()?;
    for key in ["title", "name", "id"] {
        if let Some(text) = object.get(key).and_then(serde_json::Value::as_str) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn flatten_json_value(value: &serde_json::Value, path: &str, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            if map.is_empty() {
                out.push(format!("{path}={{}}"));
                return;
            }
            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            for key in keys {
                if let Some(child) = map.get(&key) {
                    let next_path = if path == "$" {
                        format!("$.{key}")
                    } else {
                        format!("{path}.{key}")
                    };
                    flatten_json_value(child, &next_path, out);
                }
            }
        }
        serde_json::Value::Array(list) => {
            if list.is_empty() {
                out.push(format!("{path}=[]"));
                return;
            }
            for (idx, child) in list.iter().enumerate() {
                flatten_json_value(child, &format!("{path}[{idx}]"), out);
            }
        }
        _ => {
            out.push(format!("{path}={}", json_scalar_text(value)));
        }
    }
}

fn json_scalar_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => normalize_scalar_text(text),
        serde_json::Value::Bool(v) => v.to_string(),
        serde_json::Value::Number(v) => v.to_string(),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => "{}".to_string(),
    }
}

fn normalize_scalar_text(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(240).collect::<String>()
}

fn update_top_level_key_histogram(
    value: &serde_json::Value,
    histogram: &mut HashMap<String, usize>,
) {
    let Some(map) = value.as_object() else {
        return;
    };
    for key in map.keys() {
        *histogram.entry(key.clone()).or_insert(0) += 1;
    }
}

fn dominant_keys(histogram: &HashMap<String, usize>, limit: usize) -> Vec<String> {
    let mut pairs = histogram
        .iter()
        .map(|(key, count)| (key.clone(), *count))
        .collect::<Vec<_>>();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    pairs.into_iter().take(limit).map(|(key, _)| key).collect()
}

fn flatten_xml(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = Vec::<String>::new();
    let mut index = 0usize;
    let bytes = text.as_bytes();

    while index < bytes.len() {
        if bytes[index] == b'<' {
            if text[index..].starts_with("<!--") {
                if let Some(end) = text[index + 4..].find("-->") {
                    index = index + 4 + end + 3;
                } else {
                    break;
                }
                continue;
            }
            let Some(close_offset) = text[index..].find('>') else {
                break;
            };
            let close_index = index + close_offset;
            let token = text[index + 1..close_index].trim();
            if token.is_empty() || token.starts_with('?') || token.starts_with('!') {
                index = close_index.saturating_add(1);
                continue;
            }
            if let Some(rest) = token.strip_prefix('/') {
                let closing = parse_xml_tag_name(rest);
                pop_xml_stack(&mut stack, closing);
                index = close_index.saturating_add(1);
                continue;
            }

            let self_closing = token.ends_with('/');
            let body = if self_closing {
                token[..token.len().saturating_sub(1)].trim()
            } else {
                token
            };
            let (name, attrs) = split_xml_tag(body);
            if !name.is_empty() {
                stack.push(name.to_string());
                let path = xml_path(&stack);
                for (key, value) in parse_xml_attributes(attrs) {
                    out.push(format!("{path}/@{key}={value}"));
                }
                if self_closing {
                    stack.pop();
                }
            }
            index = close_index.saturating_add(1);
        } else {
            let start = index;
            while index < bytes.len() && bytes[index] != b'<' {
                index += 1;
            }
            let text_value = text[start..index].trim();
            if !text_value.is_empty() && !stack.is_empty() {
                out.push(format!(
                    "{}={}",
                    xml_path(&stack),
                    normalize_scalar_text(text_value)
                ));
            }
        }
    }

    out
}

fn parse_xml_tag_name(raw: &str) -> &str {
    raw.split_whitespace().next().unwrap_or_default().trim()
}

fn split_xml_tag(raw: &str) -> (&str, &str) {
    let mut parts = raw.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or_default().trim();
    let attrs = parts.next().unwrap_or_default().trim();
    (name, attrs)
}

fn parse_xml_attributes(raw: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut index = 0usize;
    let bytes = raw.as_bytes();
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        let key_start = index;
        while index < bytes.len() && !bytes[index].is_ascii_whitespace() && bytes[index] != b'=' {
            index += 1;
        }
        let key = raw[key_start..index].trim();
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'=' {
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        let quote = bytes[index];
        let value = if quote == b'"' || quote == b'\'' {
            index += 1;
            let value_start = index;
            while index < bytes.len() && bytes[index] != quote {
                index += 1;
            }
            let v = raw[value_start..index].to_string();
            if index < bytes.len() {
                index += 1;
            }
            v
        } else {
            let value_start = index;
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            raw[value_start..index].to_string()
        };

        if !key.is_empty() {
            out.push((key.to_string(), normalize_scalar_text(&value)));
        }
    }
    out
}

fn xml_path(stack: &[String]) -> String {
    format!("/{}", stack.join("/"))
}

fn pop_xml_stack(stack: &mut Vec<String>, closing: &str) {
    if stack.last().is_some_and(|name| name == closing) {
        stack.pop();
        return;
    }
    if let Some(pos) = stack.iter().rposition(|name| name == closing) {
        stack.truncate(pos);
    }
}

fn parse_binary(bytes: &[u8]) -> ParsedDocument {
    ParsedDocument {
        parser: "binary".to_string(),
        is_text: false,
        title: None,
        text_preview: format!("binary file ({} bytes)", bytes.len()),
        normalized_text: None,
        line_count: 0,
        tags: vec!["binary".to_string()],
    }
}

fn normalize_markdown_for_indexing(raw: &str) -> String {
    raw.strip_prefix('\u{feff}').unwrap_or(raw).to_string()
}

fn is_markdown_rule_line(trimmed: &str) -> bool {
    let bytes = trimmed.as_bytes();
    if bytes.len() < 3 {
        return false;
    }
    let first = bytes[0];
    if !matches!(first, b'-' | b'_' | b'*') {
        return false;
    }
    bytes.iter().all(|b| *b == first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_parser_extracts_title() {
        let registry = ParserRegistry::new();
        let parsed = registry.parse_file(
            Path::new("readme.md"),
            b"# Hello\n\nThis is a markdown file.",
        );

        assert_eq!(parsed.parser, "markdown");
        assert_eq!(parsed.title.as_deref(), Some("Hello"));
        assert!(parsed.is_text);
        assert!(parsed.normalized_text.is_some());
    }

    #[test]
    fn text_parser_handles_plain_text() {
        let registry = ParserRegistry::new();
        let parsed = registry.parse_file(Path::new("notes.txt"), b"first line\nsecond line");

        assert_eq!(parsed.parser, "text");
        assert_eq!(parsed.line_count, 2);
        assert_eq!(parsed.title.as_deref(), Some("first line"));
        assert_eq!(
            parsed.normalized_text.as_deref(),
            Some("first line\nsecond line")
        );
    }

    #[test]
    fn binary_parser_detects_non_utf8() {
        let registry = ParserRegistry::new();
        let parsed = registry.parse_file(Path::new("image.bin"), &[0xff, 0xfe, 0xfd]);

        assert_eq!(parsed.parser, "binary");
        assert!(!parsed.is_text);
        assert!(parsed.normalized_text.is_none());
    }

    #[test]
    fn markdown_parser_keeps_yaml_frontmatter_content() {
        let registry = ParserRegistry::new();
        let parsed = registry.parse_file(
            Path::new("note.md"),
            b"---\ntype: area\ntags: [rust]\n---\n# Real Title\n\nBody",
        );

        assert_eq!(parsed.title.as_deref(), Some("Real Title"));
        let normalized = parsed.normalized_text.expect("normalized");
        assert!(normalized.contains("type: area"));
        assert!(normalized.contains("# Real Title"));
    }

    #[test]
    fn markdown_parser_keeps_leading_metadata_lines() {
        let registry = ParserRegistry::new();
        let parsed = registry.parse_file(
            Path::new("note.md"),
            "> 작성일: 2026-02-15\n> tags: rust\n# 제목\n본문".as_bytes(),
        );

        assert_eq!(parsed.title.as_deref(), Some("제목"));
        let normalized = parsed.normalized_text.expect("normalized");
        assert!(normalized.contains("tags: rust"));
        assert!(normalized.contains("# 제목"));
    }

    #[test]
    fn markdown_parser_ignores_rule_line_when_guessing_title() {
        let registry = ParserRegistry::new();
        let parsed = registry.parse_file(Path::new("note.md"), b"---\n\nIntro line\nBody");
        assert_eq!(parsed.title.as_deref(), Some("Intro line"));
    }

    #[test]
    fn parser_json_keypath_flatten() {
        let registry = ParserRegistry::new();
        let parsed = registry.parse_file(
            Path::new("config.json"),
            br#"{"name":"oauth","oauth":{"client_id":"abc","enabled":true}}"#,
        );
        assert_eq!(parsed.parser, "json");
        assert!(parsed.tags.contains(&"json".to_string()));
        assert!(parsed.tags.contains(&"config".to_string()));
        let normalized = parsed.normalized_text.expect("normalized");
        assert!(normalized.contains("$.oauth.client_id=abc"));
        assert!(normalized.contains("$.oauth.enabled=true"));
        assert_eq!(parsed.title.as_deref(), Some("oauth"));
    }

    #[test]
    fn parser_yaml_keypath_flatten() {
        let registry = ParserRegistry::new();
        let parsed = registry.parse_file(
            Path::new("settings.yaml"),
            b"name: runtime\nqueue:\n  dead_letter_rate: 0.01\n",
        );
        assert_eq!(parsed.parser, "yaml");
        assert!(parsed.tags.contains(&"yaml".to_string()));
        assert!(parsed.tags.contains(&"config".to_string()));
        let normalized = parsed.normalized_text.expect("normalized");
        assert!(normalized.contains("$.queue.dead_letter_rate=0.01"));
        assert_eq!(parsed.title.as_deref(), Some("runtime"));
    }

    #[test]
    fn parser_jsonl_histogram_preview() {
        let registry = ParserRegistry::new();
        let parsed = registry.parse_file(
            Path::new("events.jsonl"),
            b"{\"id\":\"a1\",\"status\":\"ok\"}\n{\"id\":\"a2\",\"status\":\"fail\",\"worker\":\"w1\"}\n",
        );
        assert_eq!(parsed.parser, "jsonl");
        let normalized = parsed.normalized_text.expect("normalized");
        assert!(normalized.contains("jsonl.records=2"));
        assert!(normalized.contains("jsonl.keys=id,status,worker"));
        assert!(normalized.contains("$[1].id=a1"));
        assert!(parsed.text_preview.contains("jsonl.records=2"));
    }

    #[test]
    fn parser_xml_path_flatten() {
        let registry = ParserRegistry::new();
        let parsed = registry.parse_file(
            Path::new("schema.xml"),
            br#"<root><title>Search Contract</title><item id="x1">alpha</item></root>"#,
        );
        assert_eq!(parsed.parser, "xml");
        let normalized = parsed.normalized_text.expect("normalized");
        assert!(normalized.contains("/root/title=Search Contract"));
        assert!(normalized.contains("/root/item/@id=x1"));
        assert!(normalized.contains("/root/item=alpha"));
        assert_eq!(parsed.title.as_deref(), Some("Search Contract"));
    }
}
