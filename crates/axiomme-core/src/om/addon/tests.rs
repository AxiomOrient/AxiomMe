use std::{future::Future, pin::pin, task::Poll};

use super::*;

fn poll_ready<F>(future: std::pin::Pin<&mut F>) -> F::Output
where
    F: Future,
{
    let waker = std::task::Waker::noop();
    let mut cx = std::task::Context::from_waker(waker);
    match future.poll(&mut cx) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("future unexpectedly pending"),
    }
}

struct CountingApplyAddon {
    applied: usize,
}

impl OmApplyAddon for CountingApplyAddon {
    type Error = ();

    fn apply(&mut self, _command: OmCommand) -> Result<(), Self::Error> {
        self.applied = self.applied.saturating_add(1);
        Ok(())
    }
}

struct EchoObserverAddon;

impl OmObserverAddon for EchoObserverAddon {
    type Error = ();

    fn observe(&mut self, request: &OmObserverRequest) -> Result<OmObserverResponse, Self::Error> {
        Ok(OmObserverResponse {
            observations: request.active_observations.clone(),
            observation_token_count: 0,
            observed_message_ids: request
                .pending_messages
                .iter()
                .map(|item| item.id.clone())
                .collect::<Vec<_>>(),
            current_task: None,
            suggested_response: None,
            usage: super::super::OmInferenceUsage::default(),
        })
    }
}

struct EchoReflectorAddon;

impl OmReflectorAddon for EchoReflectorAddon {
    type Error = ();

    fn reflect(
        &mut self,
        request: &OmReflectorRequest,
    ) -> Result<OmReflectorResponse, Self::Error> {
        Ok(OmReflectorResponse {
            reflection: request.active_observations.clone(),
            reflection_token_count: 0,
            reflected_observation_line_count: 0,
            current_task: None,
            suggested_response: None,
            usage: super::super::OmInferenceUsage::default(),
        })
    }
}

#[test]
fn reflection_command_builder_maps_reflection_action_to_command() {
    let at = "2026-02-13T00:00:00Z";
    let buffer = reflection_command_from_action(ReflectionAction::Buffer, "session:s1", 3, at)
        .expect("buffer command");
    let reflect = reflection_command_from_action(ReflectionAction::Reflect, "session:s1", 3, at)
        .expect("reflect command");

    assert_eq!(
        buffer,
        OmCommand::EnqueueReflection(OmReflectionCommand {
            command_type: OmReflectionCommandType::BufferRequested,
            scope_key: "session:s1".to_string(),
            expected_generation: 3,
            requested_at_rfc3339: at.to_string(),
        })
    );
    assert_eq!(
        reflect,
        OmCommand::EnqueueReflection(OmReflectionCommand {
            command_type: OmReflectionCommandType::ReflectRequested,
            scope_key: "session:s1".to_string(),
            expected_generation: 3,
            requested_at_rfc3339: at.to_string(),
        })
    );
    assert_eq!(
        reflection_command_from_action(ReflectionAction::None, "session:s1", 3, at),
        None
    );
}

#[test]
fn apply_async_delegates_to_sync_apply_by_default() {
    let mut addon = CountingApplyAddon { applied: 0 };
    let command = OmCommand::EnqueueReflection(OmReflectionCommand {
        command_type: OmReflectionCommandType::BufferRequested,
        scope_key: "session:s1".to_string(),
        expected_generation: 1,
        requested_at_rfc3339: "2026-02-13T00:00:00Z".to_string(),
    });
    {
        let mut future = pin!(addon.apply_async(command));
        let result = poll_ready(future.as_mut());
        assert_eq!(result, Ok(()));
    }
    assert_eq!(addon.applied, 1);
}

#[test]
fn observe_async_delegates_to_sync_observe_by_default() {
    let mut addon = EchoObserverAddon;
    let request = OmObserverRequest {
        scope: super::super::OmScope::Session,
        scope_key: "session:s1".to_string(),
        model: super::super::OmInferenceModelConfig {
            provider: "local-http".to_string(),
            model: "test".to_string(),
            max_output_tokens: 128,
            temperature_milli: 0,
        },
        active_observations: "a".to_string(),
        other_conversations: None,
        pending_messages: vec![super::super::OmPendingMessage {
            id: "m1".to_string(),
            role: "user".to_string(),
            text: "hello".to_string(),
            created_at_rfc3339: None,
        }],
    };
    let mut future = pin!(addon.observe_async(&request));
    let result = poll_ready(future.as_mut()).expect("observe");
    assert_eq!(result.observations, "a");
    assert_eq!(result.observed_message_ids, vec!["m1".to_string()]);
}

#[test]
fn reflect_async_delegates_to_sync_reflect_by_default() {
    let mut addon = EchoReflectorAddon;
    let request = OmReflectorRequest {
        scope: super::super::OmScope::Session,
        scope_key: "session:s1".to_string(),
        model: super::super::OmInferenceModelConfig {
            provider: "local-http".to_string(),
            model: "test".to_string(),
            max_output_tokens: 128,
            temperature_milli: 0,
        },
        generation_count: 1,
        active_observations: "reflect-me".to_string(),
    };
    let mut future = pin!(addon.reflect_async(&request));
    let result = poll_ready(future.as_mut()).expect("reflect");
    assert_eq!(result.reflection, "reflect-me");
}
