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

        if ext == "md" || ext == "markdown" {
            return parse_markdown(bytes);
        }

        if let Ok(text) = std::str::from_utf8(bytes) {
            return parse_plain_text(text);
        }

        parse_binary(bytes)
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
}
