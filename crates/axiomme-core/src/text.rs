#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputTrimMode {
    Preserve,
    Trim,
}

#[must_use]
pub fn first_non_empty_output(
    stdout: &str,
    stderr: &str,
    trim_mode: OutputTrimMode,
) -> Option<String> {
    for raw in [stdout, stderr] {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        return Some(match trim_mode {
            OutputTrimMode::Preserve => raw.to_string(),
            OutputTrimMode::Trim => trimmed.to_string(),
        });
    }
    None
}

#[must_use]
pub fn truncate_text(text: &str, max_chars: usize) -> String {
    let Some((clip_idx, _)) = text.char_indices().nth(max_chars) else {
        return text.to_string();
    };

    let mut out = text[..clip_idx].to_string();
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_non_empty_output_prefers_stdout_and_preserves_whitespace() {
        let output = first_non_empty_output("  hello \n", "stderr", OutputTrimMode::Preserve);
        assert_eq!(output.as_deref(), Some("  hello \n"));
    }

    #[test]
    fn first_non_empty_output_can_trim_selected_output() {
        let output = first_non_empty_output("  hello \n", "stderr", OutputTrimMode::Trim);
        assert_eq!(output.as_deref(), Some("hello"));
    }

    #[test]
    fn first_non_empty_output_uses_stderr_when_stdout_is_blank() {
        let output = first_non_empty_output("  \n\t", "  error \n", OutputTrimMode::Trim);
        assert_eq!(output.as_deref(), Some("error"));
    }

    #[test]
    fn first_non_empty_output_returns_none_when_both_outputs_are_blank() {
        let output = first_non_empty_output("  \n\t", "\n  ", OutputTrimMode::Preserve);
        assert_eq!(output, None);
    }

    #[test]
    fn truncate_text_preserves_utf8_char_boundaries() {
        let input = "\u{C548}\u{B155}\u{D558}\u{C138}\u{C694}-hello";
        let clipped = truncate_text(input, 5);
        let expected = format!("{}...", "\u{C548}\u{B155}\u{D558}\u{C138}\u{C694}");
        assert_eq!(clipped, expected);
    }

    #[test]
    fn truncate_text_returns_original_when_input_fits_limit() {
        assert_eq!(truncate_text("hello", 5), "hello");
    }
}
