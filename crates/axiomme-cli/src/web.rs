use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::{Query, Request, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{self, HeaderName},
    },
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, html};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::RwLock;
use uuid::Uuid;

use axiomme_core::error::ErrorPayload;
use axiomme_core::models::{ReconcileOptions, ReconcileReport};
use axiomme_core::{AxiomError, AxiomMe, AxiomUri, Scope};

#[derive(Clone)]
pub(crate) struct WebState {
    app: AxiomMe,
    editor_gate: Arc<RwLock<()>>,
}

impl WebState {
    fn new(app: AxiomMe) -> Self {
        Self {
            app,
            editor_gate: Arc::new(RwLock::new(())),
        }
    }
}

#[derive(Debug, Deserialize)]
struct LoadMarkdownQuery {
    uri: String,
}

#[derive(Debug, Deserialize)]
struct LoadDocumentQuery {
    uri: String,
}

#[derive(Debug, Deserialize)]
struct SaveMarkdownRequest {
    uri: String,
    content: String,
    expected_etag: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PreviewRequest {
    content: String,
}

#[derive(Debug, Serialize)]
struct PreviewResponse {
    html: String,
}

#[derive(Debug, Serialize)]
struct WebDocumentResponse {
    uri: String,
    content: String,
    etag: String,
    updated_at: String,
    format: String,
    editable: bool,
}

pub fn serve_web(app: AxiomMe, host: &str, port: u16) -> Result<()> {
    let state = WebState::new(app);
    let recovery = run_startup_recovery(&state.app)
        .context("startup recovery failed; refusing to serve markdown web editor")?;
    let bind_addr = format!("{host}:{port}");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build web runtime")?;

    println!(
        "startup recovery complete: drift_count={} reindexed_scopes={} status={}",
        recovery.drift_count, recovery.reindexed_scopes, recovery.status
    );

    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(&bind_addr)
            .await
            .with_context(|| format!("failed to bind web server at {bind_addr}"))?;
        println!("web editor listening on http://{}", listener.local_addr()?);

        axum::serve(listener, app_router(state))
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await
            .context("web server failed")
    })
}

fn run_startup_recovery(app: &AxiomMe) -> Result<ReconcileReport> {
    let report = app.reconcile_state_with_options(ReconcileOptions {
        dry_run: false,
        scopes: Some(vec![
            Scope::Resources,
            Scope::User,
            Scope::Agent,
            Scope::Session,
        ]),
        max_drift_sample: 50,
    })?;
    if report.status != "success" {
        anyhow::bail!("unexpected reconcile status: {}", report.status);
    }
    Ok(report)
}

pub(crate) fn app_router(state: WebState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/document", get(load_document))
        .route("/api/document/save", post(save_document))
        .route("/api/markdown", get(load_markdown))
        .route("/api/markdown/save", post(save_markdown))
        .route("/api/markdown/preview", post(preview_markdown))
        .layer(middleware::from_fn(security_headers_middleware))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn load_document(
    State(state): State<WebState>,
    Query(query): Query<LoadDocumentQuery>,
) -> Response {
    let uri = match AxiomUri::parse(&query.uri) {
        Ok(uri) => uri,
        Err(err) => return axiom_error_response(err, "document.load", Some(query.uri)),
    };
    let _guard = match state.editor_gate.try_read() {
        Ok(guard) => guard,
        Err(_) => return locked_response("document.load", Some(query.uri)),
    };
    if !is_view_allowed_scope(uri.scope()) {
        return axiom_error_response(
            AxiomError::PermissionDenied(format!(
                "document viewer does not allow scope: {}",
                uri.scope()
            )),
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

    let name = match uri.last_segment() {
        Some(name) => name.to_string(),
        None => {
            return axiom_error_response(
                AxiomError::Validation(format!("document target must include a filename: {uri}")),
                "document.load",
                None,
            );
        }
    };
    let format = match infer_document_format(&name) {
        Some(format) => format,
        None => {
            return axiom_error_response(
                AxiomError::Validation(format!("unsupported document format for viewer: {uri}")),
                "document.load",
                None,
            );
        }
    };

    if is_editable_format(format) {
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

async fn load_markdown(
    State(state): State<WebState>,
    Query(query): Query<LoadMarkdownQuery>,
) -> Response {
    let _guard = match state.editor_gate.try_read() {
        Ok(guard) => guard,
        Err(_) => return locked_response("markdown.load", Some(query.uri)),
    };

    match state.app.load_markdown(&query.uri) {
        Ok(document) => (StatusCode::OK, Json(document)).into_response(),
        Err(err) => axiom_error_response(err, "markdown.load", Some(query.uri)),
    }
}

async fn save_document(
    State(state): State<WebState>,
    Json(request): Json<SaveMarkdownRequest>,
) -> Response {
    let uri = request.uri.clone();
    let _guard = match state.editor_gate.try_write() {
        Ok(guard) => guard,
        Err(_) => return locked_response("document.save", Some(uri)),
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

fn is_view_allowed_scope(scope: Scope) -> bool {
    matches!(
        scope,
        Scope::Resources | Scope::User | Scope::Agent | Scope::Session
    )
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

async fn save_markdown(
    State(state): State<WebState>,
    Json(request): Json<SaveMarkdownRequest>,
) -> Response {
    let uri = request.uri.clone();
    let _guard = match state.editor_gate.try_write() {
        Ok(guard) => guard,
        Err(_) => return locked_response("markdown.save", Some(uri)),
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

async fn preview_markdown(Json(request): Json<PreviewRequest>) -> Response {
    let rendered = render_markdown_html(&request.content);
    (StatusCode::OK, Json(PreviewResponse { html: rendered })).into_response()
}

fn render_markdown_html(content: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);

    let parser = Parser::new_ext(content, options).map(|event| match event {
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: sanitize_link_destination(dest_url),
            title,
            id,
        }),
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: sanitize_image_source(dest_url),
            title,
            id,
        }),
        Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(CowStr::from(raw.into_string())),
        other => other,
    });
    let mut output = String::new();
    html::push_html(&mut output, parser);
    output
}

fn sanitize_link_destination(dest_url: CowStr<'_>) -> CowStr<'static> {
    let value = dest_url.into_string();
    if is_safe_destination(&value, true) {
        CowStr::from(value)
    } else {
        CowStr::from("#")
    }
}

fn sanitize_image_source(dest_url: CowStr<'_>) -> CowStr<'static> {
    let value = dest_url.into_string();
    if is_safe_destination(&value, false) {
        CowStr::from(value)
    } else {
        CowStr::from("")
    }
}

fn is_safe_destination(value: &str, allow_mailto: bool) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return true;
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with('#')
        || lower.starts_with('/')
        || lower.starts_with("./")
        || lower.starts_with("../")
    {
        return true;
    }
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("axiom://")
        || (allow_mailto && lower.starts_with("mailto:"))
    {
        return true;
    }

    !lower.contains(':')
}

fn locked_response(operation: &str, uri: Option<String>) -> Response {
    let payload = ErrorPayload {
        code: "LOCKED".to_string(),
        message: "editor is locked by an in-flight save/reindex".to_string(),
        operation: operation.to_string(),
        trace_id: Uuid::new_v4().to_string(),
        uri,
        details: Some(json!({
            "reason": "save_reindex_in_flight"
        })),
    };
    (StatusCode::LOCKED, Json(payload)).into_response()
}

fn axiom_error_response(err: AxiomError, operation: &str, uri: Option<String>) -> Response {
    let status = status_for_axiom_error(&err);
    let mut payload = err.to_payload(operation.to_string(), uri);
    if let AxiomError::Internal(message) = &err
        && let Some(details) = rollback_details(message)
    {
        payload.details = Some(details);
    }
    (status, Json(payload)).into_response()
}

fn status_for_axiom_error(err: &AxiomError) -> StatusCode {
    match err {
        AxiomError::InvalidUri(_)
        | AxiomError::InvalidScope(_)
        | AxiomError::PathTraversal(_)
        | AxiomError::Validation(_) => StatusCode::BAD_REQUEST,
        AxiomError::PermissionDenied(_) | AxiomError::SecurityViolation(_) => StatusCode::FORBIDDEN,
        AxiomError::NotFound(_) => StatusCode::NOT_FOUND,
        AxiomError::Conflict(_) => StatusCode::CONFLICT,
        AxiomError::Io(io_err) if io_err.kind() == std::io::ErrorKind::NotFound => {
            StatusCode::NOT_FOUND
        }
        AxiomError::InvalidArchive(_)
        | AxiomError::Io(_)
        | AxiomError::Json(_)
        | AxiomError::Sqlite(_)
        | AxiomError::Zip(_)
        | AxiomError::Http(_)
        | AxiomError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn rollback_details(message: &str) -> Option<Value> {
    if !message.contains("save failed during reindex") {
        return None;
    }

    let reindex_err = extract_rollback_token(message, "reindex_err");
    let rollback_write = extract_rollback_token(message, "rollback_write");
    let rollback_reindex = extract_rollback_token(message, "rollback_reindex");

    Some(json!({
        "reindex_err": reindex_err,
        "rollback_write": rollback_write,
        "rollback_reindex": rollback_reindex,
    }))
}

fn extract_rollback_token(message: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=");
    let start = message.find(&needle)? + needle.len();
    let tail = &message[start..];
    let end = tail.find(';').unwrap_or(tail.len());
    Some(tail[..end].trim().to_string())
}

async fn security_headers_middleware(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    apply_security_headers(response.headers_mut());
    response
}

fn apply_security_headers(headers: &mut HeaderMap) {
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("geolocation=(), microphone=(), camera=()"),
    );
    headers.insert(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(
            "default-src 'self'; connect-src 'self'; img-src 'self' http: https:; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
        ),
    );
}

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>AxiomMe Document Viewer</title>
  <style>
    :root {
      --bg-top: #f7efe2;
      --bg-bottom: #ede6dc;
      --panel: rgba(255, 255, 255, 0.82);
      --line: #d8ccbc;
      --ink: #1a1713;
      --muted: #665f55;
      --accent: #03503f;
      --warn: #8a5a00;
      --danger: #b70f2d;
    }

    * { box-sizing: border-box; }

    body {
      margin: 0;
      color: var(--ink);
      font-family: "IBM Plex Sans", "Avenir Next", "Segoe UI", sans-serif;
      background: linear-gradient(145deg, var(--bg-top), var(--bg-bottom));
      min-height: 100vh;
    }

    .shell {
      max-width: 1320px;
      margin: 24px auto;
      padding: 0 16px 24px;
    }

    .header {
      display: grid;
      gap: 10px;
      margin-bottom: 14px;
      padding: 16px;
      border: 1px solid var(--line);
      border-radius: 14px;
      background: var(--panel);
      backdrop-filter: blur(6px);
      animation: reveal .35s ease-out;
    }

    @keyframes reveal {
      from { opacity: 0; transform: translateY(4px); }
      to { opacity: 1; transform: translateY(0); }
    }

    .title {
      margin: 0;
      font-family: "Iowan Old Style", "Source Serif 4", Georgia, serif;
      font-size: 26px;
      letter-spacing: 0.2px;
    }

    .toolbar {
      display: grid;
      grid-template-columns: 1fr auto auto auto;
      gap: 8px;
      align-items: center;
    }

    input[type="text"] {
      width: 100%;
      min-width: 0;
      border: 1px solid var(--line);
      border-radius: 10px;
      padding: 11px 12px;
      background: #fff;
      color: var(--ink);
      font-size: 14px;
    }

    button {
      border: 1px solid var(--line);
      border-radius: 10px;
      padding: 10px 14px;
      background: #fff;
      color: var(--ink);
      cursor: pointer;
      font-size: 14px;
      font-weight: 600;
    }

    button.primary {
      background: var(--accent);
      border-color: var(--accent);
      color: #fff;
    }

    button:disabled {
      opacity: 0.5;
      cursor: not-allowed;
    }

    .status {
      margin: 0;
      font-size: 13px;
      color: var(--muted);
    }

    .status[data-kind="saving"],
    .status[data-kind="saved"] { color: var(--accent); }
    .status[data-kind="conflict"],
    .status[data-kind="locked"] { color: var(--warn); }
    .status[data-kind="error"] { color: var(--danger); }

    .workspace {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 14px;
      min-height: 70vh;
    }

    .pane {
      border: 1px solid var(--line);
      border-radius: 14px;
      background: var(--panel);
      display: grid;
      grid-template-rows: auto 1fr;
      overflow: hidden;
      backdrop-filter: blur(6px);
    }

    .pane h2 {
      margin: 0;
      padding: 12px 14px;
      border-bottom: 1px solid var(--line);
      font-size: 14px;
      letter-spacing: 0.3px;
      text-transform: uppercase;
    }

    textarea {
      width: 100%;
      height: 100%;
      border: 0;
      resize: none;
      outline: none;
      padding: 16px;
      background: transparent;
      color: var(--ink);
      font-family: "JetBrains Mono", "SF Mono", "Consolas", monospace;
      font-size: 14px;
      line-height: 1.5;
    }

    .preview {
      height: 100%;
      overflow: auto;
      padding: 18px 18px 24px;
      font-family: "Source Serif 4", "Iowan Old Style", Georgia, serif;
      line-height: 1.6;
    }

    .preview pre {
      background: #f7f4ef;
      border: 1px solid #d8ccbc;
      border-radius: 8px;
      padding: 10px;
      overflow: auto;
    }

    .preview code {
      font-family: "JetBrains Mono", "SF Mono", "Consolas", monospace;
      font-size: 0.92em;
    }

    @media (max-width: 900px) {
      .toolbar {
        grid-template-columns: 1fr 1fr;
      }
      .workspace {
        grid-template-columns: 1fr;
      }
    }
  </style>
</head>
<body>
  <div class="shell">
    <div class="header">
      <h1 class="title">Document Viewer and Markdown Editor</h1>
      <div class="toolbar">
        <input id="uri" type="text" placeholder="axiom://resources/docs/guide.md" />
        <button id="load">Load</button>
        <button id="reload">Reload</button>
        <button id="save" class="primary">Save (Ctrl/Cmd+S)</button>
      </div>
      <p id="status" class="status" data-kind="idle">Ready</p>
    </div>

    <div class="workspace">
      <section class="pane">
        <h2>Editor</h2>
        <textarea id="editor" spellcheck="false"></textarea>
      </section>
      <section class="pane">
        <h2>Preview</h2>
        <article id="preview" class="preview"></article>
      </section>
    </div>
  </div>

  <script>
    const uriInput = document.getElementById("uri");
    const editor = document.getElementById("editor");
    const preview = document.getElementById("preview");
    const statusEl = document.getElementById("status");
    const loadBtn = document.getElementById("load");
    const reloadBtn = document.getElementById("reload");
    const saveBtn = document.getElementById("save");

    let currentEtag = null;
    let previewTimer = null;
    let currentFormat = "markdown";
    let currentEditable = true;

    function setStatus(kind, message) {
      statusEl.dataset.kind = kind;
      statusEl.textContent = message;
    }

    function parseError(payload) {
      if (!payload || !payload.code) {
        return { code: "UNKNOWN", message: "unknown error" };
      }
      return payload;
    }

    function escapeHtml(input) {
      return input
        .replaceAll("&", "&amp;")
        .replaceAll("<", "&lt;")
        .replaceAll(">", "&gt;")
        .replaceAll("\"", "&quot;")
        .replaceAll("'", "&#39;");
    }

    function renderReadonlyPreview(text, format) {
      const label = escapeHtml((format || "text").toUpperCase());
      const payload = escapeHtml(text || "");
      return `<h3>${label} (Read-only)</h3><pre><code>${payload}</code></pre>`;
    }

    async function loadDocument() {
      const uri = uriInput.value.trim();
      if (!uri) {
        setStatus("error", "URI is required.");
        return;
      }

      setStatus("saving", "Loading...");
      const resp = await fetch(`/api/document?uri=${encodeURIComponent(uri)}`);
      const body = await resp.json();
      if (!resp.ok) {
        const err = parseError(body);
        if (resp.status === 423) {
          setStatus("locked", "Locked: save/reindex in progress.");
        } else {
          setStatus("error", `${err.code}: ${err.message}`);
        }
        return;
      }

      editor.value = body.content || "";
      currentEtag = body.etag || null;
      currentFormat = body.format || "text";
      currentEditable = !!body.editable;
      editor.readOnly = !currentEditable;
      saveBtn.disabled = !currentEditable;
      if (currentEditable) {
        saveBtn.classList.add("primary");
        setStatus("saved", `Loaded editable ${currentFormat}: ${uri}`);
      } else {
        saveBtn.classList.remove("primary");
        setStatus("saved", `Loaded read-only ${currentFormat}: ${uri}`);
      }
      await updatePreview();
    }

    async function saveDocument() {
      const uri = uriInput.value.trim();
      if (!uri) {
        setStatus("error", "URI is required.");
        return;
      }
      if (!currentEditable) {
        setStatus("locked", `Read-only mode: ${currentFormat} is not editable.`);
        return;
      }

      setStatus("saving", "Saving and reindexing...");
      const payload = {
        uri,
        content: editor.value,
        expected_etag: currentEtag,
      };
      const resp = await fetch("/api/document/save", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(payload),
      });
      const body = await resp.json();
      if (!resp.ok) {
        const err = parseError(body);
        if (resp.status === 409) {
          setStatus("conflict", "Conflict: stale editor state. Click Reload.");
          return;
        }
        if (resp.status === 423) {
          setStatus("locked", "Locked: another save is in progress.");
          return;
        }
        setStatus("error", `${err.code}: ${err.message}`);
        return;
      }

      currentEtag = body.etag || null;
      setStatus("saved", `Saved. reindex ${body.reindex_ms}ms`);
      await updatePreview();
    }

    async function updatePreview() {
      if (!currentEditable || currentFormat !== "markdown") {
        preview.innerHTML = renderReadonlyPreview(editor.value, currentFormat);
        return;
      }

      const resp = await fetch("/api/markdown/preview", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ content: editor.value }),
      });
      const body = await resp.json();
      if (!resp.ok) {
        setStatus("error", "Preview render failed.");
        return;
      }
      preview.innerHTML = body.html || "";
    }

    function schedulePreview() {
      if (previewTimer) {
        clearTimeout(previewTimer);
      }
      previewTimer = setTimeout(() => {
        updatePreview().catch(() => setStatus("error", "Preview render failed."));
      }, 160);
    }

    loadBtn.addEventListener("click", () => {
      loadDocument().catch(() => setStatus("error", "Load request failed."));
    });
    reloadBtn.addEventListener("click", () => {
      loadDocument().catch(() => setStatus("error", "Reload request failed."));
    });
    saveBtn.addEventListener("click", () => {
      saveDocument().catch(() => setStatus("error", "Save request failed."));
    });
    editor.addEventListener("input", () => {
      setStatus("idle", "Editing...");
      schedulePreview();
    });

    window.addEventListener("keydown", (event) => {
      const key = event.key.toLowerCase();
      if ((event.ctrlKey || event.metaKey) && key === "s") {
        event.preventDefault();
        saveDocument().catch(() => setStatus("error", "Save request failed."));
      }
    });

    const params = new URLSearchParams(window.location.search);
    const uriParam = params.get("uri");
    if (uriParam) {
      uriInput.value = uriParam;
      loadDocument().catch(() => setStatus("error", "Initial load failed."));
    } else {
      updatePreview().catch(() => setStatus("error", "Preview render failed."));
    }
  </script>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use std::fs;

    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
        response::Response,
    };
    use tower::util::ServiceExt;

    use axiomme_core::AxiomUri;
    use axiomme_core::models::{MarkdownDocument, MarkdownSaveResult};

    use super::*;

    struct TestHarness {
        _temp: tempfile::TempDir,
        state: WebState,
        router: Router,
        uri: String,
        parent_uri: String,
    }

    impl TestHarness {
        fn setup() -> Self {
            let temp = tempfile::tempdir().expect("tempdir");
            let app = AxiomMe::new(temp.path()).expect("app");
            app.initialize().expect("init");

            let corpus = temp.path().join("corpus");
            fs::create_dir_all(&corpus).expect("mkdir corpus");
            fs::write(corpus.join("guide.md"), "# Guide\n\nalpha_token").expect("seed markdown");
            fs::write(corpus.join("config.json"), "{\"name\":\"axiomme\",\"v\":1}")
                .expect("seed json");
            fs::write(corpus.join("policy.yaml"), "mode: strict\nenabled: true\n")
                .expect("seed yaml");
            fs::write(
                corpus.join("events.jsonl"),
                "{\"t\":\"a\"}\n{\"t\":\"b\"}\n",
            )
            .expect("seed jsonl");
            fs::write(corpus.join("layout.xml"), "<root><item>v</item></root>").expect("seed xml");
            fs::write(corpus.join("script.sh"), "echo hi\n").expect("seed script");

            app.add_resource(
                corpus.to_str().expect("corpus str"),
                Some("axiom://resources/web-editor"),
                None,
                None,
                true,
                None,
            )
            .expect("add resource");

            let state = WebState::new(app);
            let router = app_router(state.clone());
            Self {
                _temp: temp,
                state,
                router,
                uri: "axiom://resources/web-editor/guide.md".to_string(),
                parent_uri: "axiom://resources/web-editor".to_string(),
            }
        }
    }

    #[tokio::test]
    async fn web_markdown_s1_load_save_success() {
        let harness = TestHarness::setup();

        let load = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/markdown?uri={}", harness.uri))
                    .body(Body::empty())
                    .expect("load request"),
            )
            .await
            .expect("load response");
        assert_eq!(load.status(), StatusCode::OK);
        let loaded: MarkdownDocument = decode_json(load).await;

        let save = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/markdown/save")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "uri": harness.uri,
                            "content": "# Guide\n\nbeta_token",
                            "expected_etag": loaded.etag
                        }))
                        .expect("save json"),
                    ))
                    .expect("save request"),
            )
            .await
            .expect("save response");
        assert_eq!(save.status(), StatusCode::OK);
        let saved: MarkdownSaveResult = decode_json(save).await;
        assert_eq!(saved.reindexed_root, harness.parent_uri);
        assert!(!saved.etag.is_empty());

        let find = harness
            .state
            .app
            .find(
                "beta_token",
                Some(&harness.parent_uri),
                Some(10),
                None,
                None,
            )
            .expect("find after save");
        assert!(
            find.resources.iter().any(|hit| hit.uri == harness.uri)
                || find.memories.iter().any(|hit| hit.uri == harness.uri)
                || find.skills.iter().any(|hit| hit.uri == harness.uri),
            "saved markdown should be searchable immediately"
        );
    }

    #[tokio::test]
    async fn web_markdown_s2_stale_etag_returns_conflict() {
        let harness = TestHarness::setup();

        let load = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/markdown?uri={}", harness.uri))
                    .body(Body::empty())
                    .expect("load request"),
            )
            .await
            .expect("load response");
        assert_eq!(load.status(), StatusCode::OK);
        let loaded: MarkdownDocument = decode_json(load).await;

        let first_save = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/markdown/save")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "uri": harness.uri,
                            "content": "# Guide\n\netag_v2",
                            "expected_etag": loaded.etag
                        }))
                        .expect("first save json"),
                    ))
                    .expect("first save request"),
            )
            .await
            .expect("first save response");
        assert_eq!(first_save.status(), StatusCode::OK);

        let stale_save = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/markdown/save")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "uri": harness.uri,
                            "content": "# Guide\n\netag_v3",
                            "expected_etag": loaded.etag
                        }))
                        .expect("stale save json"),
                    ))
                    .expect("stale save request"),
            )
            .await
            .expect("stale save response");
        assert_eq!(stale_save.status(), StatusCode::CONFLICT);
        let payload: serde_json::Value = decode_json(stale_save).await;
        assert_eq!(payload["code"], "CONFLICT");
    }

    #[tokio::test]
    async fn web_markdown_s3_in_flight_lock_returns_423() {
        let harness = TestHarness::setup();
        let _writer_guard = harness
            .state
            .editor_gate
            .try_write()
            .expect("acquire write gate");

        let blocked = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/markdown?uri={}", harness.uri))
                    .body(Body::empty())
                    .expect("load request"),
            )
            .await
            .expect("load response");
        assert_eq!(blocked.status(), StatusCode::LOCKED);
        let payload: serde_json::Value = decode_json(blocked).await;
        assert_eq!(payload["code"], "LOCKED");
    }

    #[tokio::test]
    async fn web_document_load_readonly_is_locked_during_in_flight_save() {
        let harness = TestHarness::setup();
        let _writer_guard = harness
            .state
            .editor_gate
            .try_write()
            .expect("acquire write gate");

        let blocked = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/document?uri=axiom://resources/web-editor/events.jsonl")
                    .body(Body::empty())
                    .expect("document request"),
            )
            .await
            .expect("document response");
        assert_eq!(blocked.status(), StatusCode::LOCKED);
        let payload: serde_json::Value = decode_json(blocked).await;
        assert_eq!(payload["code"], "LOCKED");
    }

    #[tokio::test]
    async fn web_document_load_markdown_is_editable() {
        let harness = TestHarness::setup();
        let response = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/document?uri={}", harness.uri))
                    .body(Body::empty())
                    .expect("document request"),
            )
            .await
            .expect("document response");
        assert_eq!(response.status(), StatusCode::OK);
        let payload: serde_json::Value = decode_json(response).await;
        assert_eq!(payload["format"], "markdown");
        assert_eq!(payload["editable"], true);
        assert!(payload["etag"].as_str().is_some_and(|x| !x.is_empty()));
    }

    #[tokio::test]
    async fn web_document_load_json_is_editable_and_pretty() {
        let harness = TestHarness::setup();
        let response = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/document?uri=axiom://resources/web-editor/config.json")
                    .body(Body::empty())
                    .expect("document request"),
            )
            .await
            .expect("document response");
        assert_eq!(response.status(), StatusCode::OK);
        let payload: serde_json::Value = decode_json(response).await;
        assert_eq!(payload["format"], "json");
        assert_eq!(payload["editable"], true);
        let content = payload["content"].as_str().expect("content str");
        assert!(content.contains("\"name\""));
        assert!(content.contains("axiomme"));
        assert!(payload["etag"].as_str().is_some_and(|x| !x.is_empty()));
    }

    #[tokio::test]
    async fn web_document_save_json_success() {
        let harness = TestHarness::setup();

        let load = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/document?uri=axiom://resources/web-editor/config.json")
                    .body(Body::empty())
                    .expect("load request"),
            )
            .await
            .expect("load response");
        assert_eq!(load.status(), StatusCode::OK);
        let loaded: serde_json::Value = decode_json(load).await;
        let etag = loaded["etag"].as_str().expect("etag").to_string();

        let save = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/document/save")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "uri": "axiom://resources/web-editor/config.json",
                            "content": "{\n  \"name\": \"axiomme\",\n  \"v\": 2\n}",
                            "expected_etag": etag
                        }))
                        .expect("save json"),
                    ))
                    .expect("save request"),
            )
            .await
            .expect("save response");
        assert_eq!(save.status(), StatusCode::OK);
        let saved: MarkdownSaveResult = decode_json(save).await;
        assert!(!saved.etag.is_empty());

        let after = harness
            .state
            .app
            .read("axiom://resources/web-editor/config.json")
            .expect("read after save");
        assert!(after.contains("\"v\": 2"));
    }

    #[tokio::test]
    async fn web_document_save_json_invalid_returns_400() {
        let harness = TestHarness::setup();

        let load = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/document?uri=axiom://resources/web-editor/config.json")
                    .body(Body::empty())
                    .expect("load request"),
            )
            .await
            .expect("load response");
        assert_eq!(load.status(), StatusCode::OK);
        let loaded: serde_json::Value = decode_json(load).await;
        let etag = loaded["etag"].as_str().expect("etag").to_string();

        let save = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/document/save")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "uri": "axiom://resources/web-editor/config.json",
                            "content": "{\"name\":",
                            "expected_etag": etag
                        }))
                        .expect("save json"),
                    ))
                    .expect("save request"),
            )
            .await
            .expect("save response");
        assert_eq!(save.status(), StatusCode::BAD_REQUEST);
        let payload: serde_json::Value = decode_json(save).await;
        assert_eq!(payload["code"], "VALIDATION_FAILED");

        let after = harness
            .state
            .app
            .read("axiom://resources/web-editor/config.json")
            .expect("read after failed save");
        assert_eq!(after, "{\"name\":\"axiomme\",\"v\":1}");
    }

    #[tokio::test]
    async fn web_document_load_rejects_unsupported_format() {
        let harness = TestHarness::setup();
        let response = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/document?uri=axiom://resources/web-editor/script.sh")
                    .body(Body::empty())
                    .expect("document request"),
            )
            .await
            .expect("document response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let payload: serde_json::Value = decode_json(response).await;
        assert_eq!(payload["code"], "VALIDATION_FAILED");
    }

    #[tokio::test]
    async fn web_responses_include_security_headers() {
        let harness = TestHarness::setup();

        let index = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .expect("index request"),
            )
            .await
            .expect("index response");
        assert_eq!(index.status(), StatusCode::OK);
        assert_eq!(
            header_value(index.headers(), "x-content-type-options"),
            Some("nosniff")
        );
        assert_eq!(
            header_value(index.headers(), "x-frame-options"),
            Some("DENY")
        );
        assert_eq!(
            header_value(index.headers(), "referrer-policy"),
            Some("no-referrer")
        );
        let csp = header_value(index.headers(), "content-security-policy").expect("csp header");
        assert!(csp.contains("default-src 'self'"));
        assert!(csp.contains("frame-ancestors 'none'"));

        let api = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/markdown?uri={}", harness.uri))
                    .body(Body::empty())
                    .expect("api request"),
            )
            .await
            .expect("api response");
        assert_eq!(api.status(), StatusCode::OK);
        assert_eq!(
            header_value(api.headers(), "x-content-type-options"),
            Some("nosniff")
        );
        assert!(header_value(api.headers(), "content-security-policy").is_some());
    }

    #[tokio::test]
    async fn web_markdown_save_rejects_queue_scope_with_403() {
        let harness = TestHarness::setup();
        let queue_uri = AxiomUri::parse("axiom://queue/editor/test.md").expect("queue uri");
        harness
            .state
            .app
            .fs
            .write(&queue_uri, "# queue", true)
            .expect("write queue");

        let response = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/markdown/save")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "uri": "axiom://queue/editor/test.md",
                            "content": "# queue update",
                            "expected_etag": serde_json::Value::Null
                        }))
                        .expect("save json"),
                    ))
                    .expect("save request"),
            )
            .await
            .expect("save response");

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let payload: serde_json::Value = decode_json(response).await;
        assert_eq!(payload["code"], "PERMISSION_DENIED");
    }

    #[tokio::test]
    async fn web_markdown_save_rejects_tier_file_with_403() {
        let harness = TestHarness::setup();
        let tier_uri = AxiomUri::parse("axiom://resources/web-editor/.overview.md").expect("tier");
        harness
            .state
            .app
            .fs
            .write(&tier_uri, "# tier", true)
            .expect("write tier");

        let response = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/markdown/save")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "uri": "axiom://resources/web-editor/.overview.md",
                            "content": "# update tier",
                            "expected_etag": serde_json::Value::Null
                        }))
                        .expect("save json"),
                    ))
                    .expect("save request"),
            )
            .await
            .expect("save response");

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let payload: serde_json::Value = decode_json(response).await;
        assert_eq!(payload["code"], "PERMISSION_DENIED");
    }

    #[tokio::test]
    async fn web_preview_sanitizes_unsafe_html_and_links() {
        let harness = TestHarness::setup();
        let response = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/markdown/preview")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "content": "[bad](javascript:alert(1)) [ok](/docs/readme.md)\n\n<script>alert(1)</script>\n\n![img](data:text/html;base64,abcd)"
                        }))
                        .expect("preview json"),
                    ))
                    .expect("preview request"),
            )
            .await
            .expect("preview response");
        assert_eq!(response.status(), StatusCode::OK);
        let payload: serde_json::Value = decode_json(response).await;
        let html = payload["html"].as_str().expect("html str");

        assert!(!html.contains("javascript:"));
        assert!(!html.contains("<script>"));
        assert!(!html.contains("data:text/html"));
        assert!(html.contains("href=\"#\""));
        assert!(html.contains("href=\"/docs/readme.md\""));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn web_markdown_save_reindex_failure_returns_500_with_rollback_details() {
        let harness = TestHarness::setup();
        let root_uri = AxiomUri::parse("axiom://resources/web-editor").expect("root parse");
        let bad_path = harness
            .state
            .app
            .fs
            .resolve_uri(&root_uri)
            .join("bad\\name.md");
        fs::write(bad_path, "force reindex failure").expect("write bad path");

        let load = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/markdown?uri={}", harness.uri))
                    .body(Body::empty())
                    .expect("load request"),
            )
            .await
            .expect("load response");
        assert_eq!(load.status(), StatusCode::OK);
        let loaded: MarkdownDocument = decode_json(load).await;

        let response = harness
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/markdown/save")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "uri": harness.uri,
                            "content": "# Guide\n\nmust_fail",
                            "expected_etag": loaded.etag
                        }))
                        .expect("save json"),
                    ))
                    .expect("save request"),
            )
            .await
            .expect("save response");

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let payload: serde_json::Value = decode_json(response).await;
        assert_eq!(payload["code"], "INTERNAL_ERROR");
        assert!(payload["details"]["rollback_write"].is_string());
        assert!(payload["details"]["rollback_reindex"].is_string());
    }

    #[test]
    fn startup_recovery_prunes_missing_index_entries() {
        let harness = TestHarness::setup();
        let stale_uri = "axiom://resources/web-editor/stale.md";
        harness
            .state
            .app
            .state
            .upsert_index_state(stale_uri, "deadbeef", 0, "indexed")
            .expect("insert stale index state");

        let before = harness
            .state
            .app
            .state
            .list_index_state_uris()
            .expect("list before recovery");
        assert!(before.iter().any(|x| x == stale_uri));

        let report = run_startup_recovery(&harness.state.app).expect("startup recovery");
        assert!(report.drift_count >= 1);
        assert_eq!(report.status, "success");
        assert_eq!(report.reindexed_scopes, 4);

        let after = harness
            .state
            .app
            .state
            .list_index_state_uris()
            .expect("list after recovery");
        assert!(!after.iter().any(|x| x == stale_uri));
    }

    async fn decode_json<T: serde::de::DeserializeOwned>(response: Response) -> T {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read body bytes");
        serde_json::from_slice(&bytes).expect("decode json")
    }

    fn header_value<'a>(headers: &'a axum::http::HeaderMap, key: &str) -> Option<&'a str> {
        headers.get(key).and_then(|value| value.to_str().ok())
    }
}
