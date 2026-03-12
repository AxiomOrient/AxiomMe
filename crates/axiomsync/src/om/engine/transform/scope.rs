use crate::model::{OmScope, OmThreadRefV2};
use std::borrow::Cow;

use super::OmTransformError;

fn normalize_identifier(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn scope_key_identifier(scope_key: &str, prefix: &str) -> Option<String> {
    scope_key
        .trim()
        .strip_prefix(prefix)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

#[must_use]
pub fn resolve_canonical_thread_ref(
    scope: OmScope,
    scope_key: &str,
    source_thread_id: Option<&str>,
    source_session_id: Option<&str>,
    thread_id: Option<&str>,
    session_id: Option<&str>,
    resource_id: Option<&str>,
) -> OmThreadRefV2 {
    let normalized_scope_key = scope_key.trim().to_string();
    let origin_thread_id =
        normalize_identifier(source_thread_id).or_else(|| normalize_identifier(thread_id));
    let origin_session_id =
        normalize_identifier(source_session_id).or_else(|| normalize_identifier(session_id));

    let thread_from_scope = scope_key_identifier(&normalized_scope_key, "thread:");
    let session_from_scope = scope_key_identifier(&normalized_scope_key, "session:");
    let resource_from_scope = scope_key_identifier(&normalized_scope_key, "resource:");
    let resolved_resource_id = normalize_identifier(resource_id).or_else(|| {
        matches!(scope, OmScope::Resource)
            .then(|| resource_from_scope.as_deref())
            .flatten()
            .map(ToString::to_string)
    });

    let preferred_thread = origin_thread_id.as_deref().or(thread_from_scope.as_deref());
    let preferred_session = origin_session_id
        .as_deref()
        .or(session_from_scope.as_deref());
    let preferred_resource = resolved_resource_id.as_deref();

    let (kind, canonical_id): (&str, Cow<'_, str>) = match scope {
        OmScope::Session => {
            if let Some(id) = preferred_session {
                ("session", Cow::Borrowed(id))
            } else if let Some(id) = preferred_thread {
                ("thread", Cow::Borrowed(id))
            } else if let Some(id) = preferred_resource {
                ("resource", Cow::Borrowed(id))
            } else {
                ("session", Cow::Borrowed("default"))
            }
        }
        OmScope::Thread => {
            if let Some(id) = preferred_thread {
                ("thread", Cow::Borrowed(id))
            } else if let Some(id) = preferred_session {
                ("session", Cow::Borrowed(id))
            } else if let Some(id) = preferred_resource {
                ("resource", Cow::Borrowed(id))
            } else {
                ("thread", Cow::Borrowed("default"))
            }
        }
        OmScope::Resource => {
            if let Some(id) = preferred_thread {
                ("thread", Cow::Borrowed(id))
            } else if let Some(id) = preferred_session {
                ("session", Cow::Borrowed(id))
            } else if let Some(id) = preferred_resource {
                ("resource", Cow::Borrowed(id))
            } else {
                ("resource", Cow::Borrowed("default"))
            }
        }
    };

    OmThreadRefV2 {
        canonical_thread_id: format!("{kind}:{canonical_id}"),
        scope,
        scope_key: normalized_scope_key,
        origin_thread_id,
        origin_session_id,
        resource_id: resolved_resource_id,
    }
}

pub fn build_scope_key(
    scope: OmScope,
    session_id: Option<&str>,
    thread_id: Option<&str>,
    resource_id: Option<&str>,
) -> Result<String, OmTransformError> {
    match scope {
        OmScope::Session => session_id
            .filter(|x| !x.trim().is_empty())
            .map(|x| format!("session:{}", x.trim()))
            .ok_or(OmTransformError::MissingScopeIdentifier("session_id")),
        OmScope::Thread => thread_id
            .filter(|x| !x.trim().is_empty())
            .map(|x| format!("thread:{}", x.trim()))
            .ok_or(OmTransformError::MissingScopeIdentifier("thread_id")),
        OmScope::Resource => resource_id
            .filter(|x| !x.trim().is_empty())
            .map(|x| format!("resource:{}", x.trim()))
            .ok_or(OmTransformError::MissingScopeIdentifier("resource_id")),
    }
}
