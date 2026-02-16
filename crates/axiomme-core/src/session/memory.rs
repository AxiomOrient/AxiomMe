use crate::models::{MemoryCandidate, Message};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MemorySource {
    pub session_id: String,
    pub message_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MemoryEntry {
    pub text: String,
    pub sources: Vec<MemorySource>,
}

pub(super) fn extract_memories(messages: &[Message]) -> Vec<MemoryCandidate> {
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

pub(super) fn stable_text_key(text: &str) -> String {
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

pub(super) fn build_memory_key(category: &str, text: &str) -> String {
    let suffix = stable_text_key(text);
    match category {
        "profile" => "profile".to_string(),
        "preferences" => format!("pref-{suffix}"),
        "entities" => format!("entity-{suffix}"),
        "events" => format!("event-{suffix}"),
        "cases" => format!("case-{suffix}"),
        _ => format!("pattern-{suffix}"),
    }
}

pub(super) fn normalize_memory_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn merge_memory_markdown(
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

pub(super) fn parse_memory_entries(content: &str) -> Vec<MemoryEntry> {
    let mut entries = Vec::new();
    let mut current: Option<MemoryEntry> = None;

    for line in content.lines() {
        if let Some(text) = line.strip_prefix("- ") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(MemoryEntry {
                text: normalize_memory_text(text),
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
        out.push_str(&normalize_memory_text(&entry.text));
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

pub(super) fn slugify(input: &str) -> String {
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
