use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: String,
    pub text: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub uri: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitStats {
    pub total_turns: usize,
    pub contexts_used: usize,
    pub skills_used: usize,
    pub memories_extracted: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitResult {
    pub session_id: String,
    pub status: String,
    pub memories_extracted: usize,
    pub active_count_updated: usize,
    pub archived: bool,
    pub stats: CommitStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchContext {
    pub session_id: String,
    pub recent_messages: Vec<Message>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextUsage {
    pub contexts_used: usize,
    pub skills_used: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub context_usage: ContextUsage,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCandidate {
    pub category: String,
    pub key: String,
    pub text: String,
    pub source_message_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    Profile,
    Preferences,
    Entities,
    Events,
    Cases,
    Patterns,
}

impl MemoryCategory {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Profile => "profile",
            Self::Preferences => "preferences",
            Self::Entities => "entities",
            Self::Events => "events",
            Self::Cases => "cases",
            Self::Patterns => "patterns",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromotionApplyMode {
    AllOrNothing,
    BestEffort,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommitMode {
    ArchiveAndExtract,
    ArchiveOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryPromotionFact {
    pub category: MemoryCategory,
    pub text: String,
    #[serde(default)]
    pub source_message_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub confidence_milli: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryPromotionRequest {
    pub session_id: String,
    pub checkpoint_id: String,
    pub apply_mode: PromotionApplyMode,
    pub facts: Vec<MemoryPromotionFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryPromotionResult {
    pub session_id: String,
    pub checkpoint_id: String,
    pub accepted: usize,
    pub persisted: usize,
    pub skipped_duplicates: usize,
    pub rejected: usize,
}
