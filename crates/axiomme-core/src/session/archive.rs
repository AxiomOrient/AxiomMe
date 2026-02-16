use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::jsonl::{jsonl_all_lines_invalid, parse_jsonl_tolerant};
use crate::models::Message;

use super::Session;

pub(super) fn summarize_messages(messages: &[Message]) -> String {
    let first_user = messages
        .iter()
        .find(|m| m.role == "user")
        .map_or("(none)", |m| m.text.as_str());
    let last_assistant = messages
        .iter()
        .rev()
        .find(|m| m.role == "assistant")
        .map_or("(none)", |m| m.text.as_str());

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

pub(super) fn read_relevant_archive_messages(
    session: &Session,
    query: &str,
    max_archives: usize,
    max_messages: usize,
) -> Result<Vec<Message>> {
    if max_archives == 0 || max_messages == 0 {
        return Ok(Vec::new());
    }

    let archive_paths = list_archive_paths(session)?;
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

pub(super) fn next_archive_number(session: &Session) -> Result<u32> {
    let history_uri = session.session_uri()?.join("history")?;
    let history_path = session.fs.resolve_uri(&history_uri);
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

fn list_archive_paths(session: &Session) -> Result<Vec<(u32, PathBuf)>> {
    let history_uri = session.session_uri()?.join("history")?;
    let history_path = session.fs.resolve_uri(&history_uri);
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

fn read_messages_jsonl(path: &Path) -> Result<Vec<Message>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    let parsed = parse_jsonl_tolerant::<Message>(&content);
    if parsed.items.is_empty() && parsed.skipped_lines > 0 {
        return Err(jsonl_all_lines_invalid(
            "archive messages",
            Some(path.to_string_lossy().as_ref()),
            parsed.skipped_lines,
            parsed.first_error.as_ref(),
        ));
    }
    Ok(parsed.items)
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

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max).collect::<String>() + "..."
}
