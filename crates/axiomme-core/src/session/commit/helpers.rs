use crate::models::{MemoryCandidate, Message};

pub(crate) fn stable_text_key(text: &str) -> String {
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

pub(crate) fn build_memory_key(category: &str, text: &str) -> String {
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

pub(crate) fn normalize_memory_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn slugify(input: &str) -> String {
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

pub(crate) fn extract_memories_heuristically(messages: &[Message]) -> Vec<MemoryCandidate> {
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
