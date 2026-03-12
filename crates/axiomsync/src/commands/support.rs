use std::io::{Read, Write};
use std::path::Path;
use std::{fs, io};

use anyhow::Result;
use axiomsync::models::{
    AddResourceIngestOptions, MetadataFilter, RuntimeHint, RuntimeHintKind, SearchBudget,
    SearchRequest,
};
use axiomsync::{AxiomSync, Scope};

pub(super) fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer_pretty(&mut stdout, value)?;
    writeln!(stdout)?;
    Ok(())
}

pub(super) fn parse_scope_args(values: &[String]) -> Result<Option<Vec<Scope>>> {
    if values.is_empty() {
        return Ok(None);
    }

    let mut scopes = Vec::new();
    for raw in values {
        let scope = raw
            .parse::<Scope>()
            .map_err(|e| anyhow::anyhow!("invalid --scope value '{raw}': {e}"))?;
        scopes.push(scope);
    }
    Ok(Some(scopes))
}

pub(super) fn build_add_ingest_options(
    markdown_only: bool,
    include_hidden: bool,
    exclude: &[String],
) -> Result<AddResourceIngestOptions> {
    validate_add_ingest_flags(markdown_only, include_hidden, exclude)?;

    if !markdown_only {
        return Ok(AddResourceIngestOptions::default());
    }

    let mut options = AddResourceIngestOptions::markdown_only_defaults();
    options.include_hidden = include_hidden;
    options.exclude_globs.extend(
        exclude
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
    );
    options.exclude_globs.sort();
    options.exclude_globs.dedup();
    Ok(options)
}

pub(super) fn validate_add_ingest_flags(
    markdown_only: bool,
    include_hidden: bool,
    exclude: &[String],
) -> Result<()> {
    if include_hidden && !markdown_only {
        anyhow::bail!("--include-hidden requires --markdown-only");
    }
    if !exclude.is_empty() && !markdown_only {
        anyhow::bail!("--exclude requires --markdown-only");
    }
    Ok(())
}

pub(super) const fn parse_search_budget(
    budget_ms: Option<u64>,
    budget_nodes: Option<usize>,
    budget_depth: Option<usize>,
) -> Option<SearchBudget> {
    if budget_ms.is_none() && budget_nodes.is_none() && budget_depth.is_none() {
        return None;
    }

    Some(SearchBudget {
        max_ms: budget_ms,
        max_nodes: budget_nodes,
        max_depth: budget_depth,
    })
}

pub(super) fn build_metadata_filter(
    tags: &[String],
    mime: Option<&str>,
) -> Result<Option<MetadataFilter>> {
    let normalized_tags = tags
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let normalized_mime = mime.map(str::trim).filter(|value| !value.is_empty());

    if normalized_tags.is_empty() && normalized_mime.is_none() {
        return Ok(None);
    }

    let mut fields = std::collections::HashMap::new();
    if !normalized_tags.is_empty() {
        fields.insert("tags".to_string(), serde_json::json!(normalized_tags));
    }
    if let Some(value) = normalized_mime {
        fields.insert("mime".to_string(), serde_json::json!(value));
    }
    Ok(Some(MetadataFilter { fields }))
}

pub(super) fn parse_search_request_file(path: &Path) -> Result<SearchRequest> {
    let raw = fs::read_to_string(path)?;
    let request = serde_json::from_str::<SearchRequest>(&raw).map_err(|err| {
        anyhow::anyhow!(
            "invalid --request-json payload at {}: {err}",
            path.to_string_lossy()
        )
    })?;
    Ok(request)
}

pub(super) fn parse_runtime_hints(
    hints: &[String],
    hint_file: Option<&Path>,
) -> Result<Vec<RuntimeHint>> {
    let mut out = Vec::new();
    for raw in hints {
        out.push(parse_runtime_hint_token(raw, Some("cli:hint"))?);
    }
    if let Some(path) = hint_file {
        out.extend(parse_runtime_hints_file(path)?);
    }
    Ok(out)
}

fn parse_runtime_hints_file(path: &Path) -> Result<Vec<RuntimeHint>> {
    let raw = fs::read_to_string(path)?;
    let value = serde_json::from_str::<serde_json::Value>(&raw).map_err(|err| {
        anyhow::anyhow!(
            "invalid --hint-file json at {}: {err}",
            path.to_string_lossy()
        )
    })?;

    if let Some(list) = value.as_array() {
        return list
            .iter()
            .map(|entry| parse_runtime_hint_value(entry, Some("cli:hint_file")))
            .collect();
    }
    if let Some(object) = value.as_object() {
        if let Some(list) = object
            .get("runtime_hints")
            .and_then(serde_json::Value::as_array)
        {
            return list
                .iter()
                .map(|entry| parse_runtime_hint_value(entry, Some("cli:hint_file")))
                .collect();
        }
        if let Some(list) = object.get("hints").and_then(serde_json::Value::as_array) {
            return list
                .iter()
                .map(|entry| parse_runtime_hint_value(entry, Some("cli:hint_file")))
                .collect();
        }
        if object.contains_key("kind") && object.contains_key("text") {
            return Ok(vec![parse_runtime_hint_value(
                &value,
                Some("cli:hint_file"),
            )?]);
        }
    }

    Err(anyhow::anyhow!(
        "invalid --hint-file payload: expected RuntimeHint object, array, or {{runtime_hints:[...]}}"
    ))
}

fn parse_runtime_hint_token(raw: &str, source: Option<&str>) -> Result<RuntimeHint> {
    let Some((kind_raw, text_raw)) = raw.split_once(':') else {
        anyhow::bail!("invalid --hint value '{raw}': expected KIND:TEXT");
    };
    let kind = parse_runtime_hint_kind(kind_raw)?;
    let text = text_raw.trim();
    if text.is_empty() {
        anyhow::bail!("invalid --hint value '{raw}': text must not be empty");
    }

    Ok(RuntimeHint {
        kind,
        text: text.to_string(),
        source: source.map(ToString::to_string),
    })
}

fn parse_runtime_hint_value(
    value: &serde_json::Value,
    source: Option<&str>,
) -> Result<RuntimeHint> {
    let mut hint = serde_json::from_value::<RuntimeHint>(value.clone())
        .map_err(|err| anyhow::anyhow!("invalid runtime hint entry: {err}"))?;
    if hint.text.trim().is_empty() {
        anyhow::bail!("invalid runtime hint entry: text must not be empty");
    }
    if hint.source.is_none() {
        hint.source = source.map(ToString::to_string);
    }
    Ok(hint)
}

fn parse_runtime_hint_kind(raw: &str) -> Result<RuntimeHintKind> {
    let normalized = raw.trim().to_ascii_lowercase();
    let kind = match normalized.as_str() {
        "observation" | "obs" => RuntimeHintKind::Observation,
        "current_task" | "current-task" | "task" => RuntimeHintKind::CurrentTask,
        "suggested_response" | "suggested-response" | "suggested" | "next" => {
            RuntimeHintKind::SuggestedResponse
        }
        "external" | "ext" => RuntimeHintKind::External,
        _ => anyhow::bail!("invalid runtime hint kind '{raw}'"),
    };
    Ok(kind)
}

pub(super) fn read_document_content(
    inline: Option<String>,
    from: Option<std::path::PathBuf>,
    stdin: bool,
) -> Result<String> {
    validate_document_save_source_selection(inline.as_deref(), from.as_deref(), stdin)?;

    if let Some(content) = inline {
        return Ok(content);
    }
    if let Some(path) = from {
        return Ok(fs::read_to_string(path)?);
    }

    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    Ok(buffer)
}

pub(super) fn read_preview_content(
    app: &AxiomSync,
    uri: Option<String>,
    inline: Option<String>,
    from: Option<std::path::PathBuf>,
    stdin: bool,
) -> Result<String> {
    validate_document_preview_source_selection(
        uri.as_deref(),
        inline.as_deref(),
        from.as_deref(),
        stdin,
    )?;

    if let Some(uri) = uri {
        let document = app.load_markdown(&uri)?;
        return Ok(document.content);
    }
    if let Some(content) = inline {
        return Ok(content);
    }
    if let Some(path) = from {
        return Ok(fs::read_to_string(path)?);
    }

    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    Ok(buffer)
}

pub(super) fn validate_document_save_source_selection(
    inline: Option<&str>,
    from: Option<&Path>,
    stdin: bool,
) -> Result<()> {
    let selected =
        bool_to_count(inline.is_some()) + bool_to_count(from.is_some()) + bool_to_count(stdin);
    ensure_single_source_selection(
        selected,
        "document save content source is required: use one of --content, --from <path>, --stdin",
        "document save accepts exactly one content source: choose one of --content, --from, --stdin",
    )
}

pub(super) fn validate_document_preview_source_selection(
    uri: Option<&str>,
    inline: Option<&str>,
    from: Option<&Path>,
    stdin: bool,
) -> Result<()> {
    let selected = bool_to_count(uri.is_some())
        + bool_to_count(inline.is_some())
        + bool_to_count(from.is_some())
        + bool_to_count(stdin);
    ensure_single_source_selection(
        selected,
        "document preview source is required: use one of --uri, --content, --from <path>, --stdin",
        "document preview accepts exactly one source: choose one of --uri, --content, --from, --stdin",
    )
}

const fn bool_to_count(value: bool) -> u8 {
    if value { 1 } else { 0 }
}

fn ensure_single_source_selection(
    selected: u8,
    missing_message: &str,
    multiple_message: &str,
) -> Result<()> {
    if selected == 0 {
        anyhow::bail!("{missing_message}");
    }
    if selected > 1 {
        anyhow::bail!("{multiple_message}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{
        build_metadata_filter, parse_runtime_hints, parse_search_budget, parse_search_request_file,
    };

    #[test]
    fn build_metadata_filter_returns_none_for_empty_inputs() {
        let filter = build_metadata_filter(&[], None).expect("filter");
        assert!(filter.is_none());
    }

    #[test]
    fn build_metadata_filter_maps_tags_and_mime() {
        let filter = build_metadata_filter(
            &["markdown".to_string(), "auth".to_string()],
            Some("text/markdown"),
        )
        .expect("filter")
        .expect("filter payload");
        assert_eq!(
            filter.fields.get("tags"),
            Some(&serde_json::json!(["markdown", "auth"]))
        );
        assert_eq!(
            filter.fields.get("mime"),
            Some(&serde_json::json!("text/markdown"))
        );
    }

    #[test]
    fn parse_runtime_hints_reads_cli_and_file_values() {
        let temp = tempdir().expect("tempdir");
        let file_path = temp.path().join("hints.json");
        std::fs::write(
            &file_path,
            r#"{"runtime_hints":[{"kind":"external","text":"from file"}]}"#,
        )
        .expect("write");

        let hints = parse_runtime_hints(
            &["observation:from cli".to_string()],
            Some(file_path.as_path()),
        )
        .expect("hints");
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0].text, "from cli");
        assert_eq!(hints[1].text, "from file");
    }

    #[test]
    fn parse_search_request_file_supports_contract_payload() {
        let temp = tempdir().expect("tempdir");
        let file_path = temp.path().join("request.json");
        std::fs::write(
            &file_path,
            r#"{"query":"oauth","limit":5,"runtime_hints":[{"kind":"observation","text":"hint"}]}"#,
        )
        .expect("write");
        let request = parse_search_request_file(file_path.as_path()).expect("request");
        assert_eq!(request.query, "oauth");
        assert_eq!(request.limit, Some(5));
        assert_eq!(request.runtime_hints.len(), 1);
    }

    #[test]
    fn parse_search_budget_none_when_unset() {
        assert!(parse_search_budget(None, None, None).is_none());
    }
}
