use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedDocument {
    pub parser: String,
    pub is_text: bool,
    pub title: Option<String>,
    pub text_preview: String,
    pub line_count: usize,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ParserRegistry;

impl ParserRegistry {
    pub fn new() -> Self {
        Self
    }

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
    let mut title = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("# ") {
            title = Some(rest.trim().to_string());
            break;
        }
        if !trimmed.is_empty() && title.is_none() {
            title = Some(trimmed.to_string());
        }
    }

    let preview = text.chars().take(240).collect::<String>();
    ParsedDocument {
        parser: "markdown".to_string(),
        is_text: true,
        title,
        text_preview: preview,
        line_count: text.lines().count(),
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
        line_count: 0,
        tags: vec!["binary".to_string()],
    }
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
    }

    #[test]
    fn text_parser_handles_plain_text() {
        let registry = ParserRegistry::new();
        let parsed = registry.parse_file(Path::new("notes.txt"), b"first line\nsecond line");

        assert_eq!(parsed.parser, "text");
        assert_eq!(parsed.line_count, 2);
        assert_eq!(parsed.title.as_deref(), Some("first line"));
    }

    #[test]
    fn binary_parser_detects_non_utf8() {
        let registry = ParserRegistry::new();
        let parsed = registry.parse_file(Path::new("image.bin"), &[0xff, 0xfe, 0xfd]);

        assert_eq!(parsed.parser, "binary");
        assert!(!parsed.is_text);
    }
}
