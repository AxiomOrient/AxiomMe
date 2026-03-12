use reqwest::Url;
use serde_json::Value;

pub fn parse_env_bool(raw: Option<&str>) -> bool {
    matches!(
        raw.map(|value| value.trim().to_ascii_lowercase()),
        Some(value) if matches!(value.as_str(), "1" | "true" | "yes" | "on")
    )
}

pub fn parse_local_loopback_endpoint(
    raw: &str,
    label: &str,
    host_requirement: &str,
) -> std::result::Result<Url, String> {
    let url = Url::parse(raw).map_err(|err| format!("invalid {label}: {err}"))?;
    match url.scheme() {
        "http" | "https" => {}
        other => return Err(format!("unsupported {label} scheme: {other}")),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(format!("{label} must not include credentials"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| format!("{label} host is missing"))?;
    if !matches!(host, "127.0.0.1" | "localhost" | "::1") {
        return Err(format!("{label} must use {host_requirement}, got: {host}"));
    }
    Ok(url)
}

pub fn extract_llm_content(value: &Value) -> Option<String> {
    if let Some(content) = value
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
    {
        return Some(content.to_string());
    }
    if let Some(content) = value
        .get("choices")
        .and_then(|choices| choices.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
    {
        return Some(content.to_string());
    }
    if let Some(content) = value.get("response").and_then(|response| response.as_str()) {
        return Some(content.to_string());
    }
    None
}

pub fn extract_json_fragment(text: &str) -> Option<String> {
    let start = text
        .char_indices()
        .find(|(_, c)| *c == '{' || *c == '[')
        .map(|(idx, _)| idx)?;
    let sliced = &text[start..];
    let end = sliced
        .char_indices()
        .rev()
        .find(|(_, c)| *c == '}' || *c == ']')
        .map(|(idx, c)| idx + c.len_utf8())?;
    Some(sliced[..end].to_string())
}

pub fn parse_u32_value(value: &Value) -> Option<u32> {
    if let Some(raw) = value.as_u64() {
        return Some(saturating_u64_to_u32(raw));
    }
    if let Some(raw) = value.as_i64() {
        if raw <= 0 {
            return Some(0);
        }
        return Some(u32::try_from(raw).unwrap_or(u32::MAX));
    }
    if let Some(raw) = value.as_f64()
        && raw.is_finite()
    {
        return Some(rounded_f64_to_u32_clamped(raw));
    }
    None
}

pub fn estimate_text_tokens(text: &str) -> u32 {
    let chars = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
    if chars == 0 {
        return 0;
    }
    chars.div_ceil(4)
}

fn saturating_u64_to_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn rounded_f64_to_u32_clamped(value: f64) -> u32 {
    let rounded = value.round().clamp(0.0, f64::from(u32::MAX));
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "value is pre-clamped to the representable non-negative u32 range"
    )]
    {
        rounded as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_env_bool_accepts_true_tokens() {
        assert!(parse_env_bool(Some("1")));
        assert!(parse_env_bool(Some("true")));
        assert!(parse_env_bool(Some("YES")));
        assert!(!parse_env_bool(Some("0")));
        assert!(!parse_env_bool(Some("false")));
        assert!(!parse_env_bool(None));
    }

    #[test]
    fn parse_local_loopback_endpoint_rejects_non_loopback() {
        let err =
            parse_local_loopback_endpoint("http://example.com/x", "test endpoint", "local host")
                .expect_err("must reject non-loopback host");
        assert!(err.contains("test endpoint"));
    }

    #[test]
    fn extract_llm_content_prefers_message_then_response() {
        let value = serde_json::json!({
            "message": {"content": "hello"},
            "response": "fallback"
        });
        assert_eq!(extract_llm_content(&value).as_deref(), Some("hello"));
    }

    #[test]
    fn extract_json_fragment_reads_embedded_object() {
        let value = "```json\n{\"a\":1}\n```";
        assert_eq!(extract_json_fragment(value).as_deref(), Some("{\"a\":1}"));
    }

    #[test]
    fn parse_u32_value_handles_integer_and_float_shapes() {
        assert_eq!(parse_u32_value(&serde_json::json!(7)), Some(7));
        assert_eq!(parse_u32_value(&serde_json::json!(-2)), Some(0));
        assert_eq!(parse_u32_value(&serde_json::json!(1.6)), Some(2));
    }

    #[test]
    fn estimate_text_tokens_uses_char_div4_policy() {
        assert_eq!(estimate_text_tokens(""), 0);
        assert_eq!(estimate_text_tokens("abcd"), 1);
        assert_eq!(estimate_text_tokens("abcde"), 2);
    }
}
