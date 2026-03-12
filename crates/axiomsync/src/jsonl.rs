use serde::de::DeserializeOwned;

use crate::error::AxiomError;

#[derive(Debug, Clone)]
pub struct JsonlParseOutcome<T> {
    pub items: Vec<T>,
    pub skipped_lines: usize,
    pub first_error: Option<(usize, String)>,
}

pub fn parse_jsonl_tolerant<T>(raw: &str) -> JsonlParseOutcome<T>
where
    T: DeserializeOwned,
{
    let mut items = Vec::new();
    let mut skipped_lines = 0usize;
    let mut first_error = None::<(usize, String)>;

    for (line_no, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<T>(line) {
            Ok(value) => items.push(value),
            Err(err) => {
                skipped_lines += 1;
                if first_error.is_none() {
                    first_error = Some((line_no + 1, err.to_string()));
                }
            }
        }
    }

    JsonlParseOutcome {
        items,
        skipped_lines,
        first_error,
    }
}

pub fn jsonl_all_lines_invalid(
    label: &str,
    path: Option<&str>,
    skipped_lines: usize,
    first_error: Option<&(usize, String)>,
) -> AxiomError {
    let location = path
        .filter(|value| !value.is_empty())
        .map(|value| format!(" ({value})"))
        .unwrap_or_default();

    if let Some((line_no, message)) = first_error {
        return AxiomError::Validation(format!(
            "{label} parse failed{location}: skipped {skipped_lines} invalid lines (first at line {line_no}: {message})"
        ));
    }

    AxiomError::Validation(format!(
        "{label} parse failed{location}: skipped {skipped_lines} invalid lines"
    ))
}
