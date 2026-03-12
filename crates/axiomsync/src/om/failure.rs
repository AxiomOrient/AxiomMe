use crate::error::{AxiomError, OmInferenceFailureKind, OmInferenceSource};

pub fn om_reflector_error(kind: OmInferenceFailureKind, message: impl Into<String>) -> AxiomError {
    AxiomError::OmInference {
        inference_source: OmInferenceSource::Reflector,
        kind,
        message: message.into(),
    }
}

pub fn om_observer_error(kind: OmInferenceFailureKind, message: impl Into<String>) -> AxiomError {
    AxiomError::OmInference {
        inference_source: OmInferenceSource::Observer,
        kind,
        message: message.into(),
    }
}

pub fn om_status_kind(status: reqwest::StatusCode) -> OmInferenceFailureKind {
    if status.is_server_error() || status.as_u16() == 429 {
        OmInferenceFailureKind::Transient
    } else {
        OmInferenceFailureKind::Fatal
    }
}
