use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub uri: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobResult {
    pub matches: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddResourceResult {
    pub root_uri: String,
    pub queued: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AddResourceIngestOptions {
    #[serde(default)]
    pub markdown_only: bool,
    #[serde(default = "default_include_hidden")]
    pub include_hidden: bool,
    #[serde(default)]
    pub exclude_globs: Vec<String>,
}

const fn default_include_hidden() -> bool {
    true
}

impl Default for AddResourceIngestOptions {
    fn default() -> Self {
        Self {
            markdown_only: false,
            include_hidden: true,
            exclude_globs: Vec::new(),
        }
    }
}

impl AddResourceIngestOptions {
    #[must_use]
    pub fn markdown_only_defaults() -> Self {
        Self {
            markdown_only: true,
            include_hidden: false,
            exclude_globs: vec![
                ".obsidian".to_string(),
                ".obsidian/**".to_string(),
                "**/*.json".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownDocument {
    pub uri: String,
    pub content: String,
    pub etag: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownSaveResult {
    pub uri: String,
    pub etag: String,
    pub updated_at: String,
    pub reindexed_root: String,
    pub save_ms: u128,
    pub reindex_ms: u128,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub uri: String,
    pub is_dir: bool,
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeResult {
    pub root: TreeNode,
}
