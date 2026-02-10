use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, RwLock};

use chrono::Utc;
use uuid::Uuid;

use crate::error::Result;
use crate::fs::LocalContextFs;
use crate::index::InMemoryHybridIndex;
use crate::models::{
    CommitResult, CommitStats, ContextUsage, IndexRecord, MemoryCandidate, Message, SearchContext,
    SessionMeta,
};
use crate::qdrant::QdrantMirror;
use crate::state::SqliteStateStore;
use crate::uri::{AxiomUri, Scope};

#[derive(Clone)]
pub struct Session {
    pub session_id: String,
    fs: LocalContextFs,
    state: SqliteStateStore,
    index: Arc<RwLock<InMemoryHybridIndex>>,
    qdrant: Option<QdrantMirror>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("session_id", &self.session_id)
            .finish_non_exhaustive()
    }
}

impl Session {
    pub fn new(
        session_id: impl Into<String>,
        fs: LocalContextFs,
        state: SqliteStateStore,
        index: Arc<RwLock<InMemoryHybridIndex>>,
        qdrant: Option<QdrantMirror>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            fs,
            state,
            index,
            qdrant,
        }
    }

    pub fn load(&self) -> Result<()> {
        let uri = self.session_uri()?;
        self.fs.create_dir_all(&uri, true)?;

        let messages_path = self.messages_path();
        if !messages_path.exists() {
            fs::write(&messages_path, "")?;
        }

        let meta_path = self.meta_path();
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

        let rel_path = self.relations_path();
        if !rel_path.exists() {
            fs::write(rel_path, "[]")?;
        }

        self.fs.write_tiers(
            &uri,
            &format!("Session {}", self.session_id),
            "# Session Overview\n\nNo messages yet.",
        )?;

        Ok(())
    }

    pub fn add_message(&self, role: &str, text: impl Into<String>) -> Result<Message> {
        let message = Message {
            id: Uuid::new_v4().to_string(),
            role: role.to_string(),
            text: text.into(),
            created_at: Utc::now(),
        };

        let line = serde_json::to_string(&message)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.messages_path())?;
        writeln!(file, "{}", line)?;

        self.touch_meta(|meta| {
            meta.updated_at = Utc::now();
        })?;

        Ok(message)
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
            "tool_update message_id={} tool_id={} status={}\n{}",
            message_id, tool_id, suffix, output
        );
        self.add_message("tool", text)?;
        Ok(())
    }

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
            self.read_relevant_archive_messages(query, max_archives, archive_budget)?
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

    pub fn commit(&self) -> Result<CommitResult> {
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

        let archive_num = self.next_archive_number()?;
        let archive_uri = self
            .session_uri()?
            .join(&format!("history/archive_{archive_num:03}"))?;
        self.fs.create_dir_all(&archive_uri, true)?;

        let archive_messages_uri = archive_uri.join("messages.jsonl")?;
        let raw_messages = fs::read_to_string(self.messages_path())?;
        self.fs.write(&archive_messages_uri, &raw_messages, true)?;
        fs::write(self.messages_path(), "")?;

        let session_summary = summarize_messages(&active_messages);
        self.fs.write_tiers(
            &archive_uri,
            &session_summary,
            &format!("# Archive {}\n\n{}", archive_num, session_summary),
        )?;

        let session_uri = self.session_uri()?;
        self.fs.write_tiers(
            &session_uri,
            &format!("Session {} latest commit", self.session_id),
            &format!("# Session Overview\n\nLatest archive: {}", archive_num),
        )?;

        let candidates = extract_memories(&active_messages);
        let mut persisted_uris = Vec::new();
        for candidate in &candidates {
            let uri = self.persist_memory(candidate)?;
            persisted_uris.push(uri);
        }

        self.reindex_memory_uris(&persisted_uris)?;

        self.touch_meta(|meta| {
            meta.updated_at = Utc::now();
        })?;

        Ok(CommitResult {
            session_id: self.session_id.clone(),
            status: "committed".to_string(),
            memories_extracted: candidates.len(),
            active_count_updated: persisted_uris.len(),
            archived: true,
            stats: CommitStats {
                total_turns,
                contexts_used: meta.context_usage.contexts_used,
                skills_used: meta.context_usage.skills_used,
                memories_extracted: candidates.len(),
            },
        })
    }

    fn read_messages(&self) -> Result<Vec<Message>> {
        let path = self.messages_path();
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(path)?;
        let mut out = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            out.push(serde_json::from_str::<Message>(line)?);
        }
        Ok(out)
    }

    fn read_relevant_archive_messages(
        &self,
        query: &str,
        max_archives: usize,
        max_messages: usize,
    ) -> Result<Vec<Message>> {
        if max_archives == 0 || max_messages == 0 {
            return Ok(Vec::new());
        }

        let archive_paths = self.list_archive_paths()?;
        if archive_paths.is_empty() {
            return Ok(Vec::new());
        }

        let query_terms = query_terms(query);
        let mut archives = Vec::<ArchiveMatch>::new();
        for (archive_num, archive_path) in archive_paths {
            let messages = read_messages_jsonl(&archive_path.join("messages.jsonl"))?;
            if messages.is_empty() {
                continue;
            }
            let score = messages
                .iter()
                .map(|msg| message_relevance(&msg.text, &query_terms))
                .max()
                .unwrap_or(0);
            archives.push(ArchiveMatch {
                archive_num,
                score,
                messages,
            });
        }

        if archives.is_empty() {
            return Ok(Vec::new());
        }

        archives.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| b.archive_num.cmp(&a.archive_num))
        });
        if !query_terms.is_empty() && archives.iter().any(|x| x.score > 0) {
            archives.retain(|x| x.score > 0);
        }
        archives.truncate(max_archives);

        let mut candidates = Vec::<RankedMessage>::new();
        for archive in archives {
            for message in archive.messages {
                candidates.push(RankedMessage {
                    score: message_relevance(&message.text, &query_terms),
                    archive_num: archive.archive_num,
                    message,
                });
            }
        }

        candidates.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| b.archive_num.cmp(&a.archive_num))
                .then_with(|| b.message.created_at.cmp(&a.message.created_at))
        });
        if !query_terms.is_empty() && candidates.iter().any(|x| x.score > 0) {
            candidates.retain(|x| x.score > 0);
        }
        candidates.truncate(max_messages);
        candidates.sort_by(|a, b| {
            a.score
                .cmp(&b.score)
                .then_with(|| a.message.created_at.cmp(&b.message.created_at))
        });

        Ok(candidates.into_iter().map(|x| x.message).collect())
    }

    fn list_archive_paths(&self) -> Result<Vec<(u32, std::path::PathBuf)>> {
        let history_uri = self.session_uri()?.join("history")?;
        let history_path = self.fs.resolve_uri(&history_uri);
        if !history_path.exists() {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        for entry in fs::read_dir(history_path)? {
            let entry = entry?;
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(raw) = name.strip_prefix("archive_")
                && let Ok(value) = raw.parse::<u32>()
            {
                out.push((value, entry.path()));
            }
        }
        out.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(out)
    }

    fn persist_memory(&self, candidate: &MemoryCandidate) -> Result<AxiomUri> {
        let (scope, base_path) = match candidate.category.as_str() {
            "profile" => (Scope::User, "memories/profile.md".to_string()),
            "preferences" => (
                Scope::User,
                format!("memories/preferences/{}.md", slugify(&candidate.key)),
            ),
            "entities" => (
                Scope::User,
                format!("memories/entities/{}.md", slugify(&candidate.key)),
            ),
            "events" => (
                Scope::User,
                format!("memories/events/{}.md", slugify(&candidate.key)),
            ),
            "cases" => (
                Scope::Agent,
                format!("memories/cases/{}.md", slugify(&candidate.key)),
            ),
            _ => (
                Scope::Agent,
                format!("memories/patterns/{}.md", slugify(&candidate.key)),
            ),
        };

        let uri = AxiomUri::root(scope).join(&base_path)?;
        let path = self.fs.resolve_uri(&uri);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let current = if path.exists() {
            fs::read_to_string(&path)?
        } else {
            String::new()
        };
        let source = MemorySource {
            session_id: self.session_id.clone(),
            message_id: candidate.source_message_id.clone(),
        };
        let merged = merge_memory_markdown(&current, candidate, &source);
        fs::write(path, merged)?;

        self.state.enqueue(
            "upsert",
            &uri.to_string(),
            serde_json::json!({"category": candidate.category}),
        )?;

        Ok(uri)
    }

    fn reindex_memory_uris(&self, uris: &[AxiomUri]) -> Result<()> {
        let mut index = self
            .index
            .write()
            .map_err(|_| crate::error::AxiomError::Internal("index lock poisoned".to_string()))?;

        for uri in uris {
            if let Some(parent) = uri.parent() {
                ensure_directory_record(&self.fs, &mut index, &parent)?;
                if let Some(record) = index.get(&parent.to_string()).cloned() {
                    self.state.upsert_search_document(&record)?;
                    self.try_mirror_upsert(&record)?;
                }
            }
            if uri.to_string().ends_with(".md") {
                let text = self.fs.read(uri)?;
                let parent_uri = uri.parent().map(|u| u.to_string());
                let record = crate::models::IndexRecord {
                    id: Uuid::new_v4().to_string(),
                    uri: uri.to_string(),
                    parent_uri,
                    is_leaf: true,
                    context_type: "memory".to_string(),
                    name: uri.last_segment().unwrap_or("memory").to_string(),
                    abstract_text: text.lines().next().unwrap_or_default().to_string(),
                    content: text,
                    tags: vec!["memory".to_string()],
                    updated_at: Utc::now(),
                    depth: uri.segments().len(),
                };
                index.upsert(record.clone());
                self.state.upsert_search_document(&record)?;
                self.try_mirror_upsert(&record)?;
            }
        }

        Ok(())
    }

    fn try_mirror_upsert(&self, record: &IndexRecord) -> Result<()> {
        let Some(qdrant) = &self.qdrant else {
            return Ok(());
        };

        if let Err(err) = qdrant.upsert_record(record) {
            let id = self.state.enqueue(
                "qdrant_upsert_failed",
                &record.uri,
                serde_json::json!({
                    "error": err.to_string(),
                    "source": "session.reindex_memory_uris"
                }),
            )?;
            self.state.mark_outbox_status(id, "dead_letter", true)?;
        }

        Ok(())
    }

    fn next_archive_number(&self) -> Result<u32> {
        let history_uri = self.session_uri()?.join("history")?;
        let history_path = self.fs.resolve_uri(&history_uri);
        if !history_path.exists() {
            fs::create_dir_all(&history_path)?;
            return Ok(1);
        }

        let mut max_num = 0u32;
        for entry in fs::read_dir(history_path)? {
            let entry = entry?;
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(raw) = name.strip_prefix("archive_")
                && let Ok(value) = raw.parse::<u32>()
            {
                max_num = max_num.max(value);
            }
        }

        Ok(max_num + 1)
    }

    fn session_uri(&self) -> Result<AxiomUri> {
        AxiomUri::root(Scope::Session).join(&self.session_id)
    }

    fn messages_path(&self) -> std::path::PathBuf {
        self.fs
            .resolve_uri(&self.session_uri().expect("session uri"))
            .join("messages.jsonl")
    }

    fn meta_path(&self) -> std::path::PathBuf {
        self.fs
            .resolve_uri(&self.session_uri().expect("session uri"))
            .join(".meta.json")
    }

    fn relations_path(&self) -> std::path::PathBuf {
        self.fs
            .resolve_uri(&self.session_uri().expect("session uri"))
            .join(".relations.json")
    }

    fn read_meta(&self) -> Result<SessionMeta> {
        let content = fs::read_to_string(self.meta_path())?;
        Ok(serde_json::from_str(&content)?)
    }

    fn touch_meta<F>(&self, mutate: F) -> Result<()>
    where
        F: FnOnce(&mut SessionMeta),
    {
        let mut meta = self.read_meta()?;
        mutate(&mut meta);
        fs::write(self.meta_path(), serde_json::to_string_pretty(&meta)?)?;
        Ok(())
    }
}

fn summarize_messages(messages: &[Message]) -> String {
    let first_user = messages
        .iter()
        .find(|m| m.role == "user")
        .map(|m| m.text.as_str())
        .unwrap_or("(none)");
    let last_assistant = messages
        .iter()
        .rev()
        .find(|m| m.role == "assistant")
        .map(|m| m.text.as_str())
        .unwrap_or("(none)");

    format!(
        "Session summary: user asked '{}', latest assistant response '{}'",
        truncate(first_user, 120),
        truncate(last_assistant, 120)
    )
}

#[derive(Debug)]
struct ArchiveMatch {
    archive_num: u32,
    score: usize,
    messages: Vec<Message>,
}

#[derive(Debug)]
struct RankedMessage {
    score: usize,
    archive_num: u32,
    message: Message,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemorySource {
    session_id: String,
    message_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemoryEntry {
    text: String,
    sources: Vec<MemorySource>,
}

fn read_messages_jsonl(path: &Path) -> Result<Vec<Message>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        out.push(serde_json::from_str::<Message>(line)?);
    }
    Ok(out)
}

fn query_terms(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|x| x.len() >= 2)
        .map(ToString::to_string)
        .collect::<Vec<_>>()
}

fn message_relevance(text: &str, terms: &[String]) -> usize {
    if terms.is_empty() {
        return 0;
    }
    let text = text.to_lowercase();
    terms
        .iter()
        .filter(|term| text.contains(term.as_str()))
        .count()
}

fn extract_memories(messages: &[Message]) -> Vec<MemoryCandidate> {
    let mut out = Vec::new();
    for msg in messages {
        let lower = msg.text.to_lowercase();
        let is_user = msg.role == "user";
        let key_suffix = stable_text_key(&msg.text);

        if is_user && is_profile_message(&lower, &msg.text) {
            out.push(MemoryCandidate {
                category: "profile".to_string(),
                key: "profile".to_string(),
                text: msg.text.clone(),
                source_message_id: msg.id.clone(),
            });
        }

        if is_user && is_preference_message(&lower, &msg.text) {
            out.push(MemoryCandidate {
                category: "preferences".to_string(),
                key: format!("pref-{key_suffix}"),
                text: msg.text.clone(),
                source_message_id: msg.id.clone(),
            });
        }

        if is_user && is_entity_message(&lower, &msg.text) {
            out.push(MemoryCandidate {
                category: "entities".to_string(),
                key: format!("entity-{key_suffix}"),
                text: msg.text.clone(),
                source_message_id: msg.id.clone(),
            });
        }

        if is_event_message(&lower, &msg.text) {
            out.push(MemoryCandidate {
                category: "events".to_string(),
                key: format!("event-{key_suffix}"),
                text: msg.text.clone(),
                source_message_id: msg.id.clone(),
            });
        }

        if is_case_message(&lower, &msg.text) {
            out.push(MemoryCandidate {
                category: "cases".to_string(),
                key: format!("case-{key_suffix}"),
                text: msg.text.clone(),
                source_message_id: msg.id.clone(),
            });
        }

        if is_pattern_message(&lower, &msg.text) {
            out.push(MemoryCandidate {
                category: "patterns".to_string(),
                key: format!("pattern-{key_suffix}"),
                text: msg.text.clone(),
                source_message_id: msg.id.clone(),
            });
        }
    }
    out
}

fn stable_text_key(text: &str) -> String {
    let normalized = text
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in normalized.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{hash:016x}")[..12].to_string()
}

fn merge_memory_markdown(
    existing: &str,
    candidate: &MemoryCandidate,
    source: &MemorySource,
) -> String {
    let mut entries = parse_memory_entries(existing);
    if let Some(entry) = entries
        .iter_mut()
        .find(|entry| entry.text == candidate.text)
    {
        if !entry.sources.iter().any(|item| item == source) {
            entry.sources.push(source.clone());
        }
    } else {
        entries.push(MemoryEntry {
            text: candidate.text.clone(),
            sources: vec![source.clone()],
        });
    }

    normalize_memory_entries(&mut entries);
    render_memory_entries(&entries)
}

fn parse_memory_entries(content: &str) -> Vec<MemoryEntry> {
    let mut entries = Vec::new();
    let mut current: Option<MemoryEntry> = None;

    for line in content.lines() {
        if let Some(text) = line.strip_prefix("- ") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(MemoryEntry {
                text: text.trim().to_string(),
                sources: Vec::new(),
            });
            continue;
        }

        if let Some(source_line) = line.strip_prefix("  - source: session ")
            && let Some((session_id, message_id)) = source_line.split_once(" message ")
            && let Some(entry) = current.as_mut()
        {
            entry.sources.push(MemorySource {
                session_id: session_id.trim().to_string(),
                message_id: message_id.trim().to_string(),
            });
        }
    }

    if let Some(entry) = current {
        entries.push(entry);
    }

    entries
}

fn normalize_memory_entries(entries: &mut Vec<MemoryEntry>) {
    let mut normalized = Vec::<MemoryEntry>::new();
    for entry in entries.drain(..) {
        if let Some(existing) = normalized.iter_mut().find(|item| item.text == entry.text) {
            for source in entry.sources {
                if !existing.sources.iter().any(|item| item == &source) {
                    existing.sources.push(source);
                }
            }
        } else {
            normalized.push(entry);
        }
    }

    for entry in &mut normalized {
        entry.sources.sort_by(|a, b| {
            a.session_id
                .cmp(&b.session_id)
                .then_with(|| a.message_id.cmp(&b.message_id))
        });
        entry.sources.dedup();
    }

    *entries = normalized;
}

fn render_memory_entries(entries: &[MemoryEntry]) -> String {
    let mut out = String::new();
    for entry in entries {
        out.push_str("- ");
        out.push_str(entry.text.trim());
        out.push('\n');
        for source in &entry.sources {
            out.push_str("  - source: session ");
            out.push_str(source.session_id.trim());
            out.push_str(" message ");
            out.push_str(source.message_id.trim());
            out.push('\n');
        }
    }
    out
}

fn contains_any(text: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| text.contains(pattern))
}

fn is_profile_message(lower: &str, original: &str) -> bool {
    contains_any(lower, &["my name is", "i am ", "call me "]) || original.contains("내 이름")
}

fn is_preference_message(lower: &str, original: &str) -> bool {
    contains_any(
        lower,
        &[
            "prefer",
            "preference",
            "avoid",
            "i like",
            "i dislike",
            "i don't like",
        ],
    ) || contains_any(original, &["선호", "피해", "싫어", "좋아"])
}

fn is_entity_message(lower: &str, original: &str) -> bool {
    contains_any(lower, &["project", "repository", "repo", "service", "team"])
        || original.contains("프로젝트")
}

fn is_event_message(lower: &str, original: &str) -> bool {
    contains_any(
        lower,
        &[
            "today",
            "yesterday",
            "tomorrow",
            "incident",
            "outage",
            "deploy",
            "deployed",
            "release",
            "released",
            "meeting",
            "deadline",
            "milestone",
            "happened",
            "occurred",
            "failed at",
            "rolled back",
        ],
    ) || contains_any(
        original,
        &["오늘", "어제", "내일", "발생", "배포", "릴리스", "회의"],
    )
}

fn is_case_message(lower: &str, original: &str) -> bool {
    contains_any(
        lower,
        &[
            "root cause",
            "rca",
            "postmortem",
            "fixed",
            "resolved",
            "workaround",
            "repro",
            "reproduced",
            "solution",
            "solved",
            "debugged",
            "troubleshoot",
            "investigation",
        ],
    ) || contains_any(original, &["원인", "해결", "재현", "대응"])
}

fn is_pattern_message(lower: &str, original: &str) -> bool {
    contains_any(
        lower,
        &[
            "always",
            "never",
            "whenever",
            "if we",
            "if you",
            "checklist",
            "playbook",
            "rule",
            "guideline",
            "best practice",
            "pattern",
            "must",
            "should always",
        ],
    ) || contains_any(original, &["항상", "절대", "반드시", "체크리스트", "원칙"])
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if (c.is_whitespace() || c == '-' || c == '_') && !out.ends_with('-') {
            out.push('-');
        }
    }
    out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "item".to_string()
    } else {
        out
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max).collect::<String>() + "..."
}

fn ensure_directory_record(
    fs: &LocalContextFs,
    index: &mut InMemoryHybridIndex,
    uri: &AxiomUri,
) -> Result<()> {
    if index.get(&uri.to_string()).is_some() {
        return Ok(());
    }

    let path = fs.resolve_uri(uri);
    if !Path::new(&path).exists() {
        fs.create_dir_all(uri, true)?;
    }

    if !fs.abstract_path(uri).exists() || !fs.overview_path(uri).exists() {
        fs.write_tiers(
            uri,
            &format!(
                "Directory {}",
                uri.last_segment().unwrap_or(uri.scope().as_str())
            ),
            &format!("# Overview\n\nURI: {}", uri),
        )?;
    }

    let abstract_text = fs.read_abstract(uri).unwrap_or_else(|_| String::new());
    let overview_text = fs.read_overview(uri).unwrap_or_else(|_| String::new());

    index.upsert(crate::models::IndexRecord {
        id: Uuid::new_v4().to_string(),
        uri: uri.to_string(),
        parent_uri: uri.parent().map(|p| p.to_string()),
        is_leaf: false,
        context_type: if matches!(uri.scope(), Scope::User | Scope::Agent) {
            "memory".to_string()
        } else {
            "resource".to_string()
        },
        name: uri
            .last_segment()
            .unwrap_or(uri.scope().as_str())
            .to_string(),
        abstract_text,
        content: overview_text,
        tags: vec![],
        updated_at: Utc::now(),
        depth: uri.segments().len(),
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use chrono::Utc;
    use tempfile::tempdir;

    use super::*;

    fn fixture_categories(role: &str, text: &str) -> HashSet<String> {
        extract_memories(&[Message {
            id: "fixture-msg-001".to_string(),
            role: role.to_string(),
            text: text.to_string(),
            created_at: Utc::now(),
        }])
        .into_iter()
        .map(|candidate| candidate.category)
        .collect::<HashSet<_>>()
    }

    #[test]
    fn commit_extracts_preference_memory() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init failed");
        let state =
            SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
        let index = Arc::new(RwLock::new(InMemoryHybridIndex::new()));

        let session = Session::new("s1", fs.clone(), state, index, None);
        session.load().expect("load failed");
        session
            .add_message("user", "I prefer concise Rust code.")
            .expect("append failed");
        let result = session.commit().expect("commit failed");

        assert!(result.archived);
        assert!(result.memories_extracted >= 1);

        let pref_uri = AxiomUri::parse("axiom://user/memories/preferences/pref-item.md")
            .unwrap_or_else(|_| AxiomUri::parse("axiom://user/memories/preferences").expect("uri"));
        let pref_parent = pref_uri
            .parent()
            .unwrap_or_else(|| AxiomUri::parse("axiom://user/memories/preferences").expect("uri2"));
        assert!(fs.exists(&pref_parent));
    }

    #[test]
    fn context_for_search_includes_relevant_archive_messages() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init failed");
        let state =
            SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
        let index = Arc::new(RwLock::new(InMemoryHybridIndex::new()));

        let session = Session::new("s-archive", fs.clone(), state, index, None);
        session.load().expect("load failed");
        session
            .add_message("user", "OAuth refresh token strategy")
            .expect("append failed");
        session.commit().expect("commit failed");

        let no_archive = session
            .get_context_for_search("oauth", 0, 8)
            .expect("ctx without archive");
        assert!(no_archive.recent_messages.is_empty());

        let with_archive = session
            .get_context_for_search("oauth", 1, 8)
            .expect("ctx with archive");
        assert!(
            with_archive
                .recent_messages
                .iter()
                .any(|m| m.text.contains("OAuth refresh token strategy"))
        );
    }

    #[test]
    fn context_for_search_uses_archive_relevance_not_only_recency() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init failed");
        let state =
            SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
        let index = Arc::new(RwLock::new(InMemoryHybridIndex::new()));

        let session = Session::new("s-archive-rank", fs.clone(), state, index, None);
        session.load().expect("load failed");

        session
            .add_message("user", "OAuth grant flow details")
            .expect("append");
        session.commit().expect("commit 1");

        session
            .add_message("user", "Kubernetes deployment note")
            .expect("append");
        session.commit().expect("commit 2");

        let ctx = session
            .get_context_for_search("oauth", 1, 8)
            .expect("context");
        assert!(
            ctx.recent_messages
                .iter()
                .any(|m| m.text.contains("OAuth grant flow details"))
        );
        assert!(
            !ctx.recent_messages
                .iter()
                .any(|m| m.text.contains("Kubernetes deployment note"))
        );
    }

    #[test]
    fn commit_extracts_six_categories_and_reindexes_immediately() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init failed");
        let state =
            SqliteStateStore::open(temp.path().join("state.db")).expect("state open failed");
        let index = Arc::new(RwLock::new(InMemoryHybridIndex::new()));

        let session = Session::new("s-all-categories", fs.clone(), state, index.clone(), None);
        session.load().expect("load failed");
        session
            .add_message("user", "My name is Axient")
            .expect("append profile");
        session
            .add_message("user", "I prefer concise Rust code")
            .expect("append preferences");
        session
            .add_message("user", "This project repository is AxiomMe")
            .expect("append entities");
        session
            .add_message("assistant", "Today we deployed release v1.2")
            .expect("append events");
        session
            .add_message(
                "assistant",
                "Root cause identified and fixed with workaround",
            )
            .expect("append cases");
        session
            .add_message("assistant", "Always run this checklist before release")
            .expect("append patterns");

        let result = session.commit().expect("commit failed");
        assert!(result.memories_extracted >= 6);

        let records = index
            .read()
            .expect("index read")
            .all_records()
            .into_iter()
            .map(|record| record.uri)
            .collect::<Vec<_>>();

        assert!(
            records
                .iter()
                .any(|uri| uri == "axiom://user/memories/profile.md")
        );
        assert!(
            records
                .iter()
                .any(|uri| uri.starts_with("axiom://user/memories/preferences/"))
        );
        assert!(
            records
                .iter()
                .any(|uri| uri.starts_with("axiom://user/memories/entities/"))
        );
        assert!(
            records
                .iter()
                .any(|uri| uri.starts_with("axiom://user/memories/events/"))
        );
        assert!(
            records
                .iter()
                .any(|uri| uri.starts_with("axiom://agent/memories/cases/"))
        );
        assert!(
            records
                .iter()
                .any(|uri| uri.starts_with("axiom://agent/memories/patterns/"))
        );
    }

    #[test]
    fn commit_merges_same_memory_with_provenance_across_sessions() {
        let temp = tempdir().expect("tempdir");
        let fs = LocalContextFs::new(temp.path());
        fs.initialize().expect("init failed");

        let state_one =
            SqliteStateStore::open(temp.path().join("state-one.db")).expect("state one open");
        let index_one = Arc::new(RwLock::new(InMemoryHybridIndex::new()));
        let session_one = Session::new("s-merge-1", fs.clone(), state_one, index_one, None);
        session_one.load().expect("load one");
        session_one
            .add_message("user", "I prefer concise Rust code")
            .expect("append one");
        session_one.commit().expect("commit one");

        let state_two =
            SqliteStateStore::open(temp.path().join("state-two.db")).expect("state two open");
        let index_two = Arc::new(RwLock::new(InMemoryHybridIndex::new()));
        let session_two = Session::new("s-merge-2", fs.clone(), state_two, index_two, None);
        session_two.load().expect("load two");
        session_two
            .add_message("user", "I prefer concise Rust code")
            .expect("append two");
        session_two.commit().expect("commit two");

        let key = stable_text_key("I prefer concise Rust code");
        let uri = AxiomUri::parse(&format!("axiom://user/memories/preferences/pref-{key}.md"))
            .expect("uri");
        let content = fs.read(&uri).expect("read merged memory");

        assert_eq!(content.matches("- I prefer concise Rust code").count(), 1);
        assert!(content.contains("source: session s-merge-1"));
        assert!(content.contains("source: session s-merge-2"));
    }

    #[test]
    fn extract_memories_uses_stable_key_for_same_text() {
        let messages = vec![
            Message {
                id: "msg-1".to_string(),
                role: "user".to_string(),
                text: "I prefer concise Rust code".to_string(),
                created_at: Utc::now(),
            },
            Message {
                id: "msg-2".to_string(),
                role: "user".to_string(),
                text: "I prefer concise Rust code".to_string(),
                created_at: Utc::now(),
            },
        ];

        let keys = extract_memories(&messages)
            .into_iter()
            .filter(|candidate| candidate.category == "preferences")
            .map(|candidate| candidate.key)
            .collect::<HashSet<_>>();

        assert_eq!(keys.len(), 1);
    }

    #[test]
    fn extract_memories_fixture_profile_category() {
        let categories = fixture_categories("user", "My name is Axient and I build tools.");
        assert!(categories.contains("profile"));
    }

    #[test]
    fn extract_memories_fixture_preferences_category() {
        let categories = fixture_categories("user", "I prefer concise Rust code and avoid magic.");
        assert!(categories.contains("preferences"));
    }

    #[test]
    fn extract_memories_fixture_entities_category() {
        let categories = fixture_categories("user", "This project repository is AxiomMe.");
        assert!(categories.contains("entities"));
    }

    #[test]
    fn extract_memories_fixture_events_category() {
        let categories = fixture_categories("assistant", "Today we deployed and rolled back once.");
        assert!(categories.contains("events"));
    }

    #[test]
    fn extract_memories_fixture_cases_category() {
        let categories =
            fixture_categories("assistant", "Root cause found and fixed with workaround.");
        assert!(categories.contains("cases"));
    }

    #[test]
    fn extract_memories_fixture_patterns_category() {
        let categories =
            fixture_categories("assistant", "Always run this checklist before release.");
        assert!(categories.contains("patterns"));
    }
}
