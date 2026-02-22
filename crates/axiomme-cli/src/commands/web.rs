use anyhow::Result;
use axiomme_core::AxiomMe;

#[derive(Debug, Clone, Copy)]
pub(super) struct WebServeOptions<'a> {
    pub(super) host: &'a str,
    pub(super) port: u16,
}

pub(super) fn render_preview_html(content: &str) -> String {
    axiomme_web::render_markdown_preview(content)
}

pub(super) fn serve(app: &AxiomMe, options: WebServeOptions<'_>) -> Result<()> {
    axiomme_web::serve_web(app.clone(), options.host, options.port)
}
