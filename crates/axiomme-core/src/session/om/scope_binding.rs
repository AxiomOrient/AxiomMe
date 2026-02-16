use crate::config::OmScopeConfig;
use crate::error::{AxiomError, Result};
use crate::om::{OmScope, build_scope_key};

use super::OmScopeBinding;

const DEFAULT_OM_SCOPE: &str = "session";
pub(super) const ENV_OM_SCOPE: &str = "AXIOMME_OM_SCOPE";

pub(super) fn resolve_om_scope_binding_for_session_with_config(
    session_id: &str,
    config: &OmScopeConfig,
) -> Result<OmScopeBinding> {
    resolve_om_scope_binding(
        session_id,
        config.scope.as_deref(),
        config.thread_id.as_deref(),
        config.resource_id.as_deref(),
    )
}

pub(super) fn resolve_om_scope_binding(
    session_id: &str,
    scope_raw: Option<&str>,
    thread_id: Option<&str>,
    resource_id: Option<&str>,
) -> Result<OmScopeBinding> {
    let scope_token = scope_raw
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_OM_SCOPE)
        .to_ascii_lowercase();
    let scope = match scope_token.as_str() {
        "session" => OmScope::Session,
        "thread" => OmScope::Thread,
        "resource" => OmScope::Resource,
        other => {
            return Err(AxiomError::Validation(format!(
                "invalid {ENV_OM_SCOPE}: {other} (expected: session|thread|resource)"
            )));
        }
    };

    resolve_om_scope_binding_explicit(session_id, scope, thread_id, resource_id)
}

pub(super) fn resolve_om_scope_binding_explicit(
    session_id: &str,
    scope: OmScope,
    thread_id: Option<&str>,
    resource_id: Option<&str>,
) -> Result<OmScopeBinding> {
    let thread_id = normalize_scope_identifier(thread_id);
    let resource_id = normalize_scope_identifier(resource_id);
    let scope_key = build_scope_key(
        scope,
        Some(session_id),
        thread_id.as_deref(),
        resource_id.as_deref(),
    )
    .map_err(|err| AxiomError::Validation(err.to_string()))?;

    let (resolved_session_id, resolved_thread_id, resolved_resource_id) = match scope {
        OmScope::Session => (Some(session_id.to_string()), None, None),
        OmScope::Thread => (None, thread_id, resource_id),
        OmScope::Resource => (None, None, resource_id),
    };
    Ok(OmScopeBinding {
        scope,
        scope_key,
        session_id: resolved_session_id,
        thread_id: resolved_thread_id,
        resource_id: resolved_resource_id,
    })
}

#[cfg(test)]
pub(super) fn parse_env_enabled_default_true(raw: Option<&str>) -> bool {
    let Some(raw) = raw.map(str::trim) else {
        return true;
    };
    if raw.is_empty() {
        return true;
    }
    !matches!(
        raw.to_ascii_lowercase().as_str(),
        "0" | "false" | "no" | "off" | "disabled"
    )
}

fn normalize_scope_identifier(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}
