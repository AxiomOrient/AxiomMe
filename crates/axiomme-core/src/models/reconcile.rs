use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::uri::Scope;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileRunStatus {
    Running,
    DryRun,
    Success,
    Failed,
}

impl ReconcileRunStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::DryRun => "dry_run",
            Self::Success => "success",
            Self::Failed => "failed",
        }
    }
}

impl std::fmt::Display for ReconcileRunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ReconcileRunStatus {
    type Err = String;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "running" => Ok(Self::Running),
            "dry_run" => Ok(Self::DryRun),
            "success" => Ok(Self::Success),
            "failed" => Ok(Self::Failed),
            other => Err(format!("unknown reconcile run status: {other}")),
        }
    }
}

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
    pub status: ReconcileRunStatus,
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
