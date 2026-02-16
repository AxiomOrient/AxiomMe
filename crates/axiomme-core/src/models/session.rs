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
