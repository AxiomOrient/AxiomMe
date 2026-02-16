use std::collections::HashSet;

pub(super) fn merge_activated_message_ids(
    existing: &[String],
    activated: &[String],
) -> Vec<String> {
    let mut merged = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();
    for id in existing.iter().chain(activated.iter()) {
        if id.trim().is_empty() {
            continue;
        }
        if seen.insert(id.clone()) {
            merged.push(id.clone());
        }
    }
    merged
}

pub(super) fn normalize_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut needs_space = false;

    for part in text.split_whitespace() {
        if needs_space {
            out.push(' ');
        }
        out.push_str(part);
        needs_space = true;
    }

    out
}
