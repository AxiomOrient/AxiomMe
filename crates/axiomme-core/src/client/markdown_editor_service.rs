use std::path::Path;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde_json::json;

use crate::error::{AxiomError, Result};
use crate::models::{MarkdownDocument, MarkdownSaveResult};
use crate::uri::{AxiomUri, Scope};

use super::AxiomMe;

impl AxiomMe {
    pub fn load_document(&self, uri: &str) -> Result<MarkdownDocument> {
        load_editor_document(self, uri, EditorMode::Document)
    }

    pub fn save_document(
        &self,
        uri: &str,
        content: &str,
        expected_etag: Option<&str>,
    ) -> Result<MarkdownSaveResult> {
        save_editor_document(self, uri, content, expected_etag, EditorMode::Document)
    }

    pub fn load_markdown(&self, uri: &str) -> Result<MarkdownDocument> {
        load_editor_document(self, uri, EditorMode::Markdown)
    }

    pub fn save_markdown(
        &self,
        uri: &str,
        content: &str,
        expected_etag: Option<&str>,
    ) -> Result<MarkdownSaveResult> {
        save_editor_document(self, uri, content, expected_etag, EditorMode::Markdown)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditorMode {
    Markdown,
    Document,
}

impl EditorMode {
    const fn load_operation(self) -> &'static str {
        match self {
            Self::Markdown => "markdown.load",
            Self::Document => "document.load",
        }
    }

    const fn save_operation(self) -> &'static str {
        match self {
            Self::Markdown => "markdown.save",
            Self::Document => "document.save",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Document => "document",
        }
    }

    fn supports_extension(self, ext: &str) -> bool {
        match self {
            Self::Markdown => matches!(ext, "md" | "markdown"),
            Self::Document => matches!(ext, "md" | "markdown" | "json" | "yaml" | "yml"),
        }
    }

    fn unsupported_target_message(self, uri: &AxiomUri) -> String {
        match self {
            Self::Markdown => {
                format!("markdown editor only supports .md/.markdown targets: {uri}")
            }
            Self::Document => {
                format!("document editor supports .md/.markdown/.json/.yaml/.yml targets: {uri}")
            }
        }
    }
}

fn load_editor_document(app: &AxiomMe, uri: &str, mode: EditorMode) -> Result<MarkdownDocument> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let started = Instant::now();
    let target_uri = uri.to_string();

    let output = (|| -> Result<MarkdownDocument> {
        let uri = AxiomUri::parse(uri)?;
        validate_editor_target(app, &uri, mode)?;

        let _guard = app
            .markdown_edit_gate
            .read()
            .map_err(|_| AxiomError::lock_poisoned("markdown edit gate"))?;

        let content = app.fs.read(&uri)?;
        let etag = markdown_etag(&content);

        Ok(MarkdownDocument {
            uri: uri.to_string(),
            content,
            etag,
            updated_at: uri_updated_at(app, &uri),
        })
    })();

    match output {
        Ok(document) => {
            app.log_request_status(
                request_id,
                mode.load_operation(),
                "ok",
                started,
                Some(target_uri),
                Some(json!({
                    "etag": &document.etag,
                    "content_bytes": document.content.len(),
                })),
            );
            Ok(document)
        }
        Err(err) => {
            app.log_request_error(
                request_id,
                mode.load_operation(),
                started,
                Some(target_uri),
                &err,
                None,
            );
            Err(err)
        }
    }
}

fn save_editor_document(
    app: &AxiomMe,
    uri: &str,
    content: &str,
    expected_etag: Option<&str>,
    mode: EditorMode,
) -> Result<MarkdownSaveResult> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let started = Instant::now();
    let target_uri = uri.to_string();
    let content_bytes = content.len();

    let output = (|| -> Result<MarkdownSaveResult> {
        let uri = AxiomUri::parse(uri)?;
        let ext = validate_editor_target(app, &uri, mode)?;
        validate_editor_content(mode, &ext, content)?;
        let parent_uri = uri.parent().ok_or_else(|| {
            AxiomError::Validation(format!("{} target must not be a scope root", mode.label()))
        })?;

        let _guard = app
            .markdown_edit_gate
            .write()
            .map_err(|_| AxiomError::lock_poisoned("markdown edit gate"))?;

        let previous = app.fs.read(&uri)?;
        let current_etag = markdown_etag(&previous);
        if let Some(expected_etag) = expected_etag
            && current_etag != expected_etag
        {
            return Err(AxiomError::Conflict(format!("etag mismatch for {uri}")));
        }

        let save_started = Instant::now();
        app.fs.write_atomic(&uri, content, false)?;
        let save_ms = save_started.elapsed().as_millis();

        let reindex_started = Instant::now();
        if let Err(reindex_err) = app.reindex_uri_tree(&parent_uri) {
            let rollback_write = app.fs.write_atomic(&uri, &previous, false);
            let rollback_reindex = if rollback_write.is_ok() {
                app.reindex_uri_tree(&parent_uri).err()
            } else {
                None
            };
            let rollback_write_status = rollback_write
                .as_ref()
                .map_or_else(|err| format!("err:{err}"), |()| "ok".to_string());
            let rollback_reindex_status = rollback_reindex
                .as_ref()
                .map_or_else(|| "ok_or_skipped".to_string(), |err| format!("err:{err}"));
            let label = mode.label();
            return Err(AxiomError::Internal(format!(
                "{label} save failed during reindex for {uri}: reindex_err={reindex_err}; rollback_write={rollback_write_status}; rollback_reindex={rollback_reindex_status}",
            )));
        }
        let reindex_ms = reindex_started.elapsed().as_millis();

        let committed = app.fs.read(&uri)?;
        Ok(MarkdownSaveResult {
            uri: uri.to_string(),
            etag: markdown_etag(&committed),
            updated_at: uri_updated_at(app, &uri),
            reindexed_root: parent_uri.to_string(),
            save_ms,
            reindex_ms,
        })
    })();

    match output {
        Ok(saved) => {
            app.log_request_status(
                request_id,
                mode.save_operation(),
                "ok",
                started,
                Some(target_uri),
                Some(json!({
                    "etag": &saved.etag,
                    "content_bytes": content_bytes,
                    "save_ms": saved.save_ms,
                    "reindex_ms": saved.reindex_ms,
                    "total_ms": started.elapsed().as_millis(),
                    "reindexed_root": &saved.reindexed_root,
                })),
            );
            Ok(saved)
        }
        Err(err) => {
            app.log_request_error(
                request_id,
                mode.save_operation(),
                started,
                Some(target_uri),
                &err,
                Some(json!({
                    "content_bytes": content_bytes,
                    "expected_etag_provided": expected_etag.is_some(),
                })),
            );
            Err(err)
        }
    }
}

fn validate_editor_target(app: &AxiomMe, uri: &AxiomUri, mode: EditorMode) -> Result<String> {
    if !matches!(
        uri.scope(),
        Scope::Resources | Scope::User | Scope::Agent | Scope::Session
    ) {
        return Err(AxiomError::PermissionDenied(format!(
            "{} editor does not allow scope: {}",
            mode.label(),
            uri.scope()
        )));
    }

    if !app.fs.exists(uri) {
        return Err(AxiomError::NotFound(uri.to_string()));
    }
    if app.fs.is_dir(uri) {
        return Err(AxiomError::Validation(format!(
            "{} target must be a file: {}",
            mode.label(),
            uri
        )));
    }

    let name = uri.last_segment().ok_or_else(|| {
        AxiomError::Validation(format!(
            "{} target must include a filename: {uri}",
            mode.label()
        ))
    })?;
    if name == ".abstract.md" || name == ".overview.md" {
        return Err(AxiomError::PermissionDenied(format!(
            "{} editor cannot modify generated tier file: {}",
            mode.label(),
            uri
        )));
    }

    let ext = Path::new(name)
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !mode.supports_extension(&ext) {
        return Err(AxiomError::Validation(mode.unsupported_target_message(uri)));
    }
    Ok(ext)
}

fn validate_editor_content(mode: EditorMode, ext: &str, content: &str) -> Result<()> {
    if mode == EditorMode::Document {
        if ext == "json" {
            serde_json::from_str::<serde_json::Value>(content).map_err(|err| {
                AxiomError::Validation(format!("invalid json content for document save: {err}"))
            })?;
        } else if ext == "yaml" || ext == "yml" {
            serde_yml::from_str::<serde_yml::Value>(content).map_err(|err| {
                AxiomError::Validation(format!("invalid yaml content for document save: {err}"))
            })?;
        }
    }
    Ok(())
}

fn markdown_etag(content: &str) -> String {
    blake3::hash(content.as_bytes()).to_hex().to_string()
}

fn uri_updated_at(app: &AxiomMe, uri: &AxiomUri) -> String {
    let path = app.fs.resolve_uri(uri);
    let modified = std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .map(DateTime::<Utc>::from)
        .map(|dt| dt.to_rfc3339());
    modified.unwrap_or_else(|_| Utc::now().to_rfc3339())
}
