use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    Router, middleware,
    routing::{get, post},
};
use tokio::sync::RwLock;

use axiomme_core::models::{ReconcileOptions, ReconcileReport};
use axiomme_core::{AxiomMe, Scope};

mod dto;
mod error;
mod handlers;
mod html;
mod markdown;
mod security;

#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(crate) struct WebState {
    pub(crate) app: AxiomMe,
    pub(crate) editor_gate: Arc<RwLock<()>>,
}

impl WebState {
    fn new(app: AxiomMe) -> Self {
        Self {
            app,
            editor_gate: Arc::new(RwLock::new(())),
        }
    }
}

/// Start the markdown web server and block until shutdown.
///
/// # Errors
/// Returns an error when startup reconciliation fails, the runtime cannot be created,
/// the socket cannot be bound, or the server exits with a runtime failure.
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

#[must_use]
pub fn render_markdown_preview(content: &str) -> String {
    markdown::render_markdown_html(content)
}

/// Run mandatory startup reconciliation before serving requests.
///
/// # Errors
/// Returns an error when reconciliation fails or when the reconciler reports
/// a non-success terminal status.
pub(crate) fn run_startup_recovery(app: &AxiomMe) -> Result<ReconcileReport> {
    let report = app.reconcile_state_with_options(&ReconcileOptions {
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
        .route("/", get(handlers::index))
        .route("/assets/index.css", get(handlers::index_css))
        .route("/assets/index.js", get(handlers::index_js))
        .route("/api/document", get(handlers::load_document))
        .route("/api/document/save", post(handlers::save_document))
        .route("/api/markdown", get(handlers::load_markdown))
        .route("/api/markdown/save", post(handlers::save_markdown))
        .route("/api/markdown/preview", post(handlers::preview_markdown))
        .route("/api/fs/list", get(handlers::list_fs))
        .route("/api/fs/tree", get(handlers::tree_fs))
        .route("/api/fs/mkdir", post(handlers::mkdir_fs))
        .route("/api/fs/move", post(handlers::move_fs))
        .route("/api/fs/delete", post(handlers::delete_fs))
        .layer(middleware::from_fn(security::security_headers_middleware))
        .with_state(state)
}
