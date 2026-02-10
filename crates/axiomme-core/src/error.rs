use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, AxiomError>;

#[derive(Debug, Error)]
pub enum AxiomError {
    #[error("invalid URI: {0}")]
    InvalidUri(String),

    #[error("invalid scope: {0}")]
    InvalidScope(String),

    #[error("path traversal is not allowed: {0}")]
    PathTraversal(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("invalid archive: {0}")]
    InvalidArchive(String),

    #[error("security violation: {0}")]
    SecurityViolation(String),

    #[error("validation failed: {0}")]
    Validation(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),

    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),

    #[error(transparent)]
    Http(#[from] reqwest::Error),

    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
    pub operation: String,
    pub trace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl AxiomError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidUri(_) => "INVALID_URI",
            Self::InvalidScope(_) => "INVALID_SCOPE",
            Self::PathTraversal(_) => "PATH_TRAVERSAL",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict(_) => "CONFLICT",
            Self::PermissionDenied(_) => "PERMISSION_DENIED",
            Self::InvalidArchive(_) => "INVALID_ARCHIVE",
            Self::SecurityViolation(_) => "SECURITY_VIOLATION",
            Self::Validation(_) => "VALIDATION_FAILED",
            Self::Io(_) => "IO_ERROR",
            Self::Json(_) => "JSON_ERROR",
            Self::Sqlite(_) => "SQLITE_ERROR",
            Self::Zip(_) => "ZIP_ERROR",
            Self::Http(_) => "HTTP_ERROR",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    pub fn to_payload(&self, operation: impl Into<String>, uri: Option<String>) -> ErrorPayload {
        ErrorPayload {
            code: self.code().to_string(),
            message: self.to_string(),
            operation: operation.into(),
            trace_id: Uuid::new_v4().to_string(),
            uri,
            details: None,
        }
    }
}
