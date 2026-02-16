use serde::{Deserialize, Serialize};

use crate::uri::Scope;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileReport {
    pub run_id: String,
    pub drift_count: usize,
    pub invalid_uri_entries: usize,
    pub missing_uri_entries: usize,
    pub missing_files_pruned: usize,
    pub reindexed_scopes: usize,
    pub dry_run: bool,
    pub drift_uris_sample: Vec<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileOptions {
    pub dry_run: bool,
    pub scopes: Option<Vec<Scope>>,
    pub max_drift_sample: usize,
}

impl Default for ReconcileOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            scopes: None,
            max_drift_sample: 50,
        }
    }
}
