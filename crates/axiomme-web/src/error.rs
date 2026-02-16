use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use uuid::Uuid;

use axiomme_core::AxiomError;
use axiomme_core::error::ErrorPayload;

pub fn locked_response(operation: &str, uri: Option<String>) -> Response {
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

#[expect(
    clippy::needless_pass_by_value,
    reason = "handlers naturally own error values from `Result` and pass them through"
)]
pub fn axiom_error_response(err: AxiomError, operation: &str, uri: Option<String>) -> Response {
    let status = status_for_axiom_error(&err);
    let mut payload = err.to_payload(operation.to_string(), uri);
    match &err {
        AxiomError::Internal(message) => {
            if let Some(details) = rollback_details(message) {
                payload.details = Some(details);
            }
        }
        AxiomError::OmInference {
            inference_source,
            kind,
            ..
        } => {
            payload.details = Some(json!({
                "source": inference_source.as_str(),
                "kind": kind.as_str(),
            }));
        }
        _ => {}
    }
    (status, Json(payload)).into_response()
}

fn status_for_axiom_error(err: &AxiomError) -> StatusCode {
    match err {
        AxiomError::InvalidUri(_)
        | AxiomError::InvalidScope(_)
        | AxiomError::PathTraversal(_)
        | AxiomError::Validation(_) => StatusCode::BAD_REQUEST,
        AxiomError::OmInference { kind, .. } => match kind {
            axiomme_core::error::OmInferenceFailureKind::Transient => {
                StatusCode::SERVICE_UNAVAILABLE
            }
            axiomme_core::error::OmInferenceFailureKind::Fatal => StatusCode::INTERNAL_SERVER_ERROR,
            axiomme_core::error::OmInferenceFailureKind::Schema => StatusCode::BAD_GATEWAY,
        },
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
