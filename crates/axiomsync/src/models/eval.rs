use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalQueryCase {
    pub source_trace_id: String,
    pub query: String,
    pub target_uri: Option<String>,
    pub expected_top_uri: Option<String>,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCaseResult {
    pub source_trace_id: String,
    pub query: String,
    pub target_uri: Option<String>,
    pub expected_top_uri: Option<String>,
    pub actual_top_uri: Option<String>,
    pub passed: bool,
    pub bucket: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub replay_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalBucket {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRunSelection {
    pub trace_limit: usize,
    pub query_limit: usize,
    pub search_limit: usize,
    pub include_golden: bool,
    pub golden_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalCoverageSummary {
    pub traces_scanned: usize,
    pub trace_cases_used: usize,
    pub golden_cases_used: usize,
    pub executed_cases: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalQualitySummary {
    pub passed: usize,
    pub failed: usize,
    pub top1_accuracy: f32,
    pub buckets: Vec<EvalBucket>,
    pub failures: Vec<EvalCaseResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalArtifacts {
    pub report_uri: String,
    pub query_set_uri: String,
    pub markdown_report_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalLoopReport {
    pub run_id: String,
    pub created_at: String,
    pub selection: EvalRunSelection,
    pub coverage: EvalCoverageSummary,
    pub quality: EvalQualitySummary,
    pub artifacts: EvalArtifacts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRunOptions {
    pub trace_limit: usize,
    pub query_limit: usize,
    pub search_limit: usize,
    pub include_golden: bool,
    pub golden_only: bool,
}

impl Default for EvalRunOptions {
    fn default() -> Self {
        Self {
            trace_limit: 100,
            query_limit: 50,
            search_limit: 10,
            include_golden: true,
            golden_only: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalGoldenAddResult {
    pub golden_uri: String,
    pub added: bool,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalGoldenMergeReport {
    pub golden_uri: String,
    pub before_count: usize,
    pub added_count: usize,
    pub after_count: usize,
    pub trace_limit: usize,
    pub max_add: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalGoldenDocument {
    pub version: u32,
    pub updated_at: String,
    pub cases: Vec<EvalQueryCase>,
}
