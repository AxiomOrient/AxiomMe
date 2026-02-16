use std::time::Instant;

use chrono::Utc;

use crate::catalog::request_log_uri;
use crate::error::AxiomError;
use crate::models::RequestLogEntry;

use super::AxiomMe;

impl AxiomMe {
    pub(super) fn try_log_request(&self, entry: &RequestLogEntry) {
        if let Ok(uri) = request_log_uri()
            && let Ok(serialized) = serde_json::to_string(entry)
        {
            let mut line = serialized;
            line.push('\n');
            let _ = self.fs.append(&uri, &line, true);
        }
    }

    pub(super) fn log_request_status(
        &self,
        request_id: String,
        operation: &str,
        status: &str,
        started: Instant,
        target_uri: Option<String>,
        details: Option<serde_json::Value>,
    ) {
        self.try_log_request(&RequestLogEntry {
            request_id,
            operation: operation.to_string(),
            status: status.to_string(),
            latency_ms: started.elapsed().as_millis(),
            created_at: Utc::now().to_rfc3339(),
            trace_id: None,
            target_uri,
            error_code: None,
            error_message: None,
            details,
        });
    }

    pub(super) fn log_request_error(
        &self,
        request_id: String,
        operation: &str,
        started: Instant,
        target_uri: Option<String>,
        err: &AxiomError,
        details: Option<serde_json::Value>,
    ) {
        self.try_log_request(&RequestLogEntry {
            request_id,
            operation: operation.to_string(),
            status: "error".to_string(),
            latency_ms: started.elapsed().as_millis(),
            created_at: Utc::now().to_rfc3339(),
            trace_id: None,
            target_uri,
            error_code: Some(err.code().to_string()),
            error_message: Some(err.to_string()),
            details,
        });
    }

    pub(super) fn log_request_warning(
        &self,
        request_id: String,
        operation: &str,
        started: Instant,
        target_uri: Option<String>,
        warning_message: &str,
        details: Option<serde_json::Value>,
    ) {
        self.try_log_request(&RequestLogEntry {
            request_id,
            operation: operation.to_string(),
            status: "warning".to_string(),
            latency_ms: started.elapsed().as_millis(),
            created_at: Utc::now().to_rfc3339(),
            trace_id: None,
            target_uri,
            error_code: None,
            error_message: Some(warning_message.to_string()),
            details,
        });
    }
}
