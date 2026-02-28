use std::fs;
use std::io::Write;

use chrono::Utc;
use uuid::Uuid;

use crate::error::Result;
use crate::models::{ContextUsage, Message, SessionMeta};
use crate::om::plan_process_output_result;
use crate::tier_documents::{abstract_path, overview_path, write_tiers};

use super::Session;

impl Session {
    pub fn load(&self) -> Result<()> {
        let uri = self.session_uri()?;
        self.fs.create_dir_all(&uri, true)?;

        let messages_path = self.messages_path()?;
        if !messages_path.exists() {
            fs::write(&messages_path, "")?;
        }

        let meta_path = self.meta_path()?;
        if !meta_path.exists() {
            let now = Utc::now();
            let meta = SessionMeta {
                session_id: self.session_id.clone(),
                created_at: now,
                updated_at: now,
                context_usage: ContextUsage::default(),
            };
            fs::write(meta_path, serde_json::to_string_pretty(&meta)?)?;
        }

        let rel_path = self.relations_path()?;
        if !rel_path.exists() {
            fs::write(rel_path, "[]")?;
        }

        let abstract_path = abstract_path(&self.fs, &uri);
        let overview_path = overview_path(&self.fs, &uri);
        if !abstract_path.exists() || !overview_path.exists() {
            write_tiers(
                &self.fs,
                &uri,
                &format!("Session {}", self.session_id),
                "# Session Overview\n\nNo messages yet.",
                true,
            )?;
        }

        Ok(())
    }

    pub fn add_message(&self, role: &str, text: impl Into<String>) -> Result<Message> {
        let message = Message {
            id: Uuid::new_v4().to_string(),
            role: role.to_string(),
            text: text.into(),
            created_at: Utc::now(),
        };
        let _ = self.persist_output_stage_messages(std::slice::from_ref(&message), false)?;

        Ok(message)
    }

    pub(super) fn persist_output_stage_messages(
        &self,
        messages: &[Message],
        read_only: bool,
    ) -> Result<usize> {
        let output_plan = plan_process_output_result(read_only, messages.len());
        if !output_plan.should_save_unsaved_messages {
            return Ok(0);
        }

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.messages_path()?)?;
        for message in messages {
            let line = serde_json::to_string(message)?;
            writeln!(file, "{line}")?;
        }

        self.touch_meta(|meta| {
            meta.updated_at = Utc::now();
        })?;

        // OM integration is best-effort to preserve append semantics, but failures
        // are persisted as dead-letter diagnostics for operational visibility.
        for message in messages {
            if let Err(err) = self.update_observational_memory_on_message_write(message) {
                self.record_observer_failure(&err);
            }
        }

        Ok(messages.len())
    }

    pub fn used(&self, contexts: Option<usize>, skill: Option<&str>) -> Result<()> {
        self.touch_meta(|meta| {
            if let Some(count) = contexts {
                meta.context_usage.contexts_used += count;
            }
            if skill.is_some() {
                meta.context_usage.skills_used += 1;
            }
            meta.updated_at = Utc::now();
        })
    }

    pub fn update_tool_part(
        &self,
        message_id: &str,
        tool_id: &str,
        output: &str,
        status: Option<&str>,
    ) -> Result<()> {
        let suffix = status.unwrap_or("done");
        let text = format!(
            "tool_update message_id={message_id} tool_id={tool_id} status={suffix}\n{output}"
        );
        self.add_message("tool", text)?;
        Ok(())
    }
}
