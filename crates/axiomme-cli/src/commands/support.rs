use std::io::{Read, Write};
use std::path::Path;
use std::{fs, io};

use anyhow::Result;
use axiomme_core::models::{AddResourceIngestOptions, SearchBudget};
use axiomme_core::{AxiomMe, Scope};

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
    app: &AxiomMe,
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
