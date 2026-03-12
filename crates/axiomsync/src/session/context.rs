use std::fs;

use crate::error::Result;
use crate::jsonl::{jsonl_all_lines_invalid, parse_jsonl_tolerant};
use crate::models::{Message, SearchContext};

use super::Session;
use super::archive::read_relevant_archive_messages;

impl Session {
    pub fn get_context_for_search(
        &self,
        query: &str,
        max_archives: usize,
        max_messages: usize,
    ) -> Result<SearchContext> {
        if max_messages == 0 {
            return Ok(SearchContext {
                session_id: self.session_id.clone(),
                recent_messages: Vec::new(),
            });
        }

        let active_messages = self.read_messages()?;
        let archive_budget = max_messages.saturating_sub(active_messages.len());
        let mut archive_messages = if max_archives == 0 || archive_budget == 0 {
            Vec::new()
        } else {
            read_relevant_archive_messages(self, query, max_archives, archive_budget)?
        };

        archive_messages.extend(active_messages);
        if archive_messages.len() > max_messages {
            archive_messages = archive_messages[archive_messages.len() - max_messages..].to_vec();
        }

        Ok(SearchContext {
            session_id: self.session_id.clone(),
            recent_messages: archive_messages,
        })
    }

    pub(super) fn read_messages(&self) -> Result<Vec<Message>> {
        let path = self.messages_path()?;
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&path)?;
        let parsed = parse_jsonl_tolerant::<Message>(&content);
        if parsed.items.is_empty() && parsed.skipped_lines > 0 {
            return Err(jsonl_all_lines_invalid(
                "session messages",
                Some(path.to_string_lossy().as_ref()),
                parsed.skipped_lines,
                parsed.first_error.as_ref(),
            ));
        }
        Ok(parsed.items)
    }
}
