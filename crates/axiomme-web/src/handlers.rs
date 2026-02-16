use std::path::Path;

use axum::{
    Json,
    extract::{Query, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};

use axiomme_core::{AxiomError, AxiomMe, AxiomUri, Scope};

use crate::WebState;
use crate::dto::{
    FsDeleteRequest, FsListQuery, FsListResponse, FsMkdirRequest, FsMoveRequest,
    FsMutationResponse, FsTreeQuery, LoadDocumentQuery, LoadMarkdownQuery, PreviewRequest,
    PreviewResponse, SaveMarkdownRequest, WebDocumentResponse,
};
use crate::error::{axiom_error_response, locked_response};
use crate::html::{INDEX_CSS, INDEX_HTML, INDEX_JS};
use crate::markdown::render_markdown_html;

pub async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

pub async fn index_css() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        INDEX_CSS,
    )
        .into_response()
}

pub async fn index_js() -> Response {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        INDEX_JS,
    )
        .into_response()
}

pub async fn load_document(
    State(state): State<WebState>,
    Query(query): Query<LoadDocumentQuery>,
) -> Response {
    let uri = match AxiomUri::parse(&query.uri) {
        Ok(uri) => uri,
        Err(err) => return axiom_error_response(err, "document.load", Some(query.uri)),
    };
    let Ok(_guard) = state.editor_gate.try_read() else {
        return locked_response("document.load", Some(query.uri));
    };
    let scope = uri.scope();
    if !is_view_allowed_scope(scope) {
        return axiom_error_response(
            AxiomError::PermissionDenied(format!("document viewer does not allow scope: {scope}")),
            "document.load",
            Some(query.uri),
        );
    }
    if !state.app.fs.exists(&uri) {
        return axiom_error_response(AxiomError::NotFound(uri.to_string()), "document.load", None);
    }
    if state.app.fs.is_dir(&uri) {
        return axiom_error_response(
            AxiomError::Validation(format!("document target must be a file: {uri}")),
            "document.load",
            None,
        );
    }

    let Some(name) = uri.last_segment() else {
        return axiom_error_response(
            AxiomError::Validation(format!("document target must include a filename: {uri}")),
            "document.load",
            None,
        );
    };
    let Some(format) = infer_document_format(name) else {
        return axiom_error_response(
            AxiomError::Validation(format!("unsupported document format for viewer: {uri}")),
            "document.load",
            None,
        );
    };

    if is_editable_format(format) && !is_generated_tier_file(name) {
        return match state.app.load_document(&query.uri) {
            Ok(document) => (
                StatusCode::OK,
                Json(WebDocumentResponse {
                    uri: document.uri,
                    content: document.content,
                    etag: document.etag,
                    updated_at: document.updated_at,
                    format: format.to_string(),
                    editable: true,
                }),
            )
                .into_response(),
            Err(err) => axiom_error_response(err, "document.load", Some(query.uri)),
        };
    }

    let content = match state.app.read(&query.uri) {
        Ok(content) => content,
        Err(err) => return axiom_error_response(err, "document.load", Some(query.uri)),
    };
    let content = normalize_readonly_content(&content, format);
    let response = WebDocumentResponse {
        uri: uri.to_string(),
        etag: blake3::hash(content.as_bytes()).to_hex().to_string(),
        updated_at: uri_updated_at(&state.app, &uri),
        content,
        format: format.to_string(),
        editable: false,
    };
    (StatusCode::OK, Json(response)).into_response()
}

pub async fn load_markdown(
    State(state): State<WebState>,
    Query(query): Query<LoadMarkdownQuery>,
) -> Response {
    let Ok(_guard) = state.editor_gate.try_read() else {
        return locked_response("markdown.load", Some(query.uri));
    };

    match state.app.load_markdown(&query.uri) {
        Ok(document) => (StatusCode::OK, Json(document)).into_response(),
        Err(err) => axiom_error_response(err, "markdown.load", Some(query.uri)),
    }
}

pub async fn save_document(
    State(state): State<WebState>,
    Json(request): Json<SaveMarkdownRequest>,
) -> Response {
    let uri = request.uri.clone();
    let Ok(_guard) = state.editor_gate.try_write() else {
        return locked_response("document.save", Some(uri));
    };

    match state.app.save_document(
        &request.uri,
        &request.content,
        request.expected_etag.as_deref(),
    ) {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(err) => axiom_error_response(err, "document.save", Some(request.uri)),
    }
}

pub async fn save_markdown(
    State(state): State<WebState>,
    Json(request): Json<SaveMarkdownRequest>,
) -> Response {
    let uri = request.uri.clone();
    let Ok(_guard) = state.editor_gate.try_write() else {
        return locked_response("markdown.save", Some(uri));
    };

    match state.app.save_markdown(
        &request.uri,
        &request.content,
        request.expected_etag.as_deref(),
    ) {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(err) => axiom_error_response(err, "markdown.save", Some(request.uri)),
    }
}

pub async fn preview_markdown(Json(request): Json<PreviewRequest>) -> Response {
    let rendered = render_markdown_html(&request.content);
    (StatusCode::OK, Json(PreviewResponse { html: rendered })).into_response()
}

pub async fn list_fs(State(state): State<WebState>, Query(query): Query<FsListQuery>) -> Response {
    let uri = match AxiomUri::parse(&query.uri) {
        Ok(uri) => uri,
        Err(err) => return axiom_error_response(err, "fs.list", Some(query.uri)),
    };
    let scope = uri.scope();
    if !is_view_allowed_scope(scope) {
        return axiom_error_response(
            AxiomError::PermissionDenied(format!(
                "filesystem viewer does not allow scope: {scope}"
            )),
            "fs.list",
            Some(query.uri),
        );
    }
    if !state.app.fs.exists(&uri) {
        return axiom_error_response(AxiomError::NotFound(uri.to_string()), "fs.list", None);
    }
    if !state.app.fs.is_dir(&uri) {
        return axiom_error_response(
            AxiomError::Validation(format!("filesystem list target must be a directory: {uri}")),
            "fs.list",
            None,
        );
    }

    match state
        .app
        .ls(&query.uri, query.recursive.unwrap_or(false), true)
    {
        Ok(entries) => (
            StatusCode::OK,
            Json(FsListResponse {
                uri: query.uri,
                entries,
            }),
        )
            .into_response(),
        Err(err) => axiom_error_response(err, "fs.list", Some(uri.to_string())),
    }
}

pub async fn tree_fs(State(state): State<WebState>, Query(query): Query<FsTreeQuery>) -> Response {
    let uri = match AxiomUri::parse(&query.uri) {
        Ok(uri) => uri,
        Err(err) => return axiom_error_response(err, "fs.tree", Some(query.uri)),
    };
    let scope = uri.scope();
    if !is_view_allowed_scope(scope) {
        return axiom_error_response(
            AxiomError::PermissionDenied(format!(
                "filesystem viewer does not allow scope: {scope}"
            )),
            "fs.tree",
            Some(query.uri),
        );
    }

    match state.app.tree(&query.uri) {
        Ok(tree) => (StatusCode::OK, Json(tree)).into_response(),
        Err(err) => axiom_error_response(err, "fs.tree", Some(uri.to_string())),
    }
}

pub async fn mkdir_fs(
    State(state): State<WebState>,
    Json(request): Json<FsMkdirRequest>,
) -> Response {
    let uri = match AxiomUri::parse(&request.uri) {
        Ok(uri) => uri,
        Err(err) => return axiom_error_response(err, "fs.mkdir", Some(request.uri)),
    };
    let scope = uri.scope();
    if !is_manage_allowed_scope(scope) {
        return axiom_error_response(
            AxiomError::PermissionDenied(format!(
                "filesystem manager does not allow scope: {scope}"
            )),
            "fs.mkdir",
            Some(request.uri),
        );
    }

    match state.app.mkdir(&uri.to_string()) {
        Ok(()) => (
            StatusCode::OK,
            Json(FsMutationResponse {
                status: "ok".to_string(),
                uri: Some(uri.to_string()),
                from_uri: None,
                to_uri: None,
            }),
        )
            .into_response(),
        Err(err) => axiom_error_response(err, "fs.mkdir", Some(uri.to_string())),
    }
}

pub async fn move_fs(
    State(state): State<WebState>,
    Json(request): Json<FsMoveRequest>,
) -> Response {
    let from_uri = match AxiomUri::parse(&request.from_uri) {
        Ok(uri) => uri,
        Err(err) => return axiom_error_response(err, "fs.move", Some(request.from_uri)),
    };
    let to_uri = match AxiomUri::parse(&request.to_uri) {
        Ok(uri) => uri,
        Err(err) => return axiom_error_response(err, "fs.move", Some(request.to_uri)),
    };

    let from_scope = from_uri.scope();
    let to_scope = to_uri.scope();
    if !is_manage_allowed_scope(from_scope) || !is_manage_allowed_scope(to_scope) {
        return axiom_error_response(
            AxiomError::PermissionDenied(
                "filesystem manager only allows resources/user/agent/session scopes".to_string(),
            ),
            "fs.move",
            Some(format!("{from_uri} -> {to_uri}")),
        );
    }

    match state.app.mv(&from_uri.to_string(), &to_uri.to_string()) {
        Ok(()) => (
            StatusCode::OK,
            Json(FsMutationResponse {
                status: "ok".to_string(),
                uri: None,
                from_uri: Some(from_uri.to_string()),
                to_uri: Some(to_uri.to_string()),
            }),
        )
            .into_response(),
        Err(err) => axiom_error_response(err, "fs.move", Some(from_uri.to_string())),
    }
}

pub async fn delete_fs(
    State(state): State<WebState>,
    Json(request): Json<FsDeleteRequest>,
) -> Response {
    let uri = match AxiomUri::parse(&request.uri) {
        Ok(uri) => uri,
        Err(err) => return axiom_error_response(err, "fs.delete", Some(request.uri)),
    };
    let scope = uri.scope();
    if !is_manage_allowed_scope(scope) {
        return axiom_error_response(
            AxiomError::PermissionDenied(format!(
                "filesystem manager does not allow scope: {scope}"
            )),
            "fs.delete",
            Some(request.uri),
        );
    }

    match state
        .app
        .rm(&uri.to_string(), request.recursive.unwrap_or(false))
    {
        Ok(()) => (
            StatusCode::OK,
            Json(FsMutationResponse {
                status: "ok".to_string(),
                uri: Some(uri.to_string()),
                from_uri: None,
                to_uri: None,
            }),
        )
            .into_response(),
        Err(err) => axiom_error_response(err, "fs.delete", Some(uri.to_string())),
    }
}

const fn is_view_allowed_scope(scope: Scope) -> bool {
    matches!(
        scope,
        Scope::Resources | Scope::User | Scope::Agent | Scope::Session
    )
}

const fn is_manage_allowed_scope(scope: Scope) -> bool {
    is_view_allowed_scope(scope)
}

fn infer_document_format(name: &str) -> Option<&'static str> {
    let ext = Path::new(name)
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match ext.as_str() {
        "md" | "markdown" => Some("markdown"),
        "json" => Some("json"),
        "jsonl" => Some("jsonl"),
        "yaml" | "yml" => Some("yaml"),
        "xml" => Some("xml"),
        "txt" => Some("text"),
        _ => None,
    }
}

fn is_editable_format(format: &str) -> bool {
    matches!(format, "markdown" | "json" | "yaml")
}

fn is_generated_tier_file(name: &str) -> bool {
    matches!(name, ".abstract.md" | ".overview.md" | ".meta.json")
}

fn normalize_readonly_content(content: &str, format: &str) -> String {
    match format {
        "json" => serde_json::from_str::<serde_json::Value>(content)
            .ok()
            .and_then(|parsed| serde_json::to_string_pretty(&parsed).ok())
            .unwrap_or_else(|| content.to_string()),
        _ => content.to_string(),
    }
}

fn uri_updated_at(app: &AxiomMe, uri: &AxiomUri) -> String {
    let path = app.fs.resolve_uri(uri);
    let modified = std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .map(chrono::DateTime::<chrono::Utc>::from)
        .map(|dt| dt.to_rfc3339());
    modified.unwrap_or_else(|_| chrono::Utc::now().to_rfc3339())
}
