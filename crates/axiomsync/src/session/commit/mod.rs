use std::fs;

use chrono::Utc;

use crate::error::Result;
use crate::models::{
    CommitMode, CommitResult, CommitStats, MemoryPromotionFact, MemoryPromotionRequest,
    MemoryPromotionResult,
};
use crate::tier_documents::write_tiers;
use crate::uri::AxiomUri;

use self::apply_flow::promote_memories as promote_memories_flow;
use self::apply_modes::{
    apply_promotion_all_or_nothing as apply_promotion_all_or_nothing_mode,
    apply_promotion_best_effort as apply_promotion_best_effort_mode,
};
use self::fallbacks::{
    record_memory_dedup_fallback as record_memory_dedup_fallback_event,
    record_memory_extractor_fallback as record_memory_extractor_fallback_event,
};
use self::types::ResolvedMemoryCandidate;
use self::write_path::{
    persist_memory as persist_memory_write_path,
    reindex_memory_uris as reindex_memory_uris_write_path,
};
use super::Session;
use super::archive::{next_archive_number, summarize_messages};
use super::memory_extractor::extract_memories_for_commit;

mod apply_flow;
mod apply_modes;
mod dedup;
mod fallbacks;
pub(crate) mod helpers;
mod promotion;
mod read_path;
mod resolve_path;
mod types;
mod write_path;

#[cfg(test)]
mod tests;

impl Session {
    pub fn commit(&self) -> Result<CommitResult> {
        self.commit_with_mode(CommitMode::ArchiveAndExtract)
    }

    pub fn commit_with_mode(&self, mode: CommitMode) -> Result<CommitResult> {
        let active_messages = self.read_messages()?;
        let total_turns = active_messages.len();
        let meta = self.read_meta()?;

        if total_turns == 0 {
            return Ok(CommitResult {
                session_id: self.session_id.clone(),
                status: "committed".to_string(),
                memories_extracted: 0,
                active_count_updated: 0,
                archived: false,
                stats: CommitStats {
                    total_turns: 0,
                    contexts_used: meta.context_usage.contexts_used,
                    skills_used: meta.context_usage.skills_used,
                    memories_extracted: 0,
                },
            });
        }

        let archive_num = next_archive_number(self)?;
        let archive_uri = self
            .session_uri()?
            .join(&format!("history/archive_{archive_num:03}"))?;
        self.fs.create_dir_all(&archive_uri, true)?;

        let archive_messages_uri = archive_uri.join("messages.jsonl")?;
        let messages_path = self.messages_path()?;
        let raw_messages = fs::read_to_string(&messages_path)?;
        self.fs.write(&archive_messages_uri, &raw_messages, true)?;
        fs::write(messages_path, "")?;

        let session_summary = summarize_messages(&active_messages);
        write_tiers(
            &self.fs,
            &archive_uri,
            &session_summary,
            &format!("# Archive {archive_num}\n\n{session_summary}"),
            true,
        )?;

        let session_uri = self.session_uri()?;
        write_tiers(
            &self.fs,
            &session_uri,
            &format!("Session {} latest commit", self.session_id),
            &format!("# Session Overview\n\nLatest archive: {archive_num}"),
            true,
        )?;

        let mut candidates_len = 0usize;
        let mut persisted_uris = Vec::new();
        if matches!(mode, CommitMode::ArchiveAndExtract) {
            let extracted =
                extract_memories_for_commit(&active_messages, &self.config.memory.extractor)?;
            if let Some(error) = extracted.llm_error.as_deref() {
                record_memory_extractor_fallback_event(self, &extracted.mode_requested, error);
            }

            let candidates = self.resolve_memory_candidates(&extracted.memories)?;
            candidates_len = candidates.len();
            for candidate in &candidates {
                let uri = self.persist_memory(candidate)?;
                persisted_uris.push(uri);
            }
            self.reindex_memory_uris(&persisted_uris)?;
        }

        self.touch_meta(|meta| {
            meta.updated_at = Utc::now();
        })?;

        Ok(CommitResult {
            session_id: self.session_id.clone(),
            status: "committed".to_string(),
            memories_extracted: candidates_len,
            active_count_updated: persisted_uris.len(),
            archived: true,
            stats: CommitStats {
                total_turns,
                contexts_used: meta.context_usage.contexts_used,
                skills_used: meta.context_usage.skills_used,
                memories_extracted: candidates_len,
            },
        })
    }

    pub fn promote_memories(
        &self,
        request: &MemoryPromotionRequest,
    ) -> Result<MemoryPromotionResult> {
        promote_memories_flow(
            self,
            request,
            Session::apply_promotion_all_or_nothing,
            Session::apply_promotion_best_effort,
        )
    }

    fn apply_promotion_all_or_nothing(
        &self,
        checkpoint_id: &str,
        facts: &[MemoryPromotionFact],
    ) -> Result<MemoryPromotionResult> {
        apply_promotion_all_or_nothing_mode(
            self,
            checkpoint_id,
            facts,
            record_memory_dedup_fallback_event,
        )
    }

    fn apply_promotion_best_effort(
        &self,
        checkpoint_id: &str,
        facts: &[MemoryPromotionFact],
    ) -> Result<MemoryPromotionResult> {
        apply_promotion_best_effort_mode(
            self,
            checkpoint_id,
            facts,
            record_memory_dedup_fallback_event,
        )
    }

    fn persist_memory(&self, candidate: &ResolvedMemoryCandidate) -> Result<AxiomUri> {
        persist_memory_write_path(self, candidate)
    }

    fn reindex_memory_uris(&self, uris: &[AxiomUri]) -> Result<()> {
        reindex_memory_uris_write_path(self, uris)
    }
}
