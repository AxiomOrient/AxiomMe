// Public fallible APIs in this crate share one concrete error contract (`AxiomError`).
// Repeating per-function `# Errors` boilerplate obscures behavior more than it clarifies.
#![allow(
    clippy::missing_errors_doc,
    reason = "crate-wide fallible API uses one explicit error type; per-item boilerplate would duplicate contract"
)]

pub(crate) mod catalog;
pub mod client;
pub(crate) mod config;
pub(crate) mod context_ops;
pub mod embedding;
pub mod error;
pub(crate) mod evidence;
pub mod fs;
pub(crate) mod host_tools;
pub mod index;
pub mod ingest;
pub(crate) mod jsonl;
pub(crate) mod llm_io;
#[cfg(feature = "markdown-preview")]
pub mod markdown_preview;
pub(crate) mod mime;
pub mod models;
pub mod om;
pub mod om_bridge;
pub mod ontology;
pub mod pack;
pub mod parse;
pub(crate) mod quality;
pub(crate) mod queue_policy;
pub(crate) mod release_gate;
pub mod retrieval;
pub(crate) mod security_audit;
pub mod session;
pub mod state;
pub(crate) mod text;
pub mod uri;

pub use client::AxiomMe;
pub use error::{AxiomError, Result};
pub use session::Session;
pub use uri::{AxiomUri, Scope};
