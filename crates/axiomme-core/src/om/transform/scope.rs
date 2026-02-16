use crate::om::model::OmScope;

use super::OmTransformError;

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
