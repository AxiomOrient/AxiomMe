use super::OmScope;

#[must_use]
pub fn resolve_canonical_thread_id(
    scope: OmScope,
    scope_key: &str,
    origin_thread_id: Option<&str>,
    origin_session_id: Option<&str>,
    fallback_session_id: &str,
) -> String {
    let origin_thread_id = normalize_thread_identity(origin_thread_id);
    let origin_session_id = normalize_thread_identity(origin_session_id);
    let fallback_session_id = normalize_thread_identity(Some(fallback_session_id));

    let scope_key_thread = match scope {
        OmScope::Thread => strip_scope_prefix(scope_key, "thread:"),
        OmScope::Session => strip_scope_prefix(scope_key, "session:"),
        OmScope::Resource => None,
    };

    let canonical = match scope {
        OmScope::Thread => scope_key_thread
            .or(origin_thread_id)
            .or(origin_session_id)
            .or(fallback_session_id),
        OmScope::Resource => origin_thread_id
            .or(origin_session_id)
            .or(fallback_session_id),
        OmScope::Session => scope_key_thread
            .or(origin_thread_id)
            .or(origin_session_id)
            .or(fallback_session_id),
    };

    canonical.unwrap_or_else(|| scope_key.trim().to_string())
}

fn normalize_thread_identity(raw: Option<&str>) -> Option<String> {
    let value = raw.map(str::trim).filter(|value| !value.is_empty())?;
    if let Some(stripped) = strip_scope_prefix(value, "thread:") {
        return Some(stripped);
    }
    if let Some(stripped) = strip_scope_prefix(value, "session:") {
        return Some(stripped);
    }
    Some(value.to_string())
}

fn strip_scope_prefix(value: &str, prefix: &str) -> Option<String> {
    value
        .strip_prefix(prefix)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::resolve_canonical_thread_id;
    use crate::om::OmScope;

    #[test]
    fn thread_scope_prefers_scope_key_thread_id() {
        let canonical = resolve_canonical_thread_id(
            OmScope::Thread,
            "thread:t-main",
            Some("session:s-x"),
            Some("s-x"),
            "s-fallback",
        );
        assert_eq!(canonical, "t-main");
    }

    #[test]
    fn resource_scope_prefers_origin_thread_then_session() {
        let canonical = resolve_canonical_thread_id(
            OmScope::Resource,
            "resource:r1",
            Some("thread:t-origin"),
            Some("session:s-origin"),
            "s-fallback",
        );
        assert_eq!(canonical, "t-origin");

        let session_fallback = resolve_canonical_thread_id(
            OmScope::Resource,
            "resource:r1",
            None,
            Some("session:s-origin"),
            "s-fallback",
        );
        assert_eq!(session_fallback, "s-origin");
    }

    #[test]
    fn session_scope_uses_scope_key_identifier() {
        let canonical = resolve_canonical_thread_id(
            OmScope::Session,
            "session:s-main",
            None,
            None,
            "s-fallback",
        );
        assert_eq!(canonical, "s-main");
    }
}
