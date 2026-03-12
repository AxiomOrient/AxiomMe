use std::collections::HashSet;

use crate::inference::OmPendingMessage;
use crate::model::{
    ContinuationPolicyV2, OmDeterministicEvidence, OmDeterministicEvidenceKind,
    OmDeterministicObserverResponseV2,
};

use super::super::helpers::{estimate_text_tokens, normalize_whitespace};

pub fn synthesize_observer_observations(
    active_observations: &str,
    pending_messages: &[OmPendingMessage],
    max_chars: usize,
) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let existing = active_observations
        .lines()
        .map(normalize_whitespace)
        .filter(|line| !line.is_empty())
        .collect::<HashSet<_>>();

    let mut seen = HashSet::<String>::new();
    let mut lines = Vec::<String>::new();

    for item in pending_messages {
        let role = normalize_whitespace(&item.role);
        let text = normalize_whitespace(&item.text);
        if text.is_empty() {
            continue;
        }
        let line = if role.is_empty() {
            text
        } else {
            format!("[{role}] {text}")
        };
        let normalized = normalize_whitespace(&line);
        if normalized.is_empty() || existing.contains(&normalized) || !seen.insert(normalized) {
            continue;
        }
        lines.push(line);
    }

    // Keep forward progress even when all candidates were deduplicated.
    if lines.is_empty()
        && let Some(fallback) = pending_messages.iter().find_map(|item| {
            let role = normalize_whitespace(&item.role);
            let text = normalize_whitespace(&item.text);
            if text.is_empty() {
                return None;
            }
            Some(if role.is_empty() {
                text
            } else {
                format!("[{role}] {text}")
            })
        })
    {
        lines.push(fallback);
    }

    lines.join("\n").chars().take(max_chars).collect::<String>()
}

fn message_role(message: &OmPendingMessage) -> String {
    normalize_whitespace(&message.role).to_ascii_lowercase()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LanguageProfile {
    Korean,
    Japanese,
    Chinese,
    Other,
}

fn detect_language_profile(text: &str) -> LanguageProfile {
    let mut has_hangul = false;
    let mut has_hiragana_or_katakana = false;
    let mut has_han = false;
    for ch in text.chars() {
        let code = ch as u32;
        if (0xAC00..=0xD7A3).contains(&code) || (0x1100..=0x11FF).contains(&code) {
            has_hangul = true;
        } else if (0x3040..=0x30FF).contains(&code) {
            has_hiragana_or_katakana = true;
        } else if (0x4E00..=0x9FFF).contains(&code) {
            has_han = true;
        }
    }
    if has_hangul {
        LanguageProfile::Korean
    } else if has_hiragana_or_katakana {
        LanguageProfile::Japanese
    } else if has_han {
        LanguageProfile::Chinese
    } else {
        LanguageProfile::Other
    }
}

fn has_english_task_signal(lower: &str) -> bool {
    lower.starts_with("please ")
        || [
            "fix",
            "add",
            "update",
            "implement",
            "investigate",
            "debug",
            "verify",
            "refactor",
            "remove",
            "write",
            "create",
            "check",
            "review",
            "test",
            "plan",
            "analyze",
        ]
        .iter()
        .any(|verb| {
            lower
                .split_whitespace()
                .any(|token| token.trim_matches(|c: char| !c.is_ascii_alphanumeric()) == *verb)
        })
}

fn has_korean_task_signal(text: &str, lower: &str) -> bool {
    [
        "해줘",
        "해주세요",
        "해 줘",
        "해 주세요",
        "부탁",
        "수정",
        "추가",
        "구현",
        "조사",
        "확인",
        "검토",
        "테스트",
        "삭제",
        "작성",
        "업데이트",
        "개선",
        "분석",
        "정리",
        "계획",
        "설계",
        "리팩터링",
        "디버그",
        "고쳐",
    ]
    .iter()
    .any(|marker| text.contains(marker) || lower.contains(marker))
}

fn has_japanese_task_signal(text: &str) -> bool {
    [
        "してください",
        "して下さい",
        "お願いします",
        "修正",
        "追加",
        "実装",
        "調査",
        "確認",
        "検証",
        "削除",
        "更新",
        "改善",
        "作成",
        "分析",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

fn has_chinese_task_signal(text: &str) -> bool {
    [
        "请", "請", "修复", "修復", "添加", "实现", "實現", "调查", "調查", "确认", "確認", "检查",
        "檢查", "测试", "測試", "删除", "刪除", "更新", "改进", "改進", "分析",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

fn has_question_task_signal(text: &str, lower: &str) -> bool {
    if !(lower.contains('?') || text.contains('？')) {
        return false;
    }

    let trimmed = lower.trim_start();
    if [
        "what ", "why ", "how ", "when ", "where ", "which ", "who ", "can ", "could ", "would ",
        "will ", "should ", "is ", "are ", "do ", "does ", "did ",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
    {
        return true;
    }

    if [
        "can you",
        "could you",
        "would you",
        "will you",
        "please",
        "help",
    ]
    .iter()
    .any(|cue| lower.contains(cue))
    {
        return true;
    }

    if [
        "가능",
        "할 수",
        "해줄",
        "해 줄",
        "어떻게",
        "왜",
        "무엇",
        "뭐",
        "언제",
        "어디",
        "나요",
        "까요",
        "습니까",
    ]
    .iter()
    .any(|cue| text.contains(cue) || lower.contains(cue))
    {
        return true;
    }

    if [
        "ですか",
        "ますか",
        "でしょうか",
        "かな",
        "できますか",
        "どう",
        "なぜ",
        "何",
        "いつ",
        "どこ",
    ]
    .iter()
    .any(|cue| text.contains(cue))
    {
        return true;
    }

    if [
        "吗",
        "嗎",
        "呢",
        "如何",
        "怎么",
        "怎麼",
        "为什么",
        "為什麼",
        "可以",
        "能否",
        "何时",
        "何時",
        "哪里",
        "哪裡",
        "请问",
        "請問",
    ]
    .iter()
    .any(|cue| text.contains(cue))
    {
        return true;
    }

    false
}

fn has_task_signal(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if has_question_task_signal(text, &lower) {
        return true;
    }
    if has_english_task_signal(&lower) {
        return true;
    }
    match detect_language_profile(text) {
        LanguageProfile::Korean => has_korean_task_signal(text, &lower),
        LanguageProfile::Japanese => has_japanese_task_signal(text),
        LanguageProfile::Chinese => has_chinese_task_signal(text),
        LanguageProfile::Other => false,
    }
}

fn extract_task_phrase(text: &str) -> Option<String> {
    let normalized = normalize_whitespace(text);
    if normalized.is_empty() {
        return None;
    }
    if !has_task_signal(&normalized) {
        return None;
    }

    Some(normalized.chars().take(140).collect::<String>())
}

fn contains_error_signal(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("error")
        || lower.contains("failed")
        || lower.contains("exception")
        || lower.contains("panic")
        || lower.contains("timeout")
        || lower.contains("오류")
        || lower.contains("실패")
        || lower.contains("예외")
        || lower.contains("에러")
        || text.contains("エラー")
        || text.contains("失敗")
        || text.contains("例外")
        || text.contains("タイムアウト")
        || text.contains("错误")
        || text.contains("錯誤")
        || text.contains("失败")
        || text.contains("失敗")
        || text.contains("异常")
        || text.contains("異常")
        || text.contains("超时")
}

fn is_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':' | '/' | '.')
}

fn identifier_like_spans(text: &str) -> Vec<&str> {
    let mut spans = Vec::<&str>::new();
    let mut start = None;

    for (index, ch) in text.char_indices() {
        if is_identifier_char(ch) {
            if start.is_none() {
                start = Some(index);
            }
            continue;
        }
        if let Some(begin) = start.take() {
            spans.push(&text[begin..index]);
        }
    }
    if let Some(begin) = start {
        spans.push(&text[begin..]);
    }

    spans
}

fn extract_error_identifier(text: &str) -> Option<String> {
    let tokens = identifier_like_spans(text)
        .into_iter()
        .map(|raw| {
            raw.trim_matches(|c: char| {
                !c.is_ascii_alphanumeric() && !matches!(c, '_' | '-' | ':' | '/' | '.')
            })
            .to_string()
        })
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();

    if let Some(token) = tokens.iter().find(|token| {
        token.starts_with('E')
            && token.len() > 2
            && token[1..].chars().all(|ch| ch.is_ascii_digit())
    }) {
        return Some(token.clone());
    }
    if let Some(token) = tokens.iter().find(|token| {
        token.contains('_') || token.contains("::") || token.contains('/') || token.contains(':')
    }) {
        return Some(token.clone());
    }
    tokens
        .iter()
        .find(|token| token.starts_with("ERR"))
        .cloned()
}

fn build_suggested_response(profile: LanguageProfile, task: &str) -> String {
    match profile {
        LanguageProfile::Korean => format!("사용자 요청에 응답: {task}"),
        LanguageProfile::Japanese => format!("ユーザー要求に対応: {task}"),
        LanguageProfile::Chinese => format!("回应用户请求: {task}"),
        LanguageProfile::Other => format!("Respond to user request: {task}"),
    }
}

fn build_error_recovery_response(
    profile: LanguageProfile,
    identifier: Option<&str>,
    task: &str,
) -> String {
    match profile {
        LanguageProfile::Korean => match identifier {
            Some(id) => format!("{id} 오류를 처리하고 계속 진행: {task}"),
            None => format!("최근 오류를 해결하고 계속 진행: {task}"),
        },
        LanguageProfile::Japanese => match identifier {
            Some(id) => format!("{id} エラーを解消して継続: {task}"),
            None => format!("直近のエラーを解消して継続: {task}"),
        },
        LanguageProfile::Chinese => match identifier {
            Some(id) => format!("处理 {id} 错误并继续: {task}"),
            None => format!("处理最近错误并继续: {task}"),
        },
        LanguageProfile::Other => match identifier {
            Some(id) => format!("Address {id} and continue: {task}"),
            None => format!("Resolve the latest error and continue: {task}"),
        },
    }
}

fn is_user_role(role: &str) -> bool {
    matches!(
        role,
        "user" | "사용자" | "유저" | "ユーザー" | "用户" | "用戶"
    )
}

fn is_assistant_or_tool_role(role: &str) -> bool {
    matches!(
        role,
        "assistant"
            | "tool"
            | "도구"
            | "assistant/tool"
            | "アシスタント"
            | "ツール"
            | "助手"
            | "工具"
    )
}

fn normalize_message_id(id: &str) -> Option<String> {
    let value = id.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn short_excerpt(text: &str, max_chars: usize) -> String {
    normalize_whitespace(text)
        .chars()
        .take(max_chars)
        .collect::<String>()
}

fn collect_observed_message_ids(pending_messages: &[OmPendingMessage]) -> Vec<String> {
    let mut ids = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();
    for message in pending_messages {
        let Some(id) = normalize_message_id(&message.id) else {
            continue;
        };
        if seen.insert(id.clone()) {
            ids.push(id);
        }
    }
    ids
}

fn find_latest_task_signal(
    pending_messages: &[OmPendingMessage],
) -> Option<(&OmPendingMessage, String)> {
    pending_messages.iter().rev().find_map(|message| {
        if !is_user_role(&message_role(message)) {
            return None;
        }
        extract_task_phrase(&message.text).map(|task| (message, task))
    })
}

fn find_latest_error_signal(pending_messages: &[OmPendingMessage]) -> Option<&OmPendingMessage> {
    pending_messages.iter().rev().find(|message| {
        is_assistant_or_tool_role(&message_role(message)) && contains_error_signal(&message.text)
    })
}

fn compute_confidence_milli(
    task_text: Option<&str>,
    has_error_signal: bool,
    has_observation: bool,
) -> u16 {
    let mut confidence = match task_text {
        Some(task) if task.contains('?') || task.contains('？') => 600u16,
        Some(_) => 820u16,
        None => 0u16,
    };
    if has_error_signal && confidence > 0 {
        confidence = confidence.saturating_add(120);
    }
    if has_observation {
        confidence = confidence.max(320);
    }
    confidence.min(1000)
}

pub fn infer_deterministic_continuation(
    pending_messages: &[OmPendingMessage],
) -> (Option<String>, Option<String>) {
    let last_user = pending_messages
        .iter()
        .rev()
        .find(|message| is_user_role(&message_role(message)));
    let last_error = pending_messages.iter().rev().find(|message| {
        is_assistant_or_tool_role(&message_role(message)) && contains_error_signal(&message.text)
    });

    let current_task = last_user.and_then(|message| extract_task_phrase(&message.text));
    let language_profile = current_task
        .as_deref()
        .map(detect_language_profile)
        .or_else(|| last_user.map(|message| detect_language_profile(&message.text)))
        .unwrap_or(LanguageProfile::Other);
    let suggested_response = match (current_task.as_deref(), last_error) {
        (Some(task), Some(error)) => {
            let id = extract_error_identifier(&error.text);
            Some(build_error_recovery_response(
                language_profile,
                id.as_deref(),
                task,
            ))
        }
        (Some(task), None) => Some(build_suggested_response(language_profile, task)),
        (None, _) => None,
    }
    .map(|value| value.chars().take(180).collect::<String>());

    (current_task, suggested_response)
}

#[must_use]
pub fn infer_deterministic_observer_response(
    active_observations: &str,
    pending_messages: &[OmPendingMessage],
    max_chars: usize,
) -> OmDeterministicObserverResponseV2 {
    let observations =
        synthesize_observer_observations(active_observations, pending_messages, max_chars);
    let observation_token_count = estimate_text_tokens(&observations);
    let observed_message_ids = collect_observed_message_ids(pending_messages);

    let task_signal = find_latest_task_signal(pending_messages);
    let error_signal = find_latest_error_signal(pending_messages);
    let (mut current_task, mut suggested_response) =
        infer_deterministic_continuation(pending_messages);

    let confidence_milli = compute_confidence_milli(
        task_signal.as_ref().map(|(_, task)| task.as_str()),
        error_signal.is_some(),
        !observations.trim().is_empty(),
    );

    let policy = ContinuationPolicyV2::default();
    if confidence_milli < policy.min_confidence_milli_for_task {
        current_task = None;
    }
    if confidence_milli < policy.min_confidence_milli_for_suggested_response {
        suggested_response = None;
    }

    let mut evidence = Vec::<OmDeterministicEvidence>::new();
    if let Some((message, _)) = task_signal.as_ref()
        && let Some(message_id) = normalize_message_id(&message.id)
    {
        evidence.push(OmDeterministicEvidence {
            message_id,
            role: normalize_whitespace(&message.role),
            kind: OmDeterministicEvidenceKind::TaskSignal,
            excerpt: short_excerpt(&message.text, 120),
        });
    }
    if let Some(message) = error_signal
        && let Some(message_id) = normalize_message_id(&message.id)
    {
        evidence.push(OmDeterministicEvidence {
            message_id,
            role: normalize_whitespace(&message.role),
            kind: OmDeterministicEvidenceKind::ErrorSignal,
            excerpt: short_excerpt(&message.text, 120),
        });
    }
    if let Some(message) = pending_messages
        .iter()
        .rev()
        .find(|message| !normalize_whitespace(&message.text).is_empty())
        && let Some(message_id) = normalize_message_id(&message.id)
    {
        evidence.push(OmDeterministicEvidence {
            message_id,
            role: normalize_whitespace(&message.role),
            kind: OmDeterministicEvidenceKind::ObservationLine,
            excerpt: short_excerpt(&message.text, 120),
        });
    }

    OmDeterministicObserverResponseV2 {
        observations,
        observation_token_count,
        observed_message_ids,
        current_task,
        suggested_response,
        confidence_milli,
        evidence,
    }
}
