use super::{
    OmObserverRequest, OmObserverResponse, OmReflectorRequest, OmReflectorResponse,
    ReflectionAction,
};
use std::{future::Future, pin::Pin};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmReflectionCommandType {
    BufferRequested,
    ReflectRequested,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OmReflectionCommand {
    pub command_type: OmReflectionCommandType,
    pub scope_key: String,
    pub expected_generation: u32,
    pub requested_at_rfc3339: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OmCommand {
    EnqueueReflection(OmReflectionCommand),
}

pub trait OmApplyAddon {
    type Error;

    fn apply(&mut self, command: OmCommand) -> Result<(), Self::Error>;

    fn apply_async<'a>(&'a mut self, command: OmCommand) -> OmApplyFuture<'a, Self::Error> {
        Box::pin(async move { self.apply(command) })
    }
}

pub trait OmObserverAddon {
    type Error;

    fn observe(&mut self, request: &OmObserverRequest) -> Result<OmObserverResponse, Self::Error>;

    fn observe_async<'a>(
        &'a mut self,
        request: &'a OmObserverRequest,
    ) -> OmObserverFuture<'a, Self::Error> {
        Box::pin(async move { self.observe(request) })
    }
}

pub trait OmReflectorAddon {
    type Error;

    fn reflect(&mut self, request: &OmReflectorRequest)
    -> Result<OmReflectorResponse, Self::Error>;

    fn reflect_async<'a>(
        &'a mut self,
        request: &'a OmReflectorRequest,
    ) -> OmReflectorFuture<'a, Self::Error> {
        Box::pin(async move { self.reflect(request) })
    }
}

pub type OmApplyFuture<'a, E> = Pin<Box<dyn Future<Output = Result<(), E>> + 'a>>;
pub type OmObserverFuture<'a, E> =
    Pin<Box<dyn Future<Output = Result<OmObserverResponse, E>> + 'a>>;
pub type OmReflectorFuture<'a, E> =
    Pin<Box<dyn Future<Output = Result<OmReflectorResponse, E>> + 'a>>;

pub fn reflection_command_from_action(
    action: ReflectionAction,
    scope_key: &str,
    expected_generation: u32,
    requested_at_rfc3339: &str,
) -> Option<OmCommand> {
    let normalized_scope_key = scope_key.trim();
    if normalized_scope_key.is_empty() {
        return None;
    }
    let normalized_requested_at = requested_at_rfc3339.trim();
    if chrono::DateTime::parse_from_rfc3339(normalized_requested_at).is_err() {
        return None;
    }

    let command_type = match action {
        ReflectionAction::None => return None,
        ReflectionAction::Buffer => OmReflectionCommandType::BufferRequested,
        ReflectionAction::Reflect => OmReflectionCommandType::ReflectRequested,
    };
    Some(OmCommand::EnqueueReflection(OmReflectionCommand {
        command_type,
        scope_key: normalized_scope_key.to_string(),
        expected_generation,
        requested_at_rfc3339: normalized_requested_at.to_string(),
    }))
}

#[cfg(test)]
mod tests;
